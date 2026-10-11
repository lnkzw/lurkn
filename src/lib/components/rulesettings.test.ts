import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render } from '@testing-library/svelte';

// The settings store talks to Rust; here it only has to hold values.
vi.mock('$lib/ipc', () => ({
  commands: {
    checkPattern: vi.fn(async (pattern: string, regex: boolean) =>
      regex && pattern.includes('(') && !pattern.includes(')')
        ? { status: 'error', error: 'unclosed group' }
        : { status: 'ok', data: null },
    ),
    settingsSet: vi.fn(async (next: unknown) => ({ status: 'ok', data: next })),
  },
  events: {},
}));

import RuleSettings from './RuleSettings.svelte';
import { BLANK, settings } from '$lib/stores/settings.svelte';

afterEach(() => cleanup());

describe('RuleSettings', () => {
  it('draws every page', () => {
    for (const section of ['highlights', 'ignores', 'filters', 'commands'] as const) {
      cleanup();
      const { getAllByRole } = render(RuleSettings, { section });
      expect(getAllByRole('button').length, section).toBeGreaterThan(0);
    }
  });

  it('says where a filter is broken', () => {
    settings.current = {
      ...settings.current,
      filters: [{ ...BLANK.filter(), name: 'mods', expression: 'author.nam == "x"' }],
    };
    const { getByText } = render(RuleSettings, { section: 'filters' });
    expect(getByText(/Lurkn knows no "author.nam"/)).toBeTruthy();
  });

  it('says when a saved regex does not compile', async () => {
    settings.current = {
      ...settings.current,
      highlights: {
        ...settings.current.highlights,
        phrases: [{ ...BLANK.phrase(), pattern: '(unclosed', regex: true }],
      },
    };
    const { findByText } = render(RuleSettings, { section: 'highlights' });
    expect(await findByText('unclosed group')).toBeTruthy();
  });

  it('adds a blank row', async () => {
    settings.current = { ...settings.current, commands: [] };
    const { getByText, findAllByLabelText } = render(RuleSettings, { section: 'commands' });
    getByText('+ Add command').click();
    expect((await findAllByLabelText('Trigger')).length).toBe(1);
  });
});
