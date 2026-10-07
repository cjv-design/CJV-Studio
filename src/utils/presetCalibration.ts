import type { ColorCalibration } from './adjustments';

export const mergePresetCalibration = (
  current: ColorCalibration,
  preset: Partial<ColorCalibration> | undefined,
): ColorCalibration => ({ ...current, ...preset });

export const scalePresetCalibration = (
  preset: Partial<ColorCalibration> | undefined,
  intensity: number,
): Partial<ColorCalibration> | undefined =>
  preset === undefined
    ? undefined
    : Object.fromEntries(
        Object.entries(preset).map(([key, value]) => [key, intensity === 0 ? 0 : value * (intensity / 100)]),
      );
