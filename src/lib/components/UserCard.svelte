<!--
  The card behind a chat name: who the account is, and what they have said in
  this channel since the log was opened.

  The two halves come from different places on purpose. The account details are
  one Helix call, cached in Rust for the session. The messages are filtered out
  of the log the frontend already holds — chat history is not re-fetchable from
  Twitch, so what is on screen is all there is, and saying so is better than
  pretending a longer history exists.
-->
<script lang="ts">
  import ChatMessageRow from '$lib/components/ChatMessage.svelte';
  import { commands, type ChatAuthor, type UserProfile } from '$lib/ipc';
  import { conversation } from '$lib/chat/thread';
  import {
    accountAge,
    accountDate,
    broadcasterLabel,
    isFounder,
    subscriptionLength,
    subscriptionMonths,
  } from '$lib/chat/profile';
  import { chat, rgbToCss } from '$lib/stores/chat.svelte';
  import { whispers } from '$lib/stores/whispers.svelte';
  import { place, type DOMRectLike } from '$lib/ui/anchor';

  interface Props {
    /** Taken from the message that was clicked, so the card has a name at once. */
    author: ChatAuthor;
    /** Where the clicked name is, in viewport coordinates. */
    anchor: DOMRectLike;
    close: () => void;
    /** Scroll the main log to a message, and close the card behind it. */
    jump?: (id: string) => void;
  }

  let { author, anchor, close, jump }: Props = $props();

  /**
   * The exchange this account is part of, not just their own lines: what they
   * answered and what answered them, which is what makes a card readable
   * rather than a list of fragments.
   */
  const RECENT = 12;
  const said = $derived(conversation(chat.messages, author.id, RECENT));

  let profile = $state<UserProfile | null>(null);
  let error = $state<string | null>(null);

  let card = $state<HTMLElement | null>(null);
  /** Viewport coordinates, or null until the card has been measured. */
  let at = $state<{ left: number; top: number } | null>(null);

  const age = $derived(accountAge(profile?.createdAt ?? null));
  const since = $derived(accountDate(profile?.createdAt ?? null));
  const tier = $derived(broadcasterLabel(profile?.broadcasterType ?? ''));

  /**
   * Subscription length comes from the badge on the message, not from an API
   * call: `helix/subscriptions` is readable only by the broadcaster and carries
   * a tier rather than a duration. The badge is what Twitch publishes, and it is
   * already in hand.
   */
  const subMonths = $derived(subscriptionMonths(author.badges));
  const founder = $derived(isFounder(author.badges));

  // Keyed on the author so clicking a second name inside an open card refetches
  // rather than showing the first account's details under the second's name.
  $effect(() => {
    const id = author.id;
    profile = null;
    error = null;

    let cancelled = false;
    void commands.userProfile(id).then((result) => {
      if (cancelled) return;
      if (result.status === 'ok') profile = result.data;
      else error = result.error;
    });

    return () => {
      cancelled = true;
    };
  });

  /**
   * Fixed positioning, as with the other floating boxes: the log scrolls, and an
   * absolutely positioned card would be clipped at its edges. See $lib/ui/anchor.
   */
  function locate() {
    if (!card) return;
    at = place(anchor, card.getBoundingClientRect(), { side: 'bottom', align: 'start' });
  }

  $effect(() => {
    // Re-measure when the content lands: a card that grew after being placed
    // would otherwise hang off the bottom of the window.
    void profile;
    void error;
    void said.length;
    if (!card) return;

    locate();

    const replace = () => locate();
    window.addEventListener('resize', replace);
    // Capture, so the chat log scrolling under the card counts too.
    window.addEventListener('scroll', replace, true);
    return () => {
      window.removeEventListener('resize', replace);
      window.removeEventListener('scroll', replace, true);
    };
  });

  async function openOnTwitch() {
    close();
    await commands.openLink(`https://www.twitch.tv/${author.login}`);
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (!card?.contains(e.target as Node)) close();
  }}
  onkeydown={(e) => {
    if (e.key === 'Escape') close();
  }}
/>

<div
  bind:this={card}
  class="card"
  class:placed={at !== null}
  role="dialog"
  aria-label="About {author.displayName}"
  style:left="{at?.left ?? 0}px"
  style:top="{at?.top ?? 0}px"
>
  <header>
    {#if profile?.user.avatarUrl}
      <img class="avatar" src={profile.user.avatarUrl} alt="" loading="lazy" />
    {:else}
      <span class="avatar placeholder" aria-hidden="true"></span>
    {/if}

    <div class="who">
      <span class="name" style="color: {rgbToCss(author.color)}">{author.displayName}</span>
      <!-- Only when it adds something: for most accounts the login is the
           display name in lower case, and printing both twice says nothing. -->
      {#if author.login.toLowerCase() !== author.displayName.toLowerCase()}
        <span class="login">{author.login}</span>
      {/if}
    </div>

    <button class="close" onclick={close} aria-label="Close">
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
    </button>
  </header>

  {#if author.badges.length > 0}
    <div class="badges">
      {#each author.badges as badge (badge.setId + badge.version)}
        {#if badge.url1x}
          <img src={badge.url2x ?? badge.url1x} alt={badge.title} title={badge.title} />
        {/if}
      {/each}
    </div>
  {/if}

  {#if error}
    <p class="note error">{error}</p>
  {:else if !profile}
    <p class="note">Looking up the account…</p>
  {:else}
    <dl class="facts">
      {#if since}
        <dt>On Twitch since</dt>
        <dd>{since}{age ? ` · ${age}` : ''}</dd>
      {/if}
      {#if tier}
        <dt>Account</dt>
        <dd>{tier}</dd>
      {/if}
      {#if subMonths !== null}
        <dt>{founder ? 'Founder' : 'Subscribed'}</dt>
        <dd>{subscriptionLength(subMonths)}</dd>
      {/if}
    </dl>

    {#if profile.description}
      <p class="bio">{profile.description}</p>
    {/if}
  {/if}

  <section class="said">
    <h3>In this chat</h3>
    {#if said.length === 0}
      <p class="note">Nothing since you opened the channel.</p>
    {:else}
      <!--
        The real chat row, so emotes, badges, colours and reply quotes look the
        same here as in the log. Rows whose author is someone else are dimmed,
        which is what turns a list into a conversation with a side to it.
      -->
      <div class="transcript">
        {#each said as message (message.id)}
          <div class:theirs={message.author.id !== author.id}>
            <ChatMessageRow
              {message}
              parent={chat.byId(message.replyTo)}
              jump={(id) => {
                close();
                jump?.(id);
              }}
            />
          </div>
        {/each}
      </div>
    {/if}
  </section>

  <div class="actions">
    <button
      class="open"
      onclick={() => {
        close();
        void whispers.startWith({
          id: author.id,
          login: author.login,
          name: author.displayName,
        });
      }}
    >
      Whisper
    </button>
    <button class="open" onclick={openOnTwitch}>Open on Twitch</button>
  </div>
</div>

<style>
  .card {
    /* Fixed, not absolute: the scrolling log would clip it. The script supplies
       the coordinates. */
    position: fixed;
    z-index: 70;
    /* Hidden for the frame between rendering and measuring, so it cannot be
       seen at the top-left corner first. */
    visibility: hidden;
    width: min(20rem, calc(100vw - 1rem));
    max-height: min(26rem, 80vh);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.6rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.45);
    font-size: 0.75rem;
    color: var(--text-dim);
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
  }

  .card.placed {
    visibility: visible;
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }

  .avatar {
    flex: 0 0 auto;
    width: 2.5rem;
    height: 2.5rem;
    border-radius: 50%;
    object-fit: cover;
  }

  .avatar.placeholder {
    background: color-mix(in srgb, var(--text) 12%, transparent);
  }

  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    font-size: 0.9375rem;
    font-weight: 700;
    overflow-wrap: anywhere;
  }

  .login {
    font-size: 0.6875rem;
    color: var(--text-dim);
  }

  .close {
    margin-left: auto;
    flex: 0 0 auto;
    display: grid;
    place-items: center;
    width: 1.5rem;
    height: 1.5rem;
    padding: 0;
    border: none;
    border-radius: var(--radius);
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }

  .close svg {
    width: 13px;
    height: 13px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
  }

  .close:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }

  .badges img {
    width: 18px;
    height: 18px;
  }

  .facts {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.15rem 0.5rem;
    margin: 0;
  }

  .facts dt {
    color: var(--text-dim);
  }

  .facts dd {
    margin: 0;
    color: var(--text);
  }

  .bio {
    margin: 0;
    color: var(--text);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .note {
    margin: 0;
  }

  .note.error {
    color: var(--warn);
  }

  .said h3 {
    margin: 0 0 0.3rem;
    font-size: 0.6875rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.02em;
    color: var(--text-dim);
  }

  /* The rows bring the log's own font size with them; the card sets the scale
     back down so a transcript fits without the card growing to log width. */
  .transcript {
    font-size: 0.75rem;
    margin: 0 -0.6rem;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    padding: 0.2rem 0;
  }

  /* The other side of the conversation, set back so the account the card is
     about stays the one being read. */
  .transcript .theirs {
    opacity: 0.72;
  }

  .actions {
    display: flex;
    gap: 0.4rem;
  }

  .actions .open {
    flex: 1 1 0;
    text-align: center;
  }

  .open {
    font: inherit;
    font-size: 0.75rem;
    padding: 0.35rem 0.6rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-raised);
    color: var(--text);
    cursor: pointer;
  }

  .open:hover {
    border-color: var(--accent);
  }
</style>
