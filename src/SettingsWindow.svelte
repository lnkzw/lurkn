<!--
  The settings window: the settings screen on its own, for a Lurkn too narrow
  to show it over the chat. The same frontend as the main window, loaded under
  `?view=settings` (see main.ts).

  It starts only what the screen reads — settings and the login state — and
  none of chat, follows or whispers: those belong to the main window, and a
  second copy here would only double their traffic.
-->
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import Settings from '$lib/components/Settings.svelte';
  import { commands, events, type AppInfo } from '$lib/ipc';
  import { auth } from '$lib/stores/auth.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { nativeMenu } from '$lib/ui/shortcuts';

  let info = $state<AppInfo | null>(null);
  let unlistenSection: (() => void) | null = null;

  // The group the window was opened at, before the first paint.
  const initial = new URLSearchParams(window.location.search).get('section');
  if (initial) settings.section = initial;

  onMount(async () => {
    await settings.start();
    await auth.start();
    // Already open and asked for again, at another group.
    unlistenSection = await events.settingsSection.listen((event) => {
      settings.section = event.payload;
    });
    info = await commands.appInfo();
  });

  onDestroy(() => {
    unlistenSection?.();
    auth.stop();
    settings.stop();
  });

  function onKeydown(event: KeyboardEvent) {
    // As over the chat: Escape puts settings away, and so does the shortcut
    // that opened them.
    const comma = event.key === ',' && (event.ctrlKey || event.metaKey);
    if (event.key === 'Escape' || comma) {
      event.preventDefault();
      void getCurrentWindow().close();
    }
  }
</script>

<svelte:window
  onkeydown={onKeydown}
  oncontextmenu={(e) => {
    if (!nativeMenu(e.target as HTMLInputElement | null)) e.preventDefault();
  }}
/>

<Settings {info} windowed />
