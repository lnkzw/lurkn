<!--
  The right-click menu on a chat message.

  It exists mostly to take the WebView's own menu out of the way: "Reload",
  "Inspect element" and "Save image as" are the browser showing through, and
  none of them are things to offer someone reading chat.
-->
<script lang="ts">
  import { place } from '$lib/ui/anchor';

  export interface MenuItem {
    label: string;
    run: () => void;
    /** Set apart above the item, to separate groups. */
    divider?: boolean;
    /** A toggle: drawn with a tick, and the menu stays open so several can be flipped. */
    checked?: boolean;
    /** Dimmed at the end of the row: the shortcut that does the same. */
    hint?: string;
  }

  interface Props {
    items: MenuItem[];
    /** Where the pointer was, in viewport coordinates. */
    at: { x: number; y: number };
    close: () => void;
    /** What the menu is for, for screen readers. */
    label?: string;
    /** `end` puts the menu's right edge at `at`, for a button at the right. */
    align?: 'start' | 'end';
  }

  let { items, at, close, label = 'Message actions', align = 'start' }: Props = $props();

  let menu = $state<HTMLElement | null>(null);
  /** Viewport coordinates, or null until the menu has been measured. */
  let placed = $state<{ left: number; top: number } | null>(null);

  /**
   * Anchored to the pointer rather than to the row: a menu that opens at the
   * edge of a 400px-wide message is a menu you have to go and find. The anchor
   * is a zero-size rect at the cursor, which `place` then flips and clamps like
   * any other.
   */
  function locate() {
    if (!menu) return;
    placed = place(
      { top: at.y, bottom: at.y, left: at.x, right: at.x },
      menu.getBoundingClientRect(),
      { side: 'bottom', align, gap: 2 },
    );
  }

  $effect(() => {
    if (!menu) return;
    locate();
    // Focus the first item rather than autofocus, which fires before the menu
    // has been placed and is flagged as an accessibility hazard in general.
    menu.querySelector('button')?.focus();
  });

  /**
   * Up and down walk the list; Escape closes. The first item takes focus on
   * open, so the whole menu is reachable from the keyboard once it is up.
   */
  function onKey(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      close();
      return;
    }

    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    event.preventDefault();

    const buttons = [...(menu?.querySelectorAll('button') ?? [])];
    if (buttons.length === 0) return;

    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const step = event.key === 'ArrowDown' ? 1 : -1;
    buttons[(index + step + buttons.length) % buttons.length]?.focus();
  }

  function choose(item: MenuItem) {
    if (item.checked === undefined) close();
    item.run();
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (!menu?.contains(e.target as Node)) close();
  }}
  onresize={close}
/>

<div
  bind:this={menu}
  class="menu"
  class:placed={placed !== null}
  role="menu"
  tabindex="-1"
  aria-label={label}
  style:left="{placed?.left ?? 0}px"
  style:top="{placed?.top ?? 0}px"
  onkeydown={onKey}
>
  {#each items as item, i (i)}
    <button
      type="button"
      role={item.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
      aria-checked={item.checked}
      class:divider={item.divider}
      onclick={() => choose(item)}
    >
      {#if item.checked !== undefined}
        <span class="tick" aria-hidden="true">{item.checked ? '✓' : ''}</span>
      {/if}
      <span class="label">{item.label}</span>
      {#if item.hint}
        <kbd>{item.hint}</kbd>
      {/if}
    </button>
  {/each}
</div>

<style>
  .menu {
    /* Fixed, not absolute: the log scrolls and would clip it. */
    position: fixed;
    z-index: 80;
    /* Hidden for the frame between rendering and measuring, so it cannot be
       seen at the top-left corner first. */
    visibility: hidden;
    display: flex;
    flex-direction: column;
    min-width: 10rem;
    max-width: min(18rem, calc(100vw - 1rem));
    padding: 0.25rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.45);
  }

  .menu.placed {
    visibility: visible;
  }

  .menu button {
    display: flex;
    align-items: baseline;
    font: inherit;
    font-size: 0.8125rem;
    text-align: left;
    padding: 0.3rem 0.5rem;
    border: none;
    border-radius: calc(var(--radius) - 2px);
    background: none;
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  kbd {
    flex: 0 0 auto;
    margin-left: 1rem;
    font: inherit;
    font-size: 0.6875rem;
    color: var(--text-dim);
  }

  /* A fixed column, so ticked and unticked labels line up. */
  .tick {
    flex: 0 0 auto;
    display: inline-block;
    width: 1.1em;
    color: var(--accent);
  }

  .menu button.divider {
    margin-top: 0.25rem;
    border-top: 1px solid var(--border);
    padding-top: 0.4rem;
    border-radius: 0;
  }

  .menu button:hover,
  .menu button:focus-visible {
    outline: none;
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }
</style>
