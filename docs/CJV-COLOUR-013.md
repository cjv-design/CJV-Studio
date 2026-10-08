# CJV Studio Alpha 0.1.3 colour work

This is an experimental compatibility milestone. It does not reproduce the
Lightroom rendering engine. Some photographs still differ visibly, particularly
in saturated camera colours and image-adaptive Highlights/Shadows processing.

## Imported film profiles

An XMP preset's Look UUID can now resolve against profiles already installed on
the local computer, or profiles alongside the preset. Supported RGB tables are
decoded into a private application-data cache. No Adobe profiles, LUTs, camera
tables, photographs or proprietary implementation code are bundled.

The renderer applies the supported hidden profile adjustments and RGB table
without changing the visible basic-slider values. The Colour panel displays the
profile name and Amount. Amount zero disables it completely. A missing or invalid
cache produces an export error instead of silently omitting the look. Re-import
the original XMP on the destination computer to recreate its cache.

Currently supported: version-1 RGB tables, 2 to 32 samples per axis, sRGB primaries
and transfer encoding, plus exposure, contrast, highlights, shadows, whites,
blacks, clarity, texture, dehaze, vibrance and saturation deltas. Unsupported
table encodings, profile tone curves, look tables, monochrome conversion,
calibration/grading deltas and extended tone-map strength are reported at import.
Other profile features remain outside the implemented subset.

## Reference rendering

Modern XMP imports select the optional Reference RAW tone mapper. Its neutral
tone response, parametric curves and basic-control responses were measured with
original synthetic gradients rendered through a local Lightroom installation.
The checked-in response samples are measurements of those generated images.
They are not camera profiles, photo-specific correction tables or Adobe code.

Parametric version 2 uses independently measured shadow/dark/light/highlight
responses. Region splits and Amount remain editable. A wide-gamut working space
limits colour distortion during the parametric stage. Reference Shadows use the
complete -100 to +100 range; legacy native imports retain their original scale.
The global basic model jointly samples Highlights and Shadows because those
controls interact. Local masks continue to use the native processing functions.

Important limitation: a global measured response cannot reproduce Lightroom's
image-adaptive processing. Changing the surrounding brightness of an otherwise
identical synthetic ramp changes Lightroom's output substantially. The current
Reference basic model does not account for this context. Matching one photo or
one gradient is not evidence of general parity.

Existing saved edits without Reference mode and parametric version 2 retain
their previous processing. The alpha does not rewrite saved preset libraries.
Re-import presets to create new versions using the new features.

## Reliability fixes

- Imported point curves exceeding 16 nodes are simplified while retaining their
  endpoints, rather than truncated. Uploaded counts match the GPU buffer.
- Invalid, duplicate or unordered curve points cannot index outside that buffer.
- Failed command-line exports return a nonzero process exit code.
- LUT intensity zero remains visibly zero in the Effects panel.
- Profile Amount zero works even when its cache is missing.
- The profile is included with Camera Calibration in the copy/paste settings.

## Optional camera metadata

The private calibration preparation tool now recognises Sony mosaic ARWs when
explicitly invoked with `--mosaic`. ForwardMatrix remains experimental. Its
source hash and pixel-layout checks prevent reuse on another capture. This
metadata does not supply Adobe's camera hue/saturation maps or look tables.

The two trial mosaic-camera records were disabled after comparison because they
did not consistently improve colour. They are not part of the installer. Existing
validated linear Sony calibration records are preserved.

## Validation and remaining work

Validation uses disposable RAW copies from three Sony models, local Lightroom
reference exports, independent synthetic gradients, and native GPU exports.
Checks include legacy pixel equality, malformed/long curves, missing profiles,
zero profile amount, 16-bit TIFF output and failure exit status. Private images
and comparison exports remain outside the repository.

Whole-image mean absolute RGB error is a diagnostic, not perceptual Delta E or a
percentage of Lightroom compatibility. Comparisons use matching full-frame
dimensions with lens correction disabled. The initial five-photo run improved
against 0.1.2 overall, but the remaining errors are too large to claim parity.

Further work is required on camera colour profiles, adaptive tone controls,
profile feature coverage, local masks, highlight reconstruction, sharpening and
noise reduction. The inherited frontend type-check failures also remain open.

Format references:
[Adobe DNG resources](https://www.adobe.com/support/downloads/dng/dng_sdk.html),
[Adobe enhanced-profile SDK](https://download.adobe.com/pub/adobe/lightroom/profile-sdk/ACR_and_Lightroom_Profile_SDK.zip),
[ICC ROMM RGB specification](https://www.color.org/chardata/rgb/ROMMRGB.pdf).
