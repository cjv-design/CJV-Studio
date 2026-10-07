// Keep split positions fixed when changing preset strength. Only the amount
// changes; it is independent from the preserved point curve.
export function mixImportedCurve(preset: any, intensity: number): any {
  if (!preset || typeof preset !== 'object') return null;
  return { ...preset, amount: Math.min(2, Math.max(0, (preset.amount ?? 1) * intensity / 100)) };
}
