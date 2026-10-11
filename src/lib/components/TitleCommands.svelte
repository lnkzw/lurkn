<!--
  The `!commands` a streamer advertises in their title, offered as a list at the
  end of the composer. Picking one posts it — these are one-word commands whose
  whole purpose is to be sent verbatim, so an intermediate step of filling the
  input and making the reader press Send would only add a keystroke.
-->
<script lang="ts">
  import { place } from '$lib/ui/anchor';

  interface Props {
    /** Command names without their `!`, in the order the title advertises them. */
    commands: string[];
    /** Chat is not sendable — no channel, or a send already in flight. */
    disabled?: boolean;
    pick: (command: string) => void;
  }

  let { commands, disabled = false, pick }: Props = $props();

  let open = $state(false);
  let button = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLElement | null>(null);
  /** Viewport coordinates, or null until the menu has been measured. */
  let at = $state<{ left: number; top: number } | null>(null);

  function close() {
    open = false;
    at = null;
  }

  /**
   * Fixed positioning, for the same reason as the settings hints: the composer
   * sits at the bottom of a flex column and an absolutely positioned menu would
   * be clipped by it. See $lib/ui/anchor.
   */
  function locate() {
    if (!button || !menu) return;

    // Above the button and right-aligned to it: the composer is the last row in
    // the window, so there is never room below, and the button is at the right
    // edge, so a left-aligned menu would hang off it.
    at = place(button.getBoundingClientRect(), menu.getBoundingClientRect(), {
      side: 'top',
      align: 'end',
    });
  }

  $effect(() => {
    if (!open || !menu) return;

    locate();

    const replace = () => locate();
    window.addEventListener('resize', replace);
    // Capture: the chat log scrolling under the menu counts too.
    window.addEventListener('scroll', replace, true);
    return () => {
      window.removeEventListener('resize', replace);
      window.removeEventListener('scroll', replace, true);
    };
  });

  // A title can change mid-stream, and the channel can change under the menu.
  // Either way the list the reader opened is gone, so the menu goes with it.
  $effect(() => {
    if (commands.length === 0 || disabled) close();
  });

  function choose(command: string) {
    close();
    pick(`!${command}`);
  }

  /**
   * Up and down move through the list, so the whole menu is reachable without
   * tabbing out of it. Escape returns to the button rather than dropping focus
   * to the page.
   */
  function onMenuKey(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      close();
      button?.focus();
      return;
    }

    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    event.preventDefault();

    const items = [...(menu?.querySelectorAll('button') ?? [])];
    if (items.length === 0) return;

    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    const step = event.key === 'ArrowDown' ? 1 : -1;
    // Wraps, so Up from the first lands on the last: the list is short and
    // stopping dead at the ends reads as a stuck key.
    const next = (index + step + items.length) % items.length;
    items[next]?.focus();
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (!open) return;
    const target = e.target as Node;
    if (!button?.contains(target) && !menu?.contains(target)) close();
  }}
/>

{#if commands.length > 0}
  <div class="commands">
    <button
      bind:this={button}
      type="button"
      class="trigger"
      class:on={open}
      {disabled}
      aria-label="Channel commands"
      aria-haspopup="menu"
      aria-expanded={open}
      title="Channel commands"
      onclick={() => (open ? close() : (open = true))}
      onkeydown={(e) => {
        if (e.key === 'Escape' && open) {
          e.stopPropagation();
          close();
        }
      }}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <rect x="2.5" y="2.5" width="11" height="11" rx="2.5" />
        <path d="M8 5.25v3.5" />
        <path d="M8 10.9v.1" />
      </svg>
    </button>

    {#if open}
      <div
        bind:this={menu}
        class="menu"
        class:placed={at !== null}
        role="menu"
        tabindex="-1"
        aria-label="Channel commands"
        style:left="{at?.left ?? 0}px"
        style:top="{at?.top ?? 0}px"
        onkeydown={onMenuKey}
      >
        {#each commands as command (command)}
          <button type="button" role="menuitem" onclick={() => choose(command)}>
            !{command}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .commands {
    display: flex;
    flex: 0 0 auto;
  }

  /* Borderless and transparent: it lives inside the input's border, so a box of
     its own would read as a second field. */
  .trigger {
    display: grid;
    place-items: center;
    height: 1.55rem;
    width: 1.55rem;
    padding: 0;
    border: none;
    border-radius: calc(var(--radius) - 2px);
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }

  .trigger svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.3;
    stroke-linecap: round;
  }

  .trigger:hover:not(:disabled),
  .trigger.on {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  .trigger:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
  }

  .trigger:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .menu {
    /* Fixed, not absolute: the composer would clip it. The script supplies the
       coordinates. */
    position: fixed;
    z-index: 60;
    /* Hidden for the frame between rendering and measuring, so it cannot be
       seen at the top-left corner first. */
    visibility: hidden;
    display: flex;
    flex-direction: column;
    min-width: 8rem;
    max-width: min(14rem, calc(100vw - 1rem));
    /* Past a handful of commands the list scrolls rather than growing into the
       log it is meant to sit in front of. */
    max-height: min(15rem, 50vh);
    overflow-y: auto;
    padding: 0.25rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.35);
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
  }

  .menu.placed {
    visibility: visible;
  }

  .menu button {
    font: inherit;
    font-size: 0.8125rem;
    font-family: var(--mono);
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

  .menu button:hover,
  .menu button:focus-visible {
    outline: none;
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }
</style>
