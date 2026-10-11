import { describe, expect, it } from 'vitest';

import type { ChatMessage } from '$lib/ipc';
import { unseen } from './chat.svelte';

const line = (id: string) => ({ id }) as ChatMessage;

describe('unseen', () => {
  it('passes new lines through in order', () => {
    expect(unseen([line('a')], [line('b'), line('c')]).map((m) => m.id)).toEqual(['b', 'c']);
  });

  /** The backfill landed first; the socket then delivers its last line again. */
  it('drops a live line the backfill already showed', () => {
    const log = [line('h1'), line('h2'), line('h3')];
    expect(unseen(log, [line('h3'), line('live')]).map((m) => m.id)).toEqual(['live']);
  });

  /** The socket landed first; the backfill then includes what it showed. */
  it('drops backfilled lines the socket already showed', () => {
    const log = [line('live1'), line('live2')];
    expect(unseen(log, [line('old'), line('live1')]).map((m) => m.id)).toEqual(['old']);
  });

  it('keeps an empty batch empty', () => {
    expect(unseen([line('a')], [])).toEqual([]);
  });
});
