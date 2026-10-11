<script lang="ts">
  import CogIcon from '$lib/components/CogIcon.svelte';
  import { SHORTCUTS } from '$lib/ui/shortcuts';
  import InfoHint from '$lib/components/InfoHint.svelte';
  import OverlaySettings from '$lib/components/OverlaySettings.svelte';
  import RuleSettings from '$lib/components/RuleSettings.svelte';
  import {
    commands,
    type AppInfo,
    type BrowserInfo,
    type LogInfo,
    type ThemeEntry,
  } from '$lib/ipc';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import { auth } from '$lib/stores/auth.svelte';
  import { platform } from '$lib/ui/platform';
  import { settings, type ResolvedSettings } from '$lib/stores/settings.svelte';
  import {
    MAX_FONT_SIZE,
    MAX_HEADER_FONT_SIZE,
    MIN_FONT_SIZE,
    MIN_HEADER_FONT_SIZE,
  } from '$lib/constants';

  interface Props {
    info: AppInfo | null;
    /**
     * Shown in a window of its own rather than over the chat: it fills the
     * window, and closing it closes the window.
     */
    windowed?: boolean;
  }

  let { info, windowed = false }: Props = $props();

  const account = $derived(auth.status.kind === 'authenticated' ? auth.status : null);
  const chat = $derived(settings.current.chat);
  const notifications = $derived(settings.current.notifications);
  const broadcast = $derived(settings.current.broadcast);
  const feed = $derived(settings.current.activity);

  const sidebar = $derived(settings.current.sidebar);
  const windowSettings = $derived(settings.current.window);
  const browser = $derived(settings.current.browser);
  // A local binding, not `browser.choice`: TypeScript narrows a const union,
  // not a property read off a getter.
  const choice = $derived(browser.choice);

  // Read when the panel opens rather than at startup: the scan is quick, but
  // there is no reason to touch the filesystem for a screen most sessions never
  // show. The backend caches the result for the session.
  $effect(() => {
    void (async () => {
      const result = await commands.listFonts();
      if (result.status === 'ok') fonts = result.data;
    })();
  });

  const PRESETS = ['system', 'sans', 'serif', 'monospace'] as const;

  /** The select's value: a preset key, or the family name for an installed font. */
  const fontValue = $derived(chat.font === 'custom' ? (chat.customFont ?? '') : chat.font);

  function chooseFont(value: string) {
    if ((PRESETS as readonly string[]).includes(value)) {
      void settings.updateChat({ font: value as (typeof PRESETS)[number] });
    } else {
      void settings.updateChat({ font: 'custom', customFont: value });
    }
  }

  const headerFontValue = $derived(
    windowSettings.headerFont === 'custom'
      ? windowSettings.headerCustomFont
      : windowSettings.headerFont,
  );

  function chooseHeaderFont(value: string) {
    if ((PRESETS as readonly string[]).includes(value)) {
      void settings.updateWindow({ headerFont: value as (typeof PRESETS)[number] });
    } else {
      void settings.updateWindow({ headerFont: 'custom', headerCustomFont: value });
    }
  }

  let fonts = $state<string[]>([]);
  let browsers = $state<BrowserInfo[]>([]);

  // Read when the panel opens, not at startup, and never cached: a browser
  // installed or removed while Lurkn is running should show up next time this
  // screen is opened.
  $effect(() => {
    void (async () => {
      const result = await commands.listBrowsers();
      if (result.status === 'ok') browsers = result.data;
    })();
  });

  /** The select's value: a marker, or `id:` and the desktop-entry id. */
  const browserValue = $derived(choice.kind === 'installed' ? `id:${choice.id}` : choice.kind);

  const chosenBrowser = $derived(
    choice.kind === 'installed' ? (browsers.find((b) => b.id === choice.id) ?? null) : null,
  );

  /**
   * The system default resolves to a browser only at launch time, so its
   * capabilities are unknown here; offering every mode is better than hiding
   * one that would have worked.
   */
  const modes = $derived({
    app: chosenBrowser?.supportsApp ?? true,
    kiosk: chosenBrowser?.supportsKiosk ?? true,
  });

  function chooseBrowser(value: string) {
    if (value === 'system') {
      void settings.updateBrowser({ choice: { kind: 'system' } });
    } else if (value === 'command') {
      void settings.updateBrowser({
        choice: { kind: 'command', line: commandLine },
      });
    } else {
      void settings.updateBrowser({ choice: { kind: 'installed', id: value.slice(3) } });
    }
  }

  /**
   * Held locally while it is being typed: every keystroke would otherwise be a
   * round trip, and a half-typed command would be saved and sanitised away.
   */
  let commandLine = $state('');
  $effect(() => {
    if (choice.kind === 'command') commandLine = choice.line;
  });

  function saveCommand() {
    const current = settings.current.browser.choice;
    if (current.kind !== 'command' || commandLine.trim() === current.line) return;
    void settings.updateBrowser({ choice: { kind: 'command', line: commandLine } });
  }
  let themeFiles = $state<ThemeEntry[]>([]);
  let themeFolder = $state<string | null>(null);
  let themeError = $state<string | null>(null);

  /**
   * Read when the panel opens and again whenever the window regains focus:
   * a theme is written in an editor or a file manager, and coming back to pick
   * it should find it listed.
   */
  async function refreshThemes() {
    const result = await commands.themeList();
    if (result.status === 'ok') {
      themeFiles = result.data.themes;
      themeFolder = result.data.folder;
    }
  }

  $effect(() => {
    void refreshThemes();
  });

  const customTheme = $derived(settings.current.customTheme);

  /** The select's value: a built-in preference, or `file:` and the theme id. */
  const themeValue = $derived(customTheme ? `file:${customTheme}` : settings.current.theme);

  /** The chosen file stays listed even once it is gone, so the select is not blank. */
  const themeOptions = $derived(
    customTheme && !themeFiles.some((entry) => entry.id === customTheme)
      ? [...themeFiles, { id: customTheme, name: customTheme }]
      : themeFiles,
  );

  function chooseTheme(value: string) {
    if (value.startsWith('file:')) {
      void settings.update({ customTheme: value.slice(5) });
    } else {
      void settings.update({ theme: value as ResolvedSettings['theme'], customTheme: '' });
    }
  }

  async function openThemeFolder() {
    themeError = null;
    const result = await commands.openThemeFolder();
    if (result.status === 'error') themeError = result.error;
    // The first open writes an example theme; list it straight away.
    await refreshThemes();
  }

  let logs = $state<LogInfo | null>(null);
  let logsOpen = $state(false);
  let logError = $state<string | null>(null);


  async function toggleLogs() {
    logsOpen = !logsOpen;
    if (logsOpen && !logs) {
      const result = await commands.logTail(300);
      if (result.status === 'ok') logs = result.data;
    }
  }

  async function openFolder() {
    logError = null;
    const result = await commands.openLogFolder();
    if (result.status === 'error') logError = result.error;
  }

  function close() {
    if (windowed) void getCurrentWindow().close();
    else settings.open = false;
  }

  /**
   * Back over the chat, at the same group, and kept there: the preference
   * first, so the main window does not send them straight back out.
   */
  async function popIn() {
    await settings.updateWindow({ settingsInWindow: false });
    await commands.settingsInMain(settings.section);
    close();
  }

  /**
   * Logging in or out is shown in the main window — the code to enter, or the
   * login screen — so from a window of their own, bring that one forward.
   */
  async function accountAction(action: () => Promise<void>) {
    await action();
    if (windowed) await commands.showMainWindow();
  }

  /**
   * The groups, in the order they are worth reading. Labels rather than icons
   * alone: eight categories is past the point where an icon says which is
   * which, and hovering each one to find out is slower than reading a list.
   */
  const GROUPS = [
    { id: 'account', label: 'Account' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'chat', label: 'Chat' },
    // The names chat clients have long used for these, so they are found
    // where people look.
    { id: 'highlights', label: 'Highlights' },
    { id: 'ignores', label: 'Ignores' },
    { id: 'filters', label: 'Filters' },
    { id: 'commands', label: 'Commands' },
    { id: 'alerts', label: 'Alerts' },
    { id: 'streaming', label: 'Streaming' },
    { id: 'watching', label: 'Watching' },
    { id: 'keyboard', label: 'Keyboard' },
    { id: 'advanced', label: 'Advanced' },
  ] as const;

  /**
   * Every section's search words, in one place.
   *
   * Kept beside the markup that uses them rather than derived from it: Svelte
   * cannot read its own template, and a count that disagreed with what is on
   * screen would be worse than no count.
   */
  const SEARCHABLE = [
    'account login logout session scopes permissions twitch keyring version licenses open source',
    'appearance theme dark light font size emote colour color file folder toml custom rice palette',
    'window title bar scrollbars decorations drag header font size menu settings popout pop out',
    'channel list sidebar rail position offline follows drawer',
    'chat timestamps badges messages divider unread reply history commands emotes send composer hints',
    'highlights mention name keywords phrases badges colour marked',
    'ignores ignore block hide replace users phrases mute',
    'filters filter expression show only hide view',
    'commands command custom alias trigger template slash',
    'notifications desktop alerts live whispers sound',
    'activity feed subs gifts raids bits announcements moderators column window popout pop out',
    'your stream broadcast live obs streaming go live',
    'chat overlay obs browser source stream bubbles font colour color port',
    'browser stream player app kiosk window synced watch',
    'keyboard shortcuts keys bindings',
    'advanced log logs debug folder',
  ];

  // In the store rather than here, so the chat's filter menu can open the
  // screen straight at Filters.
  const group = $derived(settings.section);
  let filter = $state('');

  const query = $derived(filter.trim().toLowerCase());

  /**
   * Whether a section is on screen.
   *
   * While something is typed the group falls away and every matching section
   * shows, wherever it lives: the usual reason to open settings is a word you
   * remember, not a category you can place it in. `words` is the section's
   * heading plus the terms someone might reach for instead of it.
   */
  function shown(id: string, words: string): boolean {
    if (query.length > 0) return words.includes(query);
    return group === id;
  }

  /** How many sections a search found, for the empty state. */
  const matches = $derived.by(() => {
    if (query.length === 0) return -1;
    return SEARCHABLE.filter((words) => words.includes(query)).length;
  });
</script>

{#snippet fontPicker(id: string, value: string, choose: (value: string) => void)}
  <select {id} class="select" {value} onchange={(e) => choose(e.currentTarget.value)}>
    <optgroup label="Built in">
      <option value="system">System</option>
      <option value="sans">Sans</option>
      <option value="serif">Serif</option>
      <option value="monospace">Monospace</option>
    </optgroup>

    {#if fonts.length > 0}
      <optgroup label="Installed">
        {#each fonts as family (family)}
          <option value={family}>{family}</option>
        {/each}
      </optgroup>
    {/if}
  </select>
{/snippet}

<svelte:window onfocus={refreshThemes} />

<div
  class="scrim"
  class:windowed
  role="presentation"
  onclick={(e) => !windowed && e.target === e.currentTarget && close()}
>
  <div
    class="panel"
    role={windowed ? 'main' : 'dialog'}
    aria-modal={windowed ? undefined : true}
    aria-label="Settings"
  >
    <!-- In a window of its own the header is the title bar: it drags the
         window, a double click maximises it, and its ✕ closes it. Every part
         but the buttons carries the attribute, which does not inherit. -->
    <header
      class:mac={windowed && platform === 'macos'}
      data-tauri-drag-region={windowed ? '' : undefined}
      role={windowed ? 'toolbar' : undefined}
      aria-label={windowed ? 'Window' : undefined}
      ondblclick={(e) => {
        if (windowed && !(e.target as HTMLElement).closest('button')) {
          void getCurrentWindow().toggleMaximize();
        }
      }}
    >
      <span class="cog" data-tauri-drag-region={windowed ? '' : undefined}
        ><CogIcon size={16} /></span
      >
      <h2 data-tauri-drag-region={windowed ? '' : undefined}>Settings</h2>
      {#if windowed}
        <button
          class="close popout"
          onclick={popIn}
          aria-label="Put settings back over the chat"
          title="Back over the chat"
        >
          <!-- An arrow into a box: back into the main window. The activity
               window's pop-in draws the same. -->
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M8 8l5.5-5.5" />
            <path d="M8 4v4h4" />
            <path d="M12 9.5v3a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3" />
          </svg>
        </button>
      {:else}
        <button
          class="close popout"
          onclick={() => settings.popOut()}
          aria-label="Open settings in their own window"
          title="Open in their own window"
        >
          <!-- A box with an arrow leaving it: out of this window, into one of
               its own. -->
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M9.5 2.5h4v4" />
            <path d="M13.5 2.5 8 8" />
            <path d="M12 9.5v3a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3" />
          </svg>
        </button>
      {/if}
      <button class="close" onclick={close} aria-label="Close settings">✕</button>
    </header>

    <div class="filter">
      <input
        bind:value={filter}
        placeholder="Search settings"
        aria-label="Search settings"
        autocapitalize="off"
        spellcheck="false"
      />
    </div>

    <div class="columns">
      <nav aria-label="Settings sections">
        {#each GROUPS as entry (entry.id)}
          <button
            class:active={group === entry.id && query.length === 0}
            aria-current={group === entry.id && query.length === 0}
            onclick={() => {
              filter = '';
              settings.section = entry.id;
            }}
          >
            {entry.label}
          </button>
        {/each}
      </nav>

      <div class="body">
        {#if matches === 0}
          <p class="none">Nothing in settings matches that.</p>
        {/if}
        {#if shown('account', "account login logout session scopes permissions twitch keyring version licenses open source")}
        <section>
          <h3>Account</h3>
          {#if account}
            <div class="row">
              <span class="label">
                Signed in as <strong>{account.login}</strong>
                <InfoHint label="where your session is kept">
                  Your session is saved in {account.storedIn}.
                </InfoHint>
              </span>
              <button class="ghost" onclick={() => accountAction(() => auth.logout())} disabled={auth.busy}>
                Log out
              </button>
            </div>
          {:else}
            <p class="note dim">Not signed in.</p>
        {/if}
          <div class="check-row">
            <span class="label">Moderation</span>
            {#if auth.moderation}
              <span class="note dim">Granted</span>
            {:else}
              <button
                class="link"
                onclick={() => accountAction(() => auth.login(false, true))}
                disabled={auth.busy}
              >
                Grant access
              </button>
            {/if}
            <InfoHint label="moderation">
              Timeouts, bans, deleting messages and chat settings, in channels where you
              already hold the moderator role. Granting it opens the Twitch login once
              more; your current session keeps working either way.
            </InfoHint>
          </div>

          {#if info}
            <p class="note dim">
              Lurkn v{info.version} ·
              <button
                class="link"
                onclick={() =>
                  void commands.openLink(
                    'https://github.com/lnkzw/lurkn/blob/main/THIRD_PARTY_LICENSES.md',
                  )}
              >
                Open-source licenses
              </button>
            </p>
          {/if}
        </section>
        {/if}

        {#if shown('appearance', "appearance theme dark light font size emote colour color file folder toml custom rice palette")}
        <section>
          <h3>Appearance</h3>
          <div class="row">
            <label class="label" for="theme">
              Theme
              <InfoHint label="theme files">
                A theme file is a short TOML file in the themes folder: colours, fonts and corner
                rounding. Lurkn rereads the one in use whenever it changes, so edits — or a
                wallpaper script writing it — show up live. Opening the folder the first time
                puts an example in it with every key.
              </InfoHint>
            </label>
            <select
              id="theme"
              class="select"
              value={themeValue}
              onchange={(e) => chooseTheme(e.currentTarget.value)}
            >
              <optgroup label="Built in">
                <option value="system">System</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </optgroup>

              {#if themeOptions.length > 0}
                <optgroup label="Theme files">
                  {#each themeOptions as entry (entry.id)}
                    <option value={`file:${entry.id}`}>{entry.name}</option>
                  {/each}
                </optgroup>
              {/if}
            </select>
          </div>

          <div class="check-row">
            <button class="link" onclick={openThemeFolder}>Open themes folder</button>
          </div>
          {#if themeFolder}<p class="path">{themeFolder}</p>{/if}
          {#if themeError}<p class="error">{themeError}</p>{/if}
          {#if customTheme}
            {#each settings.themeFile.problems as problem, i (i)}
              <pre class="problem">{problem}</pre>
            {/each}
          {/if}
        </section>
        {/if}

        {#if shown('appearance', "window title bar scrollbars decorations drag header font size menu settings popout pop out")}
        <section>
          <h3>Window</h3>
          <!-- Not on macOS: the traffic lights need the bar to sit on. -->
          {#if platform !== 'macos'}
          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={windowSettings.hideTitleBar}
                onchange={(e) => settings.updateWindow({ hideTitleBar: e.currentTarget.checked })}
              />
              Hide the title bar
            </label>
            <InfoHint label="hiding the title bar">
              With the title bar hidden, drag the chat header to move the window. Your window
              manager keeps its own close and resize shortcuts.
            </InfoHint>
          </div>
          {/if}

          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={windowSettings.showScrollbars}
                onchange={(e) => settings.updateWindow({ showScrollbars: e.currentTarget.checked })}
              />
              Show scrollbars
            </label>
            <InfoHint label="showing scrollbars">
              Scrolling with the wheel or keyboard works either way. The avatar rail never
              shows one — there is no room beside the avatars.
            </InfoHint>
          </div>

          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={windowSettings.showHeader}
                onchange={(e) => settings.updateWindow({ showHeader: e.currentTarget.checked })}
              />
              Show the chat header
            </label>
            <InfoHint label="the chat header">
              The bar above the chat with the channel name, the stream title, the watch button
              and the menu. Hidden, the menu is a right-click on any empty part of the chat,
              a right-click on a channel in the list offers to watch it, and the shortcuts
              under Keyboard still work — <strong>Ctrl O</strong> watches the open channel.
            </InfoHint>
          </div>

          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={windowSettings.settingsInWindow}
                onchange={(e) =>
                  settings.updateWindow({ settingsInWindow: e.currentTarget.checked })}
              />
              Open settings in their own window
            </label>
            <InfoHint label="settings in their own window">
              Settings open as a window of their own instead of over the chat, which leaves
              them room when Lurkn is narrow. The arrow beside the close button switches
              between the two either way. Changes apply as you make them, in both windows.
            </InfoHint>
          </div>

          <div class="row">
            <label class="label" for="header-size">
              Header text size
              <InfoHint label="the header text size">
                The chat header and the title bar. Chat text has its own size under Chat.
              </InfoHint>
            </label>
            <div class="slider">
              <input
                id="header-size"
                type="range"
                min={MIN_HEADER_FONT_SIZE}
                max={MAX_HEADER_FONT_SIZE}
                value={windowSettings.headerFontSize}
                oninput={(e) =>
                  settings.updateWindow({ headerFontSize: Number(e.currentTarget.value) })}
              />
              <span class="value">{windowSettings.headerFontSize}px</span>
            </div>
          </div>

          <div class="row">
            <label class="label" for="header-font">Header font</label>
            {@render fontPicker('header-font', headerFontValue, chooseHeaderFont)}
          </div>
        </section>
        {/if}

        {#if shown('alerts', "notifications desktop alerts live whispers sound")}
        <section>
          <h3>Notifications</h3>

          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={notifications.live}
                onchange={(e) => settings.updateNotifications({ live: e.currentTarget.checked })}
              />
              A followed channel goes live
            </label>
            <InfoHint label="live notifications">
              One notification per channel, as it starts. Nothing is announced at launch —
              everything already live went live before Lurkn was running.
            </InfoHint>
          </div>

          <label class="check">
            <input
              type="checkbox"
              checked={notifications.whispers}
              onchange={(e) => settings.updateNotifications({ whispers: e.currentTarget.checked })}
            />
            A whisper arrives
          </label>

          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={notifications.highlights}
                onchange={(e) =>
                  settings.updateNotifications({ highlights: e.currentTarget.checked })}
              />
              Someone names you in chat
            </label>
            <InfoHint label="highlight notifications">
              Off by default: a busy channel can name you often. The message is marked in the
              log either way. Phrases, chatters and badges under Highlights each have their
              own Notify switch.
            </InfoHint>
          </div>
        </section>
        {/if}

        {#if shown('streaming', "activity feed subs gifts raids bits announcements moderators column window popout pop out")}
        <section>
          <h3>Activity feed</h3>

          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={feed.enabled}
                onchange={(e) => settings.updateActivity({ enabled: e.currentTarget.checked })}
              />
              Show the activity column
            </label>
            <InfoHint label="the activity feed">
              A column beside the chat for what happens to a channel rather than in it.
              Everything in it already arrives over chat, so it works for any channel you
              open, not only your own. Follows and hype trains are not there: they need
              permissions Twitch has not been asked for yet.
            </InfoHint>
          </div>

          {#if feed.enabled}
            <div class="check-row indent">
              <label class="check">
                <input
                  type="checkbox"
                  checked={feed.inWindow}
                  onchange={(e) => settings.updateActivity({ inWindow: e.currentTarget.checked })}
                />
                In a window of its own
              </label>
              <InfoHint label="the activity window">
                The feed in a window of its own instead of a column beside the chat — for
                another screen, or beside OBS. It shows the same events, including the ones
                from before it opened. Closing the window hides the feed; the arrow in its
                header puts it back beside the chat.
              </InfoHint>
            </div>

            {#each [['subs', 'Subscriptions'], ['gifts', 'Gift subs'], ['raids', 'Raids'], ['bits', 'Bits'], ['announcements', 'Announcements']] as const as [key, label] (key)}
              <label class="check indent">
                <input
                  type="checkbox"
                  checked={feed[key]}
                  onchange={(e) => settings.updateActivity({ [key]: e.currentTarget.checked })}
                />
                {label}
              </label>
            {/each}

            <div class="check-row indent">
              <label class="check">
                <input
                  type="checkbox"
                  checked={feed.moderators}
                  onchange={(e) =>
                    settings.updateActivity({ moderators: e.currentTarget.checked })}
                />
                Anything a moderator says
              </label>
              <InfoHint label="moderator messages">
                Their ordinary messages, not moderation actions — timeouts and bans need
                permissions of their own. Off by default: in a busy channel the mods talk as
                much as anyone.
              </InfoHint>
            </div>
        {/if}
        </section>
        {/if}

        {#if shown('streaming', "your stream broadcast live obs streaming go live")}
        <section>
          <h3>Your stream</h3>

          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={broadcast.notifyOwnStream}
                onchange={(e) =>
                  settings.updateBroadcast({ notifyOwnStream: e.currentTarget.checked })}
              />
              Tell me when my stream starts and stops
            </label>
            <InfoHint label="your stream">
              From Twitch itself, so it is true whatever is running locally. Lurkn does not
              look for OBS or a screen share: both say an application is open, not that
              Twitch has a stream.
            </InfoHint>
          </div>

          <label class="check">
            <input
              type="checkbox"
              checked={broadcast.followOwnStream}
              onchange={(e) =>
                settings.updateBroadcast({ followOwnStream: e.currentTarget.checked })}
            />
            Switch to my channel when I go live
          </label>
        </section>
        {/if}

        {#if shown('streaming', "chat overlay obs browser source stream bubbles font colour color port")}
          <OverlaySettings {fonts} />
        {/if}

        {#if shown('highlights', "highlights mention name keywords phrases badges colour marked")}
          <RuleSettings section="highlights" />
        {/if}

        {#if shown('ignores', "ignores ignore block hide replace users phrases mute")}
          <RuleSettings section="ignores" />
        {/if}

        {#if shown('filters', "filters filter expression show only hide view")}
          <RuleSettings section="filters" />
        {/if}

        {#if shown('commands', "commands command custom alias trigger template slash")}
          <RuleSettings section="commands" />
        {/if}

        {#if shown('appearance', "channel list sidebar rail position offline follows drawer")}
        <section>
          <h3>Channel list</h3>

          <div class="row">
            <span class="label">Opens from</span>
            <div class="segmented">
              {#each [['left', 'Left'], ['right', 'Right'], ['top', 'Top']] as const as [value, label] (value)}
                <button
                  class:active={sidebar.position === value}
                  onclick={() => settings.updateSidebar({ position: value })}
                >
                  {label}
                </button>
              {/each}
            </div>
          </div>

          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={sidebar.collapsed}
                onchange={(e) => settings.updateSidebar({ collapsed: e.currentTarget.checked })}
              />
              Show avatars only
            </label>
            <InfoHint label="showing avatars only">
              Avatars-only stays on screen permanently as a quick switcher, instead of opening
              and closing.
            </InfoHint>
          </div>

          <label class="check">
            <input
              type="checkbox"
              checked={sidebar.showOffline}
              onchange={(e) => settings.updateSidebar({ showOffline: e.currentTarget.checked })}
            />
            Show offline channels
          </label>
        </section>
        {/if}

        {#if shown('chat', "chat timestamps badges messages divider unread reply history commands emotes send composer hints")}
        <section>
          <h3>Chat</h3>

          <div class="row">
            <label class="label" for="font-size">Font size</label>
            <div class="slider">
              <input
                id="font-size"
                type="range"
                min={MIN_FONT_SIZE}
                max={MAX_FONT_SIZE}
                value={chat.fontSize}
                oninput={(e) =>
                  settings.updateChat({ fontSize: Number(e.currentTarget.value) })}
              />
              <span class="value">{chat.fontSize}px</span>
            </div>
          </div>

          <div class="row">
            <label class="label" for="chat-font">Font</label>
            {@render fontPicker('chat-font', fontValue ?? 'system', chooseFont)}
          </div>

        {#if fonts.length === 0}
          <p class="note dim">Reading the fonts installed on this machine…</p>
        {/if}

        <div class="row">
          <span class="label">Emote size</span>
          <div class="segmented">
            {#each [['small', 'Small'], ['medium', 'Medium'], ['large', 'Large']] as const as [value, label] (value)}
              <button
                class:active={chat.emoteSize === value}
                onclick={() => settings.updateChat({ emoteSize: value })}
              >
                {label}
              </button>
            {/each}
          </div>
        </div>

        <label class="check">
          <input
            type="checkbox"
            checked={chat.showTimestamps}
            onchange={(e) =>
              settings.updateChat({ showTimestamps: e.currentTarget.checked })}
          />
          Show timestamps
        </label>

        <label class="check">
          <input
            type="checkbox"
            checked={chat.showBadges}
            onchange={(e) => settings.updateChat({ showBadges: e.currentTarget.checked })}
          />
          Show badges
        </label>

        <label class="check">
          <input
            type="checkbox"
            checked={chat.alternateBackgrounds}
            onchange={(e) =>
              settings.updateChat({ alternateBackgrounds: e.currentTarget.checked })}
          />
          Alternate message backgrounds
        </label>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={chat.showUnreadDivider}
              onchange={(e) =>
                settings.updateChat({ showUnreadDivider: e.currentTarget.checked })}
            />
            Mark where you stopped reading
          </label>
          <InfoHint label="the unread divider">
            Draws a line across the log at the point you last stopped reading — when the
            window loses focus, or when you scroll up. It stays put while you read back
            down, so you can see what you missed.
          </InfoHint>
        </div>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={chat.showCommandButton}
              onchange={(e) =>
                settings.updateChat({ showCommandButton: e.currentTarget.checked })}
            />
            Show command shortcuts
          </label>
          <InfoHint label="command shortcuts">
            Streamers advertise their chat commands in the stream title — !drops, !rested.
            A button at the end of the message box offers them, and picking one posts it.
            It only appears when the title has any.
          </InfoHint>
        </div>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={chat.showSendButton}
              onchange={(e) => settings.updateChat({ showSendButton: e.currentTarget.checked })}
            />
            Show send button
          </label>
          <InfoHint label="the send button">
            Enter sends either way. Hiding the button gives the message box the width back.
          </InfoHint>
        </div>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={chat.historyOnJoin}
              onchange={(e) => settings.updateChat({ historyOnJoin: e.currentTarget.checked })}
            />
            Load recent messages when joining
          </label>
          <InfoHint label="chat history">
            Twitch serves no chat history, so these come from a third-party service,
            recent-messages.robotty.de. Turning this on tells that service which channels
            you open, which is why it is off by default. The log marks where their
            messages end and Lurkn's own begin.
          </InfoHint>
        </div>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={chat.showChannelEvents}
              onchange={(e) =>
                settings.updateChat({ showChannelEvents: e.currentTarget.checked })}
            />
            Show channel events
          </label>
          <InfoHint label="channel events">
            Subscriptions, gift subs, raids, announcements and charity donations, shown in
            the log as Twitch words them. Bits stay on the message they were cheered with
            either way.
          </InfoHint>
        </div>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={chat.showReplyPreview}
              onchange={(e) =>
                settings.updateChat({ showReplyPreview: e.currentTarget.checked })}
            />
            Show what a reply answers
          </label>
          <InfoHint label="reply previews">
            A one-line quote of the message being replied to, above the reply. Click it to
            jump to the original. Replies still arrive when this is off, just without the
            quote.
          </InfoHint>
        </div>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={chat.showComposerHints}
              onchange={(e) =>
                settings.updateChat({ showComposerHints: e.currentTarget.checked })}
            />
            Show what the message box can do
          </label>
          <InfoHint label="composer hints">
            A reminder of the autocomplete keys, shown only while the message box is empty
            and focused. Typing replaces it with the suggestions themselves.
          </InfoHint>
        </div>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={chat.showStatusBar}
              onchange={(e) => settings.updateChat({ showStatusBar: e.currentTarget.checked })}
            />
            Show status bar
          </label>
          <InfoHint label="the status bar">
            Shows followers-only, slow mode and the connection state. It hides itself when
            there is nothing to report.
          </InfoHint>
        </div>

        {#if chat.showStatusBar}
          <div class="row">
            <span class="label">Status bar</span>
            <div class="segmented">
              {#each [['top', 'Top'], ['bottom', 'Bottom']] as const as [value, label] (value)}
                <button
                  class:active={chat.statusBarPosition === value}
                  onclick={() => settings.updateChat({ statusBarPosition: value })}
                >
                  {label}
                </button>
              {/each}
            </div>
          </div>
        {/if}
        </section>
        {/if}

        {#if shown('watching', "browser stream player app kiosk window synced watch")}
        <section>
          <h3>Browser</h3>

          <div class="row">
            <label class="label" for="browser-choice">
              Streams open in
              <InfoHint label="which browser opens streams">
                The list is every application your desktop knows opens web links. A stream
                never plays inside Lurkn — it is handed to the program you pick here, along
                with links you click in chat.
              </InfoHint>
            </label>
            <select
              id="browser-choice"
              class="select"
              value={browserValue}
              onchange={(e) => chooseBrowser(e.currentTarget.value)}
            >
              <option value="system">Your default browser</option>
              {#if browsers.length > 0}
                <optgroup label="Installed">
                  {#each browsers as installed (installed.id)}
                    <option value="id:{installed.id}">{installed.name}</option>
                  {/each}
                </optgroup>
            {/if}
            <option value="command">A command of your own…</option>
          </select>
        </div>

        {#if choice.kind === 'command'}
          <div class="row">
            <label class="label" for="browser-command">
              Command
              <InfoHint label="the custom command">
                Written as you would type it, but run directly rather than through a shell:
                <code>$HOME</code>, <code>~</code>, <code>&amp;&amp;</code> and <code>|</code>
                are ordinary characters here. <code>%u</code> marks where the stream URL
                goes; leave it out and the URL is added at the end.
              </InfoHint>
            </label>
            <input
              id="browser-command"
              class="text"
              type="text"
              spellcheck="false"
              autocapitalize="off"
              autocomplete="off"
              placeholder="streamlink %u best"
              bind:value={commandLine}
              onchange={saveCommand}
              onblur={saveCommand}
            />
          </div>
        {:else if platform === 'linux'}
          <!-- App, kiosk and a new window are flags passed to a browser Lurkn
               found through the desktop's entries, which only Linux has. On
               Windows and macOS a stream opens the way the system opens any
               link, and these would silently do nothing. -->
          <div class="row">
            <span class="label">
              Window
              <InfoHint label="the window mode">
                Applies to streams only: a link from chat always opens the way a link opens
                anywhere else. <strong>App</strong> opens one window holding only the stream —
                no tabs, no address bar. <strong>Kiosk</strong> goes fullscreen with no browser chrome at
                all. Which are available depends on the browser: Chromium-based ones do both,
                Firefox-based ones do kiosk only, and anything else gets an ordinary window.
              </InfoHint>
            </span>
            <div class="segmented">
              {#each [['normal', 'Normal'], ['app', 'App'], ['kiosk', 'Kiosk']] as const as [value, label] (value)}
                <button
                  class:active={browser.mode === value}
                  disabled={value === 'app' ? !modes.app : value === 'kiosk' ? !modes.kiosk : false}
                  onclick={() => settings.updateBrowser({ mode: value })}
                >
                  {label}
                </button>
              {/each}
            </div>
          </div>

          <div class="check-row">
            <label class="check">
              <input
                type="checkbox"
                checked={browser.newWindow}
                disabled={browser.mode !== 'normal'}
                onchange={(e) => settings.updateBrowser({ newWindow: e.currentTarget.checked })}
              />
              Always open a new window
            </label>
            <InfoHint label="always opening a new window">
              Off, a stream opens as another tab in a window you already have. On, it gets a
              window of its own every time. App and kiosk already do that, so the setting
              does not apply to them, and a browser Lurkn does not recognise ignores it.
            </InfoHint>
          </div>
        {/if}

        <div class="row">
          <span class="label">
            Page
            <InfoHint label="which page a stream opens on">
              Twitch has no URL that turns theatre mode on — the site keeps that per account
              and applies it itself. <strong>Player only</strong> is what a URL can ask for:
              the video by itself, no Twitch chat, no panels, no recommendations. Lurkn is
              already your chat, so that is usually the whole point of theatre mode anyway.
            </InfoHint>
          </span>
          <div class="segmented">
            {#each [['channel', 'Channel page'], ['player', 'Player only']] as const as [value, label] (value)}
              <button
                class:active={browser.page === value}
                disabled={browser.syncWindow}
                onclick={() => settings.updateBrowser({ page: value })}
              >
                {label}
              </button>
            {/each}
          </div>
        </div>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={browser.syncWindow}
              onchange={(e) => settings.updateBrowser({ syncWindow: e.currentTarget.checked })}
            />
            Keep one window in sync <span class="tag">experimental</span>
          </label>
          <InfoHint label="keeping one window in sync">
            Opening a second channel normally leaves you with a second tab. With this on,
            Lurkn serves a small page over your own machine's loopback address, your browser
            stays on it, and picking another channel switches the player inside the window
            that is already open.
            <br /><br />
            It shows the player, with Lurkn's chat folded away at the right edge — read-only
            and styled the way your chat is here — so the page setting above does not apply.
            Move the mouse to see the arrow that brings the chat in or folds it away, and press
            <strong>Ctrl +</strong> or <strong>Ctrl −</strong> in either window to size the
            chat text in both. The chat shows whichever channel Lurkn is reading, so it only
            appears beside the stream of that same channel.
            <br /><br />
            Playback may need one click to start, and a stream behind an age gate shows that
            gate inside the window. Nothing but your browser can reach the page: it is bound
            to this machine and carries a token that changes every time Lurkn starts.
          </InfoHint>
        </div>

        <div class="check-row">
          <label class="check">
            <input
              type="checkbox"
              checked={browser.followChat}
              disabled={!browser.syncWindow}
              onchange={(e) => settings.updateBrowser({ followChat: e.currentTarget.checked })}
            />
            Follow the channel I am reading
          </label>
          <InfoHint label="following the channel you are reading">
            Picking a channel in Lurkn switches its chat. With this on, the synced window
            switches with it, so what you are watching is always what you are reading.
            <br /><br />
            It only ever re-points a window that is already open — it never opens one, so
            running down the list with the arrow keys still costs nothing.
          </InfoHint>
        </div>
        </section>
        {/if}

        {#if shown('keyboard', "keyboard shortcuts keys bindings")}
        <section>
          <h3>Keyboard</h3>
          <!--
            The one place the shortcuts are written down. They all carry a
            modifier, so nothing here interferes with typing in chat.
          -->
          <dl class="keys">
            {#each SHORTCUTS as shortcut (shortcut.keys)}
              <dt>
                {#each shortcut.keys.split(' ') as key (key)}<kbd>{key}</kbd>{/each}
              </dt>
              <dd>{shortcut.what}</dd>
            {/each}
            <dt><kbd>Esc</kbd></dt>
            <dd>Close whatever is open</dd>
          </dl>
        </section>
        {/if}

        {#if shown('advanced', "advanced log logs debug folder")}
        <section>
          <h3>Advanced</h3>
          <div class="check-row">
            <button class="link" onclick={toggleLogs}>
              {logsOpen ? 'Hide' : 'Show'} log
            </button>
            <InfoHint label="the log">
              Credentials are removed before anything is written here.
            </InfoHint>
          </div>
          {#if logsOpen}
            {#if logs?.path}
              <p class="path">{logs.path}</p>
              <button class="link" onclick={openFolder}>Open containing folder</button>
          {/if}
          {#if logError}<p class="error">{logError}</p>{/if}
          <pre>{logs?.lines.join('\n') ?? 'Loading…'}</pre>
        {/if}
        </section>
        {/if}
      </div>
    </div>

    {#if settings.saveError}
      <p class="error footer">{settings.saveError}</p>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.5);
    display: grid;
    place-items: center;
    z-index: 50;
  }

  /* In a window of its own the panel is the window: no backdrop, no frame. */
  .scrim.windowed {
    position: static;
    display: block;
    flex: 1;
    min-height: 0;
    background: none;
  }

  .windowed .panel {
    width: 100%;
    height: 100%;
    border: none;
    border-radius: 0;
    box-shadow: none;
  }

  .panel {
    width: min(46rem, calc(100vw - 3rem));
    /* A fixed height, not a maximum: with the panel sized to its content, every
       change of group resized the whole dialog — Account is a few rows, Chat is
       dozens — and the thing being read jumped under the cursor. Only the
       content column scrolls now. */
    height: min(42rem, calc(100vh - 6rem));
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) * 1.5);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.45);
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 1rem 1.25rem;
    border-bottom: 1px solid var(--border);
  }

  /* The settings window on macOS: room for the traffic lights over the header. */
  header.mac {
    padding-left: 78px;
  }

  .cog {
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }

  header h2 {
    flex: 1;
  }

  h2 {
    margin: 0;
    font-size: 1.0625rem;
  }

  h3 {
    margin: 0 0 0.75rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }

  .columns {
    /* Takes the height the panel no longer sizes to its content, so the
       overflow lands on the content column rather than on the dialog. */
    flex: 1;
    min-height: 0;
    display: flex;
  }

  /* A list of words down the side: every label is readable at once, and adding
     a ninth group costs nothing. A row of icons across the top would need
     hovering to identify and would run out of room. */
  nav {
    flex: 0 0 auto;
    width: 9.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    padding: 0.75rem 0.5rem;
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }

  nav button {
    font: inherit;
    font-size: 0.875rem;
    text-align: left;
    padding: 0.35rem 0.5rem;
    border: none;
    border-radius: var(--radius);
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }

  nav button:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  nav button.active {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }

  .filter {
    flex: 0 0 auto;
    padding: 0.6rem 0.75rem;
    border-bottom: 1px solid var(--border);
  }

  .filter input {
    width: 100%;
    font: inherit;
    font-size: 0.875rem;
    height: 1.9rem;
    padding: 0 0.6rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
  }

  .filter input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .none {
    margin: 0;
    font-size: 0.875rem;
    color: var(--text-dim);
  }

  .body {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1.75rem;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 0.75rem;
  }

  .label {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.875rem;
  }

  .segmented {
    display: flex;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .segmented button {
    font: inherit;
    font-size: 0.8125rem;
    padding: 0.3rem 0.7rem;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }

  .segmented button.active {
    background: var(--accent);
    color: var(--on-accent);
  }

  .segmented button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .segmented button:hover:not(.active):not(:disabled) {
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
  }

  .slider {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .value {
    font-size: 0.8125rem;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    min-width: 2.5rem;
    text-align: right;
  }

  /* Pairs a control with its hint. The hint sits outside the <label>, or a
     click on the icon would toggle the checkbox the label is wrapping. */
  .check-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.875rem;
    padding: 0.25rem 0;
    cursor: pointer;
  }

  .note {
    margin: 0 0 0.5rem;
    font-size: 0.8125rem;
    line-height: 1.6;
  }

  .note.dim {
    color: var(--text-dim);
  }

  .tag {
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 0.05rem 0.3rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    color: var(--text-dim);
  }

  /* Two columns, keys right-aligned against their meaning, so the eye runs
     down the gap rather than hunting across ragged rows. */
  .keys {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: baseline;
    gap: 0.35rem 0.6rem;
    margin: 0;
    font-size: 0.8125rem;
  }

  .keys dt {
    display: flex;
    gap: 0.2rem;
    justify-content: flex-end;
    white-space: nowrap;
  }

  .keys dd {
    margin: 0;
    color: var(--text-dim);
  }

  .keys kbd {
    font: inherit;
    font-family: var(--mono);
    padding: 0 0.35em;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    color: var(--text);
  }

  /* Sub-options, set in from the toggle that governs them. */
  .indent {
    margin-left: 1.1rem;
  }

  .path {
    font-family: var(--mono);
    font-size: 0.75rem;
    color: var(--text-dim);
    margin: 0.25rem 0;
    user-select: text;
    word-break: break-all;
  }

  pre {
    margin: 0.5rem 0 0;
    padding: 0.6rem;
    max-height: 14rem;
    overflow: auto;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-family: var(--mono);
    font-size: 0.75rem;
    line-height: 1.5;
    user-select: text;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .select {
    font: inherit;
    font-size: 0.875rem;
    padding: 0.3rem 0.5rem;
    max-width: 14rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
  }

  .text {
    font: inherit;
    font-size: 0.875rem;
    padding: 0.3rem 0.5rem;
    flex: 1;
    min-width: 0;
    max-width: 18rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
  }

  .text:focus,
  .select:focus {
    outline: none;
    border-color: var(--accent);
  }

  /* WebKit honours these for the rendered list even though the popup chrome
     itself comes from color-scheme. */
  .select option,
  .select optgroup {
    background: var(--bg-raised);
    color: var(--text);
  }

  .ghost {
    font: inherit;
    font-size: 0.8125rem;
    padding: 0.25rem 0.7rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    cursor: pointer;
  }

  .ghost:hover:not(:disabled) {
    border-color: var(--text-dim);
  }

  .ghost:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .close,
  .link {
    font: inherit;
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    padding: 0;
  }

  .link {
    font-size: 0.875rem;
    text-decoration: underline;
  }

  .close:hover,
  .link:hover {
    color: var(--text);
  }

  .popout {
    display: grid;
    place-items: center;
    margin-right: 0.35rem;
  }

  .popout svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .error {
    color: var(--warn);
    font-size: 0.8125rem;
    margin: 0.5rem 0 0;
  }

  /* A TOML parse error points at a column with a caret, so it keeps its
     spacing and its own lines. */
  pre.problem {
    color: var(--warn);
    max-height: none;
    white-space: pre-wrap;
  }

  .footer {
    padding: 0 1.25rem 1rem;
  }
</style>
