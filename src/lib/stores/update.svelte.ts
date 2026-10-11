import { REPOSITORY } from '$lib/constants';
import { commands, events, type UpdateStatus } from '$lib/ipc';

/** A release's page: its notes, and its downloads for whoever updates by hand. */
export function releasePage(version: string): string {
  return `${REPOSITORY}/releases/tag/v${encodeURIComponent(version)}`;
}

/** What Settings says about updates, or nothing before the first check. */
export function statusLine(status: UpdateStatus): string | null {
  switch (status.kind) {
    case 'unknown':
      return null;
    case 'checking':
      return 'Checking for updates…';
    case 'upToDate':
      return 'Up to date.';
    case 'available':
      return `Lurkn ${status.version} is available.`;
    case 'downloading':
      return status.percent === null
        ? `Downloading ${status.version}…`
        : `Downloading ${status.version}… ${status.percent}%`;
    case 'installing':
      return `Installing ${status.version}…`;
    case 'checkFailed':
    case 'installFailed':
      return status.message;
  }
}

/**
 * Whether the bar over the chat has something to say.
 *
 * A failed check stays in Settings: being offline is not news. A version the
 * reader put away stays away until the next start or the next version.
 */
export function showBar(status: UpdateStatus, dismissed: string | null): boolean {
  switch (status.kind) {
    case 'available':
    case 'installFailed':
      return status.version !== dismissed;
    case 'downloading':
    case 'installing':
      return true;
    default:
      return false;
  }
}

/**
 * Mirror of what Rust knows about a newer Lurkn. Rust checks, downloads and
 * verifies; this only shows it and asks. See docs/architecture.md §14.1.
 */
class UpdateStore {
  status = $state<UpdateStatus>({ kind: 'unknown' });
  dismissed = $state<string | null>(null);

  #unlisten: (() => void) | null = null;

  async start(): Promise<void> {
    this.#unlisten = await events.updateChanged.listen((event) => {
      this.status = event.payload;
    });
    const result = await commands.updateStatus();
    if (result.status === 'ok') this.status = result.data;
  }

  stop(): void {
    this.#unlisten?.();
    this.#unlisten = null;
  }

  async check(): Promise<void> {
    const result = await commands.updateCheck();
    if (result.status === 'ok') this.status = result.data;
  }

  /** A failure arrives as a status, so there is nothing to handle here. */
  async install(): Promise<void> {
    await commands.updateInstall();
  }

  dismiss(): void {
    if (this.status.kind === 'available' || this.status.kind === 'installFailed') {
      this.dismissed = this.status.version;
    }
  }

  openPage(version: string): void {
    void commands.openLink(releasePage(version));
  }
}

export const update = new UpdateStore();
