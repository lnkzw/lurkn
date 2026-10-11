import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Transient } from './transient.svelte';

describe('Transient', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  /** The bug this exists for: an AutoMod notice that never went away. */
  it('clears itself', () => {
    const error = new Transient(1000);
    error.value = 'Your message is being checked by mods and has not been sent.';

    vi.advanceTimersByTime(999);
    expect(error.value).not.toBeNull();
    vi.advanceTimersByTime(1);
    expect(error.value).toBeNull();
  });

  it('gives a new message the full time', () => {
    const error = new Transient(1000);
    error.value = 'first';
    vi.advanceTimersByTime(800);
    error.value = 'second';
    vi.advanceTimersByTime(800);
    expect(error.value).toBe('second');
  });

  it('can be cleared early', () => {
    const error = new Transient(1000);
    error.value = 'first';
    error.value = null;
    expect(error.value).toBeNull();
    // A stale timer must not clear a later message early.
    error.value = 'second';
    vi.advanceTimersByTime(999);
    expect(error.value).toBe('second');
  });
});
