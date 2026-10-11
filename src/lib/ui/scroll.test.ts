import { describe, expect, it } from 'vitest';

import { stillFollowing } from './scroll';

describe('stillFollowing', () => {
  it('follows at the bottom', () => {
    expect(stillFollowing(false, 0, false)).toBe(true);
    expect(stillFollowing(false, 1.5, true)).toBe(true);
  });

  /** The reported bug: a small upward scroll was undone by the next batch. */
  it('stops at the first upward movement, however small', () => {
    expect(stillFollowing(true, 10, true)).toBe(false);
  });

  it('does not resume until the reader is back at the end', () => {
    expect(stillFollowing(false, 30, false)).toBe(false);
  });

  /** Content growing under a pinned log is not the reader scrolling away. */
  it('keeps following when nothing moved up', () => {
    expect(stillFollowing(true, 30, false)).toBe(true);
  });
});
