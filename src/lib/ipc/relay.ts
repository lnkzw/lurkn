/**
 * The main window feeding the activity window.
 *
 * The feed is a view of the chat log, and the log lives in the main window. A
 * second log in the activity window would start empty and miss everything
 * said before it opened, so instead the main window sends the feed itself —
 * the same list its own column would draw — and the activity window draws it.
 *
 * Window to window, without a Rust command in between: there is nothing for
 * Rust to check or keep. Events rather than typed bindings because no Rust
 * type is involved; the payload is the bindings' own `ChatMessage`.
 */

import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { ChatMessage } from './bindings';

/** The activity window's label. Mirrors `ACTIVITY_WINDOW` in commands/window.rs. */
export const ACTIVITY_WINDOW = 'activity';

const FEED = 'lurkn:activity-feed';
const READY = 'lurkn:activity-ready';

export interface ActivityFeed {
  /** The channel the feed belongs to, for the window's header. */
  channel: string | null;
  items: ChatMessage[];
}

/** Main window: send the feed as it now stands. */
export function sendActivity(feed: ActivityFeed): Promise<void> {
  return emitTo(ACTIVITY_WINDOW, FEED, feed);
}

/** Activity window: receive the feed. */
export function onActivity(handler: (feed: ActivityFeed) => void): Promise<UnlistenFn> {
  return listen<ActivityFeed>(FEED, (event) => handler(event.payload));
}

/** Activity window: ask for the feed once it is listening. */
export function askForActivity(): Promise<void> {
  return emitTo('main', READY);
}

/** Main window: a newly opened activity window wants the feed. */
export function onActivityAsked(handler: () => void): Promise<UnlistenFn> {
  return listen(READY, () => handler());
}
