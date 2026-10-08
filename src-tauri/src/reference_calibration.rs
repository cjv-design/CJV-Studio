//! Experimental, neutral-preserving primary calibration for Reference RAW mode.
//! Matrices are measurements of CJV-generated charts, not camera profiles.
use serde_json::Value;
mod data {
    include!("reference_calibration_data.rs");
}

pub fn matrix(settings: &Value) -> [[f32; 3]; 3] {
    const LEVELS: [f64; 6] = [-100.0, -50.0, 0.0, 10.0, 50.0, 100.0];
    const KEYS: [&str; 6] = [
        "redHue",
        "redSaturation",
        "greenHue",
        "greenSaturation",
        "blueHue",
        "blueSaturation",
    ];
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut result = identity;
    for (control, key) in KEYS.iter().enumerate() {
        let value = settings[*key]
            .as_f64()
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
            .clamp(-100.0, 100.0);
        if value == 0.0 {
            continue;
        }
        let i = (0..5).find(|&i| value <= LEVELS[i + 1]).unwrap_or(4);
        let fraction = ((value - LEVELS[i]) / (LEVELS[i + 1] - LEVELS[i])) as f32;
        for row in 0..3 {
            for col in 0..2 {
                let lo = data::RESPONSES[control][i][row][col];
                let hi = data::RESPONSES[control][i + 1][row][col];
                result[row][col] += lo + (hi - lo) * fraction - identity[row][col];
            }
        }
    }
    // Exact row normalisation preserves neutral greys even with combined controls.
    for row in &mut result {
        row[2] = 1.0 - row[0] - row[1];
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_or_invalid_controls_are_identity() {
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert_eq!(matrix(&json!({})), identity);
        assert_eq!(
            matrix(&json!({"blueHue": "invalid", "redSaturation": null})),
            identity
        );
    }

    #[test]
    fn every_primary_and_combined_extremes_preserve_greys() {
        for value in [-100.0, -75.0, -15.0, 5.0, 10.0, 33.0, 100.0] {
            for setting in [
                json!({"blueHue":value}),
                json!({"redHue":value,"redSaturation":value,"greenHue":value,"greenSaturation":value,"blueHue":value,"blueSaturation":value}),
            ] {
                for row in matrix(&setting) {
                    assert!(row.iter().all(|v| v.is_finite()));
                    assert!((row.iter().sum::<f32>() - 1.0).abs() < 0.000001);
                }
            }
        }
    }

    #[test]
    fn values_are_bounded_and_small_changes_are_continuous() {
        assert_eq!(
            matrix(&json!({"blueHue": 1e100})),
            matrix(&json!({"blueHue":100}))
        );
        let a = matrix(&json!({"greenHue": -0.001}));
        let b = matrix(&json!({"greenHue": 0.001}));
        for r in 0..3 {
            for c in 0..3 {
                assert!((a[r][c] - b[r][c]).abs() < 0.0001);
            }
        }
    }
}
