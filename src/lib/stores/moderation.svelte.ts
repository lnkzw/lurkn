import { commands, type ModeratedChannel } from '$lib/ipc';
import { auth } from '$lib/stores/auth.svelte';
import { ERROR_MS, Transient } from '$lib/stores/transient.svelte';

/**
 * Where the reader may moderate, and the actions they can take there.
 *
 * The list decides whether moderation controls appear at all: offering a
 * timeout button in a channel where it would fail is worse than not offering
 * one. It is fetched once, because being modded or unmodded is rare.
 */
class ModerationStore {
  channels = $state<ModeratedChannel[]>([]);
  #error = new Transient(ERROR_MS);
  /** The last failure, for the composer's error line. Clears itself. */
  get error(): string | null {
    return this.#error.value;
  }
  set error(value: string | null) {
    this.#error.value = value;
  }
  #loaded = false;

  /** Whether the session may moderate at all. */
  get granted(): boolean {
    return auth.moderation;
  }

  async load(): Promise<void> {
    if (this.#loaded || !this.granted) return;
    this.#loaded = true;

    const result = await commands.moderatedChannels();
    if (result.status === 'ok') this.channels = result.data;
    else this.error = result.error;
  }

  /** Whether the reader moderates a channel, by its broadcaster id. */
  canModerate(channelId: string | undefined): boolean {
    if (!this.granted || !channelId) return false;
    return this.channels.some((c) => c.id === channelId);
  }

  /** Runs an action and keeps its message. True when it went through. */
  async #run(action: Promise<{ status: 'ok' } | { status: 'error'; error: string }>) {
    const result = await action;
    this.error = result.status === 'error' ? result.error : null;
    return result.status === 'ok';
  }

  timeout(channelId: string, userId: string, seconds: number, reason?: string) {
    return this.#run(commands.moderateBan(channelId, userId, seconds, reason ?? null));
  }

  ban(channelId: string, userId: string, reason?: string) {
    return this.#run(commands.moderateBan(channelId, userId, null, reason ?? null));
  }

  unban(channelId: string, userId: string) {
    return this.#run(commands.moderateUnban(channelId, userId));
  }

  deleteMessage(channelId: string, messageId: string) {
    return this.#run(commands.moderateDelete(channelId, messageId));
  }

  clearChat(channelId: string) {
    return this.#run(commands.moderateDelete(channelId, null));
  }

  /** Change room settings, leaving alone whatever is not named. */
  room(
    channelId: string,
    change: {
      emoteOnly?: boolean;
      subscriberOnly?: boolean;
      uniqueOnly?: boolean;
      followerOnly?: boolean;
      followerMinutes?: number;
      slow?: boolean;
      slowSeconds?: number;
    },
  ) {
    return this.#run(
      commands.moderateRoom(
        channelId,
        change.emoteOnly ?? null,
        change.subscriberOnly ?? null,
        change.uniqueOnly ?? null,
        change.followerOnly ?? null,
        change.followerMinutes ?? null,
        change.slow ?? null,
        change.slowSeconds ?? null,
      ),
    );
  }

  announce(channelId: string, message: string) {
    return this.#run(commands.moderateAnnounce(channelId, message));
  }
}

export const moderation = new ModerationStore();

/** The timeout lengths worth a menu entry, and what to call them. */
export const TIMEOUTS: Array<{ label: string; seconds: number }> = [
  { label: '10 seconds', seconds: 10 },
  { label: '10 minutes', seconds: 600 },
  { label: '1 hour', seconds: 3600 },
  { label: '24 hours', seconds: 86_400 },
];
