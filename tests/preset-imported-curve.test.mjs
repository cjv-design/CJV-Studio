import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mixImportedCurve } from '../src/utils/presetImportedCurve.ts';
test('preset strength leaves curve regions unchanged and disables at zero', () => {
  const source = { shadows:-65, darks:5, highlights:-40, split1:20, split2:50, split3:86, amount:1 };
  for (const intensity of [0,50,100,200]) {
    const actual = mixImportedCurve(source,intensity);
    assert.equal(actual.amount,intensity/100);
    assert.equal(actual.shadows,-65);
    assert.equal(actual.split1,20);
    assert.equal(actual.split3,86);
  }
  assert.equal(source.amount,1);
  assert.equal(mixImportedCurve(null,100),null);
});
