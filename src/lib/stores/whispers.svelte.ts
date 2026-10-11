import { commands, events, type Whisper, type WhisperThread } from '$lib/ipc';

/**
 * Whisper read-model.
 *
 * Unlike chat, the backing store is a database rather than a live buffer:
 * Twitch publishes no whisper history, so what Lurkn recorded while running is
 * the only record there will ever be. See `lurkn_core::whisper`.
 */
class WhisperStore {
  threads = $state<WhisperThread[]>([]);
  /** The open conversation, oldest first. */
  thread = $state<Whisper[]>([]);
  /** Which peer is open, or null for the thread list. */
  openPeer = $state<string | null>(null);
  unread = $state(0);
  error = $state<string | null>(null);
  /** The panel is a view of its own, not a popup over chat. */
  open = $state(false);
  sending = $state(false);
  sendError = $state<string | null>(null);
  /**
   * Who a new conversation is with, before it has any messages. A thread only
   * exists once something has been said, so starting one from a chat name needs
   * somewhere to hold the recipient until then.
   */
  draftPeer = $state<{ id: string; login: string; name: string } | null>(null);

  #unlisten: Array<() => void> = [];

  async start(): Promise<void> {
    this.#unlisten.push(
      await events.whisperReceived.listen((event) => {
        this.unread = event.payload.unread;

        // Append to the open conversation rather than refetching it: the
        // whisper is already in hand, and a refetch would scroll the panel.
        if (this.openPeer === event.payload.whisper.peerId) {
          this.thread = [...this.thread, event.payload.whisper];
          void this.markRead(event.payload.whisper.peerId);
        }

        void this.refreshThreads();
      }),
    );

    this.#unlisten.push(
      await events.showWhispers.listen(() => {
        this.open = true;
      }),
    );

    const unread = await commands.whispersUnread();
    if (unread.status === 'ok') this.unread = unread.data;

    await this.refreshThreads();
  }

  stop(): void {
    for (const off of this.#unlisten) off();
    this.#unlisten = [];
  }

  async refreshThreads(): Promise<void> {
    const result = await commands.whisperThreads();
    if (result.status === 'ok') {
      this.threads = result.data;
      this.error = null;
    } else {
      this.error = result.error;
    }
  }

  /** Open a conversation, which is also what marks it read. */
  async openThread(peerId: string): Promise<void> {
    this.openPeer = peerId;

    const result = await commands.whisperThread(peerId);
    if (result.status === 'ok') {
      this.thread = result.data;
      this.error = null;
    } else {
      this.error = result.error;
      this.thread = [];
    }

    await this.markRead(peerId);
  }

  async markRead(peerId: string): Promise<void> {
    const result = await commands.whispersMarkRead(peerId);
    if (result.status === 'ok') {
      this.unread = result.data;
      await this.refreshThreads();
    }
  }

  back(): void {
    this.openPeer = null;
    this.thread = [];
    this.draftPeer = null;
    this.sendError = null;
  }

  /**
   * Open a conversation with a name rather than an id, resolving it first.
   *
   * Returns the error to show, or null on success — the caller decides where it
   * belongs, which differs between the panel's own field and the chat composer.
   */
  async startWithLogin(login: string): Promise<string | null> {
    const result = await commands.whisperFindUser(login);
    if (result.status === 'error') return result.error;

    await this.startWith({
      id: result.data.id,
      login: result.data.login,
      name: result.data.displayName,
    });
    return null;
  }

  /** Open a conversation with someone, whether or not one exists yet. */
  async startWith(peer: { id: string; login: string; name: string }): Promise<void> {
    this.open = true;
    this.draftPeer = peer;
    this.sendError = null;
    await this.openThread(peer.id);
  }

  /** The recipient of the open conversation, from either source. */
  peerOf(peerId: string | null): { id: string; login: string; name: string } | null {
    if (!peerId) return null;
    if (this.draftPeer?.id === peerId) return this.draftPeer;

    const thread = this.threads.find((t) => t.peerId === peerId);
    if (thread) {
      return { id: thread.peerId, login: thread.peerLogin, name: thread.peerName };
    }

    const message = this.thread[0];
    return message
      ? { id: message.peerId, login: message.peerLogin, name: message.peerName }
      : null;
  }

  async send(text: string): Promise<boolean> {
    const peer = this.peerOf(this.openPeer);
    if (!peer) return false;

    this.sending = true;
    this.sendError = null;
    try {
      const result = await commands.whisperSend(peer.id, peer.login, peer.name, text);
      if (result.status === 'error') {
        this.sendError = result.error;
        return false;
      }

      // Appended from the answer rather than refetched: the backend has already
      // written it, and a refetch would jump the transcript.
      this.thread = [...this.thread, result.data];
      await this.refreshThreads();
      return true;
    } finally {
      this.sending = false;
    }
  }
}

export const whispers = new WhisperStore();

/** The other party of the open conversation, for the panel header. */
export function peerName(threads: WhisperThread[], peerId: string | null): string {
  if (!peerId) return '';
  return threads.find((t) => t.peerId === peerId)?.peerName ?? peerId;
}
