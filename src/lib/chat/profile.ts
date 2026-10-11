/** Presentation helpers for the user card. Pure, so they are testable without a DOM. */

/** How long an account has existed, in the words a card has room for. */
export function accountAge(createdAt: string | null, now: Date = new Date()): string | null {
  if (!createdAt) return null;

  const made = new Date(createdAt);
  if (Number.isNaN(made.getTime())) return null;

  const days = Math.floor((now.getTime() - made.getTime()) / 86_400_000);
  // A clock skewed behind Twitch's would otherwise render "-0 days".
  if (days < 1) return 'today';
  if (days < 61) return `${days} days`;

  const months = Math.floor(days / 30.44);
  if (months < 24) return `${months} months`;

  return `${Math.floor(days / 365.25)} years`;
}

/** The date itself, spelled out rather than localised into ambiguity. */
export function accountDate(createdAt: string | null): string | null {
  if (!createdAt) return null;

  const made = new Date(createdAt);
  if (Number.isNaN(made.getTime())) return null;

  return made.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

/**
 * The label for Twitch's `broadcaster_type`, or null for an ordinary account —
 * which is most of chat, and needs no badge saying so.
 */
export function broadcasterLabel(type: string): string | null {
  switch (type) {
    case 'partner':
      return 'Partner';
    case 'affiliate':
      return 'Affiliate';
    default:
      return null;
  }
}

/**
 * How many months this account has been subscribed to the channel the message
 * was sent in, or null when they are not subscribed.
 *
 * Read off the `subscriber` badge that came with the message, because that is
 * the only place Twitch publishes the number: `helix/subscriptions` is
 * broadcaster-only and carries a tier, not a duration.
 *
 * `founder` is the badge a channel's first subscribers keep instead of the
 * subscriber one, and it carries the same count.
 */
export function subscriptionMonths(badges: Array<{ setId: string; info: string }>): number | null {
  const badge = badges.find((b) => b.setId === 'subscriber' || b.setId === 'founder');
  if (!badge) return null;

  const months = Number.parseInt(badge.info, 10);
  // A subscriber badge with no count is a real case in the first month, and it
  // still says they are subscribed — which the caller reports as 0.
  return Number.isFinite(months) && months >= 0 ? months : 0;
}

/** Whether the account holds the founder badge for this channel. */
export function isFounder(badges: Array<{ setId: string }>): boolean {
  return badges.some((b) => b.setId === 'founder');
}

/** A subscription length in the words the card has room for. */
export function subscriptionLength(months: number): string {
  if (months < 1) return 'less than a month';
  if (months < 24) return `${months} month${months === 1 ? '' : 's'}`;

  const years = Math.floor(months / 12);
  const rest = months % 12;
  return rest === 0 ? `${years} years` : `${years} years, ${rest} month${rest === 1 ? '' : 's'}`;
}
