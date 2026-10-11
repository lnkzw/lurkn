<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import Chat from '$lib/components/Chat.svelte';
  import Login from '$lib/components/Login.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import { activity, wantedFrom } from '$lib/chat/activity';
  import { commands, events, type AppInfo, type ChatMessage } from '$lib/ipc';
  import { onActivityAsked, sendActivity } from '$lib/ipc/relay';
  import { auth } from '$lib/stores/auth.svelte';
  import { chat } from '$lib/stores/chat.svelte';
  import { follows } from '$lib/stores/follows.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { whispers } from '$lib/stores/whispers.svelte';
  import { actionFor, isTyping, nativeMenu } from '$lib/ui/shortcuts';
  import { platform } from '$lib/ui/platform';
  import { stepFontSize, watchLook } from '$lib/ui/watchlook';
  import Whispers from '$lib/components/Whispers.svelte';
  import ChannelSearch from '$lib/components/ChannelSearch.svelte';
  import Activity from '$lib/components/Activity.svelte';

  let bootError = $state<string | null>(null);

  const authed = $derived(auth.status.kind === 'authenticated' ? auth.status : null);
  // Followed, or opened by name: `current` knows about both.
  const selectedChannel = $derived(follows.current);

  // The chat comes first in the markup so it keeps focus order. A docked rail
  // on the left or top therefore has to be pulled in front visually, which the
  // reversed flex directions below do. An undocked drawer floats, so the
  // workspace direction is irrelevant to it.
  const sidebarSettings = $derived(settings.current.sidebar);
  const railFlow = $derived(
    !sidebarSettings.collapsed
      ? ''
      : sidebarSettings.position === 'right'
        ? 'rail-row'
        : sidebarSettings.position === 'top'
          ? 'rail-column'
          : 'rail-row-reverse',
  );

  onMount(async () => {
    try {
      await settings.start();
      await auth.start();
      await follows.start();
      await chat.start();
      await whispers.start();

      // Settings over the chat: the tray's Settings, or the settings window
      // putting itself back here at the group it was on. Rust has already
      // brought this window forward.
      unlistenSettings = await events.showSettings.listen((event) => {
        if (event.payload) settings.section = event.payload;
        settings.open = true;
      });
      // A newly opened activity window has nothing until it is sent the feed.
      unlistenActivity = await onActivityAsked(() => {
        sentActivity = '';
        activityAsked += 1;
      });

      const info = await commands.appInfo();
      appInfo = info;
    } catch (error) {
      console.error('startup failed', error);
      bootError = 'Lurkn could not reach its own backend.';
    }
  });

  let appInfo = $state<AppInfo | null>(null);
  let unlistenSettings: (() => void) | null = null;
  let unlistenActivity: (() => void) | null = null;

  /**
   * Run a shortcut, or let the keystroke through.
   *
   * A shortcut never steals a key from a text field: every binding carries a
   * modifier, so typing in the composer is unaffected, and the composer's own
   * handlers see anything this does not claim.
   */
  function onKeydown(event: KeyboardEvent) {
    // Escape closes whatever is on top, innermost first, and is the one binding
    // without a modifier — so it is checked before anything claims the event.
    if (event.key === 'Escape') {
      if (follows.searchOpen) follows.searchOpen = false;
      else if (settings.open) settings.open = false;
      else if (follows.drawerOpen) follows.drawerOpen = false;
      else if (whispers.open) whispers.open = false;
      return;
    }

    const target = event.target as HTMLElement | null;
    const action = actionFor(event);
    if (!action) return;

    // Bindings all carry a modifier, so this only guards against a future
    // unmodified one being added without thinking about it.
    if (
      isTyping(target?.tagName, target?.isContentEditable ?? false) &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey
    ) {
      return;
    }

    event.preventDefault();

    switch (action) {
      case 'channels':
        // The palette rather than the drawer: it works the same whether the
        // sidebar is a drawer or a docked rail, and it reaches channels the
        // sidebar does not list.
        follows.searchOpen = !follows.searchOpen;
        break;
      case 'whispers':
        whispers.open = !whispers.open;
        break;
      case 'settings':
        settings.toggle();
        break;
      case 'watch':
        if (follows.selected) void follows.watch(follows.selected);
        break;
      case 'next-channel':
        follows.step(1);
        break;
      case 'previous-channel':
        follows.step(-1);
        break;
      case 'newest':
      case 'compose':
        chat.jumpToNewest();
        break;
      case 'font-bigger':
      case 'font-smaller': {
        const size = settings.current.chat.fontSize;
        const next = stepFontSize(size, action === 'font-bigger' ? 1 : -1);
        if (next !== size) void settings.updateChat({ fontSize: next });
        break;
      }
    }
  }

  onDestroy(() => {
    unlistenSettings?.();
    unlistenActivity?.();
    auth.stop();
    follows.stop();
    chat.stop();
    whispers.stop();
    settings.stop();
  });

  // The session is restored by a background task on the Rust side, so it is not
  // ready when onMount runs. Waiting for both the login and the follow list
  // avoids asking to join a channel while still logged out, which fails.
  $effect(() => {
    if (auth.status.kind !== 'authenticated') return;
    if (follows.view.channels.length === 0) return;
    void follows.restoreLast();
  });

  // The chat beside the player in the synced window wears Lurkn's look. Read
  // back off the document once the theme is on it, so a theme file and the
  // system preference count; sent only when something actually changed.
  let sentLook = '';
  $effect(() => {
    void settings.painted;
    const current = settings.current;
    if (!current.browser.syncWindow) return;

    const root = document.documentElement;
    const look = watchLook(
      getComputedStyle(root),
      current,
      root.dataset.theme === 'light' ? 'light' : 'dark',
    );
    const key = JSON.stringify(look);
    if (key === sentLook) return;
    sentLook = key;
    void commands.watchSetLook(look);
  });

  // The activity feed: worked out here, where the chat log is, whether it is
  // drawn as a column beside the chat or sent to a window of its own.
  const feed = $derived(settings.current.activity);
  const feedOut = $derived((feed.enabled ?? false) && (feed.inWindow ?? false));
  const feedItems = $derived<ChatMessage[]>(
    feed.enabled ? activity(chat.messages, wantedFrom(feed)) : [],
  );

  // The window follows the settings: there while the feed is on and set to a
  // window, gone otherwise. Rust leaves an open one where it is.
  $effect(() => {
    void commands.activityWindow(feedOut);
  });

  // Sent whenever what it shows changes — a new event, a removed one, another
  // channel — and not on every chat batch that adds nothing to it.
  let sentActivity = '';
  let activityAsked = $state(0);
  $effect(() => {
    void activityAsked;
    if (!feedOut) {
      sentActivity = '';
      return;
    }
    const channel = selectedChannel?.channel.user.displayName ?? null;
    const items = feedItems;
    const key = `${channel}|${items.map((m) => `${m.id}:${m.state.state}`).join(',')}`;
    if (key === sentActivity) return;
    sentActivity = key;
    void sendActivity({ channel, items });
  });

  // Selecting in the drawer drives chat; opening the browser stays separate
  // (docs/architecture.md §8.3).
  $effect(() => {
    const login = follows.selected;
    if (login) {
      void chat.select(login);
    }
  });

</script>

<!-- The WebView's own right-click menu is the browser showing through; only a
     text field keeps it, for paste and spelling. Parts with a menu of their own
     (the chat log, the channel list) draw it before this runs. -->
<svelte:window
  onkeydown={onKeydown}
  oncontextmenu={(e) => {
    if (!nativeMenu(e.target as HTMLInputElement | null)) e.preventDefault();
  }}
/>

<!-- Always on macOS: the traffic lights sit over it, and without it they would
     sit over the chat header instead. -->
{#if platform === 'macos' || !settings.current.window.hideTitleBar}
  <TitleBar />
{/if}

{#if settings.open}
  <Settings info={appInfo} />
{/if}

{#if follows.searchOpen && authed}
  <ChannelSearch />
{/if}


{#if bootError}
  <main class="centre"><p class="error">{bootError}</p></main>
{:else if authed}
  <div class="workspace {railFlow}">
    {#if whispers.open}
      <Whispers />
    {:else}
      <Chat channel={selectedChannel} />
      <!-- Beside the chat rather than replacing it: the point of a feed while
           streaming is to watch both at once. -->
      {#if feed.enabled && !feedOut && selectedChannel}
        <Activity items={feedItems} />
      {/if}
    {/if}
    <Sidebar />
  </div>
{:else}
  <main class="centre">
    <Login renewing={auth.status.kind === 'expired'} />
  </main>
{/if}

<style>
  .workspace {
    flex: 1;
    min-height: 0;
    min-width: 0;
    /* The drawer is positioned against this, so it cannot spill past the window
       when Hyprland tiles it narrow. */
    position: relative;
    overflow: hidden;
    display: flex;
  }

  .workspace.rail-row {
    flex-direction: row;
  }

  .workspace.rail-row-reverse {
    flex-direction: row-reverse;
  }

  .workspace.rail-column {
    flex-direction: column-reverse;
  }

  main {
    flex: 1;
    min-width: 0;
    min-height: 0;
    padding: 2rem;
    overflow: auto;
  }

  .centre {
    display: grid;
    place-items: center;
  }

  .error {
    color: var(--warn);
  }
</style>
