import { describe, expect, it } from 'vitest';

import { formatUptime, formatViewers, thumbnailUrl } from './follows.svelte';

describe('formatViewers', () => {
  it('leaves small counts alone', () => {
    expect(formatViewers(0)).toBe('0');
    expect(formatViewers(999)).toBe('999');
  });

  it('abbreviates thousands with one decimal below 10k', () => {
    expect(formatViewers(1000)).toBe('1k');
    expect(formatViewers(1234)).toBe('1.2k');
    expect(formatViewers(9999)).toBe('10k');
  });

  it('drops the decimal above 10k', () => {
    expect(formatViewers(12_345)).toBe('12k');
    expect(formatViewers(999_999)).toBe('1000k');
  });

  it('abbreviates millions', () => {
    expect(formatViewers(1_500_000)).toBe('1.5m');
    expect(formatViewers(2_000_000)).toBe('2m');
  });
});

describe('formatUptime', () => {
  const start = '2026-09-20T12:00:00Z';
  const at = (iso: string) => Date.parse(iso);

  it('shows minutes only under an hour', () => {
    expect(formatUptime(start, at('2026-09-20T12:12:00Z'))).toBe('12m');
    expect(formatUptime(start, at('2026-09-20T12:00:30Z'))).toBe('0m');
  });

  it('shows hours and zero-padded minutes above an hour', () => {
    expect(formatUptime(start, at('2026-09-20T15:04:00Z'))).toBe('3h 04m');
    expect(formatUptime(start, at('2026-09-20T13:00:00Z'))).toBe('1h 00m');
  });

  it('never goes negative when clocks disagree', () => {
    expect(formatUptime(start, at('2026-09-20T11:00:00Z'))).toBe('0m');
  });

  it('returns nothing for an unparseable timestamp', () => {
    expect(formatUptime('not a date')).toBe('');
  });
});

describe('thumbnailUrl', () => {
  const TEMPLATE =
    'https://static-cdn.jtvnw.net/previews-ttv/live_user_foo-{width}x{height}.jpg';

  it('fills the placeholders', () => {
    expect(thumbnailUrl(TEMPLATE, 440, 248, 0)).toBe(
      'https://static-cdn.jtvnw.net/previews-ttv/live_user_foo-440x248.jpg?t=0',
    );
  });

  it('busts the cache once a minute, and not faster', () => {
    const a = thumbnailUrl(TEMPLATE, 440, 248, 60_000);
    expect(thumbnailUrl(TEMPLATE, 440, 248, 119_999)).toBe(a);
    expect(thumbnailUrl(TEMPLATE, 440, 248, 120_000)).not.toBe(a);
  });

  // Mirrors the host checks in lurkn-core's Stream::thumbnail tests.
  it('refuses a foreign host or plaintext', () => {
    expect(thumbnailUrl('https://evil.example/{width}x{height}.jpg', 440, 248)).toBeNull();
    expect(
      thumbnailUrl('http://static-cdn.jtvnw.net/{width}x{height}.jpg', 440, 248),
    ).toBeNull();
  });

  it('is null without a template or with junk', () => {
    expect(thumbnailUrl(null, 440, 248)).toBeNull();
    expect(thumbnailUrl('not a url', 440, 248)).toBeNull();
  });
});
