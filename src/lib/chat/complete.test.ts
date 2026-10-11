import { describe, expect, it } from 'vitest';

import {
  activeToken,
  apply,
  builtinSlash,
  parseModCommand,
  parseWhisperCommand,
  rank,
  type Suggestion,
} from './complete';

/** Caret at the end of the draft, which is where typing leaves it. */
const at = (draft: string) => activeToken(draft, draft.length);

describe('activeToken', () => {
  it('finds nothing in plain text', () => {
    expect(at('hello there')).toBeNull();
    expect(at('')).toBeNull();
  });

  it('reads a mention, a title command and an emote', () => {
    expect(at('hi @for')?.kind).toBe('user');
    expect(at('!res')?.kind).toBe('command');
    expect(at('nice :pog')?.kind).toBe('emote');
  });

  it('keeps the sigil out of the query and lowercases it', () => {
    const token = at('@ForS');
    expect(token?.query).toBe('fors');
    expect(token?.text).toBe('@ForS');
    expect(token?.start).toBe(0);
  });

  /** `a:b`, an email address and a URL are not completion requests. */
  it('ignores a sigil wedged inside a word', () => {
    expect(at('key:value')).toBeNull();
    expect(at('me@example.com')).toBeNull();
    expect(at('http://x.com/y')).toBeNull();
  });

  it('treats a slash as a command only at the start of the message', () => {
    expect(at('/ba')?.kind).toBe('slash');
    expect(at('look /ba')).toBeNull();
  });

  /** A lone colon is punctuation far more often than the start of an emote. */
  it('waits for a letter before offering emotes', () => {
    expect(at('so :')).toBeNull();
    expect(at('so :p')?.kind).toBe('emote');
  });

  it('ignores a caret that has moved off the end of the token', () => {
    // Caret sits before the `f`, inside "@for" but not at its end.
    expect(activeToken('@for hi', 7)).toBeNull();
  });
});

describe('rank', () => {
  const names = ['Annabelle', 'Ann', 'McAnn', 'fors', 'Forsen'];
  const ids = (out: string[]) => out;

  it('puts prefix matches before matches in the middle', () => {
    expect(ids(rank(names, 'ann', (n) => n))).toEqual(['Ann', 'Annabelle', 'McAnn']);
  });

  it('prefers the shorter of two prefix matches', () => {
    expect(rank(names, 'fors', (n) => n)).toEqual(['fors', 'Forsen']);
  });

  it('ignores case on both sides', () => {
    expect(rank(['FORSEN'], 'for', (n) => n)).toEqual(['FORSEN']);
  });

  it('returns the head of the list when nothing has been typed', () => {
    expect(rank(names, '', (n) => n, 2)).toEqual(['Annabelle', 'Ann']);
  });

  it('honours the limit', () => {
    expect(rank(names, 'ann', (n) => n, 1)).toEqual(['Ann']);
  });
});

describe('apply', () => {
  const user: Suggestion = { kind: 'user', value: 'Forsen', label: 'Forsen' };
  const emote: Suggestion = { kind: 'emote', value: 'PogChamp', label: 'PogChamp' };

  it('replaces the token and leaves the caret after a trailing space', () => {
    const token = at('hi @for')!;
    const out = apply('hi @for', token, user);
    expect(out.draft).toBe('hi @Forsen ');
    expect(out.caret).toBe(out.draft.length);
  });

  it('keeps text after the caret', () => {
    const token = activeToken('hi @for you', 7)!;
    expect(apply('hi @for you', token, user).draft).toBe('hi @Forsen  you');
  });

  /** An emote's name stands alone: the colon was only the way in. */
  it('drops the sigil for an emote', () => {
    const token = at('nice :pog')!;
    expect(apply('nice :pog', token, emote).draft).toBe('nice PogChamp ');
  });
});

describe('parseWhisperCommand', () => {
  it('ignores anything that is not a whisper command', () => {
    expect(parseWhisperCommand('hello')).toBeNull();
    expect(parseWhisperCommand('/ban forsen')).toBeNull();
    expect(parseWhisperCommand('/whisperer forsen hi')).toBeNull();
  });

  it('reads the recipient and the message', () => {
    expect(parseWhisperCommand('/w forsen hello there')).toEqual({
      login: 'forsen',
      body: 'hello there',
    });
  });

  it('takes the long form and a leading @', () => {
    expect(parseWhisperCommand('/whisper @forsen hi')).toEqual({ login: 'forsen', body: 'hi' });
  });

  /** `/w name` alone names a recipient without saying anything yet. */
  it('accepts a recipient with no message', () => {
    expect(parseWhisperCommand('/w forsen')).toEqual({ login: 'forsen', body: '' });
  });

  it('keeps the message as typed, including its own slashes', () => {
    expect(parseWhisperCommand('/w forsen check /docs now')?.body).toBe('check /docs now');
  });
});

describe('parseModCommand', () => {
  it('ignores anything that is not one of ours', () => {
    expect(parseModCommand('hello')).toBeNull();
    expect(parseModCommand('/w forsen hi')).toBeNull();
    expect(parseModCommand('/nonsense')).toBeNull();
  });

  it('reads a timeout with its length and reason', () => {
    expect(parseModCommand('/timeout forsen 600 spam')).toEqual({
      name: 'timeout',
      target: 'forsen',
      amount: 600,
      rest: 'spam',
    });
  });

  /** A reason is prose; lowercasing the whole line would have eaten its capitals. */
  it('keeps the reason as typed', () => {
    expect(parseModCommand('/ban Forsen Being Rude')?.rest).toBe('Being Rude');
  });

  it('takes a leading @ and folds the login', () => {
    expect(parseModCommand('/ban @Forsen')?.target).toBe('forsen');
  });

  it('needs a user where one is required', () => {
    expect(parseModCommand('/ban')).toBeNull();
    expect(parseModCommand('/timeout')).toBeNull();
  });

  it('reads the commands that take nothing', () => {
    expect(parseModCommand('/clear')).toEqual({ name: 'clear' });
    expect(parseModCommand('/emoteonly')).toEqual({ name: 'emoteonly' });
    expect(parseModCommand('/subscribersoff')).toEqual({ name: 'subscribersoff' });
  });

  it('reads a room setting with its number', () => {
    expect(parseModCommand('/slow 30')).toEqual({ name: 'slow', amount: 30 });
    expect(parseModCommand('/followers 10')).toEqual({ name: 'followers', amount: 10 });
    // Twitch's own default when none is given.
    expect(parseModCommand('/slow')).toEqual({ name: 'slow' });
  });

  it('keeps an announcement whole', () => {
    expect(parseModCommand('/announce Stream starts at 8')?.rest).toBe('Stream starts at 8');
  });
});

describe('builtinSlash', () => {
  it('offers moderation only to a moderator', () => {
    expect(builtinSlash(false).map((c) => c.name)).toEqual(['w']);
    const names = builtinSlash(true).map((c) => c.name);
    expect(names).toContain('timeout');
    expect(names).toContain('ban');
    // Takes an id nobody can see; the menu does it instead.
    expect(names).not.toContain('delete');
  });

  it('completes a slash into the draft', () => {
    const token = at('/tim');
    expect(token?.kind).toBe('slash');
    if (!token) return;
    const next = apply('/tim', token, { kind: 'slash', value: 'timeout', label: '/timeout' });
    expect(next.draft).toBe('/timeout ');
  });
});
