<!--
  The activity window: the feed on its own, for a reader who wants it on
  another screen or beside OBS rather than squeezed next to the chat. Loaded
  under `?view=activity` (see main.ts).

  It keeps no chat of its own. The main window holds the log and sends the
  feed worked out from it (see $lib/ipc/relay), so this shows exactly what the
  column would — including what happened before the window opened.
-->
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import Activity from '$lib/components/Activity.svelte';
  import type { ChatMessage } from '$lib/ipc';
  import { askForActivity, onActivity } from '$lib/ipc/relay';
  import { settings } from '$lib/stores/settings.svelte';
  import { nativeMenu } from '$lib/ui/shortcuts';

  let items = $state<ChatMessage[]>([]);
  let channel = $state<string | null>(null);
  let unlisten: (() => void) | null = null;

  onMount(async () => {
    // For the theme, and for the buttons, which change settings.
    await settings.start();
    unlisten = await onActivity((feed) => {
      items = feed.items;
      channel = feed.channel;
    });
    // Listening first, then asking: the answer cannot arrive before there is
    // anyone to hear it.
    await askForActivity();
  });

  onDestroy(() => {
    unlisten?.();
    settings.stop();
  });
</script>

<svelte:window
  oncontextmenu={(e) => {
    if (!nativeMenu(e.target as HTMLInputElement | null)) e.preventDefault();
  }}
/>

<Activity {items} {channel} windowed />
