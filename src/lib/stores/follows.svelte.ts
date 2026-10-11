import {
  commands,
  events,
  type ChannelHit,
  type FollowedChannel,
  type FollowsView,
} from '$lib/ipc';
import { settings } from '$lib/stores/settings.svelte';

type IpcResult<T, E> = { status: 'ok'; data: T } | { status: 'error'; error: E };

const EMPTY: FollowsView = {
  channels: [],
  liveKnown: false,
  staleReason: null,
  loading: false,
};

function unwrap(result: IpcResult<FollowsView, null>): FollowsView {
  return result.status === 'ok' ? result.data : EMPTY;
}

/**
 * Read-model of the follow list. Rust owns the truth and pushes the whole view
 * on every change; the frontend never patches the list itself.
 * See docs/architecture.md §8.
 */
class FollowsStore {
  view = $state<FollowsView>(EMPTY);
  /** Selected channel login. Selecting shows chat; it does not open a browser. */
  selected = $state<string | null>(null);
  openError = $state<string | null>(null);
  /** The drawer is closed until asked for, so chat owns the window. */
  drawerOpen = $state(false);
  /** The channel palette, opened with Ctrl+K. */
  searchOpen = $state(false);
  /**
   * The open channel when it is not one the reader follows. The follow list is
   * Rust's and is replaced wholesale on every refresh, so a channel opened by
   * name is held beside it rather than patched into it.
   */
  extra = $state<FollowedChannel | null>(null);
  /** Channels opened without following them, most recent first. */
  recent = $state<ChannelHit[]>([]);
  /** The reader's own channel. Never in the follow list — nobody follows themselves. */
  mine = $state<FollowedChannel | null>(null);
  /** Whether the reader's own stream is on air, per Twitch. */
  ownStreamLive = $state(false);

  #unlisten: Array<() => void> = [];
  /**
   * Plain field, not `$state`: this is read from an effect, and reactive state
   * read there would make the effect depend on its own result.
   */
  #restoredLast = false;

  readonly live = $derived(this.view.channels.filter((c) => c.stream !== null));
  readonly offline = $derived(
    settings.current.sidebar.showOffline
      ? this.view.channels.filter((c) => c.stream === null)
      : [],
  );

  async start(): Promise<void> {
    this.#unlisten.push(
      await events.followsUpdated.listen((event) => {
        this.view = event.payload;
      }),
    );
    this.#unlisten.push(
      // The backend's full copy of an unfollowed channel — title, category,
      // live state — replacing the snapshot drawn from a search result, and
      // kept current by the same poll as the follow list.
      await events.openChannelChanged.listen((event) => {
        if (event.payload) this.extra = event.payload;
      }),
    );
    this.#unlisten.push(
      await events.ownStreamChanged.listen((event) => {
        this.ownStreamLive = event.payload;

        // Moving someone out of the channel they are reading is a big enough
        // change to be asked for, so it is a setting and off by default.
        if (event.payload && settings.current.broadcast.followOwnStream) {
          void this.openMine();
        }
      }),
    );

    this.view = unwrap(await commands.followsList());

    const live = await commands.ownStreamLive();
    if (live.status === 'ok') this.ownStreamLive = live.data;

    const mine = await commands.myChannel();
    if (mine.status === 'ok') this.mine = mine.data;
  }

  /** Open your own chat. */
  async openMine(): Promise<void> {
    if (!this.mine) {
      const result = await commands.myChannel();
      if (result.status !== 'ok' || !result.data) return;
      this.mine = result.data;
    }

    this.extra = this.mine;
    this.select(this.mine.channel.user.login);
  }

  stop(): void {
    for (const off of this.#unlisten) off();
    this.#unlisten = [];
    this.#restoredLast = false;
  }

  async refresh(): Promise<void> {
    this.view = unwrap(await commands.followsRefresh());
  }

  /** The open channel, followed or not. */
  get current(): FollowedChannel | null {
    if (!this.selected) return null;
    return (
      this.view.channels.find((c) => c.channel.user.login === this.selected) ??
      (this.extra?.channel.user.login === this.selected ? this.extra : null) ??
      (this.mine?.channel.user.login === this.selected ? this.mine : null)
    );
  }

  /** Whether a login is in the follow list. */
  follows(login: string): boolean {
    return this.view.channels.some((c) => c.channel.user.login === login);
  }

  /**
   * Open any channel — followed, or found by name.
   *
   * A followed one goes through `select` as always. Anything else is held in
   * `extra` first, so the chat panel has a channel to draw the moment the login
   * is selected rather than a blank frame until the next refresh.
   */
  open(hit: ChannelHit): void {
    if (!this.follows(hit.login)) this.extra = fromHit(hit);
    this.select(hit.login);
    void this.loadRecent();
  }

  async loadRecent(): Promise<void> {
    const result = await commands.recentChannels();
    if (result.status === 'ok') this.recent = result.data;
  }

  select(login: string): void {
    this.selected = login;
    // Picking a channel is the drawer's whole purpose; leaving it open would
    // cover the chat the user just asked to see. The docked rail stays put —
    // it is a switcher, not something you dismiss.
    if (!settings.current.sidebar.collapsed) {
      this.drawerOpen = false;
    }
  }

  /**
   * Restore the channel that was open when Lurkn last closed.
   *
   * Only meaningful once the session is restored and the follow list is loaded:
   * the backend refuses to join a channel while logged out, and it checks the
   * channel is still followed. Runs at most once per session.
   */
  async restoreLast(): Promise<void> {
    if (this.#restoredLast || this.selected !== null) return;
    this.#restoredLast = true;

    const result = await commands.lastChannel();
    if (result.status === 'ok' && result.data) {
      const login = result.data;

      // Outside the follow list, the recent list gives the header something to
      // draw while the backend fetches the channel properly. Not finding it
      // there is no reason to give up: selecting it makes the backend send the
      // full channel regardless.
      if (!this.follows(login)) {
        await this.loadRecent();
        const hit = this.recent.find((c) => c.login === login);
        if (hit) this.extra = fromHit(hit);
      }

      this.selected = login;
    }
  }

  /**
   * Move to the next or previous live channel, wrapping.
   *
   * Live only: the keyboard is for moving between the channels being watched,
   * and stepping through several hundred offline follows one at a time is not
   * something anyone wants to do.
   */
  step(direction: 1 | -1): void {
    const live = this.live;
    if (live.length === 0) return;

    const current = live.findIndex((c) => c.channel.user.login === this.selected);
    // Not on a live channel: start at the top rather than nowhere.
    const next = current < 0 ? 0 : (current + direction + live.length) % live.length;

    const channel = live[next];
    if (channel) this.select(channel.channel.user.login);
  }

  toggleDrawer(): void {
    this.drawerOpen = !this.drawerOpen;
  }

  /** Explicit, separate from selection — see docs/architecture.md §8.3. */
  async watch(login: string): Promise<void> {
    this.openError = null;
    const result = await commands.openChannel(login);
    if (result.status === 'error') {
      this.openError = result.error;
    }
  }
}

export const follows = new FollowsStore();

/** "1.2k" rather than "1234" — the sidebar is narrow. */
export function formatViewers(count: number): string {
  if (count < 1000) return String(count);
  if (count < 10_000) return `${(count / 1000).toFixed(1).replace(/\.0$/, '')}k`;
  if (count < 1_000_000) return `${Math.round(count / 1000)}k`;
  return `${(count / 1_000_000).toFixed(1).replace(/\.0$/, '')}m`;
}

/** Uptime as "3h 04m", or "12m" under an hour. */
export function formatUptime(startedAt: string, now: number = Date.now()): string {
  const start = Date.parse(startedAt);
  if (Number.isNaN(start)) return '';

  const minutesTotal = Math.max(0, Math.floor((now - start) / 60_000));
  const hours = Math.floor(minutesTotal / 60);
  const minutes = minutesTotal % 60;
  return hours > 0 ? `${hours}h ${String(minutes).padStart(2, '0')}m` : `${minutes}m`;
}

/** Twitch's preview CDN; the CSP allows no other host for images. */
const THUMBNAIL_HOST = 'static-cdn.jtvnw.net';

/**
 * How long a preview image may be reused. Twitch regenerates previews every few
 * minutes, and the webview would otherwise serve the first one it cached for
 * the rest of the session.
 */
const THUMBNAIL_BUCKET_MS = 60_000;

/**
 * Fill Twitch's `{width}`/`{height}` placeholders in a preview URL.
 *
 * Mirrors `Stream::thumbnail` in lurkn-core, host check included: the template
 * is whatever the API returned, and it addresses an <img> the CSP would have to
 * allow. Returns null rather than a broken URL when anything is off.
 */
export function thumbnailUrl(
  template: string | null,
  width: number,
  height: number,
  now: number = Date.now(),
): string | null {
  if (!template) return null;

  const filled = template
    .replaceAll('{width}', String(width))
    .replaceAll('{height}', String(height));

  let url: URL;
  try {
    // Parsed to validate the host and then discarded; nothing reactive reads
    // it, so SvelteURL would buy nothing here.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    url = new URL(filled);
  } catch {
    return null;
  }

  if (url.protocol !== 'https:' || url.hostname !== THUMBNAIL_HOST) return null;

  url.searchParams.set('t', String(Math.floor(now / THUMBNAIL_BUCKET_MS)));
  return url.toString();
}

/**
 * A found channel in the shape the rest of the app draws — a placeholder.
 *
 * Drawn for the moment between choosing a channel and the backend answering,
 * so the header has a name immediately rather than a blank. The backend then
 * sends the channel fetched the way a follow is, which replaces this.
 *
 * No `stream`: a search hit says whether a channel is live but not its viewer
 * count or start time, and a zero would look like a real number.
 */
export function fromHit(hit: ChannelHit): FollowedChannel {
  return {
    channel: {
      user: {
        id: hit.id,
        login: hit.login,
        displayName: hit.displayName,
        avatarUrl: hit.avatarUrl,
      },
      title: hit.title,
      category: hit.category ? { id: '', name: hit.category, boxArtUrl: null } : null,
      language: null,
      followedAt: null,
    },
    stream: null,
  };
}
