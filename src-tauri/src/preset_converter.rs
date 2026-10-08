use regex::Regex;
use regex::regex;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use uuid::Uuid;

use crate::file_management::Preset;

#[derive(Copy, Clone, Debug)]
enum Num {
    I(i64),
    F(f64),
}

fn parse_num(s: &str) -> Option<Num> {
    if let Ok(i) = s.parse::<i64>() {
        Some(Num::I(i))
    } else if let Ok(f) = s.parse::<f64>() {
        Some(Num::F(f))
    } else {
        None
    }
}

fn num_to_json(num: Num) -> Option<Value> {
    match num {
        Num::I(i) => Some(Value::Number(i.into())),
        Num::F(f) => serde_json::Number::from_f64(f).map(Value::Number),
    }
}

fn get_attr_as_f64(attrs: &HashMap<String, String>, key: &str) -> Option<f64> {
    attrs
        .get(key)
        .and_then(|s| s.trim_start_matches('+').parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

fn extract_xmp_name(xmp_content: &str) -> Option<String> {
    regex!(r#"(?s)<crs:Name>.*?<rdf:Alt>.*?<rdf:li[^>]*>([^<]+)</rdf:li>.*?</crs:Name>"#)
        .captures(xmp_content)
        .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
}

fn extract_tone_curve_points(xmp_str: &str, curve_name: &str) -> Option<Vec<Value>> {
    let pattern = format!(
        r"(?s)<crs:{}>\s*<rdf:Seq>(.*?)</rdf:Seq>\s*</crs:{}>",
        curve_name, curve_name
    );
    let re = Regex::new(&pattern).ok()?;
    let captures = re.captures(xmp_str)?;
    let seq_content = captures.get(1)?.as_str();

    let point_re = regex!(r"<rdf:li>(\d+),\s*(\d+)</rdf:li>");
    let mut points = Vec::new();

    for point_cap in point_re.captures_iter(seq_content) {
        let x: u32 = point_cap.get(1)?.as_str().parse().ok()?;
        let y: u32 = point_cap.get(2)?.as_str().parse().ok()?;

        let mut point = Map::new();
        point.insert("x".to_string(), Value::Number(x.into()));
        point.insert("y".to_string(), Value::Number(y.into()));
        points.push(Value::Object(point));
    }

    if points.is_empty() {
        None
    } else {
        Some(points)
    }
}

pub fn convert_xmp_to_preset(xmp_content: &str) -> Result<Preset, String> {
    // Only the preset's outer settings belong here. Nested Look/Parameters
    // descriptions can contain their own exposure, WB, and tonal settings.
    let outer = regex!(r"(?s)<rdf:Description\b([^>]*)>")
        .captures(xmp_content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .ok_or("XMP has no settings description")?;
    let attr_re = regex!(r#"crs:([A-Za-z0-9]+)="([^"]*)""#);
    let mut attrs: HashMap<String, String> = HashMap::new();
    for cap in attr_re.captures_iter(outer) {
        attrs.insert(cap[1].to_string(), cap[2].to_string());
    }

    let mut adjustments = Map::new();
    let mut hsl_map = Map::new();
    let mut color_grading_map = Map::new();
    let mut curves_map = Map::new();

    let mappings = vec![
        ("Exposure2012", "exposure"),
        ("Contrast2012", "contrast"),
        ("Highlights2012", "highlights"),
        ("Whites2012", "whites"),
        ("Blacks2012", "blacks"),
        ("Clarity2012", "clarity"),
        ("Dehaze", "dehaze"),
        ("Vibrance", "vibrance"),
        ("Saturation", "saturation"),
        ("Texture", "structure"),
        ("SharpenRadius", "sharpenRadius"),
        ("SharpenDetail", "sharpenDetail"),
        ("SharpenEdgeMasking", "sharpenMasking"),
        ("LuminanceSmoothing", "lumaNoiseReduction"),
        ("ColorNoiseReduction", "colorNoiseReduction"),
        ("ColorNoiseReductionDetail", "colorNoiseDetail"),
        ("ColorNoiseReductionSmoothness", "colorNoiseSmoothness"),
        ("ChromaticAberrationRedCyan", "chromaticAberrationRedCyan"),
        (
            "ChromaticAberrationBlueYellow",
            "chromaticAberrationBlueYellow",
        ),
        ("PostCropVignetteAmount", "vignetteAmount"),
        ("PostCropVignetteMidpoint", "vignetteMidpoint"),
        ("PostCropVignetteFeather", "vignetteFeather"),
        ("PostCropVignetteRoundness", "vignetteRoundness"),
        ("GrainAmount", "grainAmount"),
        ("GrainSize", "grainSize"),
        ("GrainFrequency", "grainRoughness"),
        ("ColorGradeBlending", "blending"),
    ];

    for (xmp_key, rr_key) in mappings {
        if let Some(raw_val) = attrs.get(xmp_key)
            && let Some(num) = parse_num(raw_val.trim_start_matches('+'))
            && let Some(json_val) = num_to_json(num)
        {
            if rr_key == "blending" {
                color_grading_map.insert(rr_key.to_string(), json_val);
            } else {
                adjustments.insert(rr_key.to_string(), json_val);
            }
        }
    }

    // Preserve only the Calibration fields included in the Adobe preset.
    // The renderer consumes the same UI units, but its colour maths differs
    // from Adobe's; this mapping does not promise identical rendered colours.
    let mut calibration_map = Map::new();
    for (xmp_key, rr_key) in [
        ("ShadowTint", "shadowsTint"),
        ("RedHue", "redHue"),
        ("RedSaturation", "redSaturation"),
        ("GreenHue", "greenHue"),
        ("GreenSaturation", "greenSaturation"),
        ("BlueHue", "blueHue"),
        ("BlueSaturation", "blueSaturation"),
    ] {
        if let Some(raw_val) = attrs.get(xmp_key)
            && let Some(num) = parse_num(raw_val.trim_start_matches('+'))
            && let Some(json_val) = num_to_json(num)
        {
            calibration_map.insert(rr_key.to_string(), json_val);
        }
    }
    if !calibration_map.is_empty() {
        adjustments.insert(
            "colorCalibration".to_string(),
            Value::Object(calibration_map),
        );
    }

    if let Some(shadows_val) = get_attr_as_f64(&attrs, "Shadows2012") {
        // Reference controls use the actual -100..100 Adobe range. Applying
        // the legacy native 1.5 scale would clip the upper third of the range.
        let scale = if attrs.contains_key("ProcessVersion") {
            1.0
        } else {
            1.5
        };
        let adjusted_shadows = (shadows_val * scale).clamp(-100.0, 100.0);
        adjustments.insert("shadows".to_string(), json!(adjusted_shadows));
    }

    if let Some(sharpness_val) = get_attr_as_f64(&attrs, "Sharpness") {
        let scaled_sharpness = (sharpness_val / 150.0) * 100.0;
        adjustments.insert(
            "sharpness".to_string(),
            json!(scaled_sharpness.clamp(0.0, 100.0)),
        );
    }

    let wb_mode = attrs.get("WhiteBalance").map(String::as_str);
    if wb_mode == Some("As Shot") {
        adjustments.insert("whiteBalance".to_string(), Value::Null);
        adjustments.insert("temperature".to_string(), json!(0));
        adjustments.insert("tint".to_string(), json!(0));
    } else if wb_mode != Some("Auto") {
        // Adobe Temperature/Tint are absolute values, not offsets from the
        // reference photo saved in the preset. Native rendering supports Kelvin.
        let mut wb = Map::new();
        if let Some(value) = get_attr_as_f64(&attrs, "Temperature")
            && (2000.0..=50000.0).contains(&value)
        {
            wb.insert("temperature".to_string(), json!(value));
        }
        if let Some(value) = get_attr_as_f64(&attrs, "Tint")
            && (-150.0..=150.0).contains(&value)
        {
            wb.insert("tint".to_string(), json!(value));
        }
        if !wb.is_empty() {
            adjustments.insert("whiteBalance".to_string(), Value::Object(wb));
            adjustments.insert("temperature".to_string(), json!(0));
            adjustments.insert("tint".to_string(), json!(0));
        }
    }

    let colors = [
        ("Red", "reds"),
        ("Orange", "oranges"),
        ("Yellow", "yellows"),
        ("Green", "greens"),
        ("Aqua", "aquas"),
        ("Blue", "blues"),
        ("Purple", "purples"),
        ("Magenta", "magentas"),
    ];
    for (src, dst) in colors {
        let mut color_map = Map::new();
        if let Some(raw) = attrs.get(&format!("HueAdjustment{}", src))
            && let Some(num) = parse_num(raw.trim_start_matches('+'))
            && let Some(Value::Number(n)) = num_to_json(num)
            && let Some(val_f64) = n.as_f64()
        {
            let adjusted_hue = val_f64 * 0.75;
            color_map.insert("hue".to_string(), json!(adjusted_hue));
        }
        if let Some(raw) = attrs.get(&format!("SaturationAdjustment{}", src))
            && let Some(num) = parse_num(raw.trim_start_matches('+'))
            && let Some(json_val) = num_to_json(num)
        {
            color_map.insert("saturation".to_string(), json_val);
        }
        if let Some(raw) = attrs.get(&format!("LuminanceAdjustment{}", src))
            && let Some(num) = parse_num(raw.trim_start_matches('+'))
            && let Some(json_val) = num_to_json(num)
        {
            color_map.insert("luminance".to_string(), json_val);
        }
        if !color_map.is_empty() {
            hsl_map.insert(dst.to_string(), Value::Object(color_map));
        }
    }
    if !hsl_map.is_empty() {
        adjustments.insert("hsl".to_string(), Value::Object(hsl_map));
    }

    let mut shadows_map = Map::new();
    let mut midtones_map = Map::new();
    let mut highlights_map = Map::new();
    let mut global_map = Map::new();
    if let Some(raw) = attrs.get("SplitToningShadowHue")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        shadows_map.insert("hue".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeMidtoneHue")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        midtones_map.insert("hue".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("SplitToningHighlightHue")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        highlights_map.insert("hue".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("SplitToningShadowSaturation")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        shadows_map.insert("saturation".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeMidtoneSat")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        midtones_map.insert("saturation".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("SplitToningHighlightSaturation")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        highlights_map.insert("saturation".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeShadowLum")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        shadows_map.insert("luminance".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeMidtoneLum")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        midtones_map.insert("luminance".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeHighlightLum")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        highlights_map.insert("luminance".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeGlobalHue")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        global_map.insert("hue".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeGlobalSat")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        global_map.insert("saturation".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeGlobalLum")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        global_map.insert("luminance".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("SplitToningBalance")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        color_grading_map.insert("balance".to_string(), json_val);
    }
    if !shadows_map.is_empty() {
        color_grading_map.insert("shadows".to_string(), Value::Object(shadows_map));
    }
    if !midtones_map.is_empty() {
        color_grading_map.insert("midtones".to_string(), Value::Object(midtones_map));
    }
    if !highlights_map.is_empty() {
        color_grading_map.insert("highlights".to_string(), Value::Object(highlights_map));
    }
    if !global_map.is_empty() {
        color_grading_map.insert("global".to_string(), Value::Object(global_map));
    }
    if !color_grading_map.is_empty() {
        adjustments.insert("colorGrading".to_string(), Value::Object(color_grading_map));
    }

    let curve_mappings = [
        ("ToneCurvePV2012", "luma"),
        ("ToneCurvePV2012Red", "red"),
        ("ToneCurvePV2012Green", "green"),
        ("ToneCurvePV2012Blue", "blue"),
    ];
    let preset_content = regex!(r"(?s)<crs:Look\b.*?</crs:Look>").replace_all(xmp_content, "");
    for (xmp_curve, rr_curve) in curve_mappings {
        if let Some(points) = extract_tone_curve_points(&preset_content, xmp_curve) {
            curves_map.insert(rr_curve.to_string(), Value::Array(points));
        }
    }
    if !curves_map.is_empty() {
        adjustments.insert("pointCurves".to_string(), Value::Object(curves_map.clone()));
        adjustments.insert("curveMode".to_string(), json!("point"));
        adjustments.insert("curves".to_string(), Value::Object(curves_map));
    }

    let preset_name =
        extract_xmp_name(&preset_content).unwrap_or_else(|| "Imported Preset".to_string());

    let mut import_notes = Vec::new();
    if attrs.contains_key("ProcessVersion") {
        adjustments.insert("toneMapper".into(), json!("reference"));
        import_notes.push("Reference tone rendering approximates Adobe's RAW tone response. Camera colour profiles and local adjustments may still differ.");
    }
    if xmp_content.contains("<crs:Look") || attrs.contains_key("RGBTable") {
        import_notes
            .push("The Adobe film profile is not applied. Colours will differ from Lightroom.");
    }
    if wb_mode == Some("Auto") {
        import_notes
            .push("Adobe Auto white balance is not supported; existing white balance is kept.");
    }
    let mut parametric = Map::new();
    for (src, dst, default, min, max) in [
        ("ParametricShadows", "shadows", 0.0, -100.0, 100.0),
        ("ParametricDarks", "darks", 0.0, -100.0, 100.0),
        ("ParametricLights", "lights", 0.0, -100.0, 100.0),
        ("ParametricHighlights", "highlights", 0.0, -100.0, 100.0),
        ("ParametricShadowSplit", "split1", 25.0, 1.0, 97.0),
        ("ParametricMidtoneSplit", "split2", 50.0, 2.0, 98.0),
        ("ParametricHighlightSplit", "split3", 75.0, 3.0, 99.0),
    ] {
        parametric.insert(
            dst.into(),
            json!(
                get_attr_as_f64(&attrs, src)
                    .unwrap_or(default)
                    .clamp(min, max)
            ),
        );
    }
    if [
        "ParametricShadows",
        "ParametricDarks",
        "ParametricLights",
        "ParametricHighlights",
    ]
    .iter()
    .any(|key| attrs.contains_key(*key))
    {
        parametric.insert("amount".into(), json!(1.0));
        parametric.insert("version".into(), json!(2));
        adjustments.insert("xmpParametricCurve".into(), Value::Object(parametric));
        import_notes.push("The imported tone curve uses a measured approximation. Its strength is adjustable under Curves.");
    }
    if !import_notes.is_empty() {
        adjustments.insert("xmpImportNotes".to_string(), json!(import_notes));
    }

    Ok(Preset {
        id: Uuid::new_v4().to_string(),
        name: preset_name,
        adjustments: Value::Object(adjustments),
        include_masks: Some(false),
        include_crop_transform: Some(false),
        preset_type: Some("style".to_string()),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn reference_shadows_preserve_the_complete_lightroom_range() {
        for value in [-100, -80, 46, 80, 100] {
            let xmp = format!(
                r#"<rdf:Description crs:ProcessVersion="15.4" crs:Shadows2012="{value}"/>"#
            );
            let result = super::convert_xmp_to_preset(&xmp).unwrap();
            assert_eq!(
                result.adjustments["shadows"],
                serde_json::json!(value as f64)
            );
            assert_eq!(result.adjustments["toneMapper"], "reference");
        }
    }
    use super::*;

    #[test]
    fn point_curve_black_lift_and_parametric_curve_are_both_preserved() {
        let preset = convert_xmp_to_preset(r#"<rdf:Description crs:ParametricShadows="-65" crs:ParametricDarks="5" crs:ParametricHighlights="-40" crs:ParametricShadowSplit="20" crs:ParametricHighlightSplit="86"><crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 13</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012></rdf:Description>"#).unwrap().adjustments;
        assert_eq!(preset["curves"]["luma"][0], json!({"x":0,"y":13}));
        assert_eq!(preset["xmpParametricCurve"]["shadows"], json!(-65.0));
        assert_eq!(preset["xmpParametricCurve"]["split3"], json!(86.0));
    }

    #[test]
    fn profile_curve_is_not_mistaken_for_the_presets_point_curve() {
        let p = convert_xmp_to_preset(r#"<rdf:Description><crs:Look><rdf:Description><crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 30</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012></rdf:Description></crs:Look></rdf:Description>"#).unwrap();
        assert!(p.adjustments.get("curves").is_none());
    }

    #[test]
    fn white_balance_is_absolute_and_independent_of_preset_reference_photo() {
        for reference in [
            "",
            r#"crs:AsShotTemperature="5000" crs:AsShotTint="19""#,
            r#"crs:AsShotTemperature="8500" crs:AsShotTint="-20""#,
        ] {
            let xmp = format!(
                r#"<rdf:Description crs:WhiteBalance="Custom" crs:Temperature="4863" crs:Tint="19" {reference}/>"#
            );
            let result = convert_xmp_to_preset(&xmp).unwrap().adjustments;
            assert_eq!(
                result["whiteBalance"],
                json!({"temperature":4863.0,"tint":19.0})
            );
            assert_eq!(result["temperature"], 0);
            assert_eq!(result["tint"], 0);
        }
    }

    #[test]
    fn as_shot_resets_wb_but_auto_does_not_invent_an_algorithm() {
        let shot = convert_xmp_to_preset(
            r#"<rdf:Description crs:WhiteBalance="As Shot" crs:Temperature="6000" crs:Tint="20"/>"#,
        )
        .unwrap()
        .adjustments;
        assert_eq!(shot, json!({"whiteBalance":null,"temperature":0,"tint":0}));
        let auto = convert_xmp_to_preset(
            r#"<rdf:Description crs:WhiteBalance="Auto" crs:Temperature="6000" crs:Tint="20"/>"#,
        )
        .unwrap()
        .adjustments;
        assert!(auto.get("whiteBalance").is_none());
    }

    #[test]
    fn partial_wb_and_invalid_values_do_not_invent_reference_values() {
        let partial = convert_xmp_to_preset(r#"<rdf:Description crs:Tint="-12"/>"#)
            .unwrap()
            .adjustments;
        assert_eq!(partial["whiteBalance"], json!({"tint":-12.0}));
        for invalid in ["NaN", "inf", "0", "-100", "60000", "bad"] {
            let xmp =
                format!(r#"<rdf:Description crs:Temperature="{invalid}" crs:Tint="Infinity"/>"#);
            assert_eq!(convert_xmp_to_preset(&xmp).unwrap().adjustments, json!({}));
        }
    }

    #[test]
    fn nested_profile_settings_cannot_overwrite_outer_preset_settings() {
        let result = convert_xmp_to_preset(r#"<rdf:Description crs:Exposure2012="0.13" crs:Temperature="4863" crs:Tint="19"><crs:Look><rdf:Description crs:Exposure2012="2" crs:Temperature="8500" crs:Tint="-50"/></crs:Look></rdf:Description>"#).unwrap().adjustments;
        assert_eq!(result["exposure"], json!(0.13));
        assert_eq!(result["xmpImportNotes"].as_array().unwrap().len(), 1);
        assert_eq!(
            result["whiteBalance"],
            json!({"temperature":4863.0,"tint":19.0})
        );
    }

    #[test]
    fn imports_all_camera_calibration_fields_in_ui_units() {
        let preset = convert_xmp_to_preset(
            r#"<rdf:Description crs:ShadowTint="-12" crs:RedHue="+24"
                crs:RedSaturation="-35" crs:GreenHue="+46"
                crs:GreenSaturation="-57" crs:BlueHue="+68"
                crs:BlueSaturation="-79"/>"#,
        )
        .unwrap();

        assert_eq!(
            preset.adjustments["colorCalibration"],
            json!({
                "shadowsTint": -12,
                "redHue": 24,
                "redSaturation": -35,
                "greenHue": 46,
                "greenSaturation": -57,
                "blueHue": 68,
                "blueSaturation": -79,
            })
        );
    }

    #[test]
    fn partial_calibration_does_not_reset_omitted_controls() {
        let preset = convert_xmp_to_preset(
            r#"<rdf:Description crs:Exposure2012="+1.25" crs:BlueHue="-22.5"/>"#,
        )
        .unwrap();

        assert_eq!(preset.adjustments["exposure"], json!(1.25));
        assert_eq!(
            preset.adjustments["colorCalibration"],
            json!({ "blueHue": -22.5 })
        );
    }

    #[test]
    fn preset_without_calibration_keeps_existing_import_shape() {
        let preset = convert_xmp_to_preset(r#"<rdf:Description crs:Contrast2012="+8"/>"#).unwrap();

        assert_eq!(preset.adjustments, json!({ "contrast": 8 }));
    }

    #[test]
    fn explicit_zero_calibration_values_are_preserved() {
        let preset = convert_xmp_to_preset(
            r#"<rdf:Description crs:ShadowTint="0" crs:RedHue="+0" crs:BlueSaturation="0"/>"#,
        )
        .unwrap();

        assert_eq!(
            preset.adjustments["colorCalibration"],
            json!({ "shadowsTint": 0, "redHue": 0, "blueSaturation": 0 })
        );
    }

    #[test]
    fn ignores_malformed_and_non_finite_calibration_values() {
        let preset = convert_xmp_to_preset(
            r#"<rdf:Description crs:ShadowTint="NaN" crs:RedHue="Infinity"
                crs:RedSaturation="invalid" crs:GreenHue="-inf"
                crs:GreenSaturation="" crs:BlueHue="-100"/>"#,
        )
        .unwrap();

        assert_eq!(
            preset.adjustments["colorCalibration"],
            json!({ "blueHue": -100 })
        );
        let invalid_only = convert_xmp_to_preset(r#"<rdf:Description crs:RedHue="NaN"/>"#).unwrap();
        assert!(invalid_only.adjustments.get("colorCalibration").is_none());
    }

    #[test]
    fn calibration_does_not_replace_hsl_or_colour_grading() {
        let preset = convert_xmp_to_preset(
            r#"<rdf:Description crs:RedHue="+32" crs:HueAdjustmentRed="+12"
                crs:SplitToningShadowHue="240" crs:SplitToningShadowSaturation="10"/>"#,
        )
        .unwrap();

        assert_eq!(
            preset.adjustments["colorCalibration"],
            json!({ "redHue": 32 })
        );
        assert_eq!(preset.adjustments["hsl"]["reds"]["hue"], json!(9.0));
        assert_eq!(
            preset.adjustments["colorGrading"]["shadows"],
            json!({ "hue": 240, "saturation": 10 })
        );
    }
}
