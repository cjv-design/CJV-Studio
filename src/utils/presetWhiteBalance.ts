import { resolveWhiteBalance } from './whiteBalance';
import type { WhiteBalance } from './whiteBalance';

interface WBAdjustments {
  temperature: number;
  tint: number;
  whiteBalance?: WhiteBalance | null;
}

interface WBPreset {
  temperature?: number;
  tint?: number;
  whiteBalance?: Partial<WhiteBalance> | null;
}

// Interpolate from the saved pre-preset state, never the previous slider result.
// Mired interpolation follows the renderer's relative temperature controls.
export function mixPresetWhiteBalance(
  base: WBAdjustments,
  preset: WBPreset,
  asShot: WhiteBalance | undefined,
  intensity: number,
): Partial<WBAdjustments> {
  if (!Object.hasOwn(preset, 'whiteBalance')) return {};
  if (intensity === 0) {
    return { temperature: base.temperature, tint: base.tint, whiteBalance: base.whiteBalance ?? null };
  }
  const fraction = Math.max(0, Math.min(2, intensity / 100));
  if (preset.whiteBalance === null && fraction === 1) {
    return { temperature: 0, tint: 0, whiteBalance: null };
  }
  // Images normally supply as-shot metadata when loading. Until it arrives,
  // retain the explicit preset values instead of inventing a camera illuminant.
  const origin = asShot ? resolveWhiteBalance(asShot, base) : base.whiteBalance;
  if (!origin) {
    return { temperature: 0, tint: 0, whiteBalance: preset.whiteBalance as WhiteBalance | null };
  }
  const target = preset.whiteBalance === null
    ? (asShot ?? origin)
    : { ...origin, ...preset.whiteBalance };
  const startMired = 1_000_000 / origin.temperature;
  const endMired = 1_000_000 / target.temperature;
  return {
    temperature: 0,
    tint: 0,
    whiteBalance: {
      temperature: Math.max(2000, Math.min(50000, 1_000_000 / Math.max(20, startMired + (endMired - startMired) * fraction))),
      tint: Math.max(-150, Math.min(150, origin.tint + (target.tint - origin.tint) * fraction)),
    },
  };
}
