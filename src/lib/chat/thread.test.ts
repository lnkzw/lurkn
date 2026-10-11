import { describe, expect, it } from 'vitest';

import { conversation } from './thread';

const m = (id: string, author: string, replyTo: string | null = null) => ({
  id,
  author: { id: author },
  replyTo,
});

describe('conversation', () => {
  const log = [
    m('1', 'other'),
    m('2', 'me'),
    m('3', 'other', '2'),
    m('4', 'elsewhere'),
    m('5', 'me', '4'),
    m('6', 'unrelated'),
  ];

  it('keeps what the user said', () => {
    expect(conversation(log, 'me').map((x) => x.id)).toContain('2');
    expect(conversation(log, 'me').map((x) => x.id)).toContain('5');
  });

  it('pulls in what replied to them', () => {
    expect(conversation(log, 'me').map((x) => x.id)).toContain('3');
  });

  it('pulls in what they were replying to', () => {
    expect(conversation(log, 'me').map((x) => x.id)).toContain('4');
  });

  it('leaves out messages with nothing to do with them', () => {
    const ids = conversation(log, 'me').map((x) => x.id);
    expect(ids).not.toContain('1');
    expect(ids).not.toContain('6');
  });

  it('keeps the log order rather than sorting', () => {
    expect(conversation(log, 'me').map((x) => x.id)).toEqual(['2', '3', '4', '5']);
  });

  it('has nothing to show for someone who never spoke', () => {
    expect(conversation(log, 'ghost')).toEqual([]);
  });

  it('shows the most recent exchange when the log is long', () => {
    const many = Array.from({ length: 40 }, (_, i) => m(`x${i}`, 'me'));
    const out = conversation(many, 'me', 5);
    expect(out).toHaveLength(5);
    expect(out.at(-1)?.id).toBe('x39');
  });
});
