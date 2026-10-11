/**
 * The activity feed: the events worth a column beside the chat.
 *
 * A filter over the log rather than a second feed. Everything here already
 * arrives over the chat socket — subs, gifts, raids and announcements as
 * channel events, bits as a count on an ordinary message — so the feed cannot
 * disagree with the chat it sits next to, and costs nothing to keep in step.
 *
 * Pure, so the rules about what counts are testable without a DOM.
 */

/** Just enough of a message to decide, so tests need no fixtures. */
export interface Feedable {
  id: string;
  author: { badges: Array<{ setId: string }> };
  /**
   * Optional as well as nullable: the field carries `serde(default)`, so a
   * message from a build that predates channel events arrives without it.
   */
  event?: { kind: string; count: number | null } | null;
}

/** Which kinds the reader wants. Mirrors `ActivitySettings` in lurkn-core. */
export interface Wanted {
  subs: boolean;
  gifts: boolean;
  raids: boolean;
  bits: boolean;
  announcements: boolean;
  moderators: boolean;
}

/**
 * Whether a moderator said it.
 *
 * The broadcaster is a moderator of their own channel in every way that
 * matters, and Twitch gives them their own badge rather than the mod one, so
 * both count.
 */
function fromModerator(message: Feedable): boolean {
  return message.author.badges.some((b) => b.setId === 'moderator' || b.setId === 'broadcaster');
}

/** Whether one message belongs in the feed. */
export function isActivity(message: Feedable, wanted: Wanted): boolean {
  const kind = message.event?.kind;

  switch (kind) {
    case 'sub':
    case 'resub':
    case 'upgrade':
      return wanted.subs;
    case 'subGift':
    case 'communityGift':
      return wanted.gifts;
    case 'raid':
      return wanted.raids;
    case 'cheer':
      return wanted.bits;
    case 'announcement':
      return wanted.announcements;
    case 'charity':
    case 'bitsBadge':
    case 'other':
      // Rare, and never noise: a charity donation or a new notice type Twitch
      // has added is exactly what a feed is for.
      return true;
    default:
      // An ordinary message, which is only activity if a moderator sent it and
      // the reader asked for that.
      return wanted.moderators && fromModerator(message);
  }
}

/** The feed, oldest first as the log has them, newest last. */
export function activity<T extends Feedable>(messages: T[], wanted: Wanted, limit = 100): T[] {
  const picked = messages.filter((m) => isActivity(m, wanted));
  // The tail: a feed is about what just happened, and an old event scrolled
  // past is not worth the memory of keeping it.
  return picked.slice(-limit);
}

/**
 * What the reader asked for, from the stored settings. Spelled out rather
 * than passed through: every field on the wire is optional, and the rules
 * above should not each have to decide what a missing one means.
 */
export function wantedFrom(settings: Partial<Wanted>): Wanted {
  return {
    subs: settings.subs ?? true,
    gifts: settings.gifts ?? true,
    raids: settings.raids ?? true,
    bits: settings.bits ?? true,
    announcements: settings.announcements ?? true,
    moderators: settings.moderators ?? false,
  };
}
