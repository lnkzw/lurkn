<!--
  The activity feed: a column beside the chat for the things that happen to a
  channel rather than in it.

  A filtered view of the log, not a second feed. Subs, gifts, raids and
  announcements arrive as channel events and bits as a count on a message, all
  over the chat socket Lurkn already holds — so the feed cannot drift from the
  chat it sits next to, and it works for any channel, not only your own.

  Two things are missing on purpose: follows and hype trains do not come over
  chat, and both need a scope the reader has to grant. They are absent rather
  than shown as rows that never fill.
-->
<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import type { ChatMessage } from '$lib/ipc';
  import { formatTime, rgbToCss } from '$lib/stores/chat.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { platform } from '$lib/ui/platform';

  interface Props {
    /** The feed, worked out by the main window from its chat log. */
    items: ChatMessage[];
    /**
     * In a window of its own: it fills the window, and its header is the
     * title bar.
     */
    windowed?: boolean;
    /** The channel, named in the header when the chat is not beside it. */
    channel?: string | null;
  }

  let { items, windowed = false, channel = null }: Props = $props();

  // Each button changes a setting and nothing else; the main window opens,
  // removes or swaps the window to match.
  const popOut = () => settings.updateActivity({ inWindow: true });
  const popIn = () => settings.updateActivity({ inWindow: false });
  const hide = () => settings.updateActivity({ enabled: false });

  let log = $state<HTMLDivElement | null>(null);
  /** Stick to the newest unless the reader has scrolled back. */
  let pinned = $state(true);

  function onScroll() {
    if (!log) return;
    pinned = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
  }

  $effect(() => {
    void items.length;
    if (pinned && log) log.scrollTop = log.scrollHeight;
  });

  /** The number worth showing beside an event, where there is one. */
  function amount(kind: string, count: number | null): string | null {
    if (count === null) return null;
    if (kind === 'cheer') return `${count.toLocaleString()} bits`;
    if (kind === 'raid') return `${count.toLocaleString()} viewers`;
    if (kind === 'communityGift') return `${count} subs`;
    if (kind === 'resub') return `${count} months`;
    return null;
  }
</script>

<section class="activity" class:windowed aria-label="Activity">
  <!-- In a window of its own the header is the title bar: it drags the window
       and a double click maximises it. The attribute does not inherit, so the
       text carries it too. -->
  <header
    class:mac={windowed && platform === 'macos'}
    data-tauri-drag-region={windowed ? '' : undefined}
    role={windowed ? 'toolbar' : undefined}
    aria-label={windowed ? 'Window' : undefined}
    ondblclick={(e) => {
      if (windowed && !(e.target as HTMLElement).closest('button')) {
        void getCurrentWindow().toggleMaximize();
      }
    }}
  >
    <h2 data-tauri-drag-region={windowed ? '' : undefined}>Activity</h2>
    {#if windowed && channel}
      <span class="channel" data-tauri-drag-region="">{channel}</span>
    {/if}
    <span class="spacer" data-tauri-drag-region={windowed ? '' : undefined}></span>
    {#if windowed}
      <button
        class="icon"
        onclick={popIn}
        aria-label="Put activity back beside the chat"
        title="Back beside the chat"
      >
        <!-- An arrow into a box: back into the main window. -->
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M8 8l5.5-5.5" />
          <path d="M8 4v4h4" />
          <path d="M12 9.5v3a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3" />
        </svg>
      </button>
    {:else}
      <button
        class="icon"
        onclick={popOut}
        aria-label="Open activity in its own window"
        title="Open in its own window"
      >
        <!-- The settings panel's arrow: out of this window, into its own. -->
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M9.5 2.5h4v4" />
          <path d="M13.5 2.5 8 8" />
          <path d="M12 9.5v3a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3" />
        </svg>
      </button>
    {/if}
    <button class="icon" onclick={hide} aria-label="Hide activity" title="Hide activity">
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
    </button>
  </header>

  <div class="log" bind:this={log} onscroll={onScroll}>
    {#if items.length === 0}
      <p class="empty">Subs, gifts, raids and bits appear here as they happen.</p>
    {:else}
      {#each items as message (message.id)}
        {@const event = message.event}
        <article data-event={event?.kind ?? 'chat'}>
          <span class="time">{formatTime(message.sentAt)}</span>

          <span class="body">
            {#if event && event.text.length > 0}
              <!-- Twitch's own wording, the same line the chat shows. -->
              {event.text}
            {:else}
              <span class="who" style="color: {rgbToCss(message.author.color)}">
                {message.author.displayName}
              </span>
              {message.rawText}
            {/if}
          </span>

          {#if event}
            {@const shown = amount(event.kind, event.count)}
            {#if shown}
              <span class="amount">{shown}</span>
            {/if}
          {/if}
        </article>
      {/each}
    {/if}
  </div>
</section>

<style>
  .activity {
    flex: 0 0 auto;
    width: min(20rem, 34vw);
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    border-left: 1px solid var(--border);
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    height: var(--rail-item-height);
    flex: 0 0 auto;
    padding: 0 0.5rem;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
  }

  /* The window is the feed: no column width, no rule against the chat. */
  .activity.windowed {
    flex: 1;
    width: auto;
    border-left: none;
  }

  .windowed header {
    user-select: none;
  }

  /* On macOS the traffic lights sit over the left of the header. */
  header.mac {
    padding-left: 78px;
  }

  .channel {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.75rem;
    color: var(--text-dim);
  }

  .spacer {
    flex: 1 1 auto;
  }

  .icon {
    display: grid;
    place-items: center;
    padding: 0.25rem;
    border: none;
    border-radius: var(--radius);
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }

  .icon svg {
    width: 13px;
    height: 13px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .icon:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  .log {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0.4rem 0;
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
  }

  article {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0 0.4rem;
    padding: 0.3rem 0.6rem;
    font-size: 0.75rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
    /* A rule down the edge, coloured per kind, so the feed can be read by shape
       before it is read by word. */
    border-left: 3px solid var(--edge, transparent);
  }

  article[data-event='raid'] {
    --edge: var(--ok);
  }

  article[data-event='cheer'] {
    --edge: var(--warn);
  }

  article[data-event='sub'],
  article[data-event='resub'],
  article[data-event='subGift'],
  article[data-event='communityGift'],
  article[data-event='upgrade'],
  article[data-event='announcement'] {
    --edge: var(--accent);
  }

  article[data-event='chat'] {
    --edge: var(--border);
  }

  .time {
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    font-size: 0.6875rem;
  }

  .who {
    font-weight: 600;
  }

  .amount {
    grid-column: 2;
    font-size: 0.6875rem;
    font-weight: 600;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .empty {
    margin: 0;
    padding: 0.6rem;
    font-size: 0.75rem;
    line-height: 1.5;
    color: var(--text-dim);
  }
</style>
