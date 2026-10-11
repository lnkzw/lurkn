<!--
  A newer Lurkn, said once above the chat and never installed without asking.
  Above the chat rather than in the title bar, which can be hidden.
-->
<script lang="ts">
  import { showBar, update } from '$lib/stores/update.svelte';

  // A local binding: TypeScript narrows a const union, not a getter.
  const status = $derived(update.status);
</script>

{#if showBar(status, update.dismissed)}
  <div class="update" role="status">
    {#if status.kind === 'available'}
      <span class="text">Lurkn {status.version} is out.</span>
      {#if status.installable}
        <button class="action" onclick={() => void update.install()}>Install and restart</button>
      {:else}
        <span class="text dim">Update it the way you installed it.</span>
      {/if}
      <button class="link" onclick={() => update.openPage(status.version)}>What's new</button>
    {:else if status.kind === 'downloading'}
      <span class="text">
        Downloading Lurkn {status.version}…{status.percent === null ? '' : ` ${status.percent}%`}
      </span>
    {:else if status.kind === 'installing'}
      <span class="text">Installing Lurkn {status.version}. It restarts by itself.</span>
    {:else if status.kind === 'installFailed'}
      <span class="text warn">{status.message}</span>
      <button class="action" onclick={() => void update.install()}>Try again</button>
      <button class="link" onclick={() => update.openPage(status.version)}>Download it</button>
    {/if}

    {#if status.kind === 'available' || status.kind === 'installFailed'}
      <span class="spacer"></span>
      <button class="icon" onclick={() => update.dismiss()} aria-label="Not now" title="Not now">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    {/if}
  </div>
{/if}

<style>
  .update {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.3rem 0.6rem;
    padding: 0.35rem 0.6rem;
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--accent) 12%, var(--bg));
    font-size: 0.8125rem;
    min-width: 0;
  }

  .text {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .dim {
    color: var(--text-dim);
  }

  .warn {
    color: var(--warn);
  }

  .spacer {
    flex: 1 1 auto;
  }

  .action {
    font: inherit;
    padding: 0.15rem 0.6rem;
    border-radius: var(--radius);
    border: none;
    background: var(--accent);
    color: var(--on-accent);
    cursor: pointer;
  }

  .action:hover {
    filter: brightness(1.1);
  }

  .link {
    font: inherit;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-dim);
    text-decoration: underline;
    cursor: pointer;
  }

  .link:hover {
    color: var(--text);
  }

  .icon {
    display: grid;
    place-items: center;
    padding: 0.2rem;
    border: none;
    border-radius: var(--radius);
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }

  .icon svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
  }

  .icon:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }
</style>
