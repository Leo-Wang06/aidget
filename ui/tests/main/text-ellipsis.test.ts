import { describe, expect, it } from 'vite-plus/test';

import { middleEllipsis } from '../../src/main/text-ellipsis';

describe('middle ellipsis', () => {
  it('keeps the original text when it fits', () => {
    expect(middleEllipsis('short', () => true)).toBe('short');
  });

  it('keeps both ends and inserts three dots in the middle', () => {
    const result = middleEllipsis(
      'abcdefghijklmnopqrstuvwxyz',
      (candidate) => candidate.length <= 10,
    );
    expect(result).toBe('abc...wxyz');
  });
});
