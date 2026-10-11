<script lang="ts">
  import ChannelPreview from '$lib/components/ChannelPreview.svelte';
  import MessageMenu, { type MenuItem } from '$lib/components/MessageMenu.svelte';
  import type { FollowedChannel } from '$lib/ipc';
  import { follows, formatUptime, formatViewers } from '$lib/stores/follows.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import type { Side } from '$lib/ui/anchor';

  const sidebar = $derived(settings.current.sidebar);
  const collapsed = $derived(sidebar.collapsed);
  const vertical = $derived(sidebar.position !== 'top');

  /** Away from the edge the drawer is attached to, so it never covers the list. */
  const previewSide = $derived<Side>(
    sidebar.position === 'right' ? 'left' : sidebar.position === 'top' ? 'bottom' : 'right',
  );

  /**
   * Long enough that running the pointer down the list does not strobe cards,
   * short enough that stopping on a row feels like it answered immediately.
   */
  const PREVIEW_DELAY = 350;

  let peekedLogin = $state<string | null>(null);
  let peekedAt = $state<HTMLElement | null>(null);
  // Plain, not `$state`: only the timeout callback reads it.
  let peekTimer: ReturnType<typeof setTimeout> | null = null;

  /**
   * Looked up rather than captured: the follow list is replaced wholesale every
   * refresh, so a captured entry would keep showing the viewer count from the
   * moment the pointer arrived, and would outlive a channel that went offline.
   */
  const peeked = $derived(
    peekedLogin === null
      ? null
      : (follows.view.channels.find((c) => c.channel.user.login === peekedLogin) ?? null),
  );

  function peek(entry: FollowedChannel, row: HTMLElement) {
    // Clear first: returning early for an offline row without clearing leaves
    // the previous channel's card on screen, anchored to a row the pointer has
    // already left.
    clearPeek();

    // Nothing to preview for an offline channel: no title, no viewers, no image.
    if (!entry.stream) return;

    peekTimer = setTimeout(() => {
      peekedLogin = entry.channel.user.login;
      peekedAt = row;
    }, PREVIEW_DELAY);
  }

  function clearPeek() {
    if (peekTimer !== null) {
      clearTimeout(peekTimer);
      peekTimer = null;
    }
    peekedLogin = null;
    peekedAt = null;
  }

  // A card outlives its row if the drawer closes while one is up.
  $effect(() => () => clearPeek());

  // Uptime ticks once a minute; no point re-rendering faster than that.
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 60_000);
    return () => clearInterval(id);
  });

  /**
   * The top rail is one row that scrolls sideways, and most mice only turn
   * up and down. Without this the wheel does nothing over it at all.
   * Registered by hand because the handler has to be allowed to cancel the
   * vertical scroll it replaces.
   */
  let strip = $state<HTMLElement | null>(null);
  $effect(() => {
    const el = strip;
    if (!el || !collapsed || vertical) return;

    const onWheel = (event: WheelEvent) => {
      if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      event.preventDefault();
      el.scrollLeft += event.deltaY;
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  });

  function key(entry: FollowedChannel) {
    return entry.channel.user.id;
  }

  /**
   * A right-click on a channel: what can be done with it, in place of the
   * WebView's own menu. Watching is first because it is the one thing the
   * list cannot otherwise do with a single click — and, with the chat header
   * hidden and the list down to its avatar rail, the obvious way to it.
   */
  let menu = $state<{ login: string; at: { x: number; y: number } } | null>(null);

  const menuItems = $derived.by((): MenuItem[] => {
    if (!menu) return [];
    const login = menu.login;
    const open = follows.selected === login;
    return [
      {
        label: 'Watch the stream',
        hint: open ? 'Ctrl O' : undefined,
        run: () => void follows.watch(login),
      },
      ...(open ? [] : [{ label: 'Open the chat', run: () => follows.select(login) }]),
    ];
  });

  function openMenu(event: MouseEvent, entry: FollowedChannel) {
    event.preventDefault();
    clearPeek();
    menu = { login: entry.channel.user.login, at: { x: event.clientX, y: event.clientY } };
  }

  function summary(entry: FollowedChannel): string {
    const name = entry.channel.user.displayName;
    if (!entry.stream) return `${name} — offline`;
    const category = entry.channel.category?.name ?? 'Live';
    return `${name} — ${category}, ${formatViewers(entry.stream.viewerCount)} viewers`;
  }
</script>

{#if collapsed || follows.drawerOpen}
  {#if !collapsed}
    <!-- Clicking away closes the drawer; the panel stops the click bubbling. -->
    <div
      class="scrim"
      role="presentation"
      onclick={(e) => e.target === e.currentTarget && (follows.drawerOpen = false)}
    ></div>
  {/if}

  <aside
    class="drawer {sidebar.position}"
    class:collapsed
    class:docked={collapsed}
    aria-label="Followed channels"
  >
    {#if !collapsed}
      <header>
        <h2>Following</h2>
        <button
          class="refresh"
          onclick={() => follows.refresh()}
          disabled={follows.view.loading}
          aria-label="Refresh"
          title="Refresh"
        >
          <svg viewBox="0 0 16 16" class:spinning={follows.view.loading} aria-hidden="true">
            <path d="M14 8a6 6 0 1 1-1.8-4.3M14 1v3.5h-3.5" />
          </svg>
        </button>
      </header>
    {/if}

    {#if follows.view.staleReason && !collapsed}
      <p class="stale">{follows.view.staleReason}</p>
    {/if}

    <div class="scroll" class:horizontal={!vertical} bind:this={strip}>
      {#if follows.view.channels.length === 0}
        <p class="empty">
          {follows.view.loading ? 'Loading your channels…' : 'You are not following anyone yet.'}
        </p>
      {:else}
        {#each [['Live', follows.live], ['Offline', follows.offline]] as const as [label, group] (label)}
          {#if group.length > 0}
            {#if !collapsed}
              <h3>{label} <span class="count">{group.length}</span></h3>
            {/if}

            <ul class:horizontal={!vertical}>
              {#each group as entry (key(entry))}
                <li>
                  <button
                    class="row"
                    class:offline={!entry.stream}
                    class:selected={follows.selected === entry.channel.user.login}
                    onclick={() => {
                      clearPeek();
                      follows.select(entry.channel.user.login);
                    }}
                    ondblclick={() => follows.watch(entry.channel.user.login)}
                    oncontextmenu={(e) => openMenu(e, entry)}
                    onmouseenter={(e) => peek(entry, e.currentTarget)}
                    onmouseleave={clearPeek}
                    onfocus={(e) => peek(entry, e.currentTarget)}
                    onblur={clearPeek}
                    aria-label={summary(entry)}
                    title={entry.stream ? undefined : summary(entry)}
                  >
                    <span class="avatar" class:live={entry.stream !== null}>
                      {#if entry.channel.user.avatarUrl}
                        <img src={entry.channel.user.avatarUrl} alt="" loading="lazy" />
                      {:else}
                        <span class="initial">{entry.channel.user.displayName.slice(0, 1)}</span>
                      {/if}
                    </span>

                    {#if !collapsed}
                      <span class="text">
                        <span class="name">{entry.channel.user.displayName}</span>
                        {#if entry.stream}
                          <span class="sub">{entry.channel.category?.name ?? 'Live'}</span>
                        {/if}
                      </span>

                      {#if entry.stream}
                        <span class="meta">
                          <span class="viewers"
                            ><i class="dot"></i>{formatViewers(entry.stream.viewerCount)}</span
                          >
                          <span class="uptime">{formatUptime(entry.stream.startedAt, now)}</span>
                        </span>
                      {/if}
                    {/if}
                  </button>

                  {#if !collapsed}
                    <button
                      class="watch"
                      onclick={() => follows.watch(entry.channel.user.login)}
                      title="Watch the stream"
                      aria-label={`Watch ${entry.channel.user.displayName}`}
                    >
                      <!-- The same television as the chat header: both start
                           watching, so both say so. -->
                      <svg viewBox="0 0 16 16" aria-hidden="true">
                        <rect x="1.5" y="5.5" width="13" height="9" rx="1.5" />
                        <path d="M5 5.5L8 2.5l3 3" />
                      </svg>
                    </button>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        {/each}
      {/if}
    </div>

    {#if follows.openError && !collapsed}
      <p class="stale">{follows.openError}</p>
    {/if}
  </aside>

  {#if peeked?.stream && peekedAt}
    <ChannelPreview entry={peeked} anchor={peekedAt} side={previewSide} {now} />
  {/if}
{/if}

{#if menu}
  <MessageMenu
    items={menuItems}
    at={menu.at}
    label="Channel actions"
    close={() => (menu = null)}
  />
{/if}

<style>
  .scrim {
    position: absolute;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    z-index: 20;
  }

  .drawer {
    position: absolute;
    z-index: 21;
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    background: var(--bg-raised);
    box-shadow: 0 8px 32px rgb(0 0 0 / 0.4);
    /* Never wider or taller than the window, however Hyprland tiles it. */
    max-width: 100%;
    max-height: 100%;
  }

  .drawer.left,
  .drawer.right {
    top: 0;
    bottom: 0;
    width: 260px;
  }

  .drawer.left {
    left: 0;
    border-right: 1px solid var(--border);
  }

  .drawer.right {
    right: 0;
    border-left: 1px solid var(--border);
  }

  .drawer.top {
    left: 0;
    right: 0;
    top: 0;
    max-height: 60%;
    border-bottom: 1px solid var(--border);
  }

  .drawer.left.collapsed,
  .drawer.right.collapsed {
    /* Avatar plus its live ring plus a symmetric gutter; no scrollbar to leave
       room for. */
    width: calc(var(--avatar-size) + 1.25rem);
  }

  /* One row of avatars. The cap is a backstop: whatever the list does, a
     docked strip must never take the chat's height, because nothing in it
     closes and the chat would be gone until the setting was changed back. */
  .drawer.top.collapsed {
    max-height: 30%;
  }

  /* Docked: part of the layout rather than floating over the chat, so it stays
     put as a quick switcher instead of covering what you just opened. */
  .drawer.docked {
    position: relative;
    box-shadow: none;
    flex: 0 0 auto;
  }

  .drawer.docked.left,
  .drawer.docked.right {
    top: auto;
    bottom: auto;
    left: auto;
    right: auto;
    align-self: stretch;
  }

  .drawer.docked.top {
    left: auto;
    right: auto;
    top: auto;
    width: 100%;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.75rem 0.5rem 0.5rem 0.875rem;
  }

  h2 {
    margin: 0;
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-dim);
  }

  h3 {
    margin: 0.75rem 0 0.25rem;
    padding: 0 0.875rem;
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    display: flex;
    gap: 0.4rem;
    align-items: baseline;
  }

  .count {
    font-weight: 400;
    opacity: 0.6;
  }

  .refresh {
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    padding: 0.25rem;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
  }

  .refresh:hover:not(:disabled) {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .refresh svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
  }

  .refresh svg.spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .refresh svg.spinning {
      animation: none;
      opacity: 0.5;
    }
  }

  .scroll {
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding-bottom: 0.75rem;
    /* Appearance comes from the inherited tokens in app.css, so the "show
       scrollbars" setting reaches here without this file knowing about it. */
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .scroll:hover {
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
  }

  /* In the avatar rail there is no room to spare: even a thin scrollbar takes
     its width out of the track and shoves the avatars off centre. Hidden here
     regardless of the setting, since there is nowhere to put it. */
  .collapsed .scroll {
    scrollbar-width: none;
    --scrollbar-size: 0;
  }

  .scroll.horizontal {
    overflow-x: hidden;
    overflow-y: auto;
  }

  /* The docked top strip: live and offline side by side in a single row that
     scrolls sideways, the horizontal twin of the side rail's single column. */
  .collapsed .scroll.horizontal {
    display: flex;
    align-items: center;
    gap: var(--rail-gap);
    overflow-x: auto;
    overflow-y: hidden;
    padding: 0 0.5rem;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  /* The top drawer has the width to spare, so its rows go in columns: the
     same full rows as the side drawer, as many across as fit. */
  ul.horizontal {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
  }

  li {
    position: relative;
    min-width: 0;
  }

  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.625rem;
    /* Right padding reserves the open button's width, so hovering never covers
       the viewer count or uptime and the row does not shift. */
    padding: 0.375rem 2.125rem 0.375rem 0.875rem;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }

  .collapsed .row {
    /* Same height as the chat header, and tall enough that the ring drawn
       around a live avatar fits inside the row rather than being clipped. */
    height: var(--rail-item-height);
    padding: 0;
    justify-content: center;
    gap: 0;
    width: 100%;
  }

  /* No padding on the list itself: the first row has to start flush with the
     top of the rail, or it no longer lines up with the chat header. */
  .collapsed .scroll,
  .collapsed ul {
    padding-top: 0;
  }

  .collapsed ul {
    display: flex;
    flex-direction: column;
    gap: var(--rail-gap);
  }

  /* Live and offline are separate lists; keep the spacing even across the seam. */
  .collapsed ul + ul {
    margin-top: var(--rail-gap);
  }

  /* Without this the side rail's column rule above wins, and the top strip
     stacks every avatar into one column as tall as the follow list. */
  .collapsed ul.horizontal {
    flex-direction: row;
    flex-wrap: nowrap;
    flex: 0 0 auto;
  }

  .collapsed ul.horizontal + ul.horizontal {
    margin-top: 0;
  }

  .collapsed ul.horizontal li {
    flex: 0 0 auto;
    width: calc(var(--avatar-size) + 0.75rem);
  }

  .row:hover {
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .row.selected {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }

  .row:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -2px;
  }

  .avatar {
    width: var(--avatar-size);
    height: var(--avatar-size);
    flex: 0 0 auto;
    border-radius: 50%;
    overflow: hidden;
    display: grid;
    place-items: center;
    background: var(--border);
    font-size: 0.75rem;
    color: var(--text-dim);
  }

  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .avatar.live {
    box-shadow:
      0 0 0 2px var(--bg-raised),
      0 0 0 3.5px var(--accent);
  }

  .row.offline .avatar {
    filter: grayscale(1);
    opacity: 0.55;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.25;
  }

  .name {
    font-size: 0.8125rem;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row.offline .name {
    font-weight: 400;
    color: var(--text-dim);
  }

  .sub {
    font-size: 0.6875rem;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta {
    flex: 0 0 auto;
    white-space: nowrap;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    font-size: 0.6875rem;
    color: var(--text-dim);
    line-height: 1.25;
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
    display: inline-block;
  }

  .uptime {
    font-variant-numeric: tabular-nums;
    opacity: 0.75;
  }

  .watch {
    position: absolute;
    right: 0.375rem;
    top: 50%;
    transform: translateY(-50%);
    opacity: 0;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text-dim);
    cursor: pointer;
    padding: 0.25rem;
    display: grid;
    place-items: center;
  }

  li:hover .watch,
  .watch:focus-visible {
    opacity: 1;
  }

  .watch:hover {
    color: var(--text);
    border-color: var(--text-dim);
  }

  .watch svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
  }

  .empty,
  .stale {
    margin: 0.5rem 0.875rem;
    font-size: 0.75rem;
    color: var(--text-dim);
    overflow-wrap: anywhere;
  }

  .stale {
    color: var(--warn);
  }
</style>
