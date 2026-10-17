import type { ChannelRaided, ChatMessage } from '$lib/ipc';

/**
 * The line that opens a raided channel's log, saying why Lurkn is there.
 *
 * Lurkn's own words, so `system` rather than the `highlight` Twitch's channel
 * events carry; shaped as a raid, so it draws like one and the activity feed
 * files it with the raids. No tokens, so the row is the notice alone.
 */
export function raidNotice(raid: ChannelRaided, now: Date = new Date()): ChatMessage {
  return {
    id: `lurkn-raid-${raid.from}-${now.getTime()}`,
    channelId: raid.to.id,
    author: { id: '', login: raid.from, displayName: raid.fromName, color: null, badges: [] },
    tokens: [],
    rawText: '',
    sentAt: now.toISOString(),
    kind: 'system',
    state: { state: 'sent' },
    replyTo: null,
    event: {
      kind: 'raid',
      text: `Followed ${raid.fromName}'s raid with ${raid.viewers.toLocaleString()} viewers`,
      count: raid.viewers,
    },
    highlighted: false,
    highlightColor: null,
  };
}
