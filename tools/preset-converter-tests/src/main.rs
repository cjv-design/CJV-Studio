// Local diagnostic helper. Input photos and presets are never uploaded.
use cjv_preset_converter_tests::{preset_converter, sony_white_balance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: cjv-preset-converter-tests xmp|sony-wb FILE".into());
    }
    match args[1].as_str() {
        "xmp" => {
            let content = std::fs::read_to_string(&args[2])?;
            let preset = preset_converter::convert_xmp_to_preset(&content)?;
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
