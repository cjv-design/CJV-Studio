//! Local Adobe enhanced-profile import. Profile assets are read from the user's
//! installation and cached privately; none are distributed with the application.
use flate2::read::ZlibDecoder;
use regex::regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

const MAX_XMP: u64 = 8 * 1024 * 1024;
const MAX_TABLE: usize = 1_000_000;
const ALPHABET: &[u8] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?`'|()[]{}@%$#";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RgbTable {
    pub size: u32,
    /// Red is the fastest-changing coordinate, as in a .cube file.
    pub data: Vec<f32>,
    pub min_amount: f32,
    pub max_amount: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub schema: u32,
    pub uuid: String,
    pub name: String,
    pub deltas: BTreeMap<String, f64>,
    pub table: Option<RgbTable>,
}

fn attrs(content: &str) -> Result<HashMap<String, String>, String> {
    let outer = regex!(r"(?s)<rdf:Description\b([^>]*)>")
        .captures(content)
        .and_then(|c| c.get(1))
        .ok_or("Missing profile description")?;
    regex!(r#"crs:([A-Za-z0-9_]+)="([^"]*)""#)
        .captures_iter(outer.as_str())
        .map(|c| {
            Ok((
                c[1].to_owned(),
                quick_xml::escape::unescape(&c[2])
                    .map_err(|_| "Invalid profile XML escape")?
                    .into_owned(),
            ))
        })
        .collect()
}

fn number(a: &HashMap<String, String>, key: &str, default: f64) -> f64 {
    a.get(key)
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

fn valid_uuid(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Decode Adobe's little-endian base85 and bounded zlib RGB-table record.
/// This parser is independent of Adobe SDK implementation code.
pub fn decode_rgb_table(encoded: &str) -> Result<RgbTable, String> {
    if encoded.len() > MAX_TABLE * 2 {
        return Err("Profile table is too large".into());
    }
    let mut binary = Vec::new();
    let mut value = 0_u64;
    let mut factor = 1_u64;
    let mut count = 0;
    for b in encoded.bytes().filter(|b| !b.is_ascii_whitespace()) {
        let digit = ALPHABET
            .iter()
            .position(|v| *v == b)
            .ok_or("Invalid profile table character")?;
        value += digit as u64 * factor;
        count += 1;
        if count == 5 {
            let word = u32::try_from(value).map_err(|_| "Invalid profile table word")?;
            binary.extend_from_slice(&word.to_le_bytes());
            value = 0;
            factor = 1;
            count = 0;
        } else {
            factor *= 85;
        }
    }
    if count == 1 {
        return Err("Truncated profile table encoding".into());
    }
    if count > 1 {
        binary.extend_from_slice(&value.to_le_bytes()[..count - 1]);
    }
    let bytes: [u8; 4] = binary
        .get(..4)
        .ok_or("Missing profile table size")?
        .try_into()
        .unwrap();
    let expected = u32::from_le_bytes(bytes) as usize;
    if !(44..=MAX_TABLE).contains(&expected) {
        return Err("Unsupported profile table size".into());
    }
    let mut raw = Vec::new();
    ZlibDecoder::new(&binary[4..])
        .take((expected + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(|_| "Invalid compressed profile table")?;
    if raw.len() != expected {
        return Err("Profile table size mismatch".into());
    }
    let u32_at = |i| u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
    let size = u32_at(12) as usize;
    if u32_at(0) != 1 || u32_at(4) != 1 || u32_at(8) != 3 || !(2..=32).contains(&size) {
        return Err("Only version-1 three-dimensional RGB profiles are supported".into());
    }
    let end = 16 + size.pow(3) * 6;
    if raw.len() != end + 28 && raw.len() != end + 32 {
        return Err("Truncated RGB profile data".into());
    }
    if u32_at(end) != 0 || u32_at(end + 4) != 1 || u32_at(end + 8) > 1 {
        return Err("This profile uses an unsupported colour space or gamma".into());
    }
    if raw.len() == end + 32 && u32_at(end + 28) != 0 {
        return Err("This profile uses unsupported table flags".into());
    }
    let amount = |i| f64::from_le_bytes(raw[i..i + 8].try_into().unwrap());
    let (min, max) = (amount(end + 12), amount(end + 20));
    if !min.is_finite()
        || !max.is_finite()
        || !(0.0..=1.0).contains(&min)
        || !(1.0..=2.0).contains(&max)
    {
        return Err("Invalid profile amount range".into());
    }
    let mut data = vec![0.0; size.pow(3) * 3];
    for r in 0..size {
        for g in 0..size {
            for b in 0..size {
                let src = 16 + ((r * size + g) * size + b) * 6;
                let dst = ((b * size + g) * size + r) * 3;
                for (channel, coordinate) in [r, g, b].into_iter().enumerate() {
                    let delta = u16::from_le_bytes(
                        raw[src + channel * 2..src + channel * 2 + 2]
                            .try_into()
                            .unwrap(),
                    );
                    let identity = ((coordinate * 65535 + (size >> 1)) / (size - 1)) as u16;
                    data[dst + channel] = delta.wrapping_add(identity) as f32 / 65535.0;
                }
            }
        }
    }
    Ok(RgbTable {
        size: size as u32,
        data,
        min_amount: min as f32,
        max_amount: max as f32,
    })
}

fn read_xmp(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > MAX_XMP {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

pub fn parse_profile(content: &str, uuid: &str) -> Result<Profile, String> {
    let a = attrs(content)?;
    if !valid_uuid(uuid) || a.get("UUID").is_none_or(|v| !v.eq_ignore_ascii_case(uuid)) {
        return Err("Profile identity does not match the preset".into());
    }
    if a.get("PresetType").is_none_or(|v| v != "Look") {
        return Err("The referenced asset is not an enhanced profile".into());
    }
    if a.contains_key("LookTable") || content.contains("<crs:ToneCurve") {
        return Err("This profile contains an unsupported look table or tone curve".into());
    }
    let table = a
        .get("RGBTable")
        .map(|id| {
            if !valid_uuid(id) {
                return Err("Invalid RGB table identity".into());
            }
            decode_rgb_table(
                a.get(&format!("Table_{id}"))
                    .ok_or("The RGB profile table is missing")?,
            )
        })
        .transpose()?;
    let mut deltas = BTreeMap::new();
    for (src, dst, scale) in [
        ("Exposure2012", "exposure", 1.0),
        ("Contrast2012", "contrast", 1.0),
        ("Highlights2012", "highlights", 1.0),
        ("Shadows2012", "shadows", 1.5),
        ("Whites2012", "whites", 1.0),
        ("Blacks2012", "blacks", 1.0),
        ("Clarity2012", "clarity", 1.0),
        ("Texture", "structure", 1.0),
        ("Dehaze", "dehaze", 1.0),
        ("Vibrance", "vibrance", 1.0),
        ("Saturation", "saturation", 1.0),
    ] {
        let v = number(&a, src, 0.0);
        if v != 0.0 {
            deltas.insert(dst.to_string(), v.clamp(-100.0, 100.0) * scale);
        }
    }
    // Do not silently drop unsupported grading/calibration or monochrome data.
    for (key, value) in &a {
        if (key.starts_with("ColorGrade")
            || key.starts_with("SplitToning")
            || key.starts_with("Parametric")
            || matches!(
                key.as_str(),
                "ShadowTint"
                    | "RedHue"
                    | "RedSaturation"
                    | "GreenHue"
                    | "GreenSaturation"
                    | "BlueHue"
                    | "BlueSaturation"
            ))
            && value.parse::<f64>().is_ok_and(|v| v != 0.0)
        {
            return Err(format!("Profile adjustment {key} is not supported yet"));
        }
    }
    if a.get("ConvertToGrayscale")
        .is_some_and(|s| s.eq_ignore_ascii_case("true"))
    {
        return Err("Monochrome enhanced profiles are not supported yet".into());
    }
    if a.get("ToneMapStrength").is_some_and(|value| value != "0") {
        return Err("Enhanced profile tone-map strength is not supported yet".into());
    }
    let name = regex!(r"(?s)<crs:Name>.*?<rdf:li[^>]*>([^<]+)</rdf:li>")
        .captures(content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .unwrap_or("Imported film profile");
    let name = quick_xml::escape::unescape(name)
        .map_err(|_| "Invalid profile name")?
        .into_owned();
    Ok(Profile {
        schema: 1,
        uuid: uuid.to_ascii_uppercase(),
        name,
        deltas,
        table,
    })
}

/// Resolve a Look UUID only against local installed profiles, never the network.
pub fn import_reference(
    xmp: &str,
    roots: &[PathBuf],
    cache: &Path,
) -> Result<Option<Value>, String> {
    let Some(look) = regex!(r"(?s)<crs:Look\b.*?</crs:Look>").find(xmp) else {
        return Ok(None);
    };
    let reference = attrs(look.as_str())?;
    let uuid = reference
        .get("UUID")
        .filter(|s| valid_uuid(s))
        .ok_or("Missing film profile identity")?;
    let amount = number(&reference, "Amount", 1.0).clamp(0.0, 2.0);
    let mut found = None;
    for root in roots {
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .max_depth(12)
            .into_iter()
            .filter_map(Result::ok)
            .take(20_000)
        {
            let path = entry.path();
            if !entry.file_type().is_file()
                || !path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("xmp"))
            {
                continue;
            }
            let Some(content) = read_xmp(path) else {
                continue;
            };
            if !content.contains(uuid) {
                continue;
            }
            let Ok(a) = attrs(&content) else {
                continue;
            };
            if a.get("UUID").is_some_and(|v| v.eq_ignore_ascii_case(uuid))
                && a.get("PresetType").is_some_and(|v| v == "Look")
            {
                found = Some(parse_profile(&content, uuid)?);
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let profile = found.ok_or("The film profile is not installed locally")?;
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    // Content-addressed files prevent an updated source profile from changing
    // existing photo edits or presets that reference the previous version.
    let bytes = serde_json::to_vec(&profile).map_err(|e| e.to_string())?;
    use sha2::{Digest, Sha256};
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let path = cache.join(format!("{digest}.json"));
    if !path.exists() {
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    }
    Ok(Some(
        json!({ "uuid": profile.uuid, "name": profile.name, "amount": amount, "path": path,
        "amountVersion": 2, "enabled": true, "minAmount": 0.0, "maxAmount": 2.0,
        "tableMinAmount": profile.table.as_ref().map_or(0.0, |t| t.min_amount) }),
    ))
}

#[derive(Default)]
struct Registry {
    paths: HashMap<String, u32>,
    profiles: Vec<Arc<Profile>>,
}
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

pub struct LoadedAdjustment {
    pub id: u32,
    pub table_amount: f32,
    pub delta_amount: f32,
    pub profile: Arc<Profile>,
}

/// A zero XMP amount can still retain a table's minimum strength. Legacy edits
/// keep zero-as-disabled semantics, including when their cache is unavailable.
pub fn is_requested(adj: &Value) -> bool {
    if !adj.is_object() || adj["enabled"].as_bool() == Some(false) {
        return false;
    }
    let amount = adj["amount"].as_f64().filter(|v| v.is_finite());
    if amount.is_none_or(|v| v > 0.0) {
        return true;
    }
    adj["amountVersion"].as_u64() == Some(2)
        && adj["tableMinAmount"]
            .as_f64()
            .filter(|v| v.is_finite())
            .is_none_or(|v| v > 0.0)
}

pub fn load_adjustment(adj: &Value) -> Option<LoadedAdjustment> {
    if !is_requested(adj) {
        return None;
    }
    let path = adj["path"].as_str()?;
    let uuid = adj["uuid"].as_str()?;
    let requested = adj["amount"]
        .as_f64()
        .filter(|v| v.is_finite())?
        .clamp(0.0, 2.0) as f32;
    let mut registry = REGISTRY.get_or_init(Default::default).lock().ok()?;
    let id = if let Some(id) = registry.paths.get(path) {
        *id
    } else {
        if registry.profiles.len() >= 4096 || std::fs::metadata(path).ok()?.len() > 4 * 1024 * 1024
        {
            return None;
        }
        let profile: Profile = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        if profile.schema != 1
            || !profile.uuid.eq_ignore_ascii_case(uuid)
            || profile.name.len() > 1024
            || profile.deltas.len() > 32
            || profile
                .deltas
                .values()
                .any(|v| !v.is_finite() || v.abs() > 150.0)
        {
            return None;
        }
        if let Some(t) = &profile.table {
            if !(2..=32).contains(&t.size)
                || t.data.len() != (t.size as usize).pow(3) * 3
                || t.data
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || !t.min_amount.is_finite()
                || !t.max_amount.is_finite()
                || !(0.0..=1.0).contains(&t.min_amount)
                || !(1.0..=2.0).contains(&t.max_amount)
            {
                return None;
            }
        }
        registry.profiles.push(Arc::new(profile));
        let id = registry.profiles.len() as u32;
        registry.paths.insert(path.to_owned(), id);
        id
    };
    let profile = registry.profiles.get(id as usize - 1)?.clone();
    if !profile.uuid.eq_ignore_ascii_case(uuid) {
        return None;
    }
    let table_amount = profile
        .table
        .as_ref()
        .map_or(requested, |t| requested.clamp(t.min_amount, t.max_amount));
    // XMP Look Amount is already the stored profile strength. The RGB table
    // clamps its own range, while hidden basic adjustments use the requested
    // strength. Preserve the old shared clamp for existing saved edits.
    let delta_amount = if adj["amountVersion"].as_u64() == Some(2) {
        requested
    } else {
        table_amount
    };
    Some(LoadedAdjustment {
        id,
        table_amount,
        delta_amount,
        profile,
    })
}

pub fn lookup(id: u32) -> Option<Arc<Profile>> {
    REGISTRY
        .get()?
        .lock()
        .ok()?
        .profiles
        .get(id.checked_sub(1)? as usize)
        .cloned()
}

pub fn with_deltas(
    adjustments: &Value,
    profile: &Profile,
    amount: f32,
    reference_raw: bool,
) -> Value {
    let mut result = adjustments.clone();
    if !result.is_object() {
        return result;
    }
    for (key, delta) in &profile.deltas {
        if !matches!(
            key.as_str(),
            "exposure"
                | "contrast"
                | "highlights"
                | "shadows"
                | "whites"
                | "blacks"
                | "clarity"
                | "structure"
                | "dehaze"
                | "vibrance"
                | "saturation"
        ) {
            continue;
        }
        let base = result[key]
            .as_f64()
            .filter(|v| v.is_finite())
            .unwrap_or(0.0);
        let limit = if key == "exposure" { 5.0 } else { 100.0 };
        // Schema 1 profile caches store shadows in the legacy 1.5x units.
        let delta = if reference_raw && key == "shadows" {
            delta / 1.5
        } else {
            *delta
        };
        result[key] = json!((base + delta * amount as f64).clamp(-limit, limit));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn encode(raw: &[u8]) -> String {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(raw).unwrap();
        let mut bytes = (raw.len() as u32).to_le_bytes().to_vec();
        bytes.extend(z.finish().unwrap());
        let mut result = String::new();
        for chunk in bytes.chunks(4) {
            let mut padded = [0; 4];
            padded[..chunk.len()].copy_from_slice(chunk);
            let mut value = u32::from_le_bytes(padded) as u64;
            for _ in 0..chunk.len() + 1 {
                result.push(ALPHABET[(value % 85) as usize] as char);
                value /= 85;
            }
        }
        result
    }
    fn identity_record() -> Vec<u8> {
        let mut raw = Vec::new();
        for v in [1_u32, 1, 3, 2] {
            raw.extend(v.to_le_bytes());
        }
        // Adobe's integer identity predictor wraps at the two-point endpoint.
        // Store the compensating delta, as its writer does for a true identity.
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    for c in [r, g, b] {
                        raw.extend((if c == 0 { 0_u16 } else { 65535_u16 }).to_le_bytes());
                    }
                }
            }
        }
        for v in [0_u32, 1, 1] {
            raw.extend(v.to_le_bytes());
        }
        raw.extend(0_f64.to_le_bytes());
        raw.extend(2_f64.to_le_bytes());
        raw
    }
    #[test]
    fn decodes_identity_and_axis_order() {
        let mut raw = identity_record();
        // R=1 G=0 B=0 is fourth RGB triplet in Adobe order; set its G to .25.
        raw[16 + 4 * 6 + 2..16 + 4 * 6 + 4].copy_from_slice(&16384_u16.to_le_bytes());
        let table = decode_rgb_table(&encode(&raw)).unwrap();
        assert_eq!(table.size, 2);
        assert_eq!(table.data[3], 1.0);
        assert!((table.data[4] - 0.25).abs() < 0.00002);
        assert_eq!(&table.data[21..24], &[1.0, 1.0, 1.0]);
    }
    #[test]
    fn rejects_truncated_and_unsupported_tables() {
        assert!(decode_rgb_table("0").is_err());
        let mut raw = identity_record();
        raw[64..68].copy_from_slice(&2_u32.to_le_bytes());
        assert!(decode_rgb_table(&encode(&raw)).is_err());
        raw.truncate(20);
        assert!(decode_rgb_table(&encode(&raw)).is_err());
    }
    #[test]
    fn hidden_deltas_do_not_mutate_visible_settings() {
        let p = Profile {
            schema: 1,
            uuid: String::new(),
            name: String::new(),
            table: None,
            deltas: BTreeMap::from([("highlights".into(), -40.0), ("shadows".into(), 30.0)]),
        };
        let a = json!({"highlights":-40.0,"shadows":69.0});
        let out = with_deltas(&a, &p, 0.78, false);
        assert!((out["highlights"].as_f64().unwrap() + 71.2).abs() < 0.001);
        assert!((out["shadows"].as_f64().unwrap() - 92.4).abs() < 0.001);
        assert_eq!(a["highlights"], -40.0);
        assert_eq!(with_deltas(&a, &p, 0.0, false), a);
        let reference = with_deltas(&serde_json::json!({"shadows":46}), &p, 0.78, true);
        let expected =
            46.0 + p.deltas.get("shadows").copied().unwrap_or(0.0) / 1.5 * 0.78_f32 as f64;
        assert!((reference["shadows"].as_f64().unwrap() - expected).abs() < 0.00001);
    }

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("cjv-profile-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn table_limits_do_not_limit_hidden_adjustments_in_new_imports() {
        let dir = TestDirectory::new();
        let path = dir.0.join("profile.json");
        let mut table = decode_rgb_table(&encode(&identity_record())).unwrap();
        table.min_amount = 0.5;
        table.max_amount = 1.5;
        let profile = Profile {
            schema: 1,
            uuid: "ABCDEF0123456789ABCDEF0123456789".into(),
            name: "Own synthetic profile".into(),
            table: Some(table),
            deltas: BTreeMap::from([("exposure".into(), 0.4)]),
        };
        std::fs::write(&path, serde_json::to_vec(&profile).unwrap()).unwrap();
        let mut adj = json!({"uuid":profile.uuid,"path":path,"amountVersion":2,"tableMinAmount":0.5,"amount":0});
        let zero = load_adjustment(&adj).unwrap();
        assert_eq!(zero.table_amount, 0.5);
        assert_eq!(zero.delta_amount, 0.0);
        assert_eq!(
            with_deltas(
                &json!({"exposure":0.1}),
                &zero.profile,
                zero.delta_amount,
                true
            )["exposure"],
            0.1
        );
        adj["amount"] = json!(2);
        let high = load_adjustment(&adj).unwrap();
        assert_eq!(high.table_amount, 1.5);
        assert_eq!(high.delta_amount, 2.0);
        assert!(
            (with_deltas(
                &json!({"exposure":0.1}),
                &high.profile,
                high.delta_amount,
                true
            )["exposure"]
                .as_f64()
                .unwrap()
                - 0.9)
                .abs()
                < 1e-6
        );
        adj["enabled"] = json!(false);
        assert!(!is_requested(&adj));
        assert!(load_adjustment(&adj).is_none());
        adj["enabled"] = json!(true);
        adj.as_object_mut().unwrap().remove("amountVersion");
        let legacy = load_adjustment(&adj).unwrap();
        assert_eq!(legacy.table_amount, 1.5);
        assert_eq!(legacy.delta_amount, 1.5);
        adj["amount"] = json!(0);
        assert!(load_adjustment(&adj).is_none());
    }

    #[test]
    fn zero_with_a_required_table_must_not_silently_lose_a_missing_profile() {
        let mut adj = json!({"path":"not-a-profile-cache","uuid":"ABCDEF0123456789ABCDEF0123456789","amount":0,"amountVersion":2,"tableMinAmount":0.5});
        assert!(is_requested(&adj));
        assert!(load_adjustment(&adj).is_none());
        adj["tableMinAmount"] = json!(0);
        assert!(!is_requested(&adj));
        adj["amount"] = json!(1);
        assert!(is_requested(&adj));
        adj["enabled"] = json!(false);
        assert!(!is_requested(&adj));
        assert!(!is_requested(&Value::Null));
    }

    #[test]
    fn imported_profile_keeps_full_amount_range_and_records_table_minimum() {
        let dir = TestDirectory::new();
        let mut raw = identity_record();
        raw[76..84].copy_from_slice(&0.5_f64.to_le_bytes());
        raw[84..92].copy_from_slice(&1.5_f64.to_le_bytes());
        let id = "ABCDEF0123456789ABCDEF0123456789";
        let source = format!(
            r#"<rdf:Description crs:PresetType="Look" crs:UUID="{id}" crs:RGBTable="{id}" crs:Table_{id}="{}"></rdf:Description>"#,
            encode(&raw)
        );
        std::fs::write(dir.0.join("own.xmp"), source).unwrap();
        let reference = format!(
            r#"<crs:Look><rdf:Description crs:UUID="{id}" crs:Amount="0.25"></rdf:Description></crs:Look>"#
        );
        let imported = import_reference(&reference, &[dir.0.clone()], &dir.0.join("cache"))
            .unwrap()
            .unwrap();
        assert_eq!(imported["amount"], 0.25);
        assert_eq!(imported["maxAmount"], 2.0);
        assert_eq!(imported["tableMinAmount"], 0.5);
        assert_eq!(imported["amountVersion"], 2);
        let loaded = load_adjustment(&imported).unwrap();
        assert_eq!(loaded.table_amount, 0.5);
        assert_eq!(loaded.delta_amount, 0.25);
    }
}
