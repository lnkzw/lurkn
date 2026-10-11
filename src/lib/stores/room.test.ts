import { describe, expect, it } from 'vitest';

import { roomLabels } from './chat.svelte';

const none = {
  emoteOnly: false,
  subscriberOnly: false,
  uniqueOnly: false,
  followerOnlyMinutes: null,
  slowModeSeconds: null,
};

describe('roomLabels', () => {
  it('says nothing about an unrestricted room', () => {
    expect(roomLabels(none)).toEqual([]);
  });

  /** Must match RoomState::labels in lurkn-core. */
  it('reads followers-only naturally at every duration', () => {
    expect(roomLabels({ ...none, followerOnlyMinutes: 0 })).toEqual(['Followers only']);
    expect(roomLabels({ ...none, followerOnlyMinutes: 10 })).toEqual(['Followers 10m+']);
    expect(roomLabels({ ...none, followerOnlyMinutes: 60 })).toEqual(['Followers 1h+']);
    expect(roomLabels({ ...none, followerOnlyMinutes: 1440 })).toEqual(['Followers 24h+']);
    expect(roomLabels({ ...none, followerOnlyMinutes: 90 })).toEqual(['Followers 90m+']);
  });

  it('lists every restriction in reading order', () => {
    expect(
      roomLabels({
        emoteOnly: true,
        subscriberOnly: true,
        uniqueOnly: true,
        followerOnlyMinutes: 30,
        slowModeSeconds: 5,
      }),
    ).toEqual(['Emote only', 'Subs only', 'Followers 30m+', 'Slow 5s', 'Unique only']);
  });

  it('reports slow mode on its own', () => {
    expect(roomLabels({ ...none, slowModeSeconds: 30 })).toEqual(['Slow 30s']);
  });
});
