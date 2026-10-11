/**
 * How the chat beside the player in the synced window should look: Lurkn's
 * chat, as it is painted right now.
 *
 * Read off the root element's computed style rather than worked out from the
 * settings, so a theme file, the system preference and anything else that
 * reaches the palette all count without a second copy of it. Pure apart from
 * that one read, which is passed in.
 */

import type { WatchLook } from '$lib/ipc';
import { MAX_FONT_SIZE, MIN_FONT_SIZE } from '$lib/constants';
import { chatFontStack, emoteScale, type ResolvedSettings } from '$lib/stores/settings.svelte';

type Computed = Pick<CSSStyleDeclaration, 'getPropertyValue'>;

export function watchLook(
  computed: Computed,
  settings: ResolvedSettings,
  scheme: 'dark' | 'light',
): WatchLook {
  const read = (name: string) => computed.getPropertyValue(name).trim();
  const chat = settings.chat;

  // The page has no --font or --mono of its own, so the stack goes over with
  // both already filled in.
  const font = chatFontStack(chat.font, chat.customFont)
    .replace('var(--font)', read('--font') || 'system-ui, sans-serif')
    .replace('var(--mono)', read('--mono') || 'monospace');

  return {
    scheme,
    background: read('--bg'),
    surface: read('--bg-raised'),
    border: read('--border'),
    text: read('--text'),
    textDim: read('--text-dim'),
    accent: read('--accent'),
    highlight: read('--highlight'),
    ok: read('--ok'),
    warn: read('--warn'),
    font,
    // The fallbacks are Rust's defaults; what it sends back is always complete.
    fontSize: chat.fontSize ?? DEFAULT_FONT_SIZE,
    timestamps: chat.showTimestamps ?? true,
    badges: chat.showBadges ?? true,
    emoteScale: emoteScale(chat.emoteSize),
    alternate: chat.alternateBackgrounds ?? false,
    channelEvents: chat.showChannelEvents ?? true,
    replies: chat.showReplyPreview ?? true,
  };
}

/** Mirrors `ChatSettings::default` in lurkn-core. */
const DEFAULT_FONT_SIZE = 13;

/** One step of the chat font, held to the range settings allow. */
export function stepFontSize(size: number | undefined, delta: number): number {
  return Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, (size ?? DEFAULT_FONT_SIZE) + delta));
}
