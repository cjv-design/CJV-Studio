# CJV Studio Alpha 0.1.6

Updated Reference rendering adds image-dependent Highlights and Shadows, measured
colour-grading luminance, wide-gamut point curves and neutral-preserving primary
calibration. It is an approximation, not Lightroom parity.

## Saved edits and presets

`referenceRenderingVersion: 2` enables the updated renderer for RAW images in
Reference mode. Missing versions and version 1 retain the earlier renderer.
New Adobe XMP imports with a ProcessVersion use version 2. The Basic panel has
an Updated Reference rendering switch; turning it off restores earlier rendering.
Selecting Reference explicitly enables the updated renderer. Applying an older
Reference preset retains its earlier version even over a newer edit. Preset
strength never interpolates rendering version numbers. Local-mask tone controls
retain their existing implementation.

## Correctness and reliability

- The adaptive tone guide uses a complete, centred Gaussian pyramid. Rotation
  and reflection do not change its tone response.
- Floating-point preview resampling preserves RAW highlight headroom. Fast RAW
  decoding also retains that headroom instead of clipping at display white.
- The Whites curve preserves HDR headroom without extrapolating a steep slope
  from a quantised final SDR sample.
- The tone guide is cached by its actual inputs; unrelated colour/curve edits
  do not rebuild it. Very thin previews remain bounded.
- Oversized GPU inputs now return an explicit error. Previously they could be
  exported unedited while reporting success.
- Profile UUID matching tolerates letter-case differences. Missing active film
  profiles report the correct way to switch the profile off.
- Thumbnail cache revision changes so corrected previews are regenerated.

## Evidence and remaining differences

Before installer packaging: 57 native Rust tests, 16 frontend preset tests and
10 navigation tests passed. Frontend type checking and production bundling passed.
The HDR decoder test uses the repository's own 16 by 8 synthetic DNG fixture.

On 36 separate native exports of held-out synthetic scenes, updated H/S improved
30 comparisons, retained four neutral comparisons and worsened two. Every tested
control combination improved its four-scene mean. The two regressions are a dark
spot scene with Highlights only. Synthetic tests do not establish photo parity.

Five full-preset RAW comparisons across three Sony camera models used the same
preset, white balance, full-frame geometry and local film profile. Mean absolute
RGB differences from Lightroom, measured after reducing to 416 pixels wide:

| Photo | Installed 0.1.5 | Updated candidate |
| --- | ---: | ---: |
| Cake | 30.09 | 7.11 |
| Food | 15.62 | 7.97 |
| Outdoor | 42.92 | 6.50 |
| Wide portrait | 7.67 | 9.50 |
| Close portrait | 8.82 | 6.90 |

These are 8-bit RGB error levels, not percentages or perceptual Delta E. Four
photos improve; the wide portrait regresses. Camera colour, some portrait tones,
local masks and other Adobe-specific processing still differ. A matching control
name does not imply identical output. The installed artifact must be rechecked
before its results are called verified.

Measured response tables contain only observations of CJV-generated synthetic
charts and scenes. Client photos, Adobe camera profiles and third-party film
tables are not included in the repository. Private validation photos were never
used to fit response coefficients. The primary-calibration matrices are control
responses, not camera profiles.
