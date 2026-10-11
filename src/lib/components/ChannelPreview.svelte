<!--
  What a channel is streaming, shown while the pointer rests on its row.

  The row itself only has space for a name and a number. Everything else Twitch
  already sent with the follow list — title, category, viewers, preview image —
  had nowhere to go until now.
-->
<script lang="ts">
  import type { FollowedChannel } from '$lib/ipc';
  import { formatUptime, formatViewers, thumbnailUrl } from '$lib/stores/follows.svelte';
  import { place, type Side } from '$lib/ui/anchor';

  interface Props {
    entry: FollowedChannel;
    /** The row being hovered; the card is placed against it. */
    anchor: HTMLElement;
    /** Preferred side, away from the edge the drawer is attached to. */
    side: Side;
    now: number;
  }

  let { entry, anchor, side, now }: Props = $props();

  /**
   * Asked of Twitch at twice the rendered width: the card is small enough that
   * the 440px preview is the cheapest one that still looks sharp on a HiDPI
   * screen, and Twitch serves these at arbitrary sizes.
   */
  const THUMB_WIDTH = 440;
  const THUMB_HEIGHT = 248;

  const stream = $derived(entry.stream);
  const image = $derived(
    stream ? thumbnailUrl(stream.thumbnailUrlTemplate, THUMB_WIDTH, THUMB_HEIGHT) : null,
  );

  let card = $state<HTMLElement | null>(null);
  let at = $state<{ left: number; top: number } | null>(null);
  /** A preview Twitch has not generated yet 404s; leave the space out. */
  let imageFailed = $state(false);

  // A new channel means a new image: an old failure must not hide a good one.
  $effect(() => {
    void image;
    imageFailed = false;
  });

  function locate() {
    if (!card) return;
    at = place(anchor.getBoundingClientRect(), card.getBoundingClientRect(), { side, gap: 8 });
  }

  $effect(() => {
    if (!card) return;

    // Depend on both, so moving to another row re-places the card.
    void anchor;
    void side;
    locate();

    const relocate = () => locate();
    window.addEventListener('scroll', relocate, true);
    window.addEventListener('resize', relocate);

    // The card grows when the image arrives, which can push it off the bottom
    // of the window; re-place it rather than let it hang over the edge.
    const observer =
      typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(relocate);
    observer?.observe(card);

    return () => {
      window.removeEventListener('scroll', relocate, true);
      window.removeEventListener('resize', relocate);
      observer?.disconnect();
    };
  });
</script>

{#if stream}
  <!-- Decorative: the row it describes carries the same text for a screen reader. -->
  <div
    bind:this={card}
    class="preview"
    class:placed={at !== null}
    aria-hidden="true"
    style:left="{at?.left ?? 0}px"
    style:top="{at?.top ?? 0}px"
  >
    {#if image && !imageFailed}
      <img
        class="shot"
        src={image}
        alt=""
        width={THUMB_WIDTH}
        height={THUMB_HEIGHT}
        onerror={() => (imageFailed = true)}
      />
    {/if}

    <div class="body">
      <p class="name">{entry.channel.user.displayName}</p>
      <p class="title">{stream.title}</p>
      <p class="meta">
        {#if stream.category}
          <span class="game">{stream.category.name}</span>
        {/if}
        <span class="viewers"><i class="dot"></i>{formatViewers(stream.viewerCount)}</span>
        <span class="uptime">{formatUptime(stream.startedAt, now)}</span>
      </p>
    </div>
  </div>
{/if}

<style>
  .preview {
    /* Fixed, not absolute: the channel list scrolls and would clip the card,
       and in the avatar rail there is not even room for it to start. */
    position: fixed;
    z-index: 30;
    width: 320px;
    overflow: hidden;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) * 1.5);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.45);
    /* Never a hover target: it would fight the row it is describing. */
    pointer-events: none;
    /* Hidden for the frame between rendering and measuring, so it cannot be
       seen at the top-left corner first. */
    visibility: hidden;
  }

  .preview.placed {
    visibility: visible;
  }

  .shot {
    display: block;
    width: 100%;
    height: auto;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    /* Twitch's preview is a still from minutes ago; a plain block of colour
       under it while it loads reads better than an empty white gap. */
    background: var(--bg);
  }

  .body {
    padding: 0.6rem 0.7rem 0.65rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  p {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .name {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--text-dim);
  }

  .title {
    font-size: 0.8125rem;
    line-height: 1.35;
    /* Titles run long and are often padded with tags; two lines is enough to
       tell streams apart without the card growing without bound. */
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }

  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.5rem;
    font-size: 0.6875rem;
    color: var(--text-dim);
    margin-top: 0.1rem;
  }

  .game {
    color: var(--accent);
    font-weight: 500;
    /* The category is the one field that can be arbitrarily long. */
    max-width: 60%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .viewers {
    display: flex;
    align-items: center;
    gap: 0.3em;
    color: var(--text);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--live);
  }

  .uptime {
    font-variant-numeric: tabular-nums;
  }
</style>
