//! Experimental, image-dependent reference highlights and shadows.
//! The response model contains measurements of CJV-generated grey scenes only.
//! A complete pyramid is required: stopping at a rectangular low-resolution
//! level changes the overall response with the image's aspect ratio.
mod data {
    include!("reference_tone_data.rs");
}

const KEYS: [f64; 6] = [-9.0, -6.0, -4.0, -2.0, 0.0, 2.0];
const SIGMA: f64 = 0.5;

#[derive(Clone)]
struct Plane {
    w: usize,
    h: usize,
    values: Vec<f64>,
}

fn reflect(i: isize, n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    let period = 2 * (n as isize - 1);
    let i = i.rem_euclid(period);
    if i < n as isize {
        i as usize
    } else {
        (period - i) as usize
    }
}

fn down(p: &Plane) -> Plane {
    const WEIGHTS: [f64; 5] = [0.0625, 0.25, 0.375, 0.25, 0.0625];
    let mut horizontal = vec![0.0; p.values.len()];
    for y in 0..p.h {
        for x in 0..p.w {
            horizontal[y * p.w + x] = (0..5)
                .map(|i| WEIGHTS[i] * p.values[y * p.w + reflect(x as isize + i as isize - 2, p.w)])
                .sum();
        }
    }
    let mut values = vec![0.0; p.w * p.h];
    for y in 0..p.h {
        for x in 0..p.w {
            values[y * p.w + x] = (0..5)
                .map(|i| {
                    WEIGHTS[i] * horizontal[reflect(y as isize + i as isize - 2, p.h) * p.w + x]
                })
                .sum();
        }
    }
    // Sampling at pixel centres is essential. Taking every second pixel from
    // the top left biases the coarsest level and changes exposure on rotation.
    resize(
        &Plane {
            w: p.w,
            h: p.h,
            values,
        },
        p.w.div_ceil(2),
        p.h.div_ceil(2),
    )
}

fn resize(p: &Plane, w: usize, h: usize) -> Plane {
    let mut values = vec![0.0; w * h];
    for y in 0..h {
        let sy = ((y as f64 + 0.5) * p.h as f64 / h as f64 - 0.5).clamp(0.0, (p.h - 1) as f64);
        let y0 = sy.floor() as usize;
        let y1 = (y0 + 1).min(p.h - 1);
        let fy = sy - y0 as f64;
        for x in 0..w {
            let sx = ((x as f64 + 0.5) * p.w as f64 / w as f64 - 0.5).clamp(0.0, (p.w - 1) as f64);
            let x0 = sx.floor() as usize;
            let x1 = (x0 + 1).min(p.w - 1);
            let fx = sx - x0 as f64;
            values[y * w + x] =
                (p.values[y0 * p.w + x0] * (1.0 - fx) + p.values[y0 * p.w + x1] * fx) * (1.0 - fy)
                    + (p.values[y1 * p.w + x0] * (1.0 - fx) + p.values[y1 * p.w + x1] * fx) * fy;
        }
    }
    Plane { w, h, values }
}

fn pyramid(p: Plane) -> Vec<Plane> {
    let mut result = vec![p];
    while result.last().unwrap().w > 1 || result.last().unwrap().h > 1 {
        result.push(down(result.last().unwrap()));
    }
    result
}

fn sample(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    if x <= xs[0] {
        return ys[0];
    }
    if x >= xs[xs.len() - 1] {
        return ys[ys.len() - 1];
    }
    let i = xs
        .partition_point(|v| *v < x)
        .saturating_sub(1)
        .min(xs.len() - 2);
    let f = (x - xs[i]) / (xs[i + 1] - xs[i]);
    ys[i] * (1.0 - f) + ys[i + 1] * f
}

fn model(highlights: f32, shadows: f32) -> ([f64; 36], [f64; 23]) {
    let bracket = |v: f32| {
        let p = (v.clamp(-100.0, 100.0) as f64 + 100.0) / 50.0;
        let i = (p.floor() as usize).min(3);
        (i, p - i as f64)
    };
    let (h, hf) = bracket(highlights);
    let (s, sf) = bracket(shadows);
    let mut coefficients = [0.0; 36];
    let mut solid = [0.0; 23];
    for (hi, hw) in [(h, 1.0 - hf), (h + 1, hf)] {
        for (si, sw) in [(s, 1.0 - sf), (s + 1, sf)] {
            let index = hi * 5 + si;
            let weight = hw * sw;
            for (v, source) in coefficients.iter_mut().zip(data::COEFFICIENTS[index]) {
                *v += source * weight;
            }
            for (v, source) in solid.iter_mut().zip(data::SOLID_DELTA[index]) {
                *v += source * weight;
            }
        }
    }
    (coefficients, solid)
}

/// Return log exposure gains for a small linear-light intensity preview.
/// Input and output use the same dimensions. The caller applies the scalar
/// gains before subsequent tone/colour operations, retaining full-resolution detail.
pub fn log_gains(
    input: &[f32],
    width: usize,
    height: usize,
    highlights: f32,
    shadows: f32,
) -> Result<Vec<f32>, &'static str> {
    if width == 0
        || height == 0
        || width.checked_mul(height) != Some(input.len())
        || input.len() > 65_536
    {
        return Err("Invalid reference tone preview dimensions");
    }
    if !highlights.is_finite() || !shadows.is_finite() || input.iter().any(|v| !v.is_finite()) {
        return Err("Non-finite reference tone input");
    }
    if highlights == 0.0 && shadows == 0.0 {
        return Ok(vec![0.0; input.len()]);
    }
    let (coefficients, solid) = model(highlights, shadows);
    let source = Plane {
        w: width,
        h: height,
        values: input.iter().map(|v| (*v as f64).max(1e-5).ln()).collect(),
    };
    let lo = source.values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = source
        .values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let guide = pyramid(source.clone());
    let mut output: Vec<Plane> = guide
        .iter()
        .map(|p| Plane {
            w: p.w,
            h: p.h,
            values: vec![0.0; p.values.len()],
        })
        .collect();
    if hi - lo > 1e-7 {
        let step = (hi - lo) / 31.0;
        for bin in 0..32 {
            let centre = lo + step * bin as f64;
            for sign in 0..2 {
                let remapped = Plane {
                    w: width,
                    h: height,
                    values: source
                        .values
                        .iter()
                        .map(|v| {
                            if sign == 0 {
                                (v - centre - SIGMA).max(0.0)
                            } else {
                                (centre - v - SIGMA).max(0.0)
                            }
                        })
                        .collect(),
                };
                let remap = pyramid(remapped);
                for level in 0..guide.len() - 1 {
                    let p = &guide[level];
                    let up = resize(&remap[level + 1], p.w, p.h);
                    let start = (level / 2).min(2) * 12 + sign * 6;
                    for i in 0..p.values.len() {
                        let weight = (1.0 - ((p.values[i] - centre) / step).abs()).max(0.0);
                        if weight > 0.0 {
                            let coefficient =
                                sample(&KEYS, &coefficients[start..start + 6], p.values[i]);
                            output[level].values[i] +=
                                (remap[level].values[i] - up.values[i]) * weight * coefficient;
                        }
                    }
                }
            }
        }
    }
    let last = guide.last().unwrap();
    for (out, value) in output
        .last_mut()
        .unwrap()
        .values
        .iter_mut()
        .zip(&last.values)
    {
        *out = sample(&data::SOLID_INPUT, &solid, *value);
    }
    let mut result = output.pop().unwrap();
    for p in output.into_iter().rev() {
        let up = resize(&result, p.w, p.h);
        result = Plane {
            w: p.w,
            h: p.h,
            values: p.values.iter().zip(up.values).map(|(a, b)| a + b).collect(),
        };
    }
    Ok(result
        .values
        .into_iter()
        .map(|v| v.clamp(-8.0, 8.0) as f32)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_is_exact_and_invalid_inputs_fail() {
        assert_eq!(
            log_gains(&[0.0, 0.2, 1.0, 6.0], 2, 2, 0.0, 0.0).unwrap(),
            vec![0.0; 4]
        );
        assert!(log_gains(&[1.0], 0, 1, 0.0, 0.0).is_err());
        assert!(log_gains(&[f32::NAN], 1, 1, 0.0, 0.0).is_err());
        assert!(log_gains(&[1.0], 1, 1, f32::INFINITY, 0.0).is_err());
    }
    #[test]
    fn uniform_response_does_not_depend_on_aspect_or_resolution() {
        for value in [0.0, 0.001, 0.05, 0.5, 1.0, 4.0] {
            let expected = log_gains(&[value], 1, 1, -71.2, 61.6).unwrap()[0];
            for (w, h) in [(9, 13), (32, 8), (1, 17)] {
                let actual = log_gains(&vec![value; w * h], w, h, -71.2, 61.6).unwrap();
                assert!(actual.iter().all(|v| (v - expected).abs() < 1e-5));
            }
        }
    }
    #[test]
    fn arbitrary_control_combinations_are_finite_and_bounded() {
        let input: Vec<_> = (0..117).map(|i| 0.001 + (i % 23) as f32 / 11.0).collect();
        for (h, s) in [
            (-100.0, 100.0),
            (100.0, -100.0),
            (37.0, 63.0),
            (-14.0, -79.0),
        ] {
            assert!(
                log_gains(&input, 13, 9, h, s)
                    .unwrap()
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 8.0)
            );
        }
    }
    #[test]
    fn rectangular_pyramid_matches_independent_numeric_reference() {
        let input: Vec<_> = (0..9)
            .flat_map(|y| {
                (0..13).map(move |x| 0.001 + ((x * 37 + y * 19) % 101) as f32 / 101.0 * 1.5)
            })
            .collect();
        let gains = log_gains(&input, 13, 9, -71.2, 61.6).unwrap();
        for (i, expected) in [
            (0, 2.4331105),
            (6, -0.25242176),
            (58, -0.23818237),
            (116, -0.19792583),
        ] {
            assert!(
                (gains[i] - expected).abs() < 0.0001,
                "index {i}: {} versus {expected}",
                gains[i]
            );
        }
    }

    #[test]
    fn rotation_and_reflection_do_not_change_tone() {
        for (w, h) in [(13, 9), (16, 24), (1, 17), (32, 32)] {
            let input: Vec<_> = (0..h)
                .flat_map(|y| {
                    (0..w).map(move |x| 0.001 + ((x * 37 + y * 19) % 101) as f32 / 101.0 * 3.0)
                })
                .collect();
            let expected = log_gains(&input, w, h, -71.2, 61.6).unwrap();
            let mut rotated = vec![0.0; input.len()];
            let mut reflected = rotated.clone();
            for y in 0..h {
                for x in 0..w {
                    rotated[x * h + h - 1 - y] = input[y * w + x];
                    reflected[y * w + w - 1 - x] = input[y * w + x];
                }
            }
            let rotated = log_gains(&rotated, h, w, -71.2, 61.6).unwrap();
            let reflected = log_gains(&reflected, w, h, -71.2, 61.6).unwrap();
            for y in 0..h {
                for x in 0..w {
                    assert!((expected[y * w + x] - rotated[x * h + h - 1 - y]).abs() < 1e-5);
                    assert!((expected[y * w + x] - reflected[y * w + w - 1 - x]).abs() < 1e-5);
                }
            }
        }
    }
}
