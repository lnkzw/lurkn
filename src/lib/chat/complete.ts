/**
 * Autocomplete for the composer.
 *
 * All of it is pure: the caret position and the draft go in, a token and a
 * ranked list come out. That keeps the matching rules testable without a DOM,
 * and keeps the component down to drawing a list and moving a caret.
 */

/** What the caret is sitting in, if anything completable. */
export interface ActiveToken {
  kind: 'user' | 'command' | 'emote' | 'slash';
  /** The sigil plus what has been typed, e.g. `@for`. */
  text: string;
  /** What has been typed after the sigil, lowercased for matching. */
  query: string;
  start: number;
  end: number;
}

export interface Suggestion {
  kind: 'user' | 'command' | 'emote' | 'slash';
  /** What gets inserted, without its sigil. */
  value: string;
  /** What the row shows; differs from `value` for a display name. */
  label: string;
  /** A preview image, for emotes. */
  image?: string;
  /** Provider or other one-word detail, shown dimmed. */
  detail?: string;
}

const SIGILS: Record<string, ActiveToken['kind']> = {
  '@': 'user',
  '!': 'command',
  ':': 'emote',
  '/': 'slash',
};

/**
 * The completable token the caret is inside, or null.
 *
 * Only a token the caret is at the end of counts: completing something the
 * reader has already moved past is never what they meant.
 */
export function activeToken(draft: string, caret: number): ActiveToken | null {
  if (caret < 1 || caret > draft.length) return null;

  // Walk back to the start of the run the caret is in.
  let start = caret;
  while (start > 0 && !/\s/.test(draft[start - 1] ?? '')) start -= 1;

  const text = draft.slice(start, caret);
  if (text.length === 0) return null;

  const kind = SIGILS[text[0] ?? ''];
  if (!kind) return null;

  // A sigil in the middle of a word is not a sigil: `a:b` and an email address
  // are not completion requests.
  if (start > 0 && !/\s/.test(draft[start - 1] ?? '')) return null;

  // A slash only means a command at the very start of the message, the way
  // Twitch reads it.
  if (kind === 'slash' && start !== 0) return null;

  const query = text.slice(1);

  // Emotes need a letter before they are worth suggesting: a lone `:` is far
  // more often punctuation than the start of an emote.
  if (kind === 'emote' && query.length === 0) return null;

  return { kind, text, query: query.toLowerCase(), start, end: caret };
}

/**
 * Rank candidates against a query.
 *
 * A prefix match always beats a match in the middle, which is what makes typing
 * the first letters of a name feel direct; within each group the shorter
 * candidate wins, so `@ann` offers Ann before Annabelle. Matching ignores case
 * throughout — nobody types a name's capitalisation to complete it.
 */
export function rank<T>(items: T[], query: string, key: (item: T) => string, limit = 8): T[] {
  if (query.length === 0) return items.slice(0, limit);

  const prefix: T[] = [];
  const contains: T[] = [];

  for (const item of items) {
    const name = key(item).toLowerCase();
    if (name.startsWith(query)) prefix.push(item);
    else if (name.includes(query)) contains.push(item);
  }

  const byLength = (a: T, b: T) => key(a).length - key(b).length;
  prefix.sort(byLength);
  contains.sort(byLength);

  return [...prefix, ...contains].slice(0, limit);
}

/**
 * Put a suggestion into the draft, and say where the caret lands.
 *
 * A trailing space is added because the next thing typed is always a new word;
 * an emote keeps its closing `:` for the same reason Twitch and Discord do,
 * even though Lurkn matches on the name alone.
 */
export function apply(
  draft: string,
  token: ActiveToken,
  suggestion: Suggestion,
): { draft: string; caret: number } {
  const sigil = token.text[0] ?? '';
  const inserted = suggestion.kind === 'emote' ? `${suggestion.value} ` : `${sigil}${suggestion.value} `;

  return {
    draft: draft.slice(0, token.start) + inserted + draft.slice(token.end),
    caret: token.start + inserted.length,
  };
}

/**
 * `/w <name> <message>` typed in the chat composer.
 *
 * Handled by Lurkn rather than sent: Twitch's chat API strips slash commands
 * out of a message or drops the message entirely, so `/w` has not reached
 * anyone through an app since IRC commands were removed. Routing it to the
 * whisper endpoint is what people mean when they type it.
 *
 * Returns null for anything that is not a whisper command.
 */
export function parseWhisperCommand(draft: string): { login: string; body: string } | null {
  const match = /^\/(?:w|whisper)\s+@?([a-zA-Z0-9_]{1,25})(?:\s+([\s\S]*))?$/.exec(draft.trim());
  if (!match) return null;

  const login = match[1] ?? '';
  return { login, body: (match[2] ?? '').trim() };
}

/** A moderation command typed in the composer. */
export interface ModCommand {
  name: 'ban' | 'timeout' | 'unban' | 'delete' | 'clear' | 'announce' | 'slow' | 'slowoff' | 'followers' | 'followersoff' | 'emoteonly' | 'emoteonlyoff' | 'subscribers' | 'subscribersoff' | 'uniquechat' | 'uniquechatoff';
  /** The login named, where the command takes one. */
  target?: string;
  /** Seconds or minutes, where the command takes a number. */
  amount?: number;
  /** Whatever follows: a ban reason, or the announcement itself. */
  rest?: string;
}

/** The commands Lurkn carries out itself, and what each expects. */
const MOD_COMMANDS: Record<string, { user?: boolean; amount?: boolean; rest?: boolean }> = {
  ban: { user: true, rest: true },
  unban: { user: true },
  timeout: { user: true, amount: true, rest: true },
  delete: {},
  clear: {},
  announce: { rest: true },
  slow: { amount: true },
  slowoff: {},
  followers: { amount: true },
  followersoff: {},
  emoteonly: {},
  emoteonlyoff: {},
  subscribers: {},
  subscribersoff: {},
  uniquechat: {},
  uniquechatoff: {},
};

/**
 * A moderation command typed in the composer, or null.
 *
 * These are carried out by Lurkn against the moderation endpoints rather than
 * sent as text — Twitch's chat API strips slash commands, so typing `/ban`
 * into any app has done nothing at all since IRC commands were removed. The
 * names and argument order are Twitch's own, because that is what a moderator
 * already has in their fingers.
 */
export function parseModCommand(draft: string): ModCommand | null {
  const match = /^\/([a-z]+)(?:\s+([\s\S]*))?$/.exec(draft.trim().toLowerCase());
  if (!match) return null;

  const name = match[1] ?? '';
  const shape = MOD_COMMANDS[name];
  if (!shape) return null;

  // Re-split the untouched text: a ban reason and an announcement keep their
  // capitals, which lowercasing the whole line would have eaten.
  const raw = /^\/[a-z]+(?:\s+([\s\S]*))?$/.exec(draft.trim());
  let rest = (raw?.[1] ?? '').trim();

  const command: ModCommand = { name: name as ModCommand['name'] };

  if (shape.user) {
    const [first, ...tail] = rest.split(/\s+/);
    if (!first) return null;
    command.target = first.replace(/^@/, '').toLowerCase();
    rest = tail.join(' ');
  }

  if (shape.amount) {
    const [first, ...tail] = rest.split(/\s+/);
    const amount = Number.parseInt(first ?? '', 10);
    if (Number.isFinite(amount) && amount > 0) {
      command.amount = amount;
      rest = tail.join(' ');
    }
  }

  if (shape.rest && rest.length > 0) command.rest = rest;
  return command;
}

/**
 * The slash commands Lurkn carries out itself, for the suggestion list.
 *
 * `/delete` is left out: it takes a message id nobody can see, and the
 * right-click menu is how a message is picked.
 */
export function builtinSlash(canModerate: boolean): Array<{ name: string; detail: string }> {
  const out = [{ name: 'w', detail: 'whisper' }];
  if (canModerate) {
    for (const name of Object.keys(MOD_COMMANDS)) {
      if (name !== 'delete') out.push({ name, detail: 'moderator' });
    }
  }
  return out;
}

/**
 * Why a slash command gets no suggestions.
 *
 * Twitch's chat API strips slash commands out of a message or drops the message
 * entirely, and rejects `/me` outright; the moderation commands live behind
 * their own endpoints and scopes. Offering a list of them would be offering a
 * list of things that silently fail, so the composer explains instead.
 */
export const SLASH_NOTE =
  'Twitch drops slash commands sent by apps, so Lurkn carries them out itself: /w whispers, and /ban /timeout /delete /clear /announce /slow /followers moderate, where you hold the role.';
