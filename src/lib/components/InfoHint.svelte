<!--
  An explanation attached to a setting rather than printed under it.

  The panel used to carry a paragraph beneath every option it needed to explain,
  which pushed the controls apart and made the list hard to scan. The text is the
  same; it now waits behind the icon.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { place } from '$lib/ui/anchor';

  interface Props {
    /** Names the setting, so the button reads as "About …" to a screen reader. */
    label: string;
    children: Snippet;
  }

  let { label, children }: Props = $props();

  // Hover shows it for a pointer, focus for the keyboard, and a click pins it —
  // otherwise a tap on a touchpad or touchscreen would flash the text and lose it.
  let pinned = $state(false);
  let hovered = $state(false);
  let focused = $state(false);

  const open = $derived(pinned || hovered || focused);

  let button = $state<HTMLButtonElement | null>(null);
  let bubble = $state<HTMLElement | null>(null);
  /** Viewport coordinates, or null until the bubble has been measured. */
  let at = $state<{ left: number; top: number } | null>(null);

  /**
   * The settings body scrolls, so a bubble positioned inside it is clipped at
   * its edges. Fixed positioning leaves that box entirely; the cost is that the
   * place has to be computed rather than declared, and recomputed whenever the
   * panel scrolls or the window resizes.
   */
  function locate() {
    if (!button || !bubble) return;

    // Right-aligned to the icon so a hint near the panel edge grows inwards,
    // and flipped above when the list has been scrolled near the bottom.
    at = place(button.getBoundingClientRect(), bubble.getBoundingClientRect(), {
      side: 'bottom',
      align: 'end',
    });
  }

  $effect(() => {
    if (!open || !bubble) {
      at = null;
      return;
    }

    locate();

    // Capture, so a scroll inside the settings body counts and not just the
    // window's own.
    const replace = () => locate();
    window.addEventListener('scroll', replace, true);
    window.addEventListener('resize', replace);
    return () => {
      window.removeEventListener('scroll', replace, true);
      window.removeEventListener('resize', replace);
    };
  });
</script>

<span class="hint">
  <button
    bind:this={button}
    type="button"
    class="icon"
    class:on={open}
    aria-label="About {label}"
    aria-expanded={open}
    onclick={() => (pinned = !pinned)}
    onmouseenter={() => (hovered = true)}
    onmouseleave={() => (hovered = false)}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
    onkeydown={(e) => {
      if (e.key === 'Escape' && pinned) {
        // Stop the panel from closing too: the first Escape unpins the hint.
        e.stopPropagation();
        pinned = false;
      }
    }}
  >
    <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true" fill="currentColor">
      <path
        fill-rule="evenodd"
        d="M12 2a10 10 0 1 0 0 20a10 10 0 0 0 0-20zm0 2a8 8 0 1 1 0 16a8 8 0 0 1 0-16zm0 2.75a1.25 1.25 0 1 0 0 2.5a1.25 1.25 0 0 0 0-2.5zM11 11h2v6.25h-2z"
      />
    </svg>
  </button>

  {#if open}
    <span
      bind:this={bubble}
      class="bubble"
      class:placed={at !== null}
      role="tooltip"
      style:left="{at?.left ?? 0}px"
      style:top="{at?.top ?? 0}px"
    >{@render children()}</span>
  {/if}
</span>

<style>
  .hint {
    position: relative;
    display: inline-flex;
    /* Keeps the icon on the text baseline beside a label. */
    vertical-align: middle;
  }

  .icon {
    display: grid;
    place-items: center;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-dim);
    cursor: help;
    opacity: 0.7;
  }

  .icon:hover,
  .icon.on {
    color: var(--text);
    opacity: 1;
  }

  .icon:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
    border-radius: 50%;
  }

  .bubble {
    /* Fixed, not absolute: the scrolling settings body would clip it. The
       script above supplies the coordinates. */
    position: fixed;
    z-index: 60;
    width: max-content;
    max-width: min(17rem, calc(100vw - 1rem));
    /* Hidden for the frame between rendering and measuring, so it cannot be
       seen at the top-left corner first. */
    visibility: hidden;
    padding: 0.5rem 0.6rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.35);
    color: var(--text-dim);
    font-size: 0.75rem;
    line-height: 1.55;
    font-weight: normal;
    text-align: left;
    /* The bubble is not a hover target: moving onto it hides it, and there is
       nothing inside to click. */
    pointer-events: none;
  }

  .bubble.placed {
    visibility: visible;
  }
</style>
