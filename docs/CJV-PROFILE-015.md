# CJV Studio Alpha 0.1.5 profile and loading fixes

New enhanced-profile imports retain the complete 0 to 200 amount range. The
RGB table clamps its strength to its own supported range, independently of the
profile's hidden basic adjustments. A nonzero table minimum can retain a colour
effect at amount zero. The profile switch disables the entire effect explicitly.

Saved edits from earlier versions retain their previous zero and clamp behaviour.
The new behaviour is recorded by `amountVersion: 2` on newly imported profiles.
Private profile caches remain content addressed and are not redistributed.
Missing required profiles fail exports, including a zero-amount profile whose
table minimum is nonzero. Disabled profiles do not require their cache.

A native loading race is also fixed: a slow white-balance metadata read could
publish an old photo after a newer selection had already finished loading.
Publishing now checks the selection generation while holding the image lock.

## Validation

- 38 Rust tests cover production helpers, including profile limits, old saved
  edits, missing required profiles, and delayed native loading.
- 23 frontend regression tests, TypeScript checking and the frontend build pass.
- Twelve native export scenarios verify zero and high profile amounts, explicit
  disabling, missing caches and legacy behaviour. Corresponding reference
  renders are pixel-identical within the application.
- Independent Lightroom exports of CJV-generated linear DNGs and RGB tables
  distinguish stored XMP amount from the profile creation dialog's visible
  slider mapping. With a table range of 0.5 to 1.5, stored amounts 0 and 0.5 give
  the same table-only result; 1.5 and 2 also match. Hidden exposure remains
  independent of the table clamp. No Adobe table or client photo is included.

The standard CC1 profile has a table range of 0 to 2, so these range fixes do not
change its expected colour. Primary-calibration and adaptive-tone experiments
remain outside this release. Image-adaptive highlights and shadows and camera
colour rendering still differ materially from Lightroom. This is not parity.
