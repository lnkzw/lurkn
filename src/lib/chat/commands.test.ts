import { describe, expect, it } from 'vitest';

import { titleCommands } from './commands';

describe('titleCommands', () => {
  it('finds nothing in an absent or plain title', () => {
    expect(titleCommands(null)).toEqual([]);
    expect(titleCommands(undefined)).toEqual([]);
    expect(titleCommands('')).toEqual([]);
    expect(titleCommands('WoW Forever, Beta Tag 4')).toEqual([]);
  });

  /** A real title, from the channel this was built against. */
  it('pulls the advertised commands out of a real title', () => {
    expect(titleCommands('[DROPS] WoW Forever (Beta Tag 4) | !rested !holy !ppm !albion #werbung')).toEqual([
      'rested',
      'holy',
      'ppm',
      'albion',
    ]);
  });

  it('keeps the order they are advertised in', () => {
    expect(titleCommands('!zulu then !alpha')).toEqual(['zulu', 'alpha']);
  });

  it('offers a repeated command once, in its first spelling', () => {
    expect(titleCommands('!Drops | rules: !drops !DROPS')).toEqual(['Drops']);
  });

  it('ignores an exclamation that is not a command', () => {
    // Mid-word, doubled for emphasis, and inside a URL path.
    expect(titleCommands('wow!!! amazing')).toEqual([]);
    expect(titleCommands('go to example.com/wow!now')).toEqual([]);
    expect(titleCommands('hype!!')).toEqual([]);
  });

  it('reads a command wedged against punctuation', () => {
    expect(titleCommands('drops (!drops), rules [!rules]')).toEqual(['drops', 'rules']);
  });

  it('takes hyphens and underscores', () => {
    expect(titleCommands('!sub-goal !ppm_2')).toEqual(['sub-goal', 'ppm_2']);
  });

  it('drops an absurdly long run rather than posting half of it', () => {
    expect(titleCommands('!ppm !' + 'a'.repeat(40))).toEqual(['ppm']);
  });

  it('caps a title stuffed with exclamations', () => {
    const many = Array.from({ length: 30 }, (_, i) => `!c${i}`).join(' ');
    expect(titleCommands(many)).toHaveLength(12);
  });
});
