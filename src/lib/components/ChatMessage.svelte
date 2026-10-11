<script lang="ts">
  import type { ChatMessage } from '$lib/ipc';
  import { chat as chatLog, formatTime, rgbToCss } from '$lib/stores/chat.svelte';
  import { commands } from '$lib/ipc';
  import { emoteScale, settings } from '$lib/stores/settings.svelte';

  /**
   * The WebView cannot open a new tab, and letting it follow the href would
   * navigate away from Lurkn itself. Hand the URL to the backend, which
   * re-checks the scheme before passing it to the system browser.
   */
  async function openLink(event: MouseEvent, url: string) {
    event.preventDefault();
    await commands.openLink(url);
  }

  interface Props {
    message: ChatMessage;
    /** Tint this row, for the alternating-background setting. */
    alt?: boolean;
    /**
     * Opens the user card. The rect is passed rather than the element so the
     * card can be positioned by the one component that owns it, instead of
     * every row holding a card of its own.
     */
    author?: (author: ChatMessage['author'], at: DOMRect) => void;
    /** The message this one replies to, when it is still in the log. */
    parent?: ChatMessage | null;
    /** Scroll to the parent. Absent when the parent has been trimmed away. */
    jump?: (id: string) => void;
    /** Briefly marked, because a jump just landed here. */
    flash?: boolean;
  }

  let { message, alt = false, author, parent = null, jump, flash = false }: Props = $props();

  const chat = $derived(settings.current.chat);
  const emoteHeight = $derived(`${emoteScale(chat.emoteSize)}em`);
  const deleted = $derived(message.state.state === 'deleted');

  /**
   * A channel event: a sub, a gift, a raid. Its wording is Twitch's own, which
   * is why there is no phrasing to assemble here.
   *
   * A cheer is the exception: it carries no wording because the chatter's own
   * message is the line, and only the bit count is shown beside it.
   */
  const event = $derived(message.event ?? null);
  const notice = $derived(event && event.text.length > 0 ? event : null);
  const bits = $derived(event?.kind === 'cheer' ? event.count : null);
</script>

<!--
  Every token renders through Svelte's escaping interpolation. Raw-HTML injection
  is never used here, and CI greps the whole of src/ to keep it that way
  (docs/architecture.md §11).
-->
<div
  class="message"
  class:highlight={message.kind === 'highlight' && !notice}
  class:deleted
  class:alt
  class:notice={notice !== null}
  class:mentions={message.highlighted}
  class:flash
  data-event={event?.kind ?? null}
  style="--emote-height: {emoteHeight}"
  style:--mark={message.highlightColor ? rgbToCss(message.highlightColor) : null}
  data-message-id={message.id}
>
  {#if message.replyTo && chat.showReplyPreview}
    {#if parent}
      <button
        type="button"
        class="replying"
        title="Jump to the message this replies to"
        onclick={() => jump?.(parent.id)}
      >
        ↩ @{parent.author.displayName}: {parent.rawText}
      </button>
    {:else}
      <!-- Trimmed out of the buffer. Said plainly rather than dropped: a reply
           with no context reads as a non-sequitur. -->
      <span class="replying gone">↩ a message no longer shown</span>
    {/if}
  {/if}
  {#if chat.showTimestamps}
    <span class="time">{formatTime(message.sentAt)}</span>
  {/if}

  {#each chat.showBadges ? message.author.badges : [] as badge (badge.setId + badge.version)}
    {#if badge.url1x}
      <img
        class="badge"
        src={badge.url2x ?? badge.url1x}
        alt={badge.title}
        title={badge.title}
        loading="lazy"
      />
    {/if}
  {/each}

  {#if notice}
    <span class="notice-text">{notice.text}</span>
  {/if}

  {#if !notice || message.tokens.length > 0}
    <button
      type="button"
      class="author"
      style="color: {rgbToCss(message.author.color)}"
      aria-label="About {message.author.displayName}"
      onclick={(e) => author?.(message.author, e.currentTarget.getBoundingClientRect())}
      >{message.author.displayName}</button
    ><span class="colon">:</span>
  {/if}

  {#if bits}
    <span class="bits">{bits.toLocaleString()} bits</span>
  {/if}

  <span class="body" class:action={message.kind === 'action'}>
    {#if deleted}<span class="removed">message removed</span>{/if}
    {#each message.tokens as token, i (i)}
      {#if token.kind === 'text'}{token.text}{:else if token.kind === 'emote'}<img
          class="emote"
          class:zero-width={token.emote.zeroWidth}
          src={token.emote.url2x ?? token.emote.url1x}
          alt={token.emote.name}
          title={token.emote.name}
          loading="lazy"
        />{:else if token.kind === 'link'}<a
          href={token.url}
          rel="noreferrer noopener"
          data-url={token.url}
          onclick={(e) => openLink(e, token.url)}>{token.display}</a
        >{:else if token.kind === 'mention'}<button
          type="button"
          class="mention"
          aria-label="About {token.display}"
          onclick={(e) =>
            author?.(
              // The log's own record when they have spoken, so the card keeps
              // their colour and badges; otherwise what the mention carries,
              // which is enough to look the account up.
              chatLog.authorOf(token.id) ?? {
                id: token.id,
                login: token.login,
                displayName: token.display,
                color: null,
                badges: [],
              },
              e.currentTarget.getBoundingClientRect(),
            )}>{token.display}</button
        >{/if}
    {/each}
  </span>
</div>

<style>
  .message {
    padding: 0.15rem 0.75rem;
    /* A minimum, not a fixed row height: an emote taller than this grows the
       line rather than spilling into its neighbours. */
    line-height: 1.45;
    min-height: 0;
    user-select: text;
    /* A narrow tiled window must wrap long words rather than push the panel
       wider: `anywhere` breaks mid-word, which `break-word` will not do for an
       unbroken run like a pasted URL. */
    overflow-wrap: anywhere;
    min-width: 0;
  }

  .message {
    font-family: var(--chat-font, var(--font));
  }

  .alt {
    background: color-mix(in srgb, var(--text) 4%, transparent);
  }

  /* Hover has to beat the stripe, so it comes after and is a touch stronger. */
  .message:hover,
  .alt:hover {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  /* Removed messages stay in place: a line vanishing without trace reads as a
     bug, and the gap hides what a moderator actually did. */
  .deleted .body,
  .deleted .author,
  .deleted .time {
    opacity: 0.45;
  }

  .deleted .body {
    text-decoration: line-through;
  }

  .removed {
    text-decoration: none;
    font-size: 0.75em;
    color: var(--text-dim);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0 0.3em;
    margin-right: 0.35em;
    display: inline-block;
    opacity: 1;
  }

  .highlight {
    background: color-mix(in srgb, var(--highlight) 14%, transparent);
    box-shadow: inset 3px 0 0 var(--highlight);
  }

  .time {
    color: var(--text-dim);
    font-size: 0.6875rem;
    font-variant-numeric: tabular-nums;
    margin-right: 0.4rem;
  }

  /* A button, but it has to sit in the text like the span it replaced: no box,
     no padding, and the row's own font rather than the browser's control font. */
  .author {
    font: inherit;
    font-weight: 600;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
    /* Wraps with the rest of the line instead of forcing the row wider. */
    display: inline;
    text-align: left;
    overflow-wrap: anywhere;
  }

  .author:hover {
    text-decoration: underline;
  }

  .author:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
    border-radius: 2px;
  }

  .colon {
    color: var(--text-dim);
    margin-right: 0.25rem;
  }

  /* Named, or matching a highlight word. A rule down the edge and a wash of
     colour, the same shape as a channel event but in the accent, so a mention
     is findable when scrolling past without reading. */
  /* A highlight rule may bring its own colour; being named uses --highlight. */
  .message.mentions {
    border-left: 3px solid var(--mark, var(--highlight));
    padding-left: calc(0.75rem - 3px);
    background: color-mix(in srgb, var(--mark, var(--highlight)) 12%, transparent);
  }

  /* A channel event reads as an event, not as someone talking: an accent rule
     down the left edge and a tinted row, with the chatter's own words (a resub
     message, an announcement) following underneath in the ordinary style. */
  .message.notice {
    --event: var(--accent);
    border-left: 3px solid var(--event);
    padding-left: calc(0.75rem - 3px);
    background: color-mix(in srgb, var(--event) 9%, transparent);
  }

  /* One hue per family, so a raid is distinguishable from a gift at a glance
     without reading the line. */
  .message.notice[data-event='raid'] {
    --event: var(--ok);
  }

  .message.notice[data-event='charity'] {
    --event: var(--ok);
  }

  .message.notice[data-event='bitsBadge'] {
    --event: var(--warn);
  }

  .message.notice[data-event='other'] {
    --event: var(--text-dim);
  }

  /* One line, clipped: it is a pointer back to the parent, not a second copy
     of it. The parent is a scroll away in the log. */
  .replying {
    display: block;
    width: 100%;
    font: inherit;
    font-size: 0.85em;
    text-align: left;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  button.replying {
    cursor: pointer;
  }

  button.replying:hover {
    color: var(--text);
    text-decoration: underline;
  }

  button.replying:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
    border-radius: 2px;
  }

  .replying.gone {
    font-style: italic;
  }

  /* Marks where a jump landed, then fades: without it the log looks unchanged
     and the reader has to find the line they were sent to. */
  .message.flash {
    animation: flash 1.4s ease-out;
  }

  @keyframes flash {
    from {
      background: color-mix(in srgb, var(--accent) 35%, transparent);
    }
    to {
      background: transparent;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    /* No fade, but still a mark: the point is to be found, not to animate. */
    .message.flash {
      animation: none;
      background: color-mix(in srgb, var(--accent) 22%, transparent);
    }
  }

  .notice-text {
    color: var(--text);
    font-weight: 600;
    /* Its own line: Twitch's wording is a sentence, not a name, and letting a
       resub message run on from it reads as one long sentence. */
    display: block;
    overflow-wrap: anywhere;
  }

  .bits {
    display: inline-block;
    margin-right: 0.3rem;
    padding: 0 0.35em;
    border-radius: 999px;
    background: color-mix(in srgb, var(--warn) 22%, transparent);
    color: var(--warn);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .body.action {
    font-style: italic;
  }

  /* Inline children need the same treatment: a long <a> or mention would
     otherwise be laid out as one unbreakable box. */
  .body,
  .author,
  a,
  .mention {
    overflow-wrap: anywhere;
    min-width: 0;
  }

  a {
    /* Keep the underline readable when a URL wraps across lines. */
    text-decoration-skip-ink: none;
  }

  .emote {
    height: var(--emote-height, 1.75em);
    vertical-align: middle;
    /* No negative vertical margin: it pulls the image out of the line box, so a
       tall emote overlaps the line above once the emote size exceeds the line
       height. Letting the image grow its own line is what keeps rows clear. */
    margin: 0 0.05em;
  }

  /* 7TV and BTTV overlay emotes stack onto the emote before them. Pulling the
     element back by its own width puts it on top without a wrapper. */
  .emote.zero-width {
    margin-left: calc(var(--emote-height, 1.75em) * -1.03);
    position: relative;
    z-index: 1;
  }

  .badge {
    height: 1.1em;
    width: 1.1em;
    vertical-align: middle;
    margin-right: 0.2em;
  }

  a {
    color: var(--accent);
  }

  /* A button that has to sit in a sentence: the browser's control font, box and
     padding all have to go, leaving only the highlight it already had. */
  .mention {
    display: inline;
    font: inherit;
    font-weight: 600;
    color: inherit;
    border: none;
    cursor: pointer;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border-radius: 3px;
    padding: 0 0.2em;
  }

  .mention:hover {
    background: color-mix(in srgb, var(--accent) 32%, transparent);
  }

  .mention:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
  }
</style>
