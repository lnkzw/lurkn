import { commands, events, type AuthStatus } from '$lib/ipc';

/** The tagged result tauri-specta emits for a command returning Result. */
type IpcResult<T, E> = { status: 'ok'; data: T } | { status: 'error'; error: E };

/**
 * Commands that borrow Tauri state must return a Result, so tauri-specta emits a
 * tagged union. These commands encode every real failure inside AuthStatus, so an
 * error arm can only mean the IPC bridge itself broke.
 */
function unwrap(result: IpcResult<AuthStatus, null>): AuthStatus {
  if (result.status === 'ok') {
    return result.data;
  }
  return { kind: 'failed', message: 'Lurkn could not reach its own backend.' };
}

/**
 * Mirror of the backend's auth state. Rust owns the truth; this is a read-model
 * that only ever changes in response to a command result or an event.
 * See docs/architecture.md §8.
 */
class AuthStore {
  status = $state<AuthStatus>({ kind: 'loggedOut' });
  /** Set while a command is in flight, so buttons can be disabled. */
  busy = $state(false);
  /** A failure from the last user action, separate from a failed login. */
  actionError = $state<string | null>(null);

  #unlisten: (() => void) | null = null;

  async start(): Promise<void> {
    const unlisten = await events.authChanged.listen((event) => {
      this.status = event.payload;
    });
    this.#unlisten = unlisten;
    this.status = unwrap(await commands.authStatus());
  }

  stop(): void {
    this.#unlisten?.();
    this.#unlisten = null;
  }

  /**
   * `whispers` asks for the optional whisper scope as well. It is opt-in, so a
   * plain login never widens consent for a feature the user has not turned on.
   */
  async login(whispers = false, moderation = false): Promise<void> {
    this.busy = true;
    this.actionError = null;
    try {
      // Whatever is already granted is asked for again, so turning one feature
      // on does not silently turn another off.
      this.status = unwrap(
        await commands.authBegin(whispers || this.whispers, moderation || this.moderation),
      );
    } finally {
      this.busy = false;
    }
  }

  /** Whether the current session may receive whispers. */
  get whispers(): boolean {
    return this.status.kind === 'authenticated' && this.status.whispers;
  }

  /** Whether the current session may moderate. */
  get moderation(): boolean {
    return this.status.kind === 'authenticated' && this.status.moderation;
  }

  async cancel(): Promise<void> {
    this.busy = true;
    try {
      this.status = unwrap(await commands.authCancel());
    } finally {
      this.busy = false;
    }
  }

  async logout(): Promise<void> {
    this.busy = true;
    try {
      this.status = unwrap(await commands.authLogout());
    } finally {
      this.busy = false;
    }
  }

  async openVerification(): Promise<void> {
    this.actionError = null;
    const result = await commands.authOpenVerification();
    if (result.status === 'error') {
      // The backend already phrased this for a human.
      this.actionError = result.error;
    }
  }
}

export const auth = new AuthStore();
