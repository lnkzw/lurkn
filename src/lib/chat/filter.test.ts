import { describe, expect, it } from 'vitest';

import type { ChatMessage } from '$lib/ipc';

import { contextOf, parseFilter, passes, type Context } from './filter';

function message(overrides: Partial<ChatMessage> = {}): ChatMessage {
  return {
    id: 'm1',
    channelId: '1',
    author: {
      id: '2',
      login: 'forsen',
      displayName: 'Forsen',
      color: { r: 255, g: 0, b: 0 },
      badges: [
        {
          setId: 'subscriber',
          version: '12',
          title: 'Subscriber',
          info: '14',
          url1x: null,
          url2x: null,
          url4x: null,
        },
      ],
    },
    tokens: [],
    rawText: 'hello chat',
    sentAt: '2026-10-05T12:00:00Z',
    kind: 'chat',
    state: { state: 'sent' },
    replyTo: null,
    event: null,
    highlighted: false,
    highlightColor: null,
    ...overrides,
  };
}

const ctx = (overrides: Partial<ChatMessage> = {}): Context =>
  contextOf(message(overrides), { login: 'forsen', live: true });

/** Parse and run in one, failing loudly if the filter does not parse. */
function run(source: string, context: Context = ctx()): boolean {
  const parsed = parseFilter(source);
  if (!parsed.ok) throw new Error(`${source}: ${parsed.error} at ${parsed.at}`);
  return passes(parsed.expr, context);
}

describe('parseFilter', () => {
  /** The forms the settings screen documents, all of which have to parse. */
  it('reads the documented forms', () => {
    for (const source of [
      'author.badges contains "moderator"',
      'author.subbed && author.sub_length >= 12',
      '!flags.system_message',
      'message.content match r"^!\\w+"',
      'message.content match ri"forsen"',
      'channel.name == "forsen" || (flags.highlighted && !flags.action)',
      'author.badges contains "vip" || author.badges contains "moderator"',
      '{ "a", "b" } contains "a"',
      'message.length > 10 * 2 - 1',
    ]) {
      expect(parseFilter(source).ok, source).toBe(true);
    }
  });

  it('names a variable it does not know, and where', () => {
    const parsed = parseFilter('author.nam == "x"');
    expect(parsed.ok).toBe(false);
    if (!parsed.ok) {
      expect(parsed.error).toContain('author.nam');
      expect(parsed.at).toBe(0);
    }
  });

  it('says what is wrong with broken syntax', () => {
    for (const source of [
      '(flags.action',
      'flags.action &&',
      'message.content == "open',
      'message.content match r"(unclosed"',
      'flags.action flags.reply',
      '1 == 1 == 1',
      '#',
      '',
    ]) {
      const parsed = parseFilter(source);
      expect(parsed.ok, source).toBe(false);
      if (!parsed.ok) expect(parsed.error.length, source).toBeGreaterThan(0);
    }
  });
});

describe('evaluating', () => {
  /** Nobody filtering for "moderator" means to miss "Moderator". */
  it('compares strings without case', () => {
    expect(run('author.name == "forsen"')).toBe(true);
    expect(run('message.content contains "CHAT"')).toBe(true);
    expect(run('message.content startswith "Hello"')).toBe(true);
    expect(run('message.content endswith "CHAT"')).toBe(true);
  });

  it('asks a list whether it holds something', () => {
    expect(run('author.badges contains "Subscriber"')).toBe(true);
    expect(run('author.badges contains "moderator"')).toBe(false);
  });

  it('runs regexes, case-sensitive unless ri', () => {
    expect(run('message.content match r"^hello"')).toBe(true);
    expect(run('message.content match r"^HELLO"')).toBe(false);
    expect(run('message.content match ri"^HELLO"')).toBe(true);
  });

  it('reads sub months off the badge', () => {
    expect(run('author.subbed && author.sub_length >= 12')).toBe(true);
    expect(run('author.sub_length > 14')).toBe(false);
  });

  it('honours precedence: && binds tighter than ||', () => {
    // false || (true && true)
    expect(run('flags.action || author.subbed && channel.live')).toBe(true);
    // (false || true) && false
    expect(run('(flags.action || author.subbed) && flags.reply')).toBe(false);
  });

  it('negates', () => {
    expect(run('!flags.action')).toBe(true);
    expect(run('!(message.content contains "hello")')).toBe(false);
  });

  it('does arithmetic', () => {
    expect(run('message.length == 5 * 2')).toBe(true);
    expect(run('message.length % 3 == 1')).toBe(true);
    expect(run('-1 < 0')).toBe(true);
    // A typo, not a reason to hide the whole chat.
    expect(run('message.length / 0 == 0')).toBe(true);
  });

  it('sees channel events', () => {
    const sub = ctx({ event: { kind: 'resub', text: 'x resubscribed', count: 3 } });
    expect(run('flags.sub_message', sub)).toBe(true);
    expect(run('flags.system_message', sub)).toBe(true);

    const cheer = ctx({ event: { kind: 'cheer', text: '', count: 100 } });
    expect(run('flags.cheer_message', cheer)).toBe(true);
    // Bits ride on an ordinary message; it is still a chatter talking.
    expect(run('flags.system_message', cheer)).toBe(false);
  });

  it('sees replies, deletions and highlights', () => {
    expect(run('flags.reply', ctx({ replyTo: 'm0' }))).toBe(true);
    expect(run('flags.deleted', ctx({ state: { state: 'deleted' } }))).toBe(true);
    expect(run('flags.highlighted', ctx({ highlighted: true }))).toBe(true);
    expect(run('author.no_color', ctx())).toBe(false);
  });

  /** Hostile chat text is data to a filter, never syntax. */
  it('treats message text as data', () => {
    const hostile = ctx({ rawText: '" || true || "' });
    expect(run('message.content == "hello"', hostile)).toBe(false);
  });
});
