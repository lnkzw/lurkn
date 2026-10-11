<script lang="ts">
  import { tick, untrack } from 'svelte';

  import ChatMessageRow from '$lib/components/ChatMessage.svelte';
  import SidebarIcon from '$lib/components/SidebarIcon.svelte';
  import StatusBar from '$lib/components/StatusBar.svelte';
  import TitleCommands from '$lib/components/TitleCommands.svelte';
  import UserCard from '$lib/components/UserCard.svelte';
  import MessageMenu, { type MenuItem } from '$lib/components/MessageMenu.svelte';
  import Completions from '$lib/components/Completions.svelte';
  import EmotePicker from '$lib/components/EmotePicker.svelte';
  import { copyText } from '$lib/clipboard';
  import { captureAnchor, restoreAnchor, stillFollowing } from '$lib/ui/scroll';
  import { moderation, TIMEOUTS } from '$lib/stores/moderation.svelte';
  import { expand as expandCommand, findCommand, type CommandVars } from '$lib/chat/custom';
  import { contextOf, parseFilter, passes, type Expr } from '$lib/chat/filter';
  import { auth } from '$lib/stores/auth.svelte';
  import {
    activeToken,
    apply as applyCompletion,
    builtinSlash,
    rank,
    parseModCommand,
    parseWhisperCommand,
    SLASH_NOTE,
    type Suggestion,
  } from '$lib/chat/complete';
  import type { ChatAuthor, ChatMessage } from '$lib/ipc';
  import { titleCommands } from '$lib/chat/commands';
  import { chat, MAX_MESSAGE_LENGTH, unreadIndex } from '$lib/stores/chat.svelte';
  import { commands, events } from '$lib/ipc';
  import { chatFontStack, settings } from '$lib/stores/settings.svelte';

  import { follows } from '$lib/stores/follows.svelte';
  import { whispers } from '$lib/stores/whispers.svelte';
  import type { FollowedChannel } from '$lib/ipc';

  interface Props {
    /** `null` until a channel is picked from the drawer. */
    channel: FollowedChannel | null;
  }

  let { channel }: Props = $props();

  const channelName = $derived(channel?.channel.user.displayName ?? 'Lurkn');
  const streamTitle = $derived(channel?.channel.title ?? null);
  /** The `!commands` this stream's title advertises, offered by the composer. */
  const advertised = $derived(titleCommands(streamTitle));

  const chatSettings = $derived(settings.current.chat);
  const statusBar = $derived(chatSettings.showStatusBar);
  const fontStack = $derived(chatFontStack(chatSettings.font, chatSettings.customFont));
  /** The filters switched on, parsed once rather than per message. */
  const activeFilters = $derived(
    settings.current.filters
      .filter((f) => f.enabled)
      .map((f) => parseFilter(f.expression))
      // A broken filter is reported in settings; here it is skipped, because
      // hiding the whole chat over a typo is worse than not filtering.
      .flatMap((p): Expr[] => (p.ok ? [p.expr] : [])),
  );

  /**
   * Ignored chatters are dropped in Rust as they arrive; this hides the lines
   * they had already sent, so ignoring someone takes effect at once.
   */
  const ignoredUsers = $derived(new Set(settings.current.ignores.users));

  /**
   * A channel event carries wording of its own; a cheer is an ordinary message
   * that happens to have bits on it, so turning events off must not hide it.
   */
  const visible = $derived.by(() => {
    const showEvents = chatSettings.showChannelEvents;
    const filters = activeFilters;
    const where = { login: channel?.channel.user.login ?? null, live: channel?.stream != null };

    if (showEvents && filters.length === 0 && ignoredUsers.size === 0) return chat.messages;

    return chat.messages.filter((m) => {
      if (!showEvents && m.event && m.event.kind !== 'cheer') return false;
      if (ignoredUsers.has(m.author.login)) return false;
      if (filters.length === 0) return true;
      const context = contextOf(m, where);
      return filters.every((f) => passes(f, context));
    });
  });

  /** The trigger only earns its place in the field when it has something to offer. */
  const showCommands = $derived(chatSettings.showCommandButton && advertised.length > 0);

  /**
   * The open user card, if any. One card for the whole log rather than one per
   * row: only one can be open, and a card per message would mean 500 of them.
   */
  let card = $state<{ author: ChatAuthor; at: DOMRect } | null>(null);

  /** The open right-click menu, if any. */
  let menu = $state<{ items: MenuItem[]; at: { x: number; y: number } } | null>(null);
  /** The message the composer is replying to, if any. */
  let replyTo = $state<ChatMessage | null>(null);
  /** The message a jump just landed on, marked briefly so it can be found. */
  let flashed = $state<string | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;

  let draft = $state('');
  let input = $state<HTMLInputElement | null>(null);
  /** Caret position, tracked so the active token can be worked out from it. */
  let caret = $state(0);
  /** Which suggestion is highlighted. */
  let picked = $state(0);
  /** Dismissed with Escape; typing again brings the list back. */
  let dismissed = $state(false);
  /** Whether the composer has focus, for the hint line. */
  let composerFocused = $state(false);
  /** The channel's third-party emotes, fetched once per channel. */
  let emotes = $state<Suggestion[]>([]);
  /** Messages already sent, newest last, for recall with the up arrow. */
  let history = $state<string[]>([]);
  /** How far back the up arrow has walked; -1 is the live draft. */
  let recall = $state(-1);
  let log = $state<HTMLDivElement | null>(null);
  /** Stick to the bottom unless the reader has scrolled up to read back. */
  let pinned = $state(true);
  let focused = $state(typeof document === 'undefined' ? true : document.hasFocus());

  /** Reading means: this window has focus and the newest line is on screen. */
  const reading = $derived(focused && pinned && channel !== null);

  /** Where the unread divider goes, or -1 for no divider. */
  // Indexed against the rendered list, not the raw log: with channel events
  // hidden the two differ, and the divider would land on the wrong row.
  const unreadAt = $derived(
    chatSettings.showUnreadDivider ? unreadIndex(visible, chat.lastSeenId) : -1,
  );
  const unreadCount = $derived(unreadAt < 0 ? 0 : visible.length - unreadAt);

  const remaining = $derived(MAX_MESSAGE_LENGTH - draft.length);
  const connecting = $derived(
    chat.status.state === 'connecting' || chat.status.state === 'reconnecting',
  );

  /** Where the last scroll left the log, to tell up from down. */
  let lastTop = 0;

  function onScroll() {
    if (!log) return;
    const distance = log.scrollHeight - log.scrollTop - log.clientHeight;
    pinned = stillFollowing(pinned, distance, log.scrollTop < lastTop - 1);
    lastTop = log.scrollTop;
  }

  /**
   * A wheel turned upwards stops following before the scroll lands. The scroll
   * event comes a frame later, and a batch arriving in between would otherwise
   * cancel the smooth scroll and snap the log back down.
   */
  function onWheel(event: WheelEvent) {
    if (event.deltaY < 0 && log && log.scrollHeight > log.clientHeight) pinned = false;
  }

  /*
   * The divider is placed at the moment you *stop* reading,
   * and then stays put while you read back down — so it marks where you left
   * off rather than chasing the newest line. Messages arriving while you are
   * away must not move it, hence `untrack`: this effect depends on `reading`
   * only.
   */
  $effect(() => {
    if (!reading) untrack(() => chat.markSeen());
  });

  // The jump-to-newest shortcut, which the app raises and the log owns.
  $effect(() => {
    void chat.jumpRequest;
    if (!log) return;
    pinned = true;
    log.scrollTop = log.scrollHeight;
  });

  /*
   * Before the log redraws: follow the newest line while pinned, otherwise
   * remember the row at the top of the view and put it back afterwards, so
   * trimming and backfill do not slide the log under the reader. `pre`, and
   * `tick` for the second half, because the anchor has to be measured before
   * the DOM changes and restored after.
   */
  $effect.pre(() => {
    void visible;
    const el = untrack(() => log);
    if (!el) return;

    if (untrack(() => pinned)) {
      void tick().then(() => (el.scrollTop = el.scrollHeight));
      return;
    }

    const mark = captureAnchor(el);
    if (!mark) return;
    void tick().then(() => {
      // Trimmed away, and everything above it with it: the oldest line left
      // is the nearest thing to where the reader was.
      if (!restoreAnchor(el, mark)) el.scrollTop = 0;
    });
  });

  // Scrolled up, the buffer keeps more, so what is being read is not trimmed.
  $effect(() => {
    chat.hold(!pinned);
  });

  $effect(() => {
    void channel?.channel.user.id;
    card = null;
    menu = null;
    replyTo = null;
    history = [];
    recall = -1;
    // A new channel starts at its newest line, whatever the last one was doing.
    pinned = true;
  });

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const typed = draft;

    const custom = findCommand(typed, settings.current.commands);
    const text = custom ? expandCommand(custom.command.template, custom.args, commandVars()) : typed;
    if (custom && text.length === 0) {
      chat.sendError = `${custom.command.trigger} expands to nothing.`;
      return;
    }

    if (await dispatch(text, replyTo?.id ?? null)) {
      // What was typed, not what it expanded to: the up arrow should bring
      // back `/hi forsen`, ready to run again on someone else.
      // Kept at 30: enough to walk back through a conversation, not so much
      // that it becomes a log of its own.
      history = [...history, typed].slice(-30);
      recall = -1;
      draft = '';
      replyTo = null;
      pinned = true;
    }
  }

  /**
   * Carry out one line: a moderation command, a whisper, or chat. Shared by the
   * composer and by commands run from a message's menu. True when it went
   * through.
   */
  async function dispatch(text: string, reply: string | null): Promise<boolean> {
    // Moderation commands are carried out here rather than sent: Twitch strips
    // slash commands out of chat, so typing /ban has done nothing in any app
    // since IRC commands were removed.
    const mod = parseModCommand(text);
    if (mod) return runModCommand(mod);

    // `/w name message` never goes to chat: Twitch would strip it or drop the
    // message, so Lurkn sends it as a whisper, which is what was meant.
    const whisper = parseWhisperCommand(text);
    if (whisper) {
      chat.sendError = null;
      const failed = await whispers.startWithLogin(whisper.login);
      if (failed) {
        chat.sendError = failed;
        return false;
      }

      // `/w name` on its own just opens the conversation, ready to type.
      if (whisper.body.length > 0 && !(await whispers.send(whisper.body))) {
        chat.sendError = whispers.sendError;
        return false;
      }
      return true;
    }

    return chat.send(text, reply);
  }

  /** What a custom command can fill in, as things stand. */
  function commandVars(message: ChatMessage | null = null): CommandVars {
    const me = auth.status.kind === 'authenticated' ? auth.status : null;
    return {
      'channel.name': channel?.channel.user.login,
      'channel.id': channel?.channel.user.id,
      'stream.title': channel?.stream?.title ?? channel?.channel.title ?? undefined,
      'stream.game': channel?.stream?.category?.name ?? channel?.channel.category?.name,
      'my.name': me?.login,
      'my.id': me?.userId,
      'user.name': message?.author.login,
      'user.id': message?.author.id,
      'msg.id': message?.id,
      'msg.text': message?.rawText,
    };
  }

  /** A custom command run from a message's menu, about that message. */
  async function runFromMenu(template: string, message: ChatMessage) {
    const text = expandCommand(template, [], commandVars(message));
    if (text.length > 0 && (await dispatch(text, null))) pinned = true;
  }

  /**
   * Scroll to the message a reply answers and mark it.
   *
   * Unpins deliberately: the parent is above the newest line, so following
   * incoming messages would scroll straight back off it.
   */
  function jumpTo(id: string) {
    const row = log?.querySelector(`[data-message-id="${CSS.escape(id)}"]`);
    if (!row) return;

    pinned = false;
    row.scrollIntoView({ block: 'center', behavior: 'smooth' });

    flashed = id;
    if (flashTimer) clearTimeout(flashTimer);
    // Just past the fade, so the class is gone before it could be re-triggered.
    flashTimer = setTimeout(() => (flashed = null), 1500);
  }

  /**
   * Carry out a moderation command typed in the composer.
   *
   * Reports through the same error line the menu actions use: a command that
   * failed because the reader is not a moderator here should say so once, in
   * one place, however it was invoked.
   */
  async function runModCommand(command: ReturnType<typeof parseModCommand>): Promise<boolean> {
    if (!command) return false;

    if (!channelId) {
      chat.sendError = 'Open a channel first.';
      return false;
    }
    if (!canModerate) {
      chat.sendError = moderation.granted
        ? 'You are not a moderator in this channel.'
        : 'Grant Lurkn permission to moderate in settings first.';
      return false;
    }

    const id = channelId;
    chat.sendError = null;

    /** The login a command names, resolved against who has spoken here. */
    const idOf = (login: string) =>
      chat.messages.find((m) => m.author.login === login)?.author.id ?? null;

    switch (command.name) {
      case 'clear':
        return moderation.clearChat(id);
      case 'announce':
        return command.rest
          ? moderation.announce(id, command.rest)
          : ((chat.sendError = 'Nothing to announce.'), false);
      case 'emoteonly':
        return moderation.room(id, { emoteOnly: true });
      case 'emoteonlyoff':
        return moderation.room(id, { emoteOnly: false });
      case 'subscribers':
        return moderation.room(id, { subscriberOnly: true });
      case 'subscribersoff':
        return moderation.room(id, { subscriberOnly: false });
      case 'uniquechat':
        return moderation.room(id, { uniqueOnly: true });
      case 'uniquechatoff':
        return moderation.room(id, { uniqueOnly: false });
      case 'slow':
        // Twitch's own default when no wait is given.
        return moderation.room(id, { slow: true, slowSeconds: command.amount ?? 30 });
      case 'slowoff':
        return moderation.room(id, { slow: false });
      case 'followers':
        return moderation.room(id, {
          followerOnly: true,
          followerMinutes: command.amount ?? 0,
        });
      case 'followersoff':
        return moderation.room(id, { followerOnly: false });
      case 'delete':
        // Twitch's /delete takes a message id nobody can see; the right-click
        // menu is how a message is chosen, so this says so rather than failing.
        chat.sendError = 'Right-click the message you want to delete.';
        return false;
      default: {
        const login = command.target;
        if (!login) return false;

        const user = idOf(login);
        if (!user) {
          // Only the open log can turn a name into an id without a lookup.
          chat.sendError = `${login} has not spoken here, so Lurkn cannot act on them yet.`;
          return false;
        }

        if (command.name === 'unban') return moderation.unban(id, user);
        if (command.name === 'timeout') {
          return moderation.timeout(id, user, command.amount ?? 600, command.rest);
        }
        return moderation.ban(id, user, command.rest);
      }
    }
  }

  function reply(message: ChatMessage) {
    replyTo = message;
    input?.focus();
  }

  /**
   * Everyone who has spoken in the open log, most recent first.
   *
   * The log is the only roster available: Twitch's chatters endpoint is
   * moderator-only, so a viewer client knows exactly who it has seen.
   */
  const chatters = $derived.by(() => {
    // A plain record rather than a Map: this is a throwaway inside a derived,
    // not reactive state, and the Svelte lint rule cannot tell the difference.
    const seen: Record<string, true> = Object.create(null);
    const names: string[] = [];

    for (let i = chat.messages.length - 1; i >= 0; i--) {
      const author = chat.messages[i]?.author;
      if (!author || seen[author.id]) continue;
      seen[author.id] = true;
      names.push(author.displayName);
    }

    return names;
  });

  const token = $derived(dismissed ? null : activeToken(draft, caret));

  const suggestions = $derived.by((): Suggestion[] => {
    if (!token) return [];

    switch (token.kind) {
      case 'user':
        return rank(chatters, token.query, (n) => n).map((name) => ({
          kind: 'user' as const,
          value: name,
          label: name,
        }));

      case 'command': {
        // The reader's own `!` commands first: they wrote them to be used.
        const own = triggers('!').map((name) => ({ name, detail: 'yours' }));
        const titled = advertised
          .filter((name) => !own.some((o) => o.name === name))
          .map((name) => ({ name, detail: undefined }));
        return rank([...own, ...titled], token.query, (c) => c.name).map((c) => ({
          kind: 'command' as const,
          value: c.name,
          label: `!${c.name}`,
          detail: c.detail,
        }));
      }

      case 'emote':
        return rank(emotes, token.query, (e) => e.value);

      // What Lurkn carries out itself, and the reader's own commands.
      case 'slash': {
        const own = triggers('/').map((name) => ({ name, detail: 'yours' }));
        const builtin = builtinSlash(canModerate).filter((b) => !own.some((o) => o.name === b.name));
        return rank([...own, ...builtin], token.query, (c) => c.name).map((c) => ({
          kind: 'slash' as const,
          value: c.name,
          label: `/${c.name}`,
          detail: c.detail,
        }));
      }
    }
  });

  /** The reader's command triggers that start with a sigil, without it. */
  function triggers(sigil: string): string[] {
    return settings.current.commands
      .map((c) => c.trigger.trim())
      .filter((t) => t.length > 1 && t.startsWith(sigil))
      .map((t) => t.slice(1));
  }

  // Only when nothing matches: a slash that completes needs no lecture.
  const note = $derived(token?.kind === 'slash' && suggestions.length === 0 ? SLASH_NOTE : null);
  const showCompletions = $derived(Boolean(token) && (suggestions.length > 0 || note !== null));

  /**
   * The hint takes the slot the suggestions will take, so it teaches where the
   * feature happens. It is only up while there is nothing to suggest — an empty
   * composer with the caret in it — so the first character typed replaces it
   * with the real thing.
   */
  const showHints = $derived(
    chatSettings.showComposerHints &&
      composerFocused &&
      draft.length === 0 &&
      channel !== null &&
      !showCompletions,
  );

  /** One entry per thing the composer does that is not obvious from looking. */
  const hints = $derived([
    { key: '@', what: 'name' },
    ...(advertised.length > 0 ? [{ key: '!', what: 'channel command' }] : []),
    ...(emotes.length > 0 ? [{ key: ':', what: 'emote' }] : []),
    { key: 'Tab', what: 'complete' },
    ...(history.length > 0 ? [{ key: '↑', what: 'last sent' }] : []),
  ]);

  // A new token means a new list, so the highlight goes back to the top rather
  // than pointing at whatever index happened to survive.
  $effect(() => {
    void token?.text;
    picked = 0;
  });

  /*
   * Emotes belong to a channel, so they are refetched when it changes — and
   * again when the backend says the sets have landed.
   *
   * The registry is filled by a background task: selecting a channel starts a
   * fetch of the 7TV, BTTV and FFZ sets, and until it finishes the registry
   * still holds the previous channel's emotes, or the globals alone. Reading it
   * once at switch time is therefore a race the switch usually wins, which is
   * why this also listens for EmotesReady.
   */
  $effect(() => {
    const id = channel?.channel.user.id;
    if (!id) {
      emotes = [];
      return;
    }

    let cancelled = false;

    const load = async () => {
      const result = await commands.chatEmotes();
      if (cancelled || result.status !== 'ok') return;
      emotes = result.data.map((emote) => ({
        kind: 'emote' as const,
        value: emote.name,
        label: emote.name,
        image: emote.url,
        detail: emote.provider,
      }));
    };

    void load();

    let off: (() => void) | null = null;
    void events.emotesReady.listen(() => void load()).then((unlisten) => {
      // The effect may have been torn down while the listener was being set up.
      if (cancelled) unlisten();
      else off = unlisten;
    });

    return () => {
      cancelled = true;
      off?.();
    };
  });

  function accept(item: Suggestion) {
    if (!token) return;

    const next = applyCompletion(draft, token, item);
    draft = next.draft;
    dismissed = true;

    // The caret has to be moved after Svelte has written the new value, or the
    // browser puts it at the end of the old one.
    queueMicrotask(() => {
      input?.setSelectionRange(next.caret, next.caret);
      caret = next.caret;
      input?.focus();
      // Re-armed only once the caret is settled, so the freshly inserted word
      // does not immediately suggest against itself.
      dismissed = false;
    });
  }

  /**
   * Keys the suggestion list and the history recall claim before the composer
   * sees them. Returns true when the key was used.
   */
  function onComposerKey(event: KeyboardEvent): boolean {
    if (showCompletions) {
      switch (event.key) {
        case 'ArrowDown':
          picked = (picked + 1) % Math.max(suggestions.length, 1);
          return true;
        case 'ArrowUp':
          picked = (picked - 1 + suggestions.length) % Math.max(suggestions.length, 1);
          return true;
        case 'Tab':
        case 'Enter': {
          const item = suggestions[picked];
          if (!item) return false;
          accept(item);
          return true;
        }
        case 'Escape':
          dismissed = true;
          return true;
      }
    }

    // Tab completes even before a list is showing, which is how chat clients
    // have always behaved.
    if (event.key === 'Tab' && suggestions.length > 0) {
      const item = suggestions[picked];
      if (item) {
        accept(item);
        return true;
      }
    }

    // An empty composer walks back through what was sent, like a shell.
    if (event.key === 'ArrowUp' && draft.length === 0 && history.length > 0 && recall === -1) {
      recall = history.length - 1;
      draft = history[recall] ?? '';
      return true;
    }
    if (recall >= 0 && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
      const step = event.key === 'ArrowUp' ? -1 : 1;
      const next = recall + step;
      if (next < 0) return true;
      if (next >= history.length) {
        recall = -1;
        draft = '';
        return true;
      }
      recall = next;
      draft = history[next] ?? '';
      return true;
    }

    return false;
  }

  /**
   * Right-click anywhere in the log: never the WebView's own menu, which offers
   * Reload and Inspect Element — the browser showing through where a chat client
   * should be. Over a message, Lurkn's menu takes its place.
   */
  function onContextMenu(event: MouseEvent) {
    event.preventDefault();

    const target = event.target as HTMLElement | null;
    const row = target?.closest?.('[data-message-id]');
    const id = row?.getAttribute('data-message-id');
    const message = chat.byId(id ?? null);
    if (!message) {
      // Between messages, the chat's own menu: the one way to it when the
      // header is hidden.
      appMenu = { x: event.clientX, y: event.clientY };
      return;
    }

    // Right-clicking the link itself, rather than the message around it, is a
    // question about the link.
    const url = target?.closest?.('a[data-url]')?.getAttribute('data-url') ?? null;

    menuFor(message, { x: event.clientX, y: event.clientY }, url);
  }

  /**
   * What right-clicking a message offers. Assembled here rather than in the row
   * so the menu has the composer and the user card in reach.
   */
  /** The channel being read, as the moderation API names it. */
  const channelId = $derived(channel?.channel.user.id ?? null);
  const canModerate = $derived(moderation.canModerate(channelId ?? undefined));

  // Fetched once, and only for a session that granted the permission.
  $effect(() => {
    void moderation.load();
  });

  /**
   * Moderation entries for one message.
   *
   * Only where the reader actually holds the role: a timeout button in a
   * channel they do not moderate is a button that fails. A ban is destructive
   * and public, so it asks first; a timeout is reversible and does not.
   */
  function moderationItems(message: ChatMessage): MenuItem[] {
    if (!canModerate || !channelId) return [];

    const id = channelId;
    const name = message.author.displayName;
    const user = message.author.id;

    return [
      {
        label: 'Delete message',
        divider: true,
        run: () => void moderation.deleteMessage(id, message.id),
      },
      ...TIMEOUTS.map((t) => ({
        label: `Timeout ${name} — ${t.label}`,
        run: () => void moderation.timeout(id, user, t.seconds),
      })),
      {
        label: `Ban ${name}`,
        run: () => {
          // The one action with no undo worth relying on, so it is confirmed.
          if (confirm(`Ban ${name} from this channel?`)) {
            void moderation.ban(id, user);
          }
        },
      },
      {
        label: `Unban ${name}`,
        run: () => void moderation.unban(id, user),
      },
    ];
  }

  function menuFor(message: ChatMessage, at: { x: number; y: number }, url: string | null = null) {
    const name = message.author.displayName;

    menu = {
      at,
      items: [
        // A link the reader aimed at comes first, and offers the copy that
        // clicking it cannot give them.
        ...(url
          ? [
              { label: 'Open link', run: () => void commands.openLink(url) },
              { label: 'Copy link', run: () => void copyText(url) },
            ]
          : []),
        { label: `Reply to ${name}`, run: () => reply(message), divider: url !== null },
        {
          label: `Mention ${name}`,
          run: () => {
            // Appended, not replacing: a half-typed message survives.
            draft = `${draft.trimEnd()} @${name} `.trimStart();
            input?.focus();
          },
        },
        {
          label: 'Open user card',
          run: () => (card = { author: message.author, at: rectAt(at) }),
        },
        {
          label: `Whisper ${name}`,
          run: () =>
            void whispers.startWith({
              id: message.author.id,
              login: message.author.login,
              name,
            }),
        },
        { label: 'Copy message', run: () => void copyText(message.rawText), divider: true },
        { label: `Copy @${name}`, run: () => void copyText(`@${name}`) },
        {
          label: 'Open channel on Twitch',
          divider: true,
          run: () => void commands.openLink(`https://www.twitch.tv/${message.author.login}`),
        },
        {
          // Their lines go now and no more arrive. Undone
          // under Settings → Ignores.
          label: `Ignore ${name}`,
          run: () => void settings.ignoreUser(message.author.login),
        },
        ...menuCommands(message),
        ...moderationItems(message),
      ],
    };
  }

  /** The reader's commands marked for the message menu, run about this message. */
  function menuCommands(message: ChatMessage): MenuItem[] {
    return settings.current.commands
      .filter((c) => c.inMenu && c.template.trim().length > 0)
      .map((c, i) => ({
        label: c.trigger || c.template,
        divider: i === 0,
        run: () => void runFromMenu(c.template, message),
      }));
  }

  /**
   * The chat's menu: everything that used to be a row of icons in the header.
   *
   * One button rather than six: the
   * header is for knowing what you are reading, and the row of icons was
   * costing the stream title its room. Each entry names its shortcut, so the
   * menu also teaches the faster way. With the header hidden, a right-click on
   * the chat opens the same menu.
   */
  let appMenu = $state<{ x: number; y: number; align?: 'start' | 'end' } | null>(null);

  const windowSettings = $derived(settings.current.window);
  const headerFont = $derived(
    chatFontStack(windowSettings.headerFont, windowSettings.headerCustomFont),
  );

  const appItems = $derived.by((): MenuItem[] => {
    const hidden = !windowSettings.showHeader;
    const filters = settings.current.filters;

    return [
      // What the header carries itself, offered here only when it is hidden.
      ...(hidden && channel
        ? [
            {
              label: 'Watch the stream',
              hint: 'Ctrl O',
              run: () => follows.watch(channel.channel.user.login),
            },
          ]
        : []),
      ...(hidden && !settings.current.sidebar.collapsed
        ? [{ label: 'Followed channels', run: () => follows.toggleDrawer() }]
        : []),
      { label: 'Open a channel…', hint: 'Ctrl K', run: () => (follows.searchOpen = true) },
      {
        label: whispers.unread > 0 ? `Whispers · ${whispers.unread} unread` : 'Whispers',
        hint: 'Ctrl Shift W',
        run: () => (whispers.open = true),
      },
      {
        label: 'Activity',
        checked: settings.current.activity.enabled,
        divider: true,
        run: () => void settings.updateActivity({ enabled: !settings.current.activity.enabled }),
      },
      ...filters.map((f, i) => ({
        label: `Filter: ${f.name.trim() || `#${i + 1}`}`,
        checked: f.enabled,
        run: () =>
          void settings.update({
            filters: settings.current.filters.map((g, j) =>
              j === i ? { ...g, enabled: !g.enabled } : g,
            ),
          }),
      })),
      {
        label: 'Show header',
        checked: windowSettings.showHeader,
        run: () => void settings.updateWindow({ showHeader: !windowSettings.showHeader }),
      },
      {
        label: filters.length > 0 ? 'Edit filters…' : 'Filters…',
        divider: true,
        run: () => settings.show('filters'),
      },
      { label: 'Settings', hint: 'Ctrl ,', run: () => settings.show() },
    ];
  });

  const filtering = $derived(activeFilters.length > 0);
  /** Named in the status bar, so a quiet chat is not mistaken for a dead one. */
  const filterNames = $derived(
    settings.current.filters
      .filter((f) => f.enabled && parseFilter(f.expression).ok)
      .map((f, i) => f.name.trim() || `#${i + 1}`),
  );

  /** A zero-size rect at the pointer, so the card anchors where it was opened. */
  function rectAt(at: { x: number; y: number }) {
    return { top: at.y, bottom: at.y, left: at.x, right: at.x } as DOMRect;
  }

  /**
   * Posted as it stands rather than dropped into the input: a title command is
   * a whole message, and anything the reader had half-typed stays put.
   */
  async function post(command: string) {
    if (await chat.send(command)) {
      pinned = true;
    }
  }
</script>

<svelte:window onfocus={() => (focused = true)} onblur={() => (focused = false)} />

<section class="chat">
  {#if windowSettings.showHeader}
  <header
    data-tauri-drag-region={windowSettings.hideTitleBar ? '' : undefined}
    style:font-size="{windowSettings.headerFontSize}px"
    style:--header-font={headerFont}
  >
    {#if !settings.current.sidebar.collapsed}
      <button
        class="icon"
        onclick={() => follows.toggleDrawer()}
        aria-label="Followed channels"
        aria-expanded={follows.drawerOpen}
        title="Followed channels"
      >
        <SidebarIcon size={Math.round(windowSettings.headerFontSize * 1.1)} />
      </button>
    {/if}

    <h2>{channelName}</h2>

    {#if streamTitle}
      <span class="title" title={streamTitle}>{streamTitle}</span>
    {/if}

    <span class="spacer"></span>

    {#if channel}
      <button
        class="icon watch"
        onclick={() => follows.watch(channel.channel.user.login)}
        aria-label="Watch the stream"
        title="Watch the stream"
      >
        <!-- Material Icons `live_tv` (Apache-2.0), the glyph react-icons ships as
             MdLiveTv: a set with an antenna and a play mark, which says "watch
             this live" where a plain screen only says "screen". -->
        <svg class="filled" viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M21 6h-7.59l3.29-3.29L16 2l-4 4-4-4-.71.71L10.59 6H3c-1.1 0-2 .89-2 2v12c0 1.1.9 2 2 2h18c1.1 0 2-.9 2-2V8c0-1.11-.9-2-2-2zm0 14H3V8h18v12zM9 10v8l7-4z"
          />
        </svg>
      </button>
    {/if}

    <!-- The open menu closes on any pointerdown outside it, this button
         included: without stopping it, a click meant to close the menu would
         close it and then open it again. -->
    <button
      class="icon"
      class:on={appMenu !== null}
      onpointerdown={(e) => e.stopPropagation()}
      onclick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        appMenu = appMenu ? null : { x: r.right, y: r.bottom, align: 'end' };
      }}
      aria-label={whispers.unread > 0 ? `Menu, ${whispers.unread} unread whispers` : 'Menu'}
      aria-haspopup="menu"
      aria-expanded={appMenu !== null}
      title="Menu"
    >
      <svg class="filled" viewBox="0 0 16 16" aria-hidden="true">
        <circle cx="3" cy="8" r="1.4" />
        <circle cx="8" cy="8" r="1.4" />
        <circle cx="13" cy="8" r="1.4" />
      </svg>
      {#if whispers.unread > 0}
        <span class="badge">{whispers.unread > 99 ? '99+' : whispers.unread}</span>
      {/if}
    </button>
  </header>
  {/if}

  {#if statusBar && chatSettings.statusBarPosition === 'top'}
    <StatusBar filters={filterNames} />
  {/if}

  <!--
    One handler for the whole log rather than one per row: it must suppress the
    WebView's own menu everywhere inside the log, including the gaps between
    messages, and 500 listeners to do it would be 499 too many.
  -->
  <div
    class="log"
    bind:this={log}
    onscroll={onScroll}
    onwheel={onWheel}
    oncontextmenu={onContextMenu}
    role="log"
    aria-label="Chat messages"
    style="font-size: {chatSettings.fontSize}px; --chat-font: {fontStack}"
  >
    {#if !channel}
      <p class="empty">Pick a channel to read its chat.</p>
    {:else if visible.length === 0}
      <p class="empty">
        {connecting
          ? 'Joining chat…'
          : filtering && chat.messages.length > 0
            ? 'Nothing here passes the filters that are on.'
            : 'No messages yet.'}
      </p>
    {:else}
      {#each visible as message, i (message.id)}
        {#if chat.history > 0 && i === chat.history}
          <p class="backfill" role="separator">
            Older messages above are from recent-messages.robotty.de
          </p>
        {/if}

        {#if i === unreadAt}
          <div class="unread" role="separator">
            <span>New messages</span>
          </div>
        {/if}
        <ChatMessageRow
          {message}
          alt={chatSettings.alternateBackgrounds && (i + chat.trimmed) % 2 === 1}
          author={(author, at) => (card = { author, at })}
          parent={chat.byId(message.replyTo)}
          jump={jumpTo}
          flash={flashed === message.id}
        />
      {/each}
    {/if}
  </div>

  {#if statusBar && chatSettings.statusBarPosition === 'bottom'}
    <StatusBar filters={filterNames} />
  {/if}

  {#if menu}
    <MessageMenu items={menu.items} at={menu.at} close={() => (menu = null)} />
  {/if}

  {#if appMenu}
    <MessageMenu
      items={appItems}
      at={appMenu}
      align={appMenu.align}
      label="Chat menu"
      close={() => (appMenu = null)}
    />
  {/if}

  {#if card}
    <UserCard
      author={card.author}
      anchor={card.at}
      close={() => (card = null)}
      jump={jumpTo}
    />
  {/if}

  {#if !pinned}
    <button
      class="jump"
      onclick={() => {
        pinned = true;
        if (log) log.scrollTop = log.scrollHeight;
      }}
    >
      {unreadCount > 0
        ? `${unreadCount} new message${unreadCount === 1 ? '' : 's'}`
        : 'Jump to newest'}
    </button>
  {/if}

  <form onsubmit={submit}>
    {#if replyTo}
      <div class="replying-to">
        <span>Replying to</span>
        <span class="name">@{replyTo.author.displayName}</span>
        <span class="excerpt">{replyTo.rawText}</span>
        <button type="button" onclick={() => (replyTo = null)} aria-label="Cancel reply">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      </div>
    {/if}

    {#if showCompletions}
      <Completions items={suggestions} active={picked} {note} choose={accept} />
    {:else if showHints}
      <p class="hints">
        {#each hints as hint (hint.key)}
          <span><kbd>{hint.key}</kbd>{hint.what}</span>
        {/each}
      </p>
    {/if}

    <div class="field" class:inset={showCommands || emotes.length > 0} class:two={showCommands && emotes.length > 0}>
      <input
        bind:this={input}
        bind:value={draft}
        placeholder={channel ? 'Send a message' : 'Pick a channel first'}
        maxlength={MAX_MESSAGE_LENGTH}
        disabled={chat.sending || !channel}
        aria-label="Send a message"
        onkeydown={(e) => {
          // The suggestion list and the history recall get first refusal on
          // arrows, Tab, Enter and Escape.
          if (onComposerKey(e)) {
            e.preventDefault();
            e.stopPropagation();
            return;
          }

          // Escape drops the reply next; only a third one would be free to do
          // anything else, which matches how the rest of the app treats it.
          if (e.key === 'Escape' && replyTo) {
            e.stopPropagation();
            replyTo = null;
          }
        }}
        onkeyup={(e) => (caret = e.currentTarget.selectionStart ?? 0)}
        oninput={(e) => {
          caret = e.currentTarget.selectionStart ?? 0;
          // Typing again is moving on from whatever failed last.
          chat.sendError = null;
          moderation.error = null;
        }}
        onclick={(e) => (caret = e.currentTarget.selectionStart ?? 0)}
        onblur={() => {
          dismissed = true;
          composerFocused = false;
        }}
        onfocus={() => {
          dismissed = false;
          composerFocused = true;
        }}
      />

      {#if showCommands || emotes.length > 0}
        <span class="trailing">
          <EmotePicker
            {emotes}
            disabled={chat.sending || !channel}
            pick={(name) => {
              // Appended with a trailing space, like an accepted completion:
              // the next thing typed is always a new word.
              draft = `${draft.trimEnd()} ${name} `.trimStart();
              input?.focus();
            }}
          />
          {#if showCommands}
            <TitleCommands commands={advertised} disabled={chat.sending || !channel} pick={post} />
          {/if}
        </span>
      {/if}
    </div>

    {#if remaining < 60}
      <span class="counter" class:warn={remaining < 0}>{remaining}</span>
    {/if}

    {#if chatSettings.showSendButton}
      <button type="submit" disabled={chat.sending || !channel || draft.trim().length === 0}>
        Send
      </button>
    {/if}
  </form>

  {#if chat.sendError}
    <p class="error">{chat.sendError}</p>
  {/if}

  {#if moderation.error}
    <p class="error">{moderation.error}</p>
  {/if}
</section>

<style>
  .chat {
    flex: 1;
    /* min-* lets the panel shrink below its content when Hyprland tiles the
       window narrow, instead of overflowing the frame. */
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    position: relative;
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    /* Matches one avatar row so the first avatar aligns with the header. */
    height: var(--rail-item-height);
    flex: 0 0 auto;
    padding: 0 0.5rem;
    border-bottom: 1px solid var(--border);
    min-width: 0;
    /* Size and family are settings, set inline; everything in the bar is
       sized in em so it all grows together. */
    font-family: var(--header-font, var(--font));
  }

  /* Never shrinks: the channel you are reading is the one thing in the header
     that must always be legible. The stream title gives way instead. */
  h2 {
    margin: 0;
    flex: 0 0 auto;
    font-size: 1em;
    font-weight: 600;
    white-space: nowrap;
  }

  .spacer {
    flex: 1 1 auto;
    min-width: 0;
  }

  .icon {
    flex: 0 0 auto;
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    padding: 0.25rem;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
  }

  .icon svg {
    width: 1em;
    height: 1em;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.3;
    stroke-linecap: round;
  }

  /* Material icons are drawn as shapes, not lines. A touch larger, because a
     filled 24-unit glyph keeps a margin the 16-unit line icons do not. */
  .icon svg.filled {
    width: 1.2em;
    height: 1.2em;
    fill: currentColor;
    stroke: none;
  }

  .icon:hover,
  .icon.on {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  /* Watching is what the header is mostly for, so its button carries the
     accent where the rest stay dim grey: findable at a glance, still an icon. */
  .icon.watch {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }

  .icon.watch:hover {
    background: color-mix(in srgb, var(--accent) 26%, transparent);
  }

  /* Rides the corner of the icon rather than sitting beside it, so an unread
     count never shifts the header's other controls. */
  .icon {
    position: relative;
  }

  .badge {
    position: absolute;
    top: 0;
    right: 0;
    min-width: 1.1em;
    padding: 0 0.25em;
    border-radius: 999px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 0.5625rem;
    font-weight: 700;
    line-height: 1.3;
    text-align: center;
  }

  /* The title takes whatever room is left and truncates; the full text is in
     the tooltip. `min-width: 0` is what lets it shrink at all inside flex. */
  .title {
    /* Shrinks to an ellipsis rather than growing: the spacer takes the slack so
       the icons stay right-aligned when there is no title at all. */
    flex: 0 1 auto;
    min-width: 0;
    font-size: 0.85em;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .log {
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow-y: auto;
    /* Nothing should ever scroll sideways; long words wrap instead. */
    overflow-x: hidden;
    padding: 0.5rem 0;
    scrollbar-width: var(--scrollbar-width);
    scrollbar-color: var(--scrollbar-thumb-hover) transparent;
    display: flex;
    flex-direction: column;
    /* Lurkn keeps the reader's place itself (see $lib/ui/scroll); an engine
       that also anchored would move the log twice. */
    overflow-anchor: none;
  }

  /* Bottom-anchors a short log against the composer instead of leaving dead
     space under the last line.

     A growing spacer rather than `justify-content: flex-end`: once the log
     overflows, flex-end pushes the earliest messages above the scroll
     container's origin, where no scrollbar can reach them. The spacer instead
     computes to zero height as soon as there is no free space, so a full log
     scrolls normally. */
  .log::before {
    content: '';
    flex: 1 1 0;
  }

  /* The spacer is the only thing in the log allowed to flex. Message rows set
     `min-height: 0`, which as flex items removes the floor that normally stops
     an item shrinking below its own text — without this they squash and the
     lines overlap. */
  .log > :global(*) {
    flex: 0 0 auto;
  }

  .empty {
    color: var(--text-dim);
    font-size: 0.75rem;
    padding: 0 0.75rem;
    overflow-wrap: anywhere;
  }

  /* Says where the lines above came from, once, at the point Lurkn's own record
     begins. Twitch serves no history, so backfill is somebody else's data and
     the log should not quietly present it as its own. */
  .backfill {
    margin: 0.3rem 0;
    padding: 0 0.75rem;
    font-size: 0.6875rem;
    color: var(--text-dim);
    text-align: center;
    overflow-wrap: anywhere;
  }

  /* The rule runs edge to edge, unlike the padded message rows: a line that
     stops short of both edges reads as decoration rather than as a boundary.
     The label sits in it rather than above it, so the divider stays one row
     high and does not shove the log down when it appears. */
  .unread {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.25rem 0;
  }

  .unread::before,
  .unread::after {
    content: '';
    flex: 1 1 auto;
    border-top: 1px solid var(--unread);
  }

  /* Short lead-in on the left so the label reads as attached to the line's
     start, near the timestamps, not floating in the middle of the log. */
  .unread::before {
    flex: 0 0 0.75rem;
  }

  .unread span {
    flex: 0 0 auto;
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--unread);
    text-transform: uppercase;
  }

  .jump {
    position: absolute;
    bottom: 3.5rem;
    left: 50%;
    transform: translateX(-50%);
    font: inherit;
    font-size: 0.75rem;
    padding: 0.25rem 0.75rem;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
  }

  form {
    --control-height: 2rem;
    display: flex;
    /* Wraps so the reply preview can take a full row above the controls without
       a second flex container. */
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    padding: 0.625rem 0.75rem;
    border-top: 1px solid var(--border);
    min-width: 0;
  }

  /* A note, not a panel: no tint and no rule, because the row only has to say
     where the next message is going, and the composer under it is the thing to
     look at. */
  /* Sits in the slot the suggestions use, and is as quiet as a caption: it is a
     reminder, not a toolbar. */
  .hints {
    flex: 0 0 100%;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.15rem 0.7rem;
    margin: 0 0 0.1rem;
    font-size: 0.6875rem;
    color: var(--text-dim);
  }

  .hints span {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }

  .hints kbd {
    font: inherit;
    font-family: var(--mono);
    padding: 0 0.3em;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    color: var(--text);
  }

  .replying-to {
    flex: 0 0 100%;
    display: flex;
    align-items: center;
    gap: 0.3rem;
    min-width: 0;
    font-size: 0.6875rem;
    line-height: 1.3;
    color: var(--text-dim);
  }

  .replying-to .name {
    flex: 0 0 auto;
    color: var(--text-dim);
    font-weight: 600;
  }

  /* The excerpt is the part that gives: the name must stay readable. */
  .replying-to .excerpt {
    flex: 0 1 auto;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .replying-to button {
    margin-left: auto;
    flex: 0 0 auto;
    display: grid;
    place-items: center;
    width: 1.1rem;
    height: 1.1rem;
    padding: 0;
    border: none;
    border-radius: var(--radius);
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }

  .replying-to button svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
  }

  .replying-to button:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  /* The command trigger sits inside the field rather than beside it, so the
     composer stays one control wide however many buttons it grows. */
  .field {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
  }

  /* Centred by stretching to the field's full height, never by a transform: a
     transformed ancestor becomes the containing block for `position: fixed`
     descendants, which would anchor the command menu to this 1.55rem box
     instead of the viewport and throw it off screen. */
  .trailing {
    position: absolute;
    right: 0.25rem;
    top: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 0.1rem;
  }

  input {
    flex: 1;
    min-width: 0;
    font: inherit;
    font-size: 0.8125rem;
    /* Height is set explicitly rather than left to padding: an input and a
       button compute their intrinsic height differently, so equal padding does
       not give equal size. */
    height: var(--control-height);
    padding: 0 0.6rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    user-select: text;
  }

  input:focus {
    outline: none;
    border-color: var(--accent);
  }

  /* Only when the trigger is there: an empty gutter in every other channel
     would read as a text cursor that stops short of the border. */
  .field.inset input {
    padding-right: 2.1rem;
  }

  .field.two input {
    padding-right: 3.8rem;
  }

  .counter {
    font-size: 0.6875rem;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .counter.warn {
    color: var(--warn);
  }

  button[type='submit'] {
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

  button[type='submit']:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .error {
    margin: 0;
    padding: 0 0.75rem 0.5rem;
    font-size: 0.75rem;
    color: var(--warn);
  }
</style>
