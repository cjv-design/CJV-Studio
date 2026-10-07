import assert from 'node:assert/strict';
import test from 'node:test';
import { mergePresetCalibration, scalePresetCalibration } from '../src/utils/presetCalibration.ts';

const current = {
  shadowsTint: 12,
  redHue: 20,
  redSaturation: 30,
  greenHue: 40,
  greenSaturation: 50,
  blueHue: 60,
  blueSaturation: 70,
};

test('partial calibration preserves omitted controls', () => {
  assert.deepEqual(mergePresetCalibration(current, { blueHue: -22.5 }), {
    ...current,
    blueHue: -22.5,
  });
});

test('explicit zero replaces an existing calibration value', () => {
  assert.deepEqual(mergePresetCalibration(current, { shadowsTint: 0 }), {
    ...current,
    shadowsTint: 0,
  });
});

test('a preset without calibration preserves the current group', () => {
  assert.deepEqual(mergePresetCalibration(current, undefined), current);
});

test('complete calibration replaces all seven fields', () => {
  const full = Object.fromEntries(Object.keys(current).map((key) => [key, -10]));
  assert.deepEqual(mergePresetCalibration(current, full), full);
});

test('preset application does not mutate saved or current values', () => {
  const base = Object.freeze({ ...current });
  const preset = Object.freeze({ redHue: -17 });
  const result = mergePresetCalibration(base, preset);
  assert.notEqual(result, base);
  assert.notEqual(result, preset);
  assert.deepEqual(base, current);
  assert.deepEqual(preset, { redHue: -17 });
});

test('zero intensity resets only controls included in a partial preset', () => {
  assert.deepEqual(mergePresetCalibration(current, scalePresetCalibration({ blueHue: -20 }, 0)), {
    ...current,
    blueHue: 0,
  });
});

test('changing intensity scales included controls and preserves the rest', () => {
  assert.deepEqual(mergePresetCalibration(current, scalePresetCalibration({ blueHue: -20 }, 50)), {
    ...current,
    blueHue: -10,
  });
  assert.deepEqual(mergePresetCalibration(current, scalePresetCalibration(undefined, 0)), current);
});
