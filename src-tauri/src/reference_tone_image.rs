//! Connect the measured tone model to a cached linear-light image preview.
use crate::image_processing::GlobalAdjustments;
use image::{DynamicImage, Rgb32FImage};
use std::hash::{Hash, Hasher};

pub struct ToneMap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 4]>,
}

/// Only inputs that change the tone guide invalidate this cache. Editing a
/// curve, crop overlay, grain or colour grade must not recompute the pyramid.
pub fn cache_key(g: &GlobalAdjustments) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for value in [
        g.exposure,
        g.highlights,
        g.shadows,
        g.whites,
        g.wb_log_gain_l,
        g.wb_log_gain_m,
        g.wb_log_gain_s,
    ] {
        value.to_bits().hash(&mut hash);
    }
    bytemuck::bytes_of(&g.wb_rgb_to_lms_matrix).hash(&mut hash);
    bytemuck::bytes_of(&g.wb_lms_to_rgb_matrix).hash(&mut hash);
    g.reference_white_curve_count.hash(&mut hash);
    bytemuck::cast_slice::<_, u8>(&g.reference_white_curve).hash(&mut hash);
    let calibrate_before_tone = g.reference_calibration_stage == 2;
    calibrate_before_tone.hash(&mut hash);
    if calibrate_before_tone {
        bytemuck::bytes_of(&g.reference_calibration_matrix).hash(&mut hash);
    }
    hash.finish()
}

pub fn preview(image: &DynamicImage) -> Rgb32FImage {
    // Generic image resizing clamps floating-point channels to SDR [0,1].
    // The existing linear-light area resampler retains recoverable RAW values.
    crate::image_processing::downscale_f32_image(image, 128, 128).to_rgb32f()
}

fn encode(v: f32) -> f32 {
    if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
fn decode(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

pub fn build(preview: &Rgb32FImage, g: &GlobalAdjustments) -> Result<ToneMap, String> {
    let to_wide = |v: [f32; 3]| {
        [
            0.52934593 * v[0] + 0.33007280 * v[1] + 0.14058127 * v[2],
            0.09837434 * v[0] + 0.87346102 * v[1] + 0.02816463 * v[2],
            0.01688322 * v[0] + 0.11767247 * v[1] + 0.86544431 * v[2],
        ]
    };
    let wb_gains = [
        g.wb_log_gain_l.exp(),
        g.wb_log_gain_m.exp(),
        g.wb_log_gain_s.exp(),
    ];
    let apply_wb = |v: [f32; 3]| {
        let lms = g.wb_rgb_to_lms_matrix.transform(v);
        g.wb_lms_to_rgb_matrix
            .transform(std::array::from_fn(|i| lms[i] * wb_gains[i]))
    };
    let white = apply_wb([1.0; 3]);
    let normalisation = (white[0] * 0.2126 + white[1] * 0.7152 + white[2] * 0.0722).max(1e-6);
    let exposure = g.exposure.exp2();
    // Sample the same compressed curve and unit HDR extension as the shader.
    let white_curve: Vec<(f64, f64)> = g
        .reference_white_curve
        .iter()
        .take(g.reference_white_curve_count.min(16) as usize)
        .map(|p| (p.x as f64, p.y as f64))
        .collect();
    let white_response = |v: f32| {
        if white_curve.len() < 2 {
            return v;
        }
        let last = white_curve[white_curve.len() - 1];
        if v > 1.0 {
            return (last.1 / 255.0 + (v as f64 - 1.0)) as f32;
        }
        let x = v as f64 * 255.0;
        if x <= white_curve[0].0 {
            return (white_curve[0].1 / 255.0) as f32;
        }
        if x >= last.0 {
            return (last.1 / 255.0) as f32;
        }
        (crate::imported_curve::interpolate(x, &white_curve) / 255.0).clamp(0.0, 1.0) as f32
    };
    let mut intensities = Vec::with_capacity(preview.width() as usize * preview.height() as usize);
    for p in preview.pixels() {
        let mut rgb = apply_wb(p.0).map(|v| v * exposure / normalisation);
        if g.reference_calibration_stage == 2 {
            rgb = g.reference_calibration_matrix.transform(rgb);
        }
        let wide = to_wide(rgb).map(|v| encode(v.max(0.0)));
        let lo = wide.into_iter().fold(f32::INFINITY, f32::min);
        let hi = wide.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let mapped = white_response(lo)
            + (wide[1] - lo) * (white_response(hi) - white_response(lo)) / (hi - lo).max(1e-6);
        intensities.push(decode(mapped).max(1e-5));
    }
    let gains = crate::reference_tone::log_gains(
        &intensities,
        preview.width() as usize,
        preview.height() as usize,
        g.highlights * 120.0,
        g.shadows * 120.0,
    )
    .map_err(str::to_owned)?;
    Ok(ToneMap {
        width: preview.width(),
        height: preview.height(),
        pixels: gains
            .into_iter()
            .zip(intensities)
            .map(|(gain, intensity)| [gain, intensity.ln(), 0.0, 0.0])
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrelated_adjustments_reuse_the_tone_guide_but_wb_and_exposure_invalidate_it() {
        let original = GlobalAdjustments::default();
        let key = cache_key(&original);
        let mut changed = original;
        changed.contrast = 0.5;
        changed.blacks = 0.25;
        changed.reference_grading_stage = 3;
        assert_eq!(cache_key(&changed), key);
        changed.exposure = 1.0;
        assert_ne!(cache_key(&changed), key);
        changed = original;
        changed.wb_log_gain_m = 0.03;
        assert_ne!(cache_key(&changed), key);
        changed = original;
        changed.shadows = 0.5;
        assert_ne!(cache_key(&changed), key);
    }

    #[test]
    fn guide_preview_dimensions_and_pixels_follow_rotation() {
        let input = Rgb32FImage::from_fn(300, 600, |x, y| {
            image::Rgb([x as f32 / 100.0, y as f32 / 150.0, 2.0])
        });
        let image = DynamicImage::ImageRgb32F(input);
        let expected = image::imageops::rotate90(&preview(&image));
        let actual = preview(&image.rotate90());
        assert_eq!(actual.dimensions(), expected.dimensions());
        assert!(
            actual
                .as_raw()
                .iter()
                .zip(expected.as_raw())
                .all(|(a, b)| (a - b).abs() < 1e-5)
        );
        assert!(actual.as_raw().iter().any(|v| *v > 1.0));
    }

    #[test]
    fn thin_panorama_preview_is_bounded_without_losing_headroom() {
        let input =
            DynamicImage::ImageRgb32F(Rgb32FImage::from_pixel(16000, 1, image::Rgb([4.0; 3])));
        let small = preview(&input);
        assert_eq!(small.dimensions(), (128, 1));
        assert!(small.as_raw().iter().all(|v| (*v - 4.0).abs() < 1e-4));
    }
}
