<!--
  The emote picker.

  `:` autocomplete covers the emotes whose names you already know; this covers
  the ones you do not. Same list, same source — the channel's 7TV, BTTV and FFZ
  sets — so the two never disagree about what is available.
-->
<script lang="ts">
  import { rank, type Suggestion } from '$lib/chat/complete';
  import { place } from '$lib/ui/anchor';

  interface Props {
    /** The channel's emotes, already fetched for autocomplete. */
    emotes: Suggestion[];
    disabled?: boolean;
    pick: (name: string) => void;
  }

  let { emotes, disabled = false, pick }: Props = $props();

  let open = $state(false);
  let query = $state('');
  let button = $state<HTMLButtonElement | null>(null);
  let panel = $state<HTMLElement | null>(null);
  let search = $state<HTMLInputElement | null>(null);
  /** Viewport coordinates, or null until the panel has been measured. */
  let at = $state<{ left: number; top: number } | null>(null);

  /** A grid, so the cap is generous where the autocomplete list is not. */
  const shown = $derived(rank(emotes, query.toLowerCase(), (e) => e.value, 120));

  function close() {
    open = false;
    at = null;
    query = '';
  }

  function choose(name: string) {
    close();
    pick(name);
  }

  function locate() {
    if (!button || !panel) return;
    at = place(button.getBoundingClientRect(), panel.getBoundingClientRect(), {
      side: 'top',
      align: 'end',
    });
  }

  $effect(() => {
    if (!open || !panel) return;

    // Re-measured when the filter changes the panel's height.
    void shown.length;
    locate();
    search?.focus();

    const replace = () => locate();
    window.addEventListener('resize', replace);
    window.addEventListener('scroll', replace, true);
    return () => {
      window.removeEventListener('resize', replace);
      window.removeEventListener('scroll', replace, true);
    };
  });

  // Nothing to pick from means nothing to open.
  $effect(() => {
    if (emotes.length === 0 || disabled) close();
  });
</script>

<svelte:window
  onpointerdown={(e) => {
    if (!open) return;
    const target = e.target as Node;
    if (!button?.contains(target) && !panel?.contains(target)) close();
  }}
/>

{#if emotes.length > 0}
  <div class="picker">
    <button
      bind:this={button}
      type="button"
      class="trigger"
      class:on={open}
      {disabled}
      aria-label="Emotes"
      aria-haspopup="dialog"
      aria-expanded={open}
      title="Emotes"
      onclick={() => (open ? close() : (open = true))}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <circle cx="8" cy="8" r="6" />
        <path d="M5.5 9.5a3.2 3.2 0 0 0 5 0" />
        <path d="M6 6v.01M10 6v.01" />
      </svg>
    </button>

    {#if open}
      <div
        bind:this={panel}
        class="panel"
        class:placed={at !== null}
        role="dialog"
        aria-label="Emotes"
        style:left="{at?.left ?? 0}px"
        style:top="{at?.top ?? 0}px"
      >
        <input
          bind:this={search}
          bind:value={query}
          class="search"
          placeholder="Search {emotes.length} emotes"
          aria-label="Search emotes"
          onkeydown={(e) => {
            if (e.key === 'Escape') {
              e.stopPropagation();
              close();
              return;
            }
            // Enter takes the first match, which is what a search box implies.
            if (e.key === 'Enter' && shown[0]) {
              e.preventDefault();
              choose(shown[0].value);
            }
          }}
        />

        {#if shown.length === 0}
          <p class="none">No emote matches that.</p>
        {:else}
          <div class="grid">
            {#each shown as emote (emote.value)}
              <button
                type="button"
                title="{emote.value} · {emote.detail ?? ''}"
                onclick={() => choose(emote.value)}
              >
                <img src={emote.image} alt={emote.value} loading="lazy" />
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .picker {
    display: flex;
    flex: 0 0 auto;
  }

  /* Borderless, like the command trigger it sits beside: both live inside the
     input's border, where a box of their own would read as a second field. */
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

  .trigger:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .trigger:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
  }

  .panel {
    /* Fixed, not absolute: the composer would clip it. */
    position: fixed;
    z-index: 70;
    /* Hidden for the frame between rendering and measuring. */
    visibility: hidden;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    width: min(22rem, calc(100vw - 1rem));
    padding: 0.5rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.45);
  }

  .panel.placed {
    visibility: visible;
  }

  .search {
    font: inherit;
    font-size: 0.8125rem;
    height: 1.9rem;
    padding: 0 0.5rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-raised);
    color: var(--text);
  }

  .search:focus {
    outline: none;
    border-color: var(--accent);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(2.2rem, 1fr));
    gap: 0.15rem;
    max-height: 15rem;
    overflow-y: auto;
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
  }

  .grid button {
    display: grid;
    place-items: center;
    height: 2.2rem;
    padding: 0.15rem;
    border: none;
    border-radius: calc(var(--radius) - 2px);
    background: none;
    cursor: pointer;
  }

  .grid button:hover,
  .grid button:focus-visible {
    outline: none;
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }

  .grid img {
    max-width: 100%;
    max-height: 1.9rem;
    object-fit: contain;
  }

  .none {
    margin: 0;
    padding: 0.4rem 0.2rem;
    font-size: 0.75rem;
    color: var(--text-dim);
  }
</style>
