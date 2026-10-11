<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import { chatFontStack, settings } from '$lib/stores/settings.svelte';
  import { platform } from '$lib/ui/platform';

  interface Props {
    title?: string;
  }

  let { title = 'Lurkn' }: Props = $props();

  const appWindow = getCurrentWindow();
  // macOS keeps its own traffic lights, drawn over the left of this bar
  // (tauri.macos.conf.json); the bar leaves them room and draws no buttons.
  const mac = platform === 'macos';
  // The same text settings as the chat header, which is the title bar's twin.
  const look = $derived(settings.current.window);
  let maximized = $state(false);

  // Keep the restore/maximize glyph honest when the WM changes the state itself
  // (a tiling keybind, a drag to an edge) rather than only on our own clicks.
  $effect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void appWindow.isMaximized().then((value) => {
      if (!disposed) maximized = value;
    });

    void appWindow
      .onResized(async () => {
        const value = await appWindow.isMaximized();
        if (!disposed) maximized = value;
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<!--
  data-tauri-drag-region makes the bar itself draggable. Buttons sit above it and
  must not carry the attribute, or a click would start a window drag instead.
-->
<div
  class="titlebar"
  class:mac
  role="toolbar"
  aria-label="Window"
  tabindex="-1"
  data-tauri-drag-region
  ondblclick={() => appWindow.toggleMaximize()}
>
  <span
    class="brand"
    data-tauri-drag-region
    style:font-size="{look.headerFontSize}px"
    style:font-family={chatFontStack(look.headerFont, look.headerCustomFont)}>{title}</span
  >

  <span class="spacer" data-tauri-drag-region></span>

  {#if !mac}
  <div class="controls">
    <button class="ctl" aria-label="Minimise" onclick={() => appWindow.minimize()}>
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" /></svg>
    </button>
    <button
      class="ctl"
      aria-label={maximized ? 'Restore' : 'Maximise'}
      onclick={() => appWindow.toggleMaximize()}
    >
      {#if maximized}
        <svg viewBox="0 0 10 10" aria-hidden="true">
          <path d="M2.5 0.5h7v7" />
          <rect x="0.5" y="2.5" width="7" height="7" />
        </svg>
      {:else}
        <svg viewBox="0 0 10 10" aria-hidden="true"><rect x="0.5" y="0.5" width="9" height="9" /></svg>
      {/if}
    </button>
    <button class="ctl close" aria-label="Close" onclick={() => appWindow.close()}>
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 0l10 10M10 0L0 10" /></svg>
    </button>
  </div>
  {/if}
</div>

<style>
  .titlebar {
    height: 36px;
    flex: 0 0 auto;
    display: flex;
    align-items: stretch;
    gap: 0.5rem;
    padding-left: 0.875rem;
    background: var(--bg-raised);
    border-bottom: 1px solid var(--border);
    /* The bar is a grab handle, not content. */
    user-select: none;
  }

  /* Room for the traffic lights, which macOS draws over this end of the bar. */
  .titlebar.mac {
    padding-left: 78px;
  }

  .brand {
    display: flex;
    align-items: center;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text);
  }

  .spacer {
    flex: 1;
  }

  .controls {
    display: flex;
  }

  .ctl {
    width: 42px;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: default;
    display: grid;
    place-items: center;
    padding: 0;
  }

  .ctl svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.1;
  }

  .ctl:hover {
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: var(--text);
  }

  .ctl.close:hover {
    background: var(--danger);
    color: var(--on-accent);
  }

  .ctl:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -3px;
  }
</style>
