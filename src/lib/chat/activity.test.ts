import { describe, expect, it } from 'vitest';

import { activity, isActivity, wantedFrom, type Feedable, type Wanted } from './activity';

const ALL: Wanted = {
  subs: true,
  gifts: true,
  raids: true,
  bits: true,
  announcements: true,
  moderators: true,
};

const msg = (id: string, kind: string | null, badges: string[] = []): Feedable => ({
  id,
  author: { badges: badges.map((setId) => ({ setId })) },
  event: kind ? { kind, count: null } : null,
});

describe('isActivity', () => {
  it('leaves ordinary chat out', () => {
    expect(isActivity(msg('1', null), ALL)).toBe(false);
  });

  it('takes every kind the reader asked for', () => {
    for (const kind of ['sub', 'resub', 'upgrade', 'subGift', 'communityGift', 'raid', 'cheer']) {
      expect(isActivity(msg('1', kind), ALL), kind).toBe(true);
    }
  });

  it('honours each toggle on its own', () => {
    const noBits = { ...ALL, bits: false };
    expect(isActivity(msg('1', 'cheer'), noBits)).toBe(false);
    expect(isActivity(msg('2', 'sub'), noBits)).toBe(true);

    const noSubs = { ...ALL, subs: false };
    expect(isActivity(msg('3', 'resub'), noSubs)).toBe(false);
    // Gifts are somebody else's money, and a separate toggle.
    expect(isActivity(msg('4', 'subGift'), noSubs)).toBe(true);
  });

  /** A notice type added after this build shipped is what a feed is for. */
  it('keeps rare and unknown notices whatever the toggles say', () => {
    const nothing: Wanted = {
      subs: false,
      gifts: false,
      raids: false,
      bits: false,
      announcements: false,
      moderators: false,
    };
    expect(isActivity(msg('1', 'charity'), nothing)).toBe(true);
    expect(isActivity(msg('2', 'other'), nothing)).toBe(true);
  });

  it('takes a moderator talking only when asked', () => {
    const mod = msg('1', null, ['moderator']);
    expect(isActivity(mod, ALL)).toBe(true);
    expect(isActivity(mod, { ...ALL, moderators: false })).toBe(false);
  });

  /** The broadcaster moderates their own channel, with their own badge. */
  it('counts the broadcaster as a moderator', () => {
    expect(isActivity(msg('1', null, ['broadcaster']), ALL)).toBe(true);
    expect(isActivity(msg('2', null, ['subscriber']), ALL)).toBe(false);
  });
});

describe('activity', () => {
  it('keeps log order', () => {
    const log = [msg('1', 'sub'), msg('2', null), msg('3', 'raid')];
    expect(activity(log, ALL).map((m) => m.id)).toEqual(['1', '3']);
  });

  it('keeps the recent end when there is more than fits', () => {
    const log = Array.from({ length: 20 }, (_, i) => msg(`x${i}`, 'sub'));
    const out = activity(log, ALL, 5);
    expect(out).toHaveLength(5);
    expect(out.at(-1)?.id).toBe('x19');
  });
});

describe('wantedFrom', () => {
  /** Every field is optional on the wire; a missing one means the default. */
  it('fills in what the stored settings leave out', () => {
    expect(wantedFrom({})).toEqual({
      subs: true,
      gifts: true,
      raids: true,
      bits: true,
      announcements: true,
      moderators: false,
    });
    expect(wantedFrom({ raids: false, moderators: true })).toMatchObject({
      raids: false,
      moderators: true,
    });
  });
});
