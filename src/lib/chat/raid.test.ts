import { describe, expect, it } from 'vitest';

import type { ChannelRaided } from '$lib/ipc';

import { isActivity } from './activity';
import { raidNotice } from './raid';

const raid: ChannelRaided = {
  from: 'forsen',
  fromName: 'Forsen',
  to: {
    id: '33333',
    login: 'xqc',
    displayName: 'xQc',
    avatarUrl: null,
    live: true,
    category: null,
    title: null,
    viewers: null,
  },
  viewers: 42,
};

describe('raidNotice', () => {
  const now = new Date('2026-10-17T12:00:00Z');

  it('belongs to the channel raided, not the raider', () => {
    expect(raidNotice(raid, now).channelId).toBe('33333');
  });

  it('says whose raid was followed and how many came along', () => {
    const notice = raidNotice(raid, now);
    expect(notice.event?.text).toBe("Followed Forsen's raid with 42 viewers");
    expect(notice.event?.count).toBe(42);
  });

  it('is Lurkn talking, with nothing to render but the notice', () => {
    const notice = raidNotice(raid, now);
    expect(notice.kind).toBe('system');
    expect(notice.tokens).toEqual([]);
  });

  it('is filed with the raids in the activity feed', () => {
    const wanted = {
      subs: false,
      gifts: false,
      raids: true,
      bits: false,
      announcements: false,
      moderators: false,
    };
    expect(isActivity(raidNotice(raid, now), wanted)).toBe(true);
    expect(isActivity(raidNotice(raid, now), { ...wanted, raids: false })).toBe(false);
  });

  it('never collides with another raid followed later', () => {
    const later = new Date(now.getTime() + 1000);
    expect(raidNotice(raid, now).id).not.toBe(raidNotice(raid, later).id);
  });
});
