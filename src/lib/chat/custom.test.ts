import { describe, expect, it } from 'vitest';

import { expand, findCommand } from './custom';

const COMMANDS = [
  { trigger: '/hi', template: 'Hello {1}, welcome to {channel.name}!' },
  { trigger: '!so', template: '!shoutout {1}' },
  { trigger: '', template: 'never matches' },
];

describe('findCommand', () => {
  it('matches the first word, without case', () => {
    expect(findCommand('/HI forsen', COMMANDS)?.command.trigger).toBe('/hi');
    expect(findCommand('  /hi   forsen  now ', COMMANDS)?.args).toEqual(['forsen', 'now']);
  });

  it('needs the whole word', () => {
    expect(findCommand('/hiya', COMMANDS)).toBeNull();
    expect(findCommand('say /hi', COMMANDS)).toBeNull();
  });

  /** A blank row in settings must not turn every message into a command. */
  it('ignores a command with no trigger', () => {
    expect(findCommand('never', COMMANDS)).toBeNull();
    expect(findCommand('', COMMANDS)).toBeNull();
  });
});

describe('expand', () => {
  const vars = { 'channel.name': 'forsen', 'my.name': 'lurker', 'user.name': 'nymn' };

  it('fills words and variables', () => {
    expect(expand('Hello {1}, welcome to {channel.name}!', ['xqc'], vars)).toBe(
      'Hello xqc, welcome to forsen!',
    );
  });

  it('takes the rest of the line with {n+}', () => {
    expect(expand('/timeout {1} 600 {2+}', ['spammer', 'too', 'many', 'links'], vars)).toBe(
      '/timeout spammer 600 too many links',
    );
  });

  it('leaves a missing word empty', () => {
    expect(expand('{1}-{2}', ['a'], vars)).toBe('a-');
  });

  /** A template that takes no words passes them on. */
  it('appends the words when the template names none', () => {
    expect(expand('!drops', ['now'], vars)).toBe('!drops now');
    expect(expand('!drops', [], vars)).toBe('!drops');
  });

  it('keeps an unknown variable as written, and a doubled brace as one', () => {
    expect(expand('{nope.nothing} {{1}', [], vars)).toBe('{nope.nothing} {1}');
  });

  it('empties a known variable that has no value here', () => {
    expect(expand('[{msg.text}]', [], vars)).toBe('[]');
  });

  it('reads the right-clicked message', () => {
    expect(expand('@{user.name} hi', [], vars)).toBe('@nymn hi');
  });
});
