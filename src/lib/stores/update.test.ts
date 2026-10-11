import { describe, expect, it } from 'vitest';

import { releasePage, showBar, statusLine } from './update.svelte';

describe('showBar', () => {
  it('shows a new version until it is put away', () => {
    const status = { kind: 'available', version: '0.2.0', installable: true } as const;
    expect(showBar(status, null)).toBe(true);
    expect(showBar(status, '0.2.0')).toBe(false);
  });

  it('shows the next version even after one was put away', () => {
    const status = { kind: 'available', version: '0.3.0', installable: false } as const;
    expect(showBar(status, '0.2.0')).toBe(true);
  });

  it('keeps a running install on screen', () => {
    expect(showBar({ kind: 'downloading', version: '0.2.0', percent: 40 }, '0.2.0')).toBe(true);
    expect(showBar({ kind: 'installing', version: '0.2.0' }, '0.2.0')).toBe(true);
  });

  it('leaves a failed check to Settings', () => {
    expect(showBar({ kind: 'checkFailed', message: 'Could not reach GitHub.' }, null)).toBe(false);
    expect(showBar({ kind: 'upToDate' }, null)).toBe(false);
  });
});

describe('statusLine', () => {
  it('says nothing before the first check', () => {
    expect(statusLine({ kind: 'unknown' })).toBeNull();
  });

  it('shows the download as it goes', () => {
    expect(statusLine({ kind: 'downloading', version: '0.2.0', percent: null })).toBe(
      'Downloading 0.2.0…',
    );
    expect(statusLine({ kind: 'downloading', version: '0.2.0', percent: 42 })).toBe(
      'Downloading 0.2.0… 42%',
    );
  });
});

describe('releasePage', () => {
  it('points at the tag', () => {
    expect(releasePage('0.2.0')).toBe('https://github.com/lnkzw/lurkn/releases/tag/v0.2.0');
  });
});
