import { describe, expect, it } from 'vitest';

import { chatFontStack } from './settings.svelte';

describe('chatFontStack', () => {
  it('maps the presets to real stacks', () => {
    expect(chatFontStack('system', '')).toBe('var(--font)');
    expect(chatFontStack('monospace', '')).toBe('var(--mono)');
    expect(chatFontStack('serif', '')).toContain('serif');
    expect(chatFontStack('sans', '')).toContain('sans-serif');
  });

  it('quotes a custom name and keeps a fallback', () => {
    expect(chatFontStack('custom', 'JetBrains Mono')).toBe('"JetBrains Mono", var(--font)');
  });

  /** Rust strips the dangerous characters; an empty result must still be safe. */
  it('falls back when the custom name is empty or missing', () => {
    expect(chatFontStack('custom', '')).toBe('var(--font)');
    expect(chatFontStack('custom', undefined)).toBe('var(--font)');
  });
});
