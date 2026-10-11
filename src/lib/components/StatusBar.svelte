<script lang="ts">
  import { chat } from '$lib/stores/chat.svelte';

  interface Props {
    /** Names of the filters switched on. */
    filters?: string[];
  }

  let { filters = [] }: Props = $props();

  const connecting = $derived(
    chat.status.state === 'connecting' || chat.status.state === 'reconnecting',
  );
  const failed = $derived(chat.status.state === 'failed');
  // Nothing to say is better than an empty bar taking up a row.
  const hasContent = $derived(
    chat.status.roomLabels.length > 0 || connecting || failed || filters.length > 0,
  );
</script>

{#if hasContent}
  <div class="status">
    {#each chat.status.roomLabels as label (label)}
      <span class="pill" title="This channel restricts who can chat">{label}</span>
    {/each}

    {#if filters.length > 0}
      <!-- Said, because a filtered chat that goes quiet looks like a broken one. -->
      <span class="pill live" title="Only messages passing: {filters.join(', ')}">Filtered</span>
    {/if}

    {#if chat.status.state === 'reconnecting'}
      <span class="pill live">Reconnecting…</span>
    {:else if chat.status.state === 'connecting'}
      <span class="pill live">Connecting…</span>
    {:else if failed}
      <span class="pill warn">{chat.status.detail ?? 'Chat unavailable'}</span>
    {/if}
  </div>
{/if}

<style>
  .status {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.3rem;
    padding: 0.35rem 0.6rem;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--text) 3%, transparent);
    min-width: 0;
  }

  /* Only one divider where the bar meets the header or the message box. */
  .status:first-child {
    border-top: none;
  }

  .status:last-child {
    border-bottom: none;
  }

  .pill {
    font-size: 0.625rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-dim);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.05rem 0.45rem;
    white-space: nowrap;
  }

  .pill.live {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
  }

  .pill.warn {
    color: var(--warn);
    border-color: color-mix(in srgb, var(--warn) 55%, transparent);
    /* A failure reason is a sentence, not a label; let it wrap. */
    text-transform: none;
    letter-spacing: 0;
    white-space: normal;
    overflow-wrap: anywhere;
  }
</style>
