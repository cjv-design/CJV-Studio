//! Optional, per-source DNG calibration. No camera constants or user profiles
//! are bundled. The source SHA-256 prevents one photograph's AnalogBalance or
//! AsShotNeutral from being reused for a different capture.
use serde_json::Value;
use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::OnceLock;

type Matrix = [[f64; 3]; 3];
static RECORDS: OnceLock<BTreeMap<String, String>> = OnceLock::new();

pub fn initialize(directory: PathBuf) {
    let mut records = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten().take(4096) {
            let path = entry.path();
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && name.len() == 64
                && name.bytes().all(|b| b.is_ascii_hexdigit())
                && entry
                    .metadata()
                    .is_ok_and(|m| m.is_file() && m.len() <= 32_768)
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                records.insert(name.to_string(), content);
            }
        }
    }
    // One immutable snapshot keeps WB, rendering and thumbnails consistent.
    let _ = RECORDS.set(records);
}

pub fn revision() -> u64 {
    let mut hash = DefaultHasher::new();
    "cjv-colour-0.1.6".hash(&mut hash);
    if let Some(records) = RECORDS.get() {
        records.hash(&mut hash);
    }
    hash.finish()
}

#[derive(Clone, Debug)]
pub struct CameraCalibration {
    pub input_kind: String,
    pub color_a: Vec<f32>,
    pub color_d65: Vec<f32>,
    pub neutral: [f32; 3],
    pub baseline_exposure: f32,
    calibration_a: Matrix,
    calibration_d65: Matrix,
    forward_a: Matrix,
    forward_d65: Matrix,
    analog_balance: [f64; 3],
    use_forward_matrix: bool,
}

fn vector<const N: usize>(value: &Value) -> Option<[f64; N]> {
    let values = value.as_array()?;
    if values.len() != N {
        return None;
    }
    let mut result = [0.0; N];
    for (dst, src) in result.iter_mut().zip(values) {
        *dst = src.as_f64().filter(|v| v.is_finite() && v.abs() < 100.0)?;
    }
    Some(result)
}

fn matrix(value: &Value) -> Option<Matrix> {
    let v = vector::<9>(value)?;
    let m = [[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]];
    inverse(m)?;
    Some(m)
}

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

pub fn transform(m: Matrix, v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| (0..3).map(|c| m[r][c] * v[c] as f64).sum::<f64>() as f32)
}

fn diagonal(v: [f64; 3]) -> Matrix {
    [[v[0], 0.0, 0.0], [0.0, v[1], 0.0], [0.0, 0.0, v[2]]]
}

fn inverse(m: Matrix) -> Option<Matrix> {
    let [[a, b, c], [d, e, f], [g, h, i]] = m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det.abs() < 1e-9 {
        return None;
    }
    Some([
        [
            (e * i - f * h) / det,
            (c * h - b * i) / det,
            (b * f - c * e) / det,
        ],
        [
            (f * g - d * i) / det,
            (a * i - c * g) / det,
            (c * d - a * f) / det,
        ],
        [
            (d * h - e * g) / det,
            (b * g - a * h) / det,
            (a * e - b * d) / det,
        ],
    ])
}

impl CameraCalibration {
    pub fn parse(value: &Value, hash: &str, model: &str) -> Option<Self> {
        if value["schema"] != 1
            || value["sourceSha256"] != hash
            || value["model"] != model
            || !matches!(
                value["inputKind"].as_str(),
                Some("sony-linear-arw" | "sony-mosaic-arw")
            )
            || value["enabled"] != true
        {
            return None;
        }
        let analog_balance = vector::<3>(&value["analogBalance"])?;
        let neutral = vector::<3>(&value["asShotNeutral"])?;
        if analog_balance.iter().chain(&neutral).any(|v| *v <= 0.0) {
            return None;
        }
        let calibration_a = matrix(&value["cameraCalibration1"])?;
        let calibration_d65 = matrix(&value["cameraCalibration2"])?;
        let ab = diagonal(analog_balance);
        let combined = |cc, cm| {
            multiply(multiply(ab, cc), cm)
                .into_iter()
                .flatten()
                .map(|v| v as f32)
                .collect()
        };
        let baseline = value["baselineExposure"].as_f64()?;
        if !baseline.is_finite() || !(-5.0..=5.0).contains(&baseline) {
            return None;
        }
        Some(Self {
            input_kind: value["inputKind"].as_str()?.to_string(),
            color_a: combined(calibration_a, matrix(&value["colorMatrix1"])?),
            color_d65: combined(calibration_d65, matrix(&value["colorMatrix2"])?),
            neutral: neutral.map(|v| v as f32),
            baseline_exposure: baseline as f32,
            calibration_a,
            calibration_d65,
            forward_a: matrix(&value["forwardMatrix1"])?,
            forward_d65: matrix(&value["forwardMatrix2"])?,
            analog_balance,
            use_forward_matrix: value["useForwardMatrix"].as_bool().unwrap_or(false),
        })
    }

    /// DNG 1.7.1 camera-to-XYZ transform with ForwardMatrix, followed by
    /// Bradford D50-to-D65 adaptation and the standard linear-sRGB matrix.
    pub fn rendering_matrix(&self, temperature: f64) -> Option<Matrix> {
        if !self.use_forward_matrix || !temperature.is_finite() || temperature <= 0.0 {
            return None;
        }
        let w =
            ((1.0 / temperature - 1.0 / 6504.0) / (1.0 / 2856.0 - 1.0 / 6504.0)).clamp(0.0, 1.0);
        let mix = |a: Matrix, b: Matrix| {
            std::array::from_fn(|r| std::array::from_fn(|c| a[r][c] * w + b[r][c] * (1.0 - w)))
        };
        let individual_to_reference = inverse(multiply(
            diagonal(self.analog_balance),
            mix(self.calibration_a, self.calibration_d65),
        ))?;
        let reference_neutral = transform(individual_to_reference, self.neutral);
        if reference_neutral
            .iter()
            .any(|v| !v.is_finite() || *v <= 1e-6)
        {
            return None;
        }
        let fm = mix(self.forward_a, self.forward_d65);
        let camera_to_d50 = multiply(
            multiply(fm, diagonal(reference_neutral.map(|v| 1.0 / v as f64))),
            individual_to_reference,
        );
        let d50_to_d65 = [
            [0.9555766, -0.0230393, 0.0631636],
            [-0.0282895, 1.0099416, 0.0210077],
            [0.0122982, -0.0204830, 1.3299098],
        ];
        let xyz_to_rgb = [
            [3.2404542, -1.5371385, -0.4985314],
            [-0.9692660, 1.8760108, 0.0415560],
            [0.0556434, -0.2040259, 1.0572252],
        ];
        Some(multiply(multiply(xyz_to_rgb, d50_to_d65), camera_to_d50))
    }
}

pub fn load(hash: &str, model: &str) -> Option<CameraCalibration> {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let json = RECORDS.get()?.get(hash)?;
    CameraCalibration::parse(&serde_json::from_str(&json).ok()?, hash, model)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        let identity = vec![1, 0, 0, 0, 1, 0, 0, 0, 1];
        serde_json::json!({"schema":1,"enabled":true,"sourceSha256":"test","model":"test",
            "inputKind":"sony-linear-arw", "analogBalance":[2,1,3],"asShotNeutral":[1,1,1],
            "cameraCalibration1":identity,"cameraCalibration2":identity,
            "colorMatrix1":identity,"colorMatrix2":identity,
            "forwardMatrix1":[0.96422,0,0,0,1,0,0,0,0.82521],
            "forwardMatrix2":[0.96422,0,0,0,1,0,0,0,0.82521],
            "baselineExposure":0.35,"useForwardMatrix":true})
    }
    #[test]
    fn rejects_another_capture_camera_or_invalid_data() {
        let mut v = fixture();
        assert!(CameraCalibration::parse(&v, "other", "test").is_none());
        assert!(CameraCalibration::parse(&v, "test", "other").is_none());
        v["cameraCalibration1"] = serde_json::json!(vec![0; 9]);
        assert!(CameraCalibration::parse(&v, "test", "test").is_none());
        v = fixture();
        v["analogBalance"][0] = serde_json::json!(0);
        assert!(CameraCalibration::parse(&v, "test", "test").is_none());
        v = fixture();
        v["baselineExposure"] = serde_json::json!(50);
        assert!(CameraCalibration::parse(&v, "test", "test").is_none());
    }
    #[test]
    fn forward_matrix_maps_neutral_to_neutral_without_double_wb() {
        let p = CameraCalibration::parse(&fixture(), "test", "test").unwrap();
        for temperature in [2000.0, 2856.0, 5000.0, 6504.0, 15000.0] {
            let rgb = transform(p.rendering_matrix(temperature).unwrap(), [1.0; 3]);
            assert!(rgb.iter().all(|v| (*v - 1.0).abs() < 0.001), "{rgb:?}");
        }
    }
    #[test]
    fn analog_balance_is_included_in_wb_matrices() {
        let p = CameraCalibration::parse(&fixture(), "test", "test").unwrap();
        assert_eq!(p.color_a, vec![2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 3.0]);
        let mut v = fixture();
        v["useForwardMatrix"] = serde_json::json!(false);
        assert!(
            CameraCalibration::parse(&v, "test", "test")
                .unwrap()
                .rendering_matrix(5000.0)
                .is_none()
        );
    }

    #[test]
    fn mosaic_neutral_is_applied_once_and_layout_is_preserved() {
        let mut v = fixture();
        v["inputKind"] = serde_json::json!("sony-mosaic-arw");
        v["asShotNeutral"] = serde_json::json!([0.45, 1.0, 0.70]);
        let p = CameraCalibration::parse(&v, "test", "test").unwrap();
        assert_eq!(p.input_kind, "sony-mosaic-arw");
        let rgb = transform(p.rendering_matrix(5000.0).unwrap(), p.neutral);
        assert!(rgb.iter().all(|v| (*v - 1.0).abs() < 0.001), "{rgb:?}");
        v["inputKind"] = serde_json::json!("unknown");
        assert!(CameraCalibration::parse(&v, "test", "test").is_none());
    }
}
