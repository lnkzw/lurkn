import { describe, expect, it } from 'vitest';

import { MAX_FONT_SIZE, MIN_FONT_SIZE } from '$lib/constants';
import type { Settings } from '$lib/ipc';
import { normalise } from '$lib/stores/settings.svelte';
import { stepFontSize, watchLook } from './watchlook';

const DEFAULTS = normalise({} as Settings);

const PALETTE: Record<string, string> = {
  '--bg': ' #0e0e10',
  '--bg-raised': '#18181b',
  '--border': '#2a2a2e',
  '--text': '#efeff1',
  '--text-dim': '#adadb8',
  '--accent': '#9146ff',
  '--highlight': '#9146ff',
  '--ok': '#00c08b',
  '--warn': '#ffb020',
  '--font': "system-ui, 'Segoe UI', sans-serif",
  '--mono': 'ui-monospace, monospace',
};

const computed = { getPropertyValue: (name: string) => PALETTE[name] ?? '' };

describe('watchLook', () => {
  it('carries the palette Lurkn is painted with', () => {
    const look = watchLook(computed, DEFAULTS, 'dark');
    expect(look.background).toBe('#0e0e10');
    expect(look.textDim).toBe('#adadb8');
    expect(look.highlight).toBe('#9146ff');
    expect(look.scheme).toBe('dark');
  });

  /** The page has no --font of its own; a var() would resolve to nothing. */
  it('fills the font stack in', () => {
    expect(watchLook(computed, DEFAULTS, 'dark').font).toBe(PALETTE['--font']);

    const mono = { ...DEFAULTS, chat: { ...DEFAULTS.chat, font: 'monospace' as const } };
    expect(watchLook(computed, mono, 'dark').font).toBe(PALETTE['--mono']);

    const custom = {
      ...DEFAULTS,
      chat: { ...DEFAULTS.chat, font: 'custom' as const, customFont: 'Inter' },
    };
    expect(watchLook(computed, custom, 'dark').font).toBe(`"Inter", ${PALETTE['--font']}`);
  });

  it('carries the chat settings', () => {
    const settings = {
      ...DEFAULTS,
      chat: {
        ...DEFAULTS.chat,
        fontSize: 18,
        showTimestamps: false,
        emoteSize: 'large' as const,
        alternateBackgrounds: true,
        showChannelEvents: false,
      },
    };
    const look = watchLook(computed, settings, 'light');
    expect(look.fontSize).toBe(18);
    expect(look.timestamps).toBe(false);
    expect(look.emoteScale).toBe(2.25);
    expect(look.alternate).toBe(true);
    expect(look.channelEvents).toBe(false);
    expect(look.scheme).toBe('light');
  });
});

describe('stepFontSize', () => {
  it('steps by one and stops at the ends of the range', () => {
    expect(stepFontSize(13, 1)).toBe(14);
    expect(stepFontSize(13, -1)).toBe(12);
    expect(stepFontSize(MAX_FONT_SIZE, 1)).toBe(MAX_FONT_SIZE);
    expect(stepFontSize(MIN_FONT_SIZE, -1)).toBe(MIN_FONT_SIZE);
  });
});
