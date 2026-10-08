import assert from 'node:assert/strict';
import test from 'node:test';
import { presetReferenceRendering } from '../src/utils/presetReferenceRendering.ts';

test('older Reference presets keep their rendering when applied over a newer edit', () => {
  const current = { referenceRenderingVersion: 2 };
  assert.equal({ ...current, ...presetReferenceRendering({ toneMapper: 'reference' }) }.referenceRenderingVersion, 1);
});

test('new presets retain their discrete rendering version at any strength', () => {
  for (const amount of [0, 0.5, 1, 2]) {
    const mixed = { referenceRenderingVersion: 2 * amount };
    assert.equal({ ...mixed, ...presetReferenceRendering({ toneMapper: 'reference', referenceRenderingVersion: 2 }) }.referenceRenderingVersion, 2);
  }
});

test('partial presets without rendering settings preserve the current choice', () => {
  assert.deepEqual(presetReferenceRendering({}), {});
  assert.deepEqual(presetReferenceRendering({ toneMapper: 'basic' }), {});
});
