import { describe, expect, it } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import InfoHint from './InfoHint.svelte';
import { createRawSnippet } from 'svelte';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const text = (value: string) =>
  createRawSnippet(() => ({ render: () => `<span>${value}</span>` }));

function mount(body: string) {
  cleanup();
  return render(InfoHint, { label: 'the log', children: text(body) });
}

describe('InfoHint', () => {
  it('shows only the button until asked', () => {
    const { getByRole, queryByRole } = mount('Credentials are removed.');
    expect(getByRole('button', { name: 'About the log' })).toBeTruthy();
    expect(queryByRole('tooltip')).toBeNull();
  });

  it('reveals the text on hover', async () => {
    const { getByRole, findByRole } = mount('Credentials are removed.');
    getByRole('button').dispatchEvent(new MouseEvent('mouseenter'));
    expect((await findByRole('tooltip')).textContent).toContain('Credentials are removed.');
  });

  it('keeps the text up after a click, so a tap does not lose it', async () => {
    const { getByRole, findByRole, queryByRole } = mount('Credentials are removed.');
    const button = getByRole('button');

    button.click();
    button.dispatchEvent(new MouseEvent('mouseleave'));
    expect(await findByRole('tooltip')).toBeTruthy();

    button.click();
    await Promise.resolve();
    expect(queryByRole('tooltip')).toBeNull();
  });
});

/**
 * jsdom reports every rect as zero, so placement itself cannot be exercised
 * here. What can be pinned is the rule the bug came down to: an absolutely
 * positioned bubble is clipped by the settings body, which scrolls.
 */
describe('InfoHint placement', () => {
  const css = readFileSync(join(process.cwd(), 'src/lib/components/InfoHint.svelte'), 'utf8');

  it('escapes the scrolling settings body', () => {
    expect(css).toMatch(/\.bubble\s*\{[^}]*position:\s*fixed/s);
    expect(css).not.toMatch(/\.bubble\s*\{[^}]*position:\s*absolute/s);
  });

  it('stays hidden until it has been measured', () => {
    expect(css).toMatch(/\.bubble\s*\{[^}]*visibility:\s*hidden/s);
    expect(css).toMatch(/\.bubble\.placed\s*\{[^}]*visibility:\s*visible/s);
  });
});
