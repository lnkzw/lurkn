import { describe, expect, it } from 'vitest';

import { applyThemeVars } from './settings.svelte';

/** Enough of CSSStyleDeclaration to watch what a theme sets. */
function fakeStyle() {
  const props = new Map<string, string>();
  return {
    props,
    setProperty: (name: string, value: string | null) => void props.set(name, value ?? ''),
    removeProperty: (name: string) => {
      const old = props.get(name) ?? '';
      props.delete(name);
      return old;
    },
  };
}

describe('applyThemeVars', () => {
  it('sets each token and reports what it set', () => {
    const style = fakeStyle();
    const set = applyThemeVars(style, [], [
      { name: '--bg', value: '#282828' },
      { name: '--accent', value: '#d3869b' },
    ]);

    expect(style.props.get('--bg')).toBe('#282828');
    expect(set).toEqual(['--bg', '--accent']);
  });

  it('clears what the previous theme set and this one does not', () => {
    const style = fakeStyle();
    const first = applyThemeVars(style, [], [
      { name: '--bg', value: '#000' },
      { name: '--accent', value: '#f00' },
    ]);
    applyThemeVars(style, first, [{ name: '--accent', value: '#0f0' }]);

    expect(style.props.has('--bg')).toBe(false);
    expect(style.props.get('--accent')).toBe('#0f0');
  });

  it('going back to the built-in palette clears everything', () => {
    const style = fakeStyle();
    const first = applyThemeVars(style, [], [{ name: '--bg', value: '#000' }]);
    expect(applyThemeVars(style, first, [])).toEqual([]);
    expect(style.props.size).toBe(0);
  });

  it('never touches an ordinary property', () => {
    const style = fakeStyle();
    applyThemeVars(style, [], [
      { name: 'background-image', value: 'url(https://example.com)' },
      { name: '--ok', value: '#0f0' },
    ]);
    expect([...style.props.keys()]).toEqual(['--ok']);
  });
});
