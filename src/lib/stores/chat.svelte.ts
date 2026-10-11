import {
  commands,
  events,
  type ChatMessage,
  type ChatStatus,
  type ModerationEvent,
  type RoomState,
} from '$lib/ipc';
import { ERROR_MS, Transient } from '$lib/stores/transient.svelte';

/** Rendered history per channel. Older lines are dropped, as chat is unbounded. */
const MAX_RENDERED = 500;
/**
 * The ceiling while the reader is scrolled up. Trimming at the usual 500 would
 * delete the lines being read out from under them: a busy channel turns over
 * 500 messages in well under a minute. Back at the bottom, the log is cut back.
 */
const MAX_HELD = 1500;

const DISCONNECTED: ChatStatus = {
  channelLogin: null,
  channelId: null,
  state: 'disconnected',
  detail: null,
  room: {
    emoteOnly: false,
    subscriberOnly: false,
    uniqueOnly: false,
    followerOnlyMinutes: null,
    slowModeSeconds: null,
  },
  roomLabels: [],
};

/**
 * Chat read-model. Rust parses messages into tokens; this only appends and trims.
 * See docs/architecture.md §8.1.
 */
class ChatStore {
  messages = $state<ChatMessage[]>([]);
  status = $state<ChatStatus>(DISCONNECTED);
  sending = $state(false);
  #error = new Transient(ERROR_MS);
  /**
   * Why the last send failed. Clears itself after a few seconds: an AutoMod
   * hold or a slow-mode refusal is about one message, not the session.
   */
  get sendError(): string | null {
    return this.#error.value;
  }
  set sendError(value: string | null) {
    this.#error.value = value;
  }
  /** Set while the reader is scrolled up, so trimming does not eat what they read. */
  #holding = false;

  #unlisten: Array<() => void> = [];
  /**
   * The channel we last asked the backend for. Deliberately a plain field, not
   * `$state`: reading reactive state here would make every caller's effect
   * depend on it, and selecting a channel emits events that write it back —
   * which loops, clearing the log on every pass.
   */
  #requested: string | null = null;
  /**
   * Parity offset for alternating backgrounds. Trimming from the front shifts
   * every index by one, so striping on the raw index would repaint the whole
   * log each time a message is dropped. Tracking how many have been trimmed
   * keeps each message's stripe fixed for its lifetime.
   */
  trimmed = $state(0);
  /**
   * The newest message the reader had seen when they last stopped reading.
   * The unread divider is drawn directly under it. `null` means everything on
   * screen counts as read, so no divider is drawn.
   */
  lastSeenId = $state<string | null>(null);
  /**
   * How many messages at the front came from the backfill, so the log can mark
   * where the record Lurkn saw for itself begins.
   */
  history = $state(0);
  /**
   * Bumped to ask the log to jump to the newest message.
   *
   * A counter rather than a flag: the request has to fire again when it is
   * already at the bottom and something has scrolled since, and a boolean would
   * have to be reset by the receiver to do that.
   */
  jumpRequest = $state(0);

  async start(): Promise<void> {
    this.#unlisten.push(
      await events.chatBatch.listen((event) => {
        this.#append(event.payload.messages);
      }),
    );
    this.#unlisten.push(
      await events.chatHistory.listen((event) => {
        this.#prepend(event.payload.messages);
      }),
    );
    this.#unlisten.push(
      await events.chatStateChanged.listen((event) => {
        this.status = event.payload;
      }),
    );
    this.#unlisten.push(
      await events.moderationEvent.listen((event) => this.#applyModeration(event.payload)),
    );

    const status = await commands.chatStatus();
    if (status.status === 'ok') {
      this.status = status.data;
    }
  }

  stop(): void {
    for (const off of this.#unlisten) off();
    this.#unlisten = [];
    this.#requested = null;
  }

  /**
   * Keep older lines while the reader is scrolled up, and cut back to the usual
   * buffer once they return to the bottom.
   */
  hold(on: boolean): void {
    if (this.#holding === on) return;
    this.#holding = on;
    if (!on) this.#trim(MAX_RENDERED);
  }

  /** Ask the log to scroll to the newest message. */
  jumpToNewest(): void {
    this.jumpRequest += 1;
  }

  /** A message still in the log, by id. Used to show what a reply replies to. */
  byId(id: string | null): ChatMessage | null {
    if (!id) return null;
    return this.messages.find((m) => m.id === id) ?? null;
  }

  /**
   * The richest author record the log holds for a user: their most recent
   * message, so a card opened from a mention carries the badges and colour it
   * would have carried from the name itself. `null` when they have not spoken,
   * and the caller falls back to what the mention alone provides.
   */
  authorOf(userId: string) {
    for (let i = this.messages.length - 1; i >= 0; i--) {
      const message = this.messages[i];
      if (message && message.author.id === userId) return message.author;
    }
    return null;
  }

  /** Test seam: what the store believes it last asked for. */
  get requestedChannel(): string | null {
    return this.#requested;
  }

  #append(incoming: ChatMessage[]): void {
    const fresh = unseen(this.messages, incoming);
    if (fresh.length === 0) return;

    this.messages = this.messages.concat(fresh);
    this.#trim(this.#holding ? MAX_HELD : MAX_RENDERED);
  }

  /** Trim from the front so the newest always survive. */
  #trim(limit: number): void {
    const drop = this.messages.length - limit;
    if (drop <= 0) return;
    this.messages = this.messages.slice(drop);
    this.trimmed += drop;
    // Backfill sits at the front, so it is what goes first.
    this.history = Math.max(0, this.history - drop);
  }

  /**
   * Put backfilled messages in front of the log.
   *
   * Prepended rather than assigned: the fetch is slower than the socket, so
   * live messages are often already on screen by the time it lands.
   */
  #prepend(incoming: ChatMessage[]): void {
    const fresh = unseen(this.messages, incoming);
    if (fresh.length === 0) return;

    this.messages = fresh.concat(this.messages);
    this.history = fresh.length;
  }

  /**
   * Moderation removes messages that are already on screen. Deleted lines are
   * kept but marked, rather than vanishing — a message disappearing without
   * trace reads as a bug.
   */
  #applyModeration(event: ModerationEvent): void {
    switch (event.kind) {
      case 'cleared':
        this.messages = [];
        this.history = 0;
        this.lastSeenId = null;
        break;

      case 'userCleared':
        this.messages = this.messages.map((m) =>
          m.author.id === event.userId ? { ...m, state: { state: 'deleted' } } : m,
        );
        break;

      case 'messageDeleted':
        this.messages = this.messages.map((m) =>
          m.id === event.messageId ? { ...m, state: { state: 'deleted' } } : m,
        );
        break;

      case 'roomState':
        this.status = {
          ...this.status,
          room: event.state,
          roomLabels: roomLabels(event.state),
        };
        break;
    }
  }

  /** Switching channels clears history: it belongs to the old channel. */
  async select(login: string): Promise<void> {
    if (this.#requested === login) return;
    this.#requested = login;

    this.messages = [];
    this.trimmed = 0;
    this.history = 0;
    this.lastSeenId = null;
    this.sendError = null;

    const result = await commands.selectChannel(login);
    if (result.status === 'error') {
      this.sendError = result.error;
      // Let a later attempt retry rather than pinning a channel that failed.
      this.#requested = null;
    }
  }

  /**
   * Freeze the unread divider under the newest message. Called when the reader
   * stops reading — the window loses focus, or they scroll up — not while they
   * are reading, so the line marks where they left off.
   */
  markSeen(): void {
    this.lastSeenId = this.messages.at(-1)?.id ?? null;
  }

  /** `replyTo` threads the message under another, as Twitch's reply feature does. */
  async send(text: string, replyTo: string | null = null): Promise<boolean> {
    const trimmed = text.trim();
    if (trimmed.length === 0) return false;

    this.sending = true;
    this.sendError = null;
    try {
      const result = await commands.sendMessage(trimmed, replyTo);
      if (result.status === 'error') {
        this.sendError = result.error;
        return false;
      }
      return true;
    } finally {
      this.sending = false;
    }
  }
}

export const chat = new ChatStore();

/**
 * Index of the first message the reader has not seen, i.e. where the unread
 * divider goes. `-1` when there is nothing unread and no line to draw.
 *
 * A marked message that is no longer in the buffer was trimmed away while the
 * reader was gone, so everything still on screen arrived after it.
 */
export function unreadIndex(messages: Array<{ id: string }>, lastSeenId: string | null): number {
  if (lastSeenId === null || messages.length === 0) return -1;

  const seen = messages.findIndex((m) => m.id === lastSeenId);
  if (seen < 0) return 0;

  return seen + 1 < messages.length ? seen + 1 : -1;
}

/**
 * The messages of `incoming` that `existing` does not already hold.
 *
 * The history service and the socket overlap by a few seconds around a join,
 * and either can land first: a line the socket delivered comes back in the
 * backfill, and a line the backfill delivered arrives again live. The same
 * line twice reads as a bug — and two rows with one id break the keyed list.
 */
export function unseen(existing: ChatMessage[], incoming: ChatMessage[]): ChatMessage[] {
  if (incoming.length === 0) return incoming;
  // A throwaway lookup inside one call; nothing reactive reads it.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const seen = new Set(existing.map((m) => m.id));
  return incoming.filter((m) => !seen.has(m.id));
}

/** Twitch's own limit; the composer shows a counter as it approaches. */
export const MAX_MESSAGE_LENGTH = 500;

export function formatTime(iso: string): string {
  // A throwaway used only to format a string; nothing reactive reads it, so
  // SvelteDate would buy nothing here.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

export function rgbToCss(color: { r: number; g: number; b: number } | null): string {
  if (!color) return 'var(--text)';
  return `rgb(${color.r} ${color.g} ${color.b})`;
}

/**
 * Labels for a room state. Mirrors RoomState::labels in lurkn-core so a state
 * pushed by an event reads the same as one fetched with the status.
 */
export function roomLabels(room: RoomState): string[] {
  const out: string[] = [];

  if (room.emoteOnly) out.push('Emote only');
  if (room.subscriberOnly) out.push('Subs only');

  const minutes = room.followerOnlyMinutes;
  if (minutes !== null) {
    if (minutes === 0) out.push('Followers only');
    else if (minutes >= 60 && minutes % 60 === 0) out.push(`Followers ${minutes / 60}h+`);
    else out.push(`Followers ${minutes}m+`);
  }

  if (room.slowModeSeconds !== null) out.push(`Slow ${room.slowModeSeconds}s`);
  if (room.uniqueOnly) out.push('Unique only');

  return out;
}
