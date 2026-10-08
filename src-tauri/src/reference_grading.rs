//! Experimental luminance responses measured with CJV-generated linear DNGs.
//! Version-gated so existing saved edits retain their rendering.
use serde_json::Value;
mod data {
    include!("reference_grading_data.rs");
}

fn sample(row: &[u16; 65], x: f64) -> f64 {
    let code = x.clamp(0.0, 1.0) * 255.0;
    let i = ((code / 4.0).floor() as usize).min(63);
    let f = (code - i as f64 * 4.0) / if i == 63 { 3.0 } else { 4.0 };
    (row[i] as f64 * (1.0 - f) + row[i + 1] as f64 * f) / 510.0
}
fn inverse_base(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0) * 510.0;
    if v <= data::BASE[0] as f64 {
        return 0.0;
    }
    if v >= data::BASE[64] as f64 {
        return 1.0;
    }
    let i = (0..64)
        .find(|&i| data::BASE[i + 1] as f64 >= v)
        .unwrap_or(63);
    let f = (v - data::BASE[i] as f64) / (data::BASE[i + 1] - data::BASE[i]).max(1) as f64;
    (i as f64 * 4.0 + f * if i == 63 { 3.0 } else { 4.0 }) / 255.0
}
fn bracket(value: f64, low: f64, step: f64, count: usize) -> (usize, f64) {
    let p = ((value - low) / step).clamp(0.0, (count - 1) as f64);
    let i = (p.floor() as usize).min(count - 2);
    (i, p - i as f64)
}
fn response(group: usize, value: f64, blend: f64, balance: f64, x: f64) -> f64 {
    if value == 0.0 {
        return x;
    }
    let (s, sf) = bracket(value, -100.0, 25.0, 9);
    if group == 3 {
        return inverse_base(
            sample(&data::GLOBAL[s], x) * (1.0 - sf) + sample(&data::GLOBAL[s + 1], x) * sf,
        );
    }
    let (b, bf) = bracket(blend, 0.0, 50.0, 3);
    let (p, pf) = bracket(balance, -100.0, 50.0, 5);
    let mut result = 0.0;
    for bi in 0..2 {
        for pi in 0..2 {
            for si in 0..2 {
                result += sample(&data::REGIONS[group][b + bi][p + pi][s + si], x)
                    * [1.0 - bf, bf][bi]
                    * [1.0 - pf, pf][pi]
                    * [1.0 - sf, sf][si];
            }
        }
    }
    inverse_base(result)
}
pub fn points_in_space(settings: &Value, display: bool) -> Vec<Value> {
    let finite = |v: &Value, default: f64| v.as_f64().filter(|v| v.is_finite()).unwrap_or(default);
    let values = ["shadows", "midtones", "highlights", "global"]
        .map(|key| finite(&settings[key]["luminance"], 0.0));
    if values.iter().all(|v| *v == 0.0) {
        return Vec::new();
    }
    let blend = finite(&settings["blending"], 50.0);
    let balance = finite(&settings["balance"], 0.0);
    let mut previous = 0.0_f64;
    let mut dense: Vec<_> = (0..256)
        .map(|i| {
            let x = i as f64 / 255.0;
            let regional = x
                + (0..3)
                    .map(|g| response(g, values[g], blend, balance, x) - x)
                    .sum::<f64>();
            let y = response(3, values[3], blend, balance, regional.clamp(0.0, 1.0))
                .clamp(0.0, 1.0)
                .max(previous);
            previous = y;
            if display {
                (sample(&data::BASE, x), sample(&data::BASE, y))
            } else {
                (x, y)
            }
        })
        .collect();
    dense.dedup_by(|a, b| a.0 == b.0);
    crate::imported_curve::compress_points(&dense)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_and_absent_luminance_do_not_create_a_curve() {
        assert!(points_in_space(&serde_json::json!({}), true).is_empty());
        assert!(
            points_in_space(
                &serde_json::json!({"midtones":{"hue":49,"saturation":14,"luminance":0}}),
                true
            )
            .is_empty()
        );
    }
    #[test]
    fn combined_extremes_are_finite_monotone_and_fit_the_gpu() {
        for balance in [-100, -43, 0, 67, 100] {
            for blend in [0, 37, 50, 82, 100] {
                for lum in [-100, -59, 0, 85, 100] {
                    for display in [false, true] {
                        let points = points_in_space(
                            &serde_json::json!({"blending":blend,"balance":balance,"shadows":{"luminance":lum},"midtones":{"luminance":-lum},"highlights":{"luminance":lum},"global":{"luminance":37}}),
                            display,
                        );
                        assert!((2..=16).contains(&points.len()));
                        for p in &points {
                            for key in ["x", "y"] {
                                let value = p[key].as_f64().unwrap();
                                assert!(value.is_finite() && (0.0..=255.0).contains(&value));
                            }
                        }
                        for pair in points.windows(2) {
                            assert!(
                                pair[0]["x"].as_f64().unwrap() < pair[1]["x"].as_f64().unwrap()
                            );
                            assert!(
                                pair[0]["y"].as_f64().unwrap() <= pair[1]["y"].as_f64().unwrap()
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn untrained_blend_balance_combination_matches_reference_samples() {
        let points = points_in_space(
            &serde_json::json!({"blending":28,"balance":45,"shadows":{"luminance":-59},"highlights":{"luminance":85}}),
            true,
        );
        // Measured on a separate own DNG, excluded from the calibration grid.
        for (x, expected) in [
            (28.0, 20.0),
            (77.0, 78.0),
            (180.0, 217.0),
            (234.0, 251.0),
            (248.0, 254.0),
        ] {
            let i = points
                .windows(2)
                .position(|p| p[1]["x"].as_f64().unwrap() >= x)
                .unwrap();
            let a = &points[i];
            let b = &points[i + 1];
            let f = (x - a["x"].as_f64().unwrap())
                / (b["x"].as_f64().unwrap() - a["x"].as_f64().unwrap());
            let result = a["y"].as_f64().unwrap() * (1.0 - f) + b["y"].as_f64().unwrap() * f;
            assert!(
                (result - expected).abs() < 4.0,
                "input {x}: got {result}, expected {expected}"
            );
        }
    }
}
