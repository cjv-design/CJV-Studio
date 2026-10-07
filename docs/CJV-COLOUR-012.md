# CJV Studio Alpha 0.1.2 colour work

The XMP importer preserves the saved point-curve black lift and imports a
separate parametric curve. The parametric stage uses CJV's existing native
curve response, not Adobe PV2012 mathematics. It runs before the point curve.
Its strength can be changed or disabled under Curves. Preset Amount changes
strength without moving the shadow, midtone and highlight split positions.
Nested film-profile curves cannot replace the outer preset's point curve.

## Optional private camera calibration

`tools/prepare_camera_calibration.py RAW1.ARW RAW2.ARW` uses a locally installed
Adobe DNG Converter to extract calibration metadata into the CJV application
data directory. Originals are read only; temporary conversions are discarded.
Restart the editor after preparing calibration. The default adds the DNG
BaselineExposure and uses AnalogBalance, CameraCalibration and ColorMatrix
to infer the as-shot illuminant. Exposure remains an editable adjustment on
top of the camera baseline. This is metadata correction, not measured chart
calibration of a camera or display.

The first version supports three-channel, reduced-resolution Sony ARWs with
baked white balance and A/D65 matrices. Every record is bound to the complete
source SHA-256 and camera model. A different capture, mosaic RAW or DNG cannot
accidentally receive that record. Unsupported inputs retain the prior renderer.
The records are private and are not part of this repository or installer.

`--forward` additionally enables the DNG ForwardMatrix transform. It is
experimental and must be compared against reference exports before use.
This does not supply Adobe's hue/saturation maps, profile tone curve, film
Look, lens corrections or proprietary basic-slider behaviour. Exact Lightroom
parity is not claimed. Imported XMP presets retain warnings for missing film
profiles and for the approximate parametric curve response.

Preparation never replaces existing calibration records. To disable a record,
set its `enabled` field to false and restart. To validate a different mode,
preserve a copy of the old JSON before changing `useForwardMatrix`.

The camera-to-XYZ transform follows the publicly documented DNG ForwardMatrix
method; XYZ D50 is adapted to D65 before conversion to linear sRGB.
Reference: [Adobe DNG resources and specification](https://helpx.adobe.com/camera-raw/desktop/dng-and-file-formats/digital-negative.html).

Local validation uses disposable copies and Lightroom JPEG reference exports.
Whole-image RGB differences include geometry, sharpening and tone differences;
they are not perceptual Delta E measurements or percentages of Adobe parity.
