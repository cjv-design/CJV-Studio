// Local diagnostic helper. Input photos and presets are never uploaded.
use cjv_preset_converter_tests::{
    enhanced_profile, imported_curve, preset_converter, sony_white_balance,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: cjv-preset-converter-tests xmp|xmp-profile|sony-wb|curve FILE".into());
    }
    match args[1].as_str() {
        "curve" => {
            let settings = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
            println!(
                "{}",
                serde_json::to_string(&imported_curve::points(&settings))?
            );
        }
        "xmp" | "xmp-profile" => {
            let content = std::fs::read_to_string(&args[2])?;
            let mut preset = preset_converter::convert_xmp_to_preset(&content)?;
            if args[1] == "xmp-profile" {
                let roaming =
                    std::path::PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA missing")?);
                let roots = [roaming.join("Adobe/CameraRaw/Settings")];
                let cache =
                    roaming.join("au.com.cameronjonesvisuals.cjvstudio.alpha/imported-profiles");
                if let Some(profile) = enhanced_profile::import_reference(&content, &roots, &cache)?
                {
                    preset.adjustments["xmpProfile"] = profile;
                    if let Some(notes) = preset.adjustments["xmpImportNotes"].as_array_mut() {
                        notes.retain(|note| {
                            !note.as_str().is_some_and(|s| {
                                s.starts_with("The Adobe film profile is not applied.")
                            })
                        });
                        notes.push(serde_json::json!("Film profile imported from this computer. Camera colour and tone rendering still differ from Lightroom."));
                    }
                }
            }
            println!("{}", serde_json::to_string_pretty(&preset.adjustments)?);
        }
        "sony-wb" => {
            let content = std::fs::read(&args[2])?;
            let coeffs =
                sony_white_balance::read_coefficients(&content).ok_or("No valid Sony WB")?;
            println!("{}", serde_json::json!(&coeffs[..3]));
        }
        _ => return Err("Unknown operation".into()),
    }
    Ok(())
}
