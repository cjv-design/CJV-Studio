//! An explicit native approximation of Adobe's separate parametric tone curve.
//! Uses the same response as the editor's native Curves panel, before the
//! preserved point curve. Adobe's proprietary PV2012 response is not emulated.
use serde_json::{Value, json};

pub fn points(settings: &Value) -> Vec<Value> {
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
