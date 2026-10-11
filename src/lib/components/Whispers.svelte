<!--
  Whispers: a thread list, and one conversation at a time.

  Sending is gated twice by Twitch: the account needs a verified phone number,
  and there is a cap of 40 unique recipients a day that cannot be raised. Both
  failures are reported in the words Twitch gives, because they are different
  problems with different fixes.

  The panel is honest about its biggest limitation: EventSub delivers whispers
  live and Twitch has no endpoint for the ones that arrived while Lurkn was
  closed, so every thread starts from the day this feature was switched on.
-->
<script lang="ts">
  import { auth } from '$lib/stores/auth.svelte';
  import { formatTime } from '$lib/stores/chat.svelte';
  import { peerName, whispers } from '$lib/stores/whispers.svelte';

  const open = $derived(whispers.openPeer);
  const peer = $derived(whispers.peerOf(open));
  const title = $derived(open ? (peer?.name ?? peerName(whispers.threads, open)) : 'Whispers');

  /** Twitch's own cap for a first whisper to someone. */
  const MAX_LENGTH = 500;

  let draft = $state('');
  const remaining = $derived(MAX_LENGTH - draft.length);

  /** The name typed into the "new whisper" field, and what came back. */
  let lookup = $state('');
  let looking = $state(false);
  let lookupError = $state<string | null>(null);

  async function startNew(event: SubmitEvent) {
    event.preventDefault();
    if (lookup.trim().length === 0) return;

    looking = true;
    lookupError = null;
    try {
      lookupError = await whispers.startWithLogin(lookup);
      if (!lookupError) lookup = '';
    } finally {
      looking = false;
    }
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (await whispers.send(draft)) draft = '';
  }

  /** Close enough to belong to the same turn. */
  function closeInTime(a: { sentAt: string } | undefined, b: { sentAt: string } | undefined) {
    if (!a || !b) return false;
    const gap = Math.abs(new Date(b.sentAt).getTime() - new Date(a.sentAt).getTime());
    return Number.isFinite(gap) && gap < 5 * 60_000;
  }

  /** The date, for the separators a thread spanning days needs. */
  function dayOf(iso: string): string {
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return '';
    return date.toLocaleDateString(undefined, {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
    });
  }
</script>

<section class="whispers">
  <header>
    {#if open}
      <button class="icon" onclick={() => whispers.back()} aria-label="Back to all whispers">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3L5 8l5 5" /></svg>
      </button>
    {/if}

    <h2>{title}</h2>
    <span class="spacer"></span>

    <button class="icon" onclick={() => (whispers.open = false)} aria-label="Close whispers">
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
    </button>
  </header>

  {#if !auth.whispers}
    <div class="empty">
      <p>Whispers need one more Twitch permission.</p>
      <p class="dim">
        Granting it opens the Twitch login once more. Your current session keeps working
        either way.
      </p>
      <button class="grant" onclick={() => auth.login(true)} disabled={auth.busy}>
        Grant access
      </button>
    </div>
  {:else if whispers.error}
    <p class="empty error">{whispers.error}</p>
  {:else if open}
    <div class="thread">
      {#each whispers.thread as message, i (message.id)}
        {@const previous = whispers.thread[i - 1]}
        {@const newDay = !previous || dayOf(previous.sentAt) !== dayOf(message.sentAt)}
        <!--
          Runs from the same person are grouped: only the last bubble of a run
          gets the pinched corner, so a burst reads as one turn rather than as
          several unrelated ones.
        -->
        {@const grouped =
          !newDay && previous?.outgoing === message.outgoing && closeInTime(previous, message)}
        {@const last =
          whispers.thread[i + 1]?.outgoing !== message.outgoing ||
          !closeInTime(message, whispers.thread[i + 1])}

        {#if newDay}
          <p class="day">{dayOf(message.sentAt)}</p>
        {/if}

        <div class="row" class:mine={message.outgoing} class:grouped>
          <div class="bubble" class:tail={last}>
            <span class="body">{message.body}</span>
            <span class="time">{formatTime(message.sentAt)}</span>
          </div>
        </div>
      {/each}

      {#if whispers.thread.length === 0}
        <p class="empty dim">
          Nothing recorded with {peer?.name ?? 'them'} yet. Anything sent before Lurkn was
          running is only on twitch.tv.
        </p>
      {/if}
    </div>

    <p class="footnote">
      Only whispers that arrived while Lurkn was running are here — Twitch does not serve
      older ones.
    </p>

    <form onsubmit={submit}>
      <input
        bind:value={draft}
        placeholder="Whisper {peer?.name ?? ''}"
        maxlength={MAX_LENGTH}
        disabled={whispers.sending || !peer}
        aria-label="Send a whisper"
      />
      {#if remaining < 60}
        <span class="counter" class:warn={remaining < 0}>{remaining}</span>
      {/if}
      <button type="submit" disabled={whispers.sending || !peer || draft.trim().length === 0}>
        Send
      </button>
    </form>

    {#if whispers.sendError}
      <p class="send-error">{whispers.sendError}</p>
    {/if}
  {:else if whispers.threads.length === 0}
    <div class="empty">
      <p class="dim">No whispers yet.</p>
      <p class="dim">
        They arrive while Lurkn is running. Twitch has no history to catch up on, so
        anything sent while it is closed will only be on twitch.tv.
      </p>
    </div>
  {:else}
    <form class="lookup" onsubmit={startNew}>
      <input
        bind:value={lookup}
        placeholder="Whisper someone by name"
        aria-label="Twitch account name"
        disabled={looking}
        autocapitalize="off"
        autocorrect="off"
        spellcheck="false"
      />
      <button type="submit" disabled={looking || lookup.trim().length === 0}>Start</button>
    </form>

    {#if lookupError}
      <p class="send-error">{lookupError}</p>
    {/if}

    <ul class="threads">
      {#each whispers.threads as thread (thread.peerId)}
        <li>
          <button onclick={() => whispers.openThread(thread.peerId)}>
            <span class="who">{thread.peerName}</span>
            <span class="preview">
              {thread.lastOutgoing ? 'You: ' : ''}{thread.lastBody}
            </span>
            {#if thread.unread > 0}
              <span class="badge">{thread.unread}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .whispers {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
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
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .spacer {
    flex: 1 1 auto;
  }

  .icon {
    flex: 0 0 auto;
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
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
  }

  .icon:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  .threads {
    list-style: none;
    margin: 0;
    padding: 0.25rem;
    overflow-y: auto;
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
  }

  .threads button {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    width: 100%;
    font: inherit;
    font-size: 0.8125rem;
    text-align: left;
    padding: 0.35rem 0.5rem;
    border: none;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    cursor: pointer;
    min-width: 0;
  }

  .threads button:hover {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .who {
    flex: 0 0 auto;
    font-weight: 600;
  }

  /* The preview is the part that gives: the name must stay readable. */
  .preview {
    flex: 0 1 auto;
    min-width: 0;
    font-size: 0.75rem;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .badge {
    margin-left: auto;
    flex: 0 0 auto;
    min-width: 1.2em;
    padding: 0 0.35em;
    border-radius: 999px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 0.6875rem;
    font-weight: 600;
    text-align: center;
  }

  .thread {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.5rem 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
  }

  .day {
    align-self: center;
    margin: 0.5rem 0 0.2rem;
    font-size: 0.6875rem;
    color: var(--text-dim);
  }

  /* Bottom-anchors a short conversation against the composer. A growing spacer
     rather than `justify-content: flex-end`, which pushes the earliest messages
     above the scroll container's origin where no scrollbar reaches them — the
     same reason the chat log does it this way. */
  .thread::before {
    content: '';
    flex: 1 1 0;
  }

  /* Nothing else may flex: a bubble allowed to shrink squashes below its own
     text and the lines overlap. */
  .thread > * {
    flex: 0 0 auto;
  }

  .row {
    display: flex;
    justify-content: flex-start;
  }

  .row.mine {
    justify-content: flex-end;
  }

  /* A continuation of the same turn sits tighter against the one above it. */
  .row.grouped {
    margin-top: -0.1rem;
  }

  .bubble {
    max-width: min(34rem, 78%);
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.3rem 0.55rem;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--text) 9%, transparent);
    color: var(--text);
    font-size: 0.8125rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
    min-width: 0;
  }

  /* Accent-tinted rather than solid accent: white-on-purple would out-shout the
     chat behind the panel, and these are the messages you already know about. */
  .row.mine .bubble {
    background: color-mix(in srgb, var(--accent) 26%, transparent);
  }

  /* The pinched corner points at whoever is speaking, and only on the last
     bubble of a run. */
  .bubble.tail {
    border-bottom-left-radius: 3px;
  }

  .row.mine .bubble.tail {
    border-bottom-left-radius: var(--radius);
    border-bottom-right-radius: 3px;
  }

  /* Trails the text on the same line, so a one-word message stays one line. */
  .time {
    flex: 0 0 auto;
    margin-left: auto;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    font-size: 0.6875rem;
  }

  .empty {
    margin: 0;
    padding: 1rem 0.75rem;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.5rem;
    font-size: 0.8125rem;
  }

  .empty p {
    margin: 0;
  }

  .dim {
    color: var(--text-dim);
    font-size: 0.75rem;
    line-height: 1.5;
  }

  .error {
    color: var(--warn);
  }

  .grant {
    font: inherit;
    font-size: 0.8125rem;
    padding: 0.35rem 0.8rem;
    border-radius: var(--radius);
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--on-accent);
    cursor: pointer;
  }

  .grant:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* At the top, where a thread list is scanned from, rather than buried under
     the conversations it is meant to sit beside. */
  .lookup {
    border-top: none;
    border-bottom: 1px solid var(--border);
  }

  form {
    --control-height: 2rem;
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    border-top: 1px solid var(--border);
    min-width: 0;
  }

  form input {
    flex: 1;
    min-width: 0;
    font: inherit;
    font-size: 0.8125rem;
    height: var(--control-height);
    padding: 0 0.6rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    user-select: text;
  }

  form input:focus {
    outline: none;
    border-color: var(--accent);
  }

  form button {
    flex: 0 0 auto;
    font: inherit;
    font-size: 0.8125rem;
    height: var(--control-height);
    padding: 0 0.9rem;
    border-radius: var(--radius);
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--on-accent);
    cursor: pointer;
  }

  form button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .counter {
    font-size: 0.6875rem;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .counter.warn {
    color: var(--warn);
  }

  .send-error {
    flex: 0 0 auto;
    margin: 0;
    padding: 0 0.75rem 0.5rem;
    font-size: 0.75rem;
    line-height: 1.45;
    color: var(--warn);
  }

  .footnote {
    flex: 0 0 auto;
    margin: 0;
    padding: 0.4rem 0.75rem;
    border-top: 1px solid var(--border);
    font-size: 0.6875rem;
    line-height: 1.45;
    color: var(--text-dim);
  }
</style>
