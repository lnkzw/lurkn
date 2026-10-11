<!--
  The suggestion list above the composer.

  Drawing only: which token is active and what matches it is worked out in
  $lib/chat/complete, which is pure and tested. This moves a highlight and
  reports a choice.
-->
<script lang="ts">
  import type { Suggestion } from '$lib/chat/complete';

  interface Props {
    items: Suggestion[];
    /** Index of the highlighted row. */
    active: number;
    /** Shown instead of a list, for a token that cannot be completed. */
    note?: string | null;
    choose: (item: Suggestion) => void;
  }

  let { items, active, note = null, choose }: Props = $props();

  let list = $state<HTMLElement | null>(null);

  // Keep the highlight in view when it is moved by the keyboard rather than
  // the mouse, which is the only way to reach a row past the eighth.
  $effect(() => {
    void active;
    list?.querySelector('[data-active="true"]')?.scrollIntoView({ block: 'nearest' });
  });
</script>

<div class="completions" bind:this={list} role="listbox" aria-label="Suggestions">
  {#if note}
    <p class="note">{note}</p>
  {:else}
    {#each items as item, i (item.kind + item.value)}
      <button
        type="button"
        role="option"
        aria-selected={i === active}
        data-active={i === active}
        class:active={i === active}
        onclick={() => choose(item)}
        onmousedown={(e) => e.preventDefault()}
      >
        {#if item.image}
          <img src={item.image} alt="" loading="lazy" />
        {/if}
        <span class="label">{item.label}</span>
        {#if item.detail}
          <span class="detail">{item.detail}</span>
        {/if}
      </button>
    {/each}
  {/if}
</div>

<style>
  .completions {
    flex: 0 0 100%;
    display: flex;
    flex-direction: column;
    max-height: 13rem;
    overflow-y: auto;
    margin-bottom: 0.15rem;
    padding: 0.2rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
  }

  button {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font: inherit;
    font-size: 0.8125rem;
    text-align: left;
    padding: 0.2rem 0.4rem;
    border: none;
    border-radius: calc(var(--radius) - 2px);
    background: none;
    color: var(--text);
    cursor: pointer;
    min-width: 0;
  }

  button.active {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }

  button img {
    flex: 0 0 auto;
    height: 1.4em;
    max-width: 2.8em;
    object-fit: contain;
  }

  .label {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detail {
    margin-left: auto;
    flex: 0 0 auto;
    font-size: 0.6875rem;
    color: var(--text-dim);
  }

  .note {
    margin: 0;
    padding: 0.3rem 0.4rem;
    font-size: 0.6875rem;
    line-height: 1.45;
    color: var(--text-dim);
  }
</style>
