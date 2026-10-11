import { describe, expect, it } from 'vitest';

import { emoteScale, resolveTheme } from './settings.svelte';

describe('resolveTheme', () => {
  it('honours an explicit choice regardless of the system', () => {
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });

  it('follows the system when set to system', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
  });
});

describe('emoteScale', () => {
  it('orders the sizes', () => {
    expect(emoteScale('small')).toBeLessThan(emoteScale('medium'));
    expect(emoteScale('medium')).toBeLessThan(emoteScale('large'));
  });

  /** Must stay in step with EmoteSize::scale in lurkn-core. */
  it('matches the Rust scale values', () => {
    expect(emoteScale('small')).toBe(1.35);
    expect(emoteScale('medium')).toBe(1.75);
    expect(emoteScale('large')).toBe(2.25);
  });
});
