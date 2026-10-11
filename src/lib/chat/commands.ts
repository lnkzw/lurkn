/**
 * Streamers advertise their chat commands in the stream title — "!rested !holy
 * !ppm" — which means the list of things worth typing is already on screen, one
 * scroll of an ellipsis away. This pulls them out so the composer can offer
 * them directly.
 */

/**
 * A command must start on a boundary that is not part of a word, a URL path or
 * another `!`: `foo!bar`, `example.com/a!b` and `!!` are not commands. Twitch
 * command names are ASCII, and the longest real ones sit well under 24.
 */
const COMMAND = /(?:^|[^\w!/])!([a-z0-9][a-z0-9_-]{0,23})\b/gi;

/**
 * Titles are a free-text field, so a title stuffed with exclamations must not
 * produce a popup taller than the window. Twelve is past any real title.
 */
const MAX = 12;

/**
 * The `!commands` a title advertises, in the order they appear, without their
 * leading `!`.
 *
 * Matching is case-insensitive and the first spelling wins: Twitch treats
 * `!Drops` and `!drops` as the same command, and showing both would offer the
 * reader a choice that makes no difference.
 */
export function titleCommands(title: string | null | undefined): string[] {
  if (!title) return [];

  const seen = new Set<string>();
  const out: string[] = [];

  for (const match of title.matchAll(COMMAND)) {
    // The group is not optional, but `noUncheckedIndexedAccess` cannot know it.
    const name = match[1] ?? '';
    const key = name.toLowerCase();
    if (seen.has(key)) continue;

    seen.add(key);
    out.push(name);
    if (out.length === MAX) break;
  }

  return out;
}
