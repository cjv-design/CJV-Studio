import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';

const compiled = await build({
  entryPoints: ['src/utils/presetWhiteBalance.ts'], bundle: true,
  format: 'esm', platform: 'node', write: false,
});
const { mixPresetWhiteBalance } = await import('data:text/javascript;base64,' + Buffer.from(compiled.outputFiles[0].text).toString('base64'));
const preset = { temperature: 0, tint: 0, whiteBalance: { temperature: 4863, tint: 19 } };
const base = { temperature: 0, tint: 0, whiteBalance: null };

test('the same preset reaches the same absolute WB on differently lit photos', () => {
  for (const asShot of [{ temperature: 3200, tint: -12 }, { temperature: 6800, tint: 25 }]) {
    const result = mixPresetWhiteBalance(base, preset, asShot, 100);
    assert.ok(Math.abs(result.whiteBalance.temperature - 4863) < 1e-8);
    assert.equal(result.whiteBalance.tint, 19);
    assert.equal(result.temperature, 0);
    assert.equal(result.tint, 0);
  }
});

test('amount 0 restores original absolute and relative WB exactly', () => {
  const edited = { temperature: 8, tint: -4, whiteBalance: { temperature: 5200, tint: 10 } };
  assert.deepEqual(mixPresetWhiteBalance(edited, preset, { temperature: 6800, tint: 25 }, 0), edited);
});

test('amount uses reciprocal temperature and does not accumulate across slider changes', () => {
  const shot = { temperature: 6800, tint: 25 };
  const first = mixPresetWhiteBalance(base, preset, shot, 50);
  assert.ok(Math.abs(first.whiteBalance.temperature - 1e6 / ((1e6 / 6800 + 1e6 / 4863) / 2)) < 1e-8);
  assert.equal(first.whiteBalance.tint, 22);
  mixPresetWhiteBalance(base, preset, shot, 100);
  assert.deepEqual(mixPresetWhiteBalance(base, preset, shot, 50), first);
});

test('tint-only preset preserves current temperature including relative edits', () => {
  const edited = { temperature: 10, tint: 0, whiteBalance: null };
  const result = mixPresetWhiteBalance(edited, { whiteBalance: { tint: -7 } }, { temperature: 5000, tint: 20 }, 100);
  assert.ok(Math.abs(result.whiteBalance.temperature - 1e6 / 185) < 1e-8);
  assert.equal(result.whiteBalance.tint, -7);
});

test('As Shot clears explicit WB and legacy relative-only presets retain their behaviour', () => {
  assert.deepEqual(mixPresetWhiteBalance(base, { whiteBalance: null }, { temperature: 6800, tint: 25 }, 100), base);
  assert.deepEqual(mixPresetWhiteBalance(base, { temperature: 20, tint: -8 }, undefined, 50), {});
});
