import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/svelte';

const status = vi.hoisted(() => ({
  current: {
    url: 'http://127.0.0.1:7373/' as string | null,
    error: null as string | null,
    listeners: 1,
    channel: 'lurker' as string | null,
  },
}));

vi.mock('$lib/ipc', () => ({
  commands: {
    overlayStatus: vi.fn(async () => ({ status: 'ok', data: status.current })),
    overlayTest: vi.fn(async () => ({ status: 'ok', data: 1 })),
    overlayPreview: vi.fn(async () => ({ status: 'ok', data: null })),
    settingsSet: vi.fn(async (next: unknown) => ({ status: 'ok', data: next })),
  },
  events: {},
}));

import OverlaySettings from './OverlaySettings.svelte';
import { settings } from '$lib/stores/settings.svelte';

afterEach(() => cleanup());

function enable(on: boolean) {
  settings.current = { ...settings.current, overlay: { ...settings.current.overlay, enabled: on } };
}

describe('OverlaySettings', () => {
  it('keeps everything behind the switch until it is on', () => {
    enable(false);
    const { queryByLabelText, getByText } = render(OverlaySettings, { fonts: [] });
    expect(getByText('Serve my chat for OBS')).toBeTruthy();
    expect(queryByLabelText('Overlay address')).toBeNull();
  });

  it('shows the address OBS needs and whether anything is connected', async () => {
    enable(true);
    const { getByLabelText, getByText } = render(OverlaySettings, { fonts: [] });
    await waitFor(() =>
      expect((getByLabelText('Overlay address') as HTMLInputElement).value).toBe(
        'http://127.0.0.1:7373/',
      ),
    );
    expect(getByText(/One page connected/)).toBeTruthy();
    expect(getByText('lurker')).toBeTruthy();
  });

  it('can follow the open channel instead of your own', async () => {
    enable(true);
    const { getByText } = render(OverlaySettings, { fonts: [] });
    expect(settings.current.overlay.source).toBe('own');
    await fireEvent.click(getByText('Open channel'));
    expect(settings.current.overlay.source).toBe('open');
  });

  it('says why it is not running', async () => {
    enable(true);
    status.current = { ...status.current, url: null, error: 'Port 7373 is already in use' };
    const { findByText } = render(OverlaySettings, { fonts: [] });
    expect(await findByText(/already in use/)).toBeTruthy();
    status.current = { ...status.current, url: 'http://127.0.0.1:7373/', error: null };
  });

  it('bubble settings go away with the bubbles', async () => {
    enable(true);
    const { getByText, queryByLabelText } = render(OverlaySettings, { fonts: [] });
    expect(queryByLabelText('Bubble opacity')).toBeTruthy();
    await fireEvent.click(getByText('Plain'));
    expect(settings.current.overlay.layout).toBe('plain');
    expect(queryByLabelText('Bubble opacity')).toBeNull();
  });

  it('adds a hidden chatter as a login', async () => {
    enable(true);
    const { getByPlaceholderText } = render(OverlaySettings, { fonts: [] });
    const input = getByPlaceholderText('add a login') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: '@SomeBot' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(settings.current.overlay.hiddenUsers).toContain('somebot');
  });
});
