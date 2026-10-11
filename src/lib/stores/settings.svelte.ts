import {
  commands,
  events,
  type ChatFilter,
  type CustomCommand,
  type HighlightBadge,
  type HighlightPhrase,
  type HighlightUser,
  type IgnorePhrase,
  type Settings,
  type ThemeState,
  type ThemeVar,
} from '$lib/ipc';

type IpcResult<T, E> = { status: 'ok'; data: T } | { status: 'error'; error: E };

/** A rule with every field present, the way Rust always sends it back. */
type Full<T> = { [K in keyof T]-?: Exclude<T[K], undefined> };

export type HighlightPhraseRule = Full<HighlightPhrase>;
export type HighlightUserRule = Full<HighlightUser>;
export type HighlightBadgeRule = Full<HighlightBadge>;
export type IgnorePhraseRule = Full<IgnorePhrase>;
export type FilterRule = Full<ChatFilter>;
export type CommandRule = Full<CustomCommand>;

/**
 * Rust marks every settings field optional so an older settings file still
 * loads. What it sends back is always complete, so the UI works with a resolved
 * shape instead of threading `undefined` through every binding.
 */
export type ResolvedSettings = {
  theme: NonNullable<Settings['theme']>;
  customTheme: string;
  chat: NonNullable<Settings['chat']>;
  notifications: NonNullable<Settings['notifications']>;
  broadcast: NonNullable<Settings['broadcast']>;
  activity: NonNullable<Settings['activity']>;
  sidebar: NonNullable<Settings['sidebar']>;
  window: Full<NonNullable<Settings['window']>>;
  highlights: {
    phrases: HighlightPhraseRule[];
    users: HighlightUserRule[];
    badges: HighlightBadgeRule[];
    excludedUsers: string[];
  };
  ignores: { phrases: IgnorePhraseRule[]; users: string[] };
  filters: FilterRule[];
  commands: CommandRule[];
  /** Fully resolved, so the UI never has to narrow an optional group. */
  browser: {
    choice: NonNullable<NonNullable<Settings['browser']>['choice']>;
    mode: NonNullable<NonNullable<Settings['browser']>['mode']>;
    newWindow: NonNullable<NonNullable<Settings['browser']>['newWindow']>;
    page: NonNullable<NonNullable<Settings['browser']>['page']>;
    syncWindow: NonNullable<NonNullable<Settings['browser']>['syncWindow']>;
    followChat: NonNullable<NonNullable<Settings['browser']>['followChat']>;
  };
  overlay: Full<NonNullable<Settings['overlay']>>;
};

/** Mirrors the Rust defaults so the UI renders before the first round trip. */
const DEFAULTS: ResolvedSettings = {
  theme: 'system',
  customTheme: '',
  chat: {
    fontSize: 13,
    showTimestamps: true,
    showBadges: true,
    emoteSize: 'medium',
    showStatusBar: true,
    statusBarPosition: 'bottom',
    font: 'system',
    customFont: '',
    alternateBackgrounds: false,
    showUnreadDivider: true,
    showCommandButton: true,
    showSendButton: true,
    showChannelEvents: true,
    showReplyPreview: true,
    showComposerHints: true,
    highlightMentions: true,
    highlightKeywords: [],
    historyOnJoin: false,
  },
  notifications: { live: true, whispers: true, highlights: false },
  broadcast: { followOwnStream: false, notifyOwnStream: true },
  activity: {
    enabled: false,
    subs: true,
    gifts: true,
    raids: true,
    bits: true,
    announcements: true,
    moderators: false,
    inWindow: false,
  },
  sidebar: { position: 'left', collapsed: false, showOffline: true },
  window: {
    hideTitleBar: false,
    showScrollbars: true,
    showHeader: true,
    headerFontSize: 15,
    headerFont: 'system',
    headerCustomFont: '',
    settingsInWindow: false,
  },
  highlights: { phrases: [], users: [], badges: [], excludedUsers: [] },
  ignores: { phrases: [], users: [] },
  filters: [],
  commands: [],
  browser: { choice: { kind: 'system' }, mode: 'normal', newWindow: false, page: 'channel', syncWindow: false, followChat: false },
  overlay: {
    enabled: false,
    port: 7373,
    source: 'own',
    layout: 'bubbles',
    font: 'system',
    customFont: '',
    fontSize: 20,
    textColor: { r: 0xef, g: 0xef, b: 0xf1 },
    bubbleColor: { r: 0x18, g: 0x18, b: 0x1b },
    bubbleOpacity: 85,
    cornerRadius: 12,
    twitchNameColors: true,
    nameColor: { r: 0xbf, g: 0x94, b: 0xff },
    outline: true,
    showBadges: true,
    emoteSize: 'medium',
    maxMessages: 12,
    fadeAfterSeconds: 0,
    animate: true,
    hideCommands: true,
    hiddenUsers: [
      'nightbot',
      'streamelements',
      'streamlabs',
      'moobot',
      'fossabot',
      'wizebot',
      'soundalerts',
    ],
  },
};

/** Blank rows, for the settings screen's Add buttons and for filling gaps. */
export const BLANK = {
  phrase: (): HighlightPhraseRule => ({
    pattern: '',
    regex: false,
    caseSensitive: false,
    color: null,
    notify: false,
  }),
  user: (): HighlightUserRule => ({ login: '', color: null, notify: false }),
  badge: (): HighlightBadgeRule => ({ badge: 'moderator', color: null, notify: false }),
  ignore: (): IgnorePhraseRule => ({
    pattern: '',
    regex: false,
    caseSensitive: false,
    action: 'hide',
    replacement: '',
  }),
  filter: (): FilterRule => ({ name: '', expression: '', enabled: false }),
  command: (): CommandRule => ({ trigger: '', template: '', inMenu: false }),
};

function full<T extends object>(blank: () => Full<T>, items: T[] | undefined): Full<T>[] {
  return (items ?? []).map((item) => ({ ...blank(), ...item }) as Full<T>);
}

export function normalise(settings: Settings): ResolvedSettings {
  return {
    theme: settings.theme ?? DEFAULTS.theme,
    customTheme: settings.customTheme ?? DEFAULTS.customTheme,
    chat: { ...DEFAULTS.chat, ...settings.chat },
    notifications: { ...DEFAULTS.notifications, ...settings.notifications },
    broadcast: { ...DEFAULTS.broadcast, ...settings.broadcast },
    activity: { ...DEFAULTS.activity, ...settings.activity },
    sidebar: { ...DEFAULTS.sidebar, ...settings.sidebar },
    window: { ...DEFAULTS.window, ...settings.window },
    highlights: {
      phrases: full(BLANK.phrase, settings.highlights?.phrases),
      users: full(BLANK.user, settings.highlights?.users),
      badges: full(BLANK.badge, settings.highlights?.badges),
      excludedUsers: settings.highlights?.excludedUsers ?? [],
    },
    ignores: {
      phrases: full(BLANK.ignore, settings.ignores?.phrases),
      users: settings.ignores?.users ?? [],
    },
    filters: full(BLANK.filter, settings.filters),
    commands: full(BLANK.command, settings.commands),
    browser: {
      choice: settings.browser?.choice ?? DEFAULTS.browser.choice,
      mode: settings.browser?.mode ?? DEFAULTS.browser.mode,
      newWindow: settings.browser?.newWindow ?? DEFAULTS.browser.newWindow,
      page: settings.browser?.page ?? DEFAULTS.browser.page,
      syncWindow: settings.browser?.syncWindow ?? DEFAULTS.browser.syncWindow,
      followChat: settings.browser?.followChat ?? DEFAULTS.browser.followChat,
    },
    overlay: { ...DEFAULTS.overlay, ...settings.overlay } as ResolvedSettings['overlay'],
  };
}

function unwrap<T>(result: IpcResult<T, unknown>, fallback: T): T {
  return result.status === 'ok' ? result.data : fallback;
}

class SettingsStore {
  current = $state<ResolvedSettings>(DEFAULTS);
  saveError = $state<string | null>(null);
  open = $state(false);
  /** Which group the settings screen shows, so another panel can open it at one. */
  section = $state('account');

  /** The theme file in effect, kept by Rust, which rereads it as it changes. */
  themeFile = $state<ThemeState>({ theme: null, problems: [] });
  /**
   * Bumped each time the theme is applied to the document, so whatever reads
   * the painted palette back — the synced window's look — can depend on it.
   * The system preference changes the palette without changing a setting.
   */
  painted = $state(0);

  #unlisten: (() => void) | null = null;
  #unlistenTheme: (() => void) | null = null;
  #media: MediaQueryList | null = null;
  #themeVars: string[] = [];
  #onSystemChange = () => this.applyTheme();

  async start(): Promise<void> {
    this.#unlisten = await events.settingsChanged.listen((event) => {
      this.current = normalise(event.payload);
      this.applyTheme();
    });
    this.#unlistenTheme = await events.themeChanged.listen((event) => {
      this.themeFile = event.payload;
      this.applyTheme();
    });

    this.current = normalise(unwrap(await commands.settingsGet(), DEFAULTS));
    this.themeFile = unwrap(await commands.themeCurrent(), this.themeFile);

    // Follow the OS while the preference is "system".
    try {
      this.#media = window.matchMedia('(prefers-color-scheme: dark)');
      this.#media.addEventListener('change', this.#onSystemChange);
    } catch {
      // matchMedia is unavailable in some test environments; the explicit
      // light/dark choices still work.
      this.#media = null;
    }

    this.applyTheme();
  }

  stop(): void {
    this.#unlisten?.();
    this.#unlisten = null;
    this.#unlistenTheme?.();
    this.#unlistenTheme = null;
    this.#media?.removeEventListener('change', this.#onSystemChange);
    this.#media = null;
  }

  applyTheme(): void {
    const root = document.documentElement;
    // A theme file only counts while it is still the one chosen: Rust's echo
    // of a newly picked theme can trail the settings change by a poll.
    const file = this.themeFile.theme;
    const custom = file && file.id === this.current.customTheme ? file : null;

    root.dataset.theme =
      custom?.variant ?? resolveTheme(this.current.theme, this.#media?.matches ?? true);
    this.#themeVars = applyThemeVars(root.style, this.#themeVars, custom?.vars ?? []);
    // Read by app.css, which turns it into the scrollbar tokens every scrollable
    // surface inherits.
    root.dataset.scrollbars = this.current.window.showScrollbars ? 'shown' : 'hidden';
    this.painted += 1;
  }

  /** Optimistic: apply locally, then persist. Rust re-sanitises and echoes back. */
  async update(patch: Partial<ResolvedSettings>): Promise<void> {
    const next: ResolvedSettings = { ...this.current, ...patch };
    this.current = next;
    this.applyTheme();
    this.saveError = null;

    const result = await commands.settingsSet(next);
    if (result.status === 'error') {
      this.saveError = result.error;
    } else {
      this.current = normalise(result.data);
      this.applyTheme();
    }
  }

  /**
   * Open settings, optionally at a group: over the chat, or in a window of
   * their own when the reader chose that — which also brings an open one
   * forward and turns it to the group.
   */
  show(section?: string): void {
    if (this.current.window.settingsInWindow) {
      void commands.openSettingsWindow(section ?? null);
      return;
    }
    if (section) this.section = section;
    this.open = true;
  }

  /** Ctrl and comma: open settings, or close them if they are over the chat. */
  toggle(): void {
    if (this.open) this.open = false;
    else this.show();
  }

  /** Move settings out of the chat window, at the same group, and keep them there. */
  popOut(): void {
    this.open = false;
    void commands.openSettingsWindow(this.section);
    void this.updateWindow({ settingsInWindow: true });
  }

  updateChat(patch: Partial<ResolvedSettings['chat']>): Promise<void> {
    return this.update({ chat: { ...this.current.chat, ...patch } });
  }

  updateNotifications(patch: Partial<ResolvedSettings['notifications']>): Promise<void> {
    return this.update({ notifications: { ...this.current.notifications, ...patch } });
  }

  updateActivity(patch: Partial<ResolvedSettings['activity']>): Promise<void> {
    return this.update({ activity: { ...this.current.activity, ...patch } });
  }

  updateBroadcast(patch: Partial<ResolvedSettings['broadcast']>): Promise<void> {
    return this.update({ broadcast: { ...this.current.broadcast, ...patch } });
  }

  updateSidebar(patch: Partial<ResolvedSettings['sidebar']>): Promise<void> {
    return this.update({ sidebar: { ...this.current.sidebar, ...patch } });
  }

  updateWindow(patch: Partial<ResolvedSettings['window']>): Promise<void> {
    return this.update({ window: { ...this.current.window, ...patch } });
  }

  updateBrowser(patch: Partial<ResolvedSettings['browser']>): Promise<void> {
    return this.update({ browser: { ...this.current.browser, ...patch } });
  }

  updateOverlay(patch: Partial<ResolvedSettings['overlay']>): Promise<void> {
    return this.update({ overlay: { ...this.current.overlay, ...patch } });
  }

  updateHighlights(patch: Partial<ResolvedSettings['highlights']>): Promise<void> {
    return this.update({ highlights: { ...this.current.highlights, ...patch } });
  }

  updateIgnores(patch: Partial<ResolvedSettings['ignores']>): Promise<void> {
    return this.update({ ignores: { ...this.current.ignores, ...patch } });
  }

  /** Stop showing a chatter. Their lines already on screen go at once. */
  ignoreUser(login: string): Promise<void> {
    const users = this.current.ignores.users;
    const name = login.toLowerCase();
    if (users.includes(name)) return Promise.resolve();
    return this.updateIgnores({ users: [...users, name] });
  }
}

export const settings = new SettingsStore();

/** Only custom properties, so a theme can never reach an ordinary property. */
const THEME_VAR = /^--[a-z][a-z-]*$/;

/**
 * Put a theme file's tokens on the root element, over the palette `app.css`
 * picked from `data-theme`, and take off any the previous theme set that this
 * one does not. Returns the names now set, for the next call.
 */
export function applyThemeVars(
  style: Pick<CSSStyleDeclaration, 'setProperty' | 'removeProperty'>,
  previous: readonly string[],
  vars: readonly ThemeVar[],
): string[] {
  const next = vars.filter((v) => THEME_VAR.test(v.name));
  const names = next.map((v) => v.name);

  for (const name of previous) {
    if (!names.includes(name)) style.removeProperty(name);
  }
  for (const v of next) style.setProperty(v.name, v.value);

  return names;
}

export function resolveTheme(
  preference: ResolvedSettings['theme'],
  systemPrefersDark: boolean,
): 'light' | 'dark' {
  if (preference === 'light') return 'light';
  if (preference === 'dark') return 'dark';
  return systemPrefersDark ? 'dark' : 'light';
}

/** Emote height as a multiple of the line's font size. Mirrors EmoteSize::scale. */
export function emoteScale(size: ResolvedSettings['chat']['emoteSize']): number {
  switch (size) {
    case 'small':
      return 1.35;
    case 'large':
      return 2.25;
    default:
      return 1.75;
  }
}

/**
 * CSS font stack for a chat font choice.
 *
 * A custom name is quoted and has already been stripped of anything that could
 * close the declaration (see `sanitize_font_name` in lurkn-core); quoting here
 * is belt and braces, and keeps names with spaces working.
 */
export function chatFontStack(
  font: ResolvedSettings['chat']['font'],
  custom: string | undefined,
): string {
  switch (font) {
    case 'sans':
      return 'ui-sans-serif, system-ui, sans-serif';
    case 'serif':
      return 'ui-serif, Georgia, serif';
    case 'monospace':
      return 'var(--mono)';
    case 'custom':
      return custom ? `"${custom}", var(--font)` : 'var(--font)';

    default:
      return 'var(--font)';
  }
}
