//! Reference tone controls calibrated with CJV-generated linear DNG gradients.
//! Highlights and shadows are sampled jointly because their responses interact.
//! This remains an approximation of image-adaptive processing, not Adobe code.
use serde_json::Value;
mod data {
    include!("reference_basic_data.rs");
}

fn sample(row: &[u16; 65], x: f64) -> f64 {
    let code = x.clamp(0.0, 1.0) * 255.0;
    let i = ((code / 4.0).floor() as usize).min(63);
    let width = if i == 63 { 3.0 } else { 4.0 };
    let f = (code - i as f64 * 4.0) / width;
    (row[i] as f64 * (1.0 - f) + row[i + 1] as f64 * f) / 510.0
}

fn inverse_base(value: f64) -> f64 {
    let code = value.clamp(0.0, 1.0) * 510.0;
    if code <= data::BASE[0] as f64 {
        return 0.0;
    }
    if code >= data::BASE[64] as f64 {
        return 1.0;
    }
    let i = (0..64)
        .find(|&i| data::BASE[i + 1] as f64 >= code)
        .unwrap_or(63);
    let f = (code - data::BASE[i] as f64) / (data::BASE[i + 1] - data::BASE[i]).max(1) as f64;
    (i as f64 * 4.0 + f * if i == 63 { 3.0 } else { 4.0 }) / 255.0
}

fn strength(value: f64) -> (usize, f64) {
    let p = value.clamp(-100.0, 100.0) / 25.0 + 4.0;
    let i = (p.floor() as usize).min(7);
    (i, p - i as f64)
}

fn single(control: usize, value: f64, x: f64) -> f64 {
    if value == 0.0 {
        return x;
    }
    let (i, f) = strength(value);
    inverse_base(
        sample(&data::SINGLE[control][i], x) * (1.0 - f)
            + sample(&data::SINGLE[control][i + 1], x) * f,
    )
}

fn joint(highlights: f64, shadows: f64, x: f64) -> f64 {
    if highlights == 0.0 && shadows == 0.0 {
        return x;
    }
    let (h, hf) = strength(highlights);
    let (s, sf) = strength(shadows);
    let a = sample(&data::HIGHLIGHTS_SHADOWS[h][s], x);
    let b = sample(&data::HIGHLIGHTS_SHADOWS[h + 1][s], x);
    let c = sample(&data::HIGHLIGHTS_SHADOWS[h][s + 1], x);
    let d = sample(&data::HIGHLIGHTS_SHADOWS[h + 1][s + 1], x);
    inverse_base(
        a * (1.0 - hf) * (1.0 - sf) + b * hf * (1.0 - sf) + c * (1.0 - hf) * sf + d * hf * sf,
    )
}

pub fn points(settings: &Value) -> Vec<Value> {
    let get = |key: &str| {
        settings[key]
            .as_f64()
            .filter(|x| x.is_finite())
            .unwrap_or(0.0)
    };
    let (contrast, highlights, shadows, whites, blacks) = (
        get("contrast"),
        get("highlights"),
        get("shadows"),
        get("whites"),
        get("blacks"),
    );
    if [contrast, highlights, shadows, whites, blacks]
        .iter()
        .all(|v| *v == 0.0)
    {
        return Vec::new();
    }
    let mut previous = 0.0_f64;
    let dense: Vec<_> = (0..256)
        .map(|i| {
            let x = i as f64 / 255.0;
            let y = single(1, whites, x);
            let y = joint(highlights, shadows, y);
            let y = single(2, blacks, y);
            let y = single(0, contrast, y).max(previous);
            previous = y;
            (x, y)
        })
        .collect();
    crate::imported_curve::compress_points(&dense)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_controls_do_not_change_existing_pixels() {
        assert!(points(&serde_json::json!({})).is_empty());
        assert!(points(&serde_json::json!({"highlights":0,"shadows":0})).is_empty());
    }
    #[test]
    fn extreme_combinations_are_finite_monotone_and_fit_gpu_capacity() {
        for h in [-100, -37, 0, 63, 100] {
            for s in [-100, -41, 0, 52, 100] {
                let pts = points(
                    &serde_json::json!({"highlights":h,"shadows":s,"whites":-38,"blacks":42,"contrast":13}),
                );
                assert!(pts.len() <= 16);
                for p in &pts {
                    assert!((0.0..=255.0).contains(&p["y"].as_f64().unwrap()));
                }
                for p in pts.windows(2) {
                    assert!(p[0]["x"].as_f64().unwrap() < p[1]["x"].as_f64().unwrap());
                    assert!(p[0]["y"].as_f64().unwrap() <= p[1]["y"].as_f64().unwrap());
                }
            }
        }
    }
}
