//! Imported parametric tone curves. Version 2 uses measured responses from our
//! synthetic gradients. Earlier edits retain their original native response.
use serde_json::{Value, json};

mod measured {
    include!("parametric_response.rs");
}

pub fn points(settings: &Value) -> Vec<Value> {
    if settings["version"].as_u64().unwrap_or(1) >= 2 {
        return measured_points(settings);
    }
    let get = |key: &str, default: f64| {
        settings[key]
            .as_f64()
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    };
    let amount = get("amount", 1.0).clamp(0.0, 2.0);
    let s1 = get("split1", 25.0).clamp(1.0, 97.0) / 100.0;
    let s2 = get("split2", 50.0).clamp(s1 * 100.0 + 1.0, 98.0) / 100.0;
    let s3 = get("split3", 75.0).clamp(s2 * 100.0 + 1.0, 99.0) / 100.0;
    let response = |key: &str, x: f64| {
        let v = (get(key, 0.0) / 100.0).clamp(-1.0, 1.0) * amount;
        (v * 1.2).tanh() * 0.35 * (if v >= 0.0 { 1.0 - x } else { x }).sqrt()
    };
    let xs = [0.0, s1 / 2.0, s1, s2, s3, (s3 + 1.0) / 2.0, 1.0];
    let ys = [
        0.0,
        xs[1] + response("shadows", xs[1]),
        s1 + (response("shadows", s1) + response("darks", s1)) / 2.0,
        s2 + (response("darks", s2) + response("lights", s2)) / 2.0,
        s3 + (response("lights", s3) + response("highlights", s3)) / 2.0,
        xs[5] + response("highlights", xs[5]),
        1.0,
    ];
    xs.into_iter()
        .zip(ys)
        .map(|(x, y)| json!({"x":x*255.0,"y":y.clamp(0.0,1.0)*255.0}))
        .collect()
}

fn warp(value: f64, from: &[f64; 5], to: &[f64; 5]) -> f64 {
    let value = value.clamp(0.0, 1.0);
    let i = (0..4).find(|&i| value <= from[i + 1]).unwrap_or(3);
    to[i] + (to[i + 1] - to[i]) * (value - from[i]) / (from[i + 1] - from[i])
}

fn response(control: usize, strength: f64, x: f64) -> f64 {
    let position = (strength.clamp(-100.0, 100.0) / 25.0 + 4.0).clamp(0.0, 8.0);
    let low = (position.floor() as usize).min(7);
    let fraction = position - low as f64;
    let code = x.clamp(0.0, 1.0) * 255.0;
    let index = ((code / 4.0).floor() as usize).min(63);
    let width = if index == 63 { 3.0 } else { 4.0 };
    let t = (code - index as f64 * 4.0) / width;
    let sample = |level: usize| {
        let row = &measured::RESPONSE[control][level];
        (row[index] as f64 * (1.0 - t) + row[index + 1] as f64 * t) / 510.0
    };
    sample(low) * (1.0 - fraction) + sample(low + 1) * fraction
}

// Match the monotone Hermite interpolator used by the GPU when selecting knots.
pub(crate) fn interpolate(x: f64, points: &[(f64, f64)]) -> f64 {
    let i = (0..points.len() - 1)
        .find(|&i| x <= points[i + 1].0)
        .unwrap_or(points.len() - 2);
    let (x1, y1) = points[i];
    let (x2, y2) = points[i + 1];
    let d = (y2 - y1) / (x2 - x1);
    let tangent = |j: usize, before: bool| {
        if (before && j == 0) || (!before && j + 1 == points.len()) {
            return d;
        }
        let k = if before { j - 1 } else { j + 1 };
        let other = (points[k].1 - points[j].1) / (points[k].0 - points[j].0);
        if other * d <= 0.0 {
            0.0
        } else {
            (other + d) / 2.0
        }
    };
    let (mut m1, mut m2) = (tangent(i, true), tangent(i + 1, false));
    if d != 0.0 {
        let norm = (m1 / d).powi(2) + (m2 / d).powi(2);
        if norm > 9.0 {
            let scale = 3.0 / norm.sqrt();
            m1 *= scale;
            m2 *= scale;
        }
    }
    let t = ((x - x1) / (x2 - x1)).clamp(0.0, 1.0);
    (2.0 * t * t * t - 3.0 * t * t + 1.0) * y1
        + (t * t * t - 2.0 * t * t + t) * m1 * (x2 - x1)
        + (-2.0 * t * t * t + 3.0 * t * t) * y2
        + (t * t * t - t * t) * m2 * (x2 - x1)
}

fn measured_points(settings: &Value) -> Vec<Value> {
    let get = |key: &str, default: f64| {
        settings[key]
            .as_f64()
            .filter(|x| x.is_finite())
            .unwrap_or(default)
    };
    let amount = get("amount", 1.0).clamp(0.0, 2.0);
    let values = ["shadows", "darks", "lights", "highlights"]
        .map(|k| (get(k, 0.0) * amount).clamp(-100.0, 100.0));
    if values.iter().all(|v| *v == 0.0) {
        return vec![json!({"x":0.0,"y":0.0}), json!({"x":255.0,"y":255.0})];
    }
    let s1 = get("split1", 25.0).clamp(1.0, 97.0) / 100.0;
    let s2 = get("split2", 50.0).clamp(s1 * 100.0 + 1.0, 98.0) / 100.0;
    let s3 = get("split3", 75.0).clamp(s2 * 100.0 + 1.0, 99.0) / 100.0;
    let splits = [0.0, s1, s2, s3, 1.0];
    let defaults = [0.0, 0.25, 0.5, 0.75, 1.0];
    let mut dense = Vec::with_capacity(256);
    let mut previous = 0.0_f64;
    for i in 0..256 {
        let x = i as f64 / 255.0;
        let mut y = warp(x, &splits, &defaults);
        for (control, strength) in values.iter().enumerate() {
            if *strength != 0.0 {
                y = response(control, *strength, y);
            }
        }
        // Combined extreme settings must never reverse tonal order.
        y = warp(y, &defaults, &splits).max(previous);
        previous = y;
        dense.push((x, y));
    }
    compress_points(&dense)
}

pub(crate) fn compress_points(dense: &[(f64, f64)]) -> Vec<Value> {
    let mut knots = vec![dense[0], dense[dense.len() - 1]];
    while knots.len() < 16 {
        let worst = dense
            .iter()
            .filter(|p| !knots.iter().any(|k| k.0 == p.0))
            .map(|p| (*p, (interpolate(p.0, &knots) - p.1).abs()))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some((point, error)) = worst else { break };
        if error < 0.35 / 255.0 {
            break;
        }
        knots.push(point);
        knots.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    knots
        .into_iter()
        .map(|(x, y)| json!({"x":x*255.0,"y":y*255.0}))
        .collect()
}

/// Prepare editable/imported point curves for the fixed-size GPU buffer. Keep
/// valid short curves unchanged; simplify longer curves instead of truncating
/// their highlights. Both the uploaded buffer and count must use this result.
pub(crate) fn prepare_points(values: &[Value]) -> Vec<Value> {
    let mut points: Vec<_> = values
        .iter()
        .filter_map(|p| {
            let (x, y) = (p["x"].as_f64()?, p["y"].as_f64()?);
            (x.is_finite() && y.is_finite()).then_some((x.clamp(0.0, 255.0), y.clamp(0.0, 255.0)))
        })
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    points.dedup_by(|a, b| a.0 == b.0);
    if points.len() < 2 {
        return Vec::new();
    }
    if points.len() <= 16 {
        return points
            .into_iter()
            .map(|(x, y)| json!({"x":x,"y":y}))
            .collect();
    }
    let start = points[0].0;
    let end = points[points.len() - 1].0;
    let mut dense: Vec<_> = (0..=1024)
        .map(|i| {
            let x = start + (end - start) * i as f64 / 1024.0;
            (x / 255.0, interpolate(x, &points) / 255.0)
        })
        .collect();
    // Include exact original knots so narrow features are not lost to sampling.
    dense.extend(points.iter().map(|&(x, y)| (x / 255.0, y / 255.0)));
    dense.sort_by(|a, b| a.0.total_cmp(&b.0));
    dense.dedup_by(|a, b| a.0 == b.0);
    compress_points(&dense)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_point_curve_retains_endpoints_and_shape_within_gpu_capacity() {
        let original: Vec<_> = (0..=64)
            .map(|i| {
                let x = i as f64 / 64.0;
                json!({"x":x*255.0,"y":12.0+232.0*x.powf(1.35)})
            })
            .collect();
        let prepared = prepare_points(&original);
        assert!((2..=16).contains(&prepared.len()));
        assert_eq!(prepared.first(), original.first());
        assert_eq!(prepared.last(), original.last());
        let knots: Vec<_> = prepared
            .iter()
            .map(|p| (p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap()))
            .collect();
        for i in 0..=255 {
            let expected = 12.0 + 232.0 * (i as f64 / 255.0).powf(1.35);
            assert!((interpolate(i as f64, &knots) - expected).abs() < 0.5);
        }
    }
    #[test]
    fn malformed_point_curves_are_safe_and_existing_valid_curves_are_unchanged() {
        let valid = json!([{"x":0.0,"y":13.0},{"x":43.0,"y":42.0},{"x":84.0,"y":81.0},{"x":213.0,"y":203.0},{"x":255.0,"y":246.0}]);
        assert_eq!(
            prepare_points(valid.as_array().unwrap()),
            *valid.as_array().unwrap()
        );
        let bad = json!([null,{"x":10},{"x":0,"y":-200},{"x":10,"y":99},{"x":10,"y":50},{"x":999,"y":900}]);
        let result = prepare_points(bad.as_array().unwrap());
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], json!({"x":0.0,"y":0.0}));
        assert_eq!(result[2], json!({"x":255.0,"y":255.0}));
        assert!(prepare_points(&[json!({"x":1,"y":1})]).is_empty());
    }
    #[test]
    fn zero_amount_is_identity_and_strength_does_not_move_splits() {
        let settings =
            json!({"shadows":-65,"darks":5,"highlights":-40,"split1":20,"split3":86,"amount":0});
        for p in points(&settings) {
            assert_eq!(p["x"], p["y"]);
        }
        let mut full = settings.clone();
        full["amount"] = json!(1);
        let p = points(&full);
        assert!(p[1]["y"].as_f64().unwrap() < p[1]["x"].as_f64().unwrap());
        assert_eq!(p[2]["x"], json!(51.0));
        assert_eq!(p[6], json!({"x":255.0,"y":255.0}));
    }
    #[test]
    fn corrupt_splits_cannot_create_duplicate_or_unordered_nodes() {
        let p = points(&json!({"split1":200,"split2":-10,"split3":0,"amount":2,"highlights":1000}));
        for pair in p.windows(2) {
            assert!(pair[0]["x"].as_f64().unwrap() < pair[1]["x"].as_f64().unwrap());
        }
        assert!(
            p.iter()
                .all(|p| (0.0..=255.0).contains(&p["y"].as_f64().unwrap()))
        );
    }

    #[test]
    fn measured_response_matches_independent_gradient_samples() {
        // Samples are measurements of our synthetic gradient, not fitted photo pixels.
        for (control, strength, x, expected) in [
            ("highlights", -100, 204.0, 168.0),
            ("shadows", 100, 51.0, 81.0),
            ("darks", -100, 102.0, 42.0),
            ("lights", 100, 153.0, 223.0),
        ] {
            let mut settings = json!({"version":2});
            settings[control] = json!(strength);
            let pts: Vec<_> = points(&settings)
                .iter()
                .map(|p| (p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap()))
                .collect();
            assert!((interpolate(x, &pts) - expected).abs() < 2.0, "{control}");
        }
    }

    #[test]
    fn measured_identity_strength_and_extremes_are_bounded() {
        assert_eq!(
            points(&json!({"version":2,"shadows":-65,"amount":0})),
            vec![json!({"x":0.0,"y":0.0}), json!({"x":255.0,"y":255.0})]
        );
        for strength in [-100.0, -37.0, 0.0, 43.0, 100.0] {
            let p = points(
                &json!({"version":2,"shadows":strength,"darks":-strength,"lights":strength,"highlights":-strength,"split1":200,"split2":-10,"split3":0,"amount":2}),
            );
            assert!(p.len() <= 16);
            for pair in p.windows(2) {
                assert!(pair[0]["x"].as_f64().unwrap() < pair[1]["x"].as_f64().unwrap());
                assert!(pair[0]["y"].as_f64().unwrap() <= pair[1]["y"].as_f64().unwrap());
            }
        }
    }
}
