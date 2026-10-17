/**
 * Keyboard shortcuts, as a lookup rather than a chain of ifs.
 *
 * Pure: an event's shape goes in, the name of an action comes out. That keeps
 * the modifier rules testable without a DOM, and keeps the component down to
 * running the action it is handed.
 */

export type Action =
  | 'channels'
  | 'whispers'
  | 'settings'
  | 'next-channel'
  | 'previous-channel'
  | 'compose'
  | 'newest'
  | 'font-bigger'
  | 'font-smaller'
  | 'watch'
  | 'refresh';

/** Just enough of a KeyboardEvent to decide, so tests need no DOM. */
export interface Keys {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}

/**
 * The action a key combination means, or null.
 *
 * Ctrl and Cmd are treated alike so the same binding works on a Mac build
 * without a second table to keep in step.
 */
export function actionFor(event: Keys): Action | null {
  const mod = Boolean(event.ctrlKey || event.metaKey);
  const key = event.key.toLowerCase();

  if (mod && !event.altKey) {
    switch (key) {
      case 'k':
        return 'channels';
      case 'w':
        // Shifted, because plain Ctrl+W closes a window everywhere else and
        // rebinding it would eventually cost someone their session.
        return event.shiftKey ? 'whispers' : null;
      case ',':
        return 'settings';
      // The open channel's stream, the one thing the chat header's button does
      // that nothing else could once the header is hidden.
      case 'o':
        return 'watch';
      // What the browser would reload is the live list: Twitch asked now
      // rather than on the next poll.
      case 'r':
        return 'refresh';
      case 'end':
        return 'newest';
      // Where `+` sits depends on the layout: its own key on a German one,
      // shifted `=` on a US one, and the keypad on both. Whatever produced it,
      // Ctrl and the key that reads + means bigger — what a browser zooms on.
      case '+':
      case '=':
        return 'font-bigger';
      case '-':
      case '_':
        return 'font-smaller';
      default:
        return null;
    }
  }

  // A function key types nothing, so it is the one binding without a modifier.
  if (key === 'f5' && !mod && !event.altKey) return 'refresh';

  if (event.altKey && !mod) {
    if (key === 'arrowdown') return 'next-channel';
    if (key === 'arrowup') return 'previous-channel';
  }

  return null;
}

/**
 * Whether the WebView's own right-click menu may show for this target.
 *
 * Only in a text field, where it offers paste and spelling. Everywhere else
 * it is the browser showing through — Back, Reload, Inspect — which nothing
 * in Lurkn wants; the parts that have a menu of their own draw it.
 */
export function nativeMenu(
  target: { tagName?: string; isContentEditable?: boolean; type?: string } | null,
): boolean {
  if (!target) return false;
  if (target.isContentEditable) return true;
  const tag = (target.tagName ?? '').toLowerCase();
  if (tag === 'textarea') return true;
  // A checkbox or a slider is an input too, and has nothing to paste into.
  return tag === 'input' && TEXT_INPUTS.has((target.type ?? 'text').toLowerCase());
}

const TEXT_INPUTS = new Set(['text', 'search', 'url', 'email', 'password', 'number']);

/**
 * Whether a keystroke belongs to what the user is typing in.
 *
 * A shortcut must never steal a key from a text field: `Ctrl+K` in the composer
 * is still `Ctrl+K`, but a bare letter is a letter. Only combinations with a
 * modifier are allowed through while typing.
 */
export function isTyping(tag: string | undefined, editable: boolean): boolean {
  if (editable) return true;
  const name = (tag ?? '').toLowerCase();
  return name === 'input' || name === 'textarea' || name === 'select';
}

/** What to show in a help list, in the order they are worth learning. */
export const SHORTCUTS: Array<{ keys: string; what: string }> = [
  { keys: 'Ctrl K', what: 'Open a channel' },
  { keys: 'Ctrl O', what: 'Watch the stream' },
  { keys: 'Alt ↑ / ↓', what: 'Previous / next live channel' },
  { keys: 'F5', what: 'Refresh who is live' },
  { keys: 'Ctrl Shift W', what: 'Whispers' },
  { keys: 'Ctrl End', what: 'Jump to newest' },
  { keys: 'Ctrl + / −', what: 'Chat text bigger / smaller, here and in the synced window' },
  { keys: 'Ctrl ,', what: 'Settings' },
];
