interface ReferencePreset {
  toneMapper?: string;
  referenceRenderingVersion?: 1 | 2;
}

// A saved preset that predates versioning keeps its original Reference look.
// Rendering versions are discrete choices, never an intensity-scaled number.
export function presetReferenceRendering(preset: ReferencePreset): { referenceRenderingVersion?: 1 | 2 } {
  if (preset.referenceRenderingVersion !== undefined) {
    return { referenceRenderingVersion: preset.referenceRenderingVersion === 2 ? 2 : 1 };
  }
  return preset.toneMapper === 'reference' ? { referenceRenderingVersion: 1 } : {};
}
