import { describe, expect, it } from 'vitest';

import { unreadIndex } from './chat.svelte';

const log = (...ids: string[]) => ids.map((id) => ({ id }));

describe('unreadIndex', () => {
  it('draws no line before the reader has ever looked away', () => {
    expect(unreadIndex(log('a', 'b', 'c'), null)).toBe(-1);
  });

  it('draws no line in an empty log', () => {
    expect(unreadIndex([], 'a')).toBe(-1);
  });

  it('draws no line while the mark is still the newest message', () => {
    expect(unreadIndex(log('a', 'b', 'c'), 'c')).toBe(-1);
  });

  it('puts the line under the marked message', () => {
    expect(unreadIndex(log('a', 'b', 'c'), 'a')).toBe(1);
    expect(unreadIndex(log('a', 'b', 'c'), 'b')).toBe(2);
  });

  it('puts the line at the top once the mark has been trimmed away', () => {
    // 500 messages arrived while the window was unfocused: everything left in
    // the buffer is newer than the mark.
    expect(unreadIndex(log('d', 'e', 'f'), 'a')).toBe(0);
  });
});
