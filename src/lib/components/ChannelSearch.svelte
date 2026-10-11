<!--
  The channel palette: open any channel by name, followed or not.

  Two sources, in the order they can answer. Your own follows filter instantly
  and locally, so the channel you meant is usually under the cursor before
  Twitch has replied; Twitch's channel search fills in below for everyone else.

  With nothing typed it also suggests. Twitch keeps its personalised
  recommendations on an internal API that third parties are not meant to use,
  so the suggestions are made here instead — live channels in the categories
  and languages your own follows use — and the section says so, so it cannot be
  mistaken for Twitch's feed.
-->
<script lang="ts">
  import { commands, type ChannelHit, type Suggestions } from '$lib/ipc';
  import { rank } from '$lib/chat/complete';
  import { follows, formatViewers } from '$lib/stores/follows.svelte';

  let query = $state('');
  let input = $state<HTMLInputElement | null>(null);
  let active = $state(0);

  let remote = $state<ChannelHit[]>([]);
  let searching = $state(false);
  let error = $state<string | null>(null);

  /** Your follows as hits, so both sources are drawn by one template. */
  const followed = $derived(
    follows.view.channels.map(
      (c): ChannelHit => ({
        id: c.channel.user.id,
        login: c.channel.user.login,
        displayName: c.channel.user.displayName,
        avatarUrl: c.channel.user.avatarUrl,
        live: c.stream !== null,
        category: c.channel.category?.name ?? null,
        title: c.channel.title,
        // Shown for live follows as for suggestions, so the two sections can
        // be compared at a glance.
        viewers: c.stream?.viewerCount ?? null,
      }),
    ),
  );

  /**
   * Live first, then by match quality. With nothing typed, the live follows
   * alone: an empty palette opened at night should show who is on, not the
   * alphabetically first eight of three hundred.
   */
  const local = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const pool = q.length === 0 ? followed.filter((c) => c.live) : followed;
    const ranked = rank(pool, q, (c) => `${c.displayName} ${c.login}`, 8);
    return [...ranked.filter((c) => c.live), ...ranked.filter((c) => !c.live)];
  });

  /** Twitch's results, minus anyone already listed above them. */
  const others = $derived(remote.filter((c) => !follows.follows(c.login)));

  /** Recent unfollowed channels, only while nothing is typed. */
  const recent = $derived(
    query.trim().length === 0 ? follows.recent.filter((c) => !follows.follows(c.login)) : [],
  );

  /** Fetched once per opening; the backend caches it for five minutes. */
  let suggestions = $state<Suggestions | null>(null);

  /**
   * Only while nothing is typed: a search is a request for a particular
   * channel, and suggestions underneath it would be noise. Anyone already
   * listed above — a recent channel, say — is not suggested twice.
   */
  const suggested = $derived.by(() => {
    if (query.trim().length > 0 || !suggestions) return [];
    const shown = new Set([...local, ...recent].map((c) => c.id));
    return suggestions.channels.filter((c) => !shown.has(c.id));
  });

  /** "World of Warcraft and Just Chatting", for the section's reason line. */
  const because = $derived.by(() => {
    const names = suggestions?.because.map((b) => b.name) ?? [];
    if (names.length === 0) return '';
    if (names.length === 1) return names[0] ?? '';
    return `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;
  });

  const rows = $derived([...local, ...recent, ...suggested, ...others]);

  // A different list means a different first row; keep the highlight on top.
  $effect(() => {
    void query;
    active = 0;
  });

  /*
   * Asked of Twitch 250ms after typing stops. Fast enough to feel live, slow
   * enough that typing a name is one request rather than one per letter —
   * and the rate limit is shared with everything else Lurkn does.
   */
  $effect(() => {
    const q = query.trim();
    if (q.length < 2) {
      remote = [];
      error = null;
      searching = false;
      return;
    }

    searching = true;
    let cancelled = false;

    const timer = setTimeout(async () => {
      const result = await commands.searchChannels(q);
      if (cancelled) return;
      searching = false;

      if (result.status === 'ok') {
        remote = result.data;
        error = null;
      } else {
        remote = [];
        error = result.error;
      }
    }, 250);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  $effect(() => {
    input?.focus();
    void follows.loadRecent();

    let cancelled = false;
    void commands.suggestedChannels().then((result) => {
      // No error line for this: suggestions are a nudge, and a failure to
      // fetch them should not put a warning in front of a search box.
      if (!cancelled && result.status === 'ok') suggestions = result.data;
    });
    return () => {
      cancelled = true;
    };
  });

  function close() {
    follows.searchOpen = false;
  }

  function choose(hit: ChannelHit) {
    follows.open(hit);
    close();
  }

  function onKey(event: KeyboardEvent) {
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        active = (active + 1) % Math.max(rows.length, 1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        active = (active - 1 + rows.length) % Math.max(rows.length, 1);
        break;
      case 'Enter': {
        event.preventDefault();
        const hit = rows[active];
        if (hit) choose(hit);
        break;
      }
      case 'Escape':
        event.preventDefault();
        event.stopPropagation();
        close();
        break;
    }
  }

  /** Where a row sits in the flat list, for the highlight. */
  function indexOf(section: 'local' | 'recent' | 'suggested' | 'others', i: number): number {
    if (section === 'local') return i;
    if (section === 'recent') return local.length + i;
    if (section === 'suggested') return local.length + recent.length + i;
    return local.length + recent.length + suggested.length + i;
  }
</script>

<div class="scrim" role="presentation" onclick={close}></div>

<div class="palette" role="dialog" aria-label="Open a channel">
  <input
    bind:this={input}
    bind:value={query}
    placeholder="Open a channel"
    aria-label="Channel name"
    autocapitalize="off"
    autocorrect="off"
    spellcheck="false"
    onkeydown={onKey}
  />

  <div class="results" role="listbox">
    {#snippet row(hit: ChannelHit, index: number)}
      <button
        type="button"
        role="option"
        aria-selected={index === active}
        class:active={index === active}
        onmouseenter={() => (active = index)}
        onclick={() => choose(hit)}
      >
        <span class="avatar">
          {#if hit.avatarUrl}
            <img src={hit.avatarUrl} alt="" loading="lazy" />
          {:else}
            <span class="initial">{hit.displayName.slice(0, 1)}</span>
          {/if}
          {#if hit.live}<i class="dot" aria-label="live"></i>{/if}
        </span>
        <span class="text">
          <span class="name">{hit.displayName}</span>
          {#if hit.category}
            <span class="sub">{hit.category}</span>
          {/if}
        </span>
        {#if hit.viewers !== null && hit.viewers !== undefined}
          <span class="viewers">{formatViewers(hit.viewers)}</span>
        {/if}
      </button>
    {/snippet}

    {#if query.trim().length === 0 && follows.mine}
      <h3>Your channel</h3>
      <button
        type="button"
        class="mine"
        onclick={() => {
          void follows.openMine();
          close();
        }}
      >
        <span class="avatar">
          {#if follows.mine.channel.user.avatarUrl}
            <img src={follows.mine.channel.user.avatarUrl} alt="" loading="lazy" />
          {:else}
            <span class="initial">{follows.mine.channel.user.displayName.slice(0, 1)}</span>
          {/if}
          {#if follows.ownStreamLive}<i class="dot" aria-label="live"></i>{/if}
        </span>
        <span class="text">
          <span class="name">{follows.mine.channel.user.displayName}</span>
          <span class="sub">{follows.ownStreamLive ? 'You are live' : 'Not live'}</span>
        </span>
      </button>
    {/if}

    {#if local.length > 0}
      <h3>{query.trim() ? 'Following' : 'Live now'}</h3>
      {#each local as hit, i (hit.id)}
        {@render row(hit, indexOf('local', i))}
      {/each}
    {/if}

    {#if recent.length > 0}
      <h3>Recent</h3>
      {#each recent as hit, i (hit.id)}
        {@render row(hit, indexOf('recent', i))}
      {/each}
    {/if}

    {#if suggested.length > 0}
      <h3>Suggested</h3>
      <!-- Says why, so it cannot be mistaken for Twitch's own feed. -->
      <p class="why">Live in {because} — what your follows stream most.</p>
      {#each suggested as hit, i (hit.id)}
        {@render row(hit, indexOf('suggested', i))}
      {/each}
    {/if}

    {#if query.trim().length >= 2}
      <h3>On Twitch</h3>
      {#if error}
        <p class="note error">{error}</p>
      {:else if searching && others.length === 0}
        <p class="note">Searching…</p>
      {:else if others.length === 0}
        <p class="note">No other channels match.</p>
      {:else}
        {#each others as hit, i (hit.id)}
          {@render row(hit, indexOf('others', i))}
        {/each}
      {/if}
    {:else if rows.length === 0}
      <p class="note">Type a channel name.</p>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 90;
    background: rgb(0 0 0 / 0.4);
  }

  /* Near the top rather than centred: the eye is already there from the
     window title, and the results grow downwards into free space. */
  .palette {
    position: fixed;
    z-index: 91;
    top: 12%;
    left: 50%;
    transform: translateX(-50%);
    width: min(26rem, calc(100vw - 1rem));
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 36px rgb(0 0 0 / 0.5);
    overflow: hidden;
  }

  input {
    flex: 0 0 auto;
    font: inherit;
    font-size: 0.9375rem;
    padding: 0.7rem 0.8rem;
    border: none;
    border-bottom: 1px solid var(--border);
    background: none;
    color: var(--text);
    user-select: text;
  }

  input:focus {
    outline: none;
  }

  .results {
    overflow-y: auto;
    padding: 0.25rem;
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
  }

  h3 {
    margin: 0.5rem 0.5rem 0.2rem;
    font-size: 0.6875rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.02em;
    color: var(--text-dim);
  }

  button {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    width: 100%;
    font: inherit;
    text-align: left;
    padding: 0.35rem 0.5rem;
    border: none;
    border-radius: calc(var(--radius) - 2px);
    background: none;
    color: var(--text);
    cursor: pointer;
    min-width: 0;
  }

  /* Not part of the keyboard list: it is always in the same place, so it does
     not need to be walked to, and including it would shift every index. */
  .mine {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    width: 100%;
    font: inherit;
    text-align: left;
    padding: 0.35rem 0.5rem;
    border: none;
    border-radius: calc(var(--radius) - 2px);
    background: none;
    color: var(--text);
    cursor: pointer;
  }

  .mine:hover {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  button.active {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }

  .avatar {
    position: relative;
    flex: 0 0 auto;
    width: 1.75rem;
    height: 1.75rem;
  }

  .avatar img,
  .initial {
    width: 100%;
    height: 100%;
    border-radius: 50%;
    object-fit: cover;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--text) 12%, transparent);
    font-size: 0.75rem;
  }

  .dot {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 50%;
    background: var(--live);
    border: 2px solid var(--bg);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    font-size: 0.8125rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sub {
    font-size: 0.6875rem;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .viewers {
    margin-left: auto;
    flex: 0 0 auto;
    font-size: 0.6875rem;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .why {
    margin: 0 0.5rem 0.25rem;
    font-size: 0.6875rem;
    line-height: 1.4;
    color: var(--text-dim);
  }

  .note {
    margin: 0;
    padding: 0.4rem 0.5rem;
    font-size: 0.75rem;
    color: var(--text-dim);
  }

  .note.error {
    color: var(--warn);
  }
</style>
