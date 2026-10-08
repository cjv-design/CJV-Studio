# CJV Studio Alpha 0.1.4 reliability fixes

Rapid photo, folder and album selections could be overwritten by an earlier
asynchronous response. Selection requests now discard superseded results,
including delayed EXIF sorting and cache checks. Returning to the library cancels
a pending photo selection. Returning to the same photo does not revive its old
load or incorrectly cancel its current load.

Background metadata synchronisation no longer replaces adjustments made after
the cached photo opened. Late failures from another photo cannot mark the current
photo ready before its pixels have loaded.

Other repairs:

- Dragging a new mask onto a sub-mask inserts an additive component at the target
  position. Previously that position was passed as the blending mode.
- Copy/paste works when an older settings file lacks copy/paste preferences.
- Delayed preview cleanup captures the previous image URL safely.
- Panel buttons have accessible names. Typed translations, panel sections, mask
  parameters, folder data, progress values and virtual rows now pass TypeScript
  without weakening the compiler settings.

Validation: ten tests exercise the actual navigation hook with controlled native
responses. The previous implementation fails seven and times out in two; the
unchanged metadata refresh case passes. All ten pass with the fixes. The existing
preset tests, frontend build and full TypeScript check are also required before
packaging. CI now enforces type checking and the navigation regressions.

This milestone retains the 0.1.3 colour renderer. Experimental primary-calibration
measurements are being evaluated separately. The limitations described in
`CJV-COLOUR-013.md`, including adaptive tonal processing and camera profiles,
remain. This is not a claim of Lightroom parity.
