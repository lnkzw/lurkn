<!--
  The rule editors: Highlights, Ignores, Filters and Commands — the names chat
  clients have long used for these, so they are found where people look.

  Every field saves on change rather than per keystroke: a half-typed pattern
  would otherwise be saved, compiled and matched against chat while it is still
  wrong. Patterns are checked as they are typed, by the Rust that will run them.
-->
<script lang="ts">
  import InfoHint from '$lib/components/InfoHint.svelte';
  import { COMMAND_VARIABLES } from '$lib/chat/custom';
  import { OPERATORS, parseFilter, VARIABLES } from '$lib/chat/filter';
  import { commands, type Rgb } from '$lib/ipc';
  import { BLANK, settings } from '$lib/stores/settings.svelte';

  interface Props {
    section: 'highlights' | 'ignores' | 'filters' | 'commands';
  }

  let { section }: Props = $props();

  const chat = $derived(settings.current.chat);
  const highlights = $derived(settings.current.highlights);
  const ignores = $derived(settings.current.ignores);
  const filters = $derived(settings.current.filters);
  const custom = $derived(settings.current.commands);

  /** Twitch's badge set ids, with what people call them. */
  const BADGES = [
    ['broadcaster', 'Broadcaster'],
    ['moderator', 'Moderator'],
    ['vip', 'VIP'],
    ['subscriber', 'Subscriber'],
    ['founder', 'Founder'],
    ['staff', 'Staff'],
    ['partner', 'Verified'],
    ['artist-badge', 'Artist'],
    ['sub-gifter', 'Gift sub giver'],
    ['bits', 'Bits'],
    ['turbo', 'Turbo'],
    ['premium', 'Prime'],
  ] as const;

  /** The accent, as the colour picker's starting point when a rule has none. */
  const ACCENT = '#9146ff';

  function hex(color: Rgb | null): string {
    if (!color) return ACCENT;
    return `#${[color.r, color.g, color.b].map((v) => v.toString(16).padStart(2, '0')).join('')}`;
  }

  function rgb(value: string): Rgb | null {
    const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(value);
    if (!match) return null;
    return {
      r: Number.parseInt(match[1] ?? '0', 16),
      g: Number.parseInt(match[2] ?? '0', 16),
      b: Number.parseInt(match[3] ?? '0', 16),
    };
  }

  function patched<T>(list: T[], index: number, patch: Partial<T>): T[] {
    return list.map((item, i) => (i === index ? { ...item, ...patch } : item));
  }

  function without<T>(list: T[], index: number): T[] {
    return list.filter((_, i) => i !== index);
  }

  // ─── Pattern checking ───────────────────────────────────────────────────────

  /** Why a pattern does not compile, keyed by `list:index`. */
  let patternErrors = $state<Record<string, string>>({});
  /** The newest check per key, so a slow answer cannot overwrite a newer one. */
  const asked: Record<string, number> = {};

  async function check(key: string, pattern: string, regex: boolean) {
    const ticket = (asked[key] = (asked[key] ?? 0) + 1);
    const result = await commands.checkPattern(pattern, regex);
    if (asked[key] !== ticket) return;
    const next = { ...patternErrors };
    if (result.status === 'error') next[key] = result.error;
    else delete next[key];
    patternErrors = next;
  }

  // Saved patterns are checked too, so a broken one from an earlier session
  // says so when the page opens rather than only once it is edited.
  $effect(() => {
    highlights.phrases.forEach((p, i) => void check(`phrase:${i}`, p.pattern, p.regex));
    ignores.phrases.forEach((p, i) => void check(`ignore:${i}`, p.pattern, p.regex));
  });

  /** Live filter errors while one is being typed, keyed by index. */
  let filterDrafts = $state<Record<number, string>>({});

  function filterError(index: number, saved: string): string | null {
    const source = filterDrafts[index] ?? saved;
    if (source.trim().length === 0) return null;
    const parsed = parseFilter(source);
    return parsed.ok ? null : `${parsed.error} (at character ${parsed.at + 1})`;
  }

  // ─── Login lists ────────────────────────────────────────────────────────────

  let excludeDraft = $state('');
  let ignoreDraft = $state('');

  function addLogin(list: string[], raw: string): string[] | null {
    const login = raw.trim().replace(/^@/, '').toLowerCase();
    if (!/^[a-z0-9_]{1,25}$/.test(login) || list.includes(login)) return null;
    return [...list, login];
  }
</script>

{#snippet colour(color: Rgb | null, set: (color: Rgb | null) => void)}
  <span class="colour">
    <input
      type="color"
      value={hex(color)}
      aria-label="Colour"
      title={color ? 'Colour' : 'Colour (the accent until you pick one)'}
      onchange={(e) => set(rgb(e.currentTarget.value))}
    />
    {#if color}
      <button class="mini" type="button" onclick={() => set(null)} title="Back to the accent"
        >↺</button
      >
    {/if}
  </span>
{/snippet}

{#snippet remove(run: () => void)}
  <button class="mini" type="button" onclick={run} aria-label="Remove" title="Remove">✕</button>
{/snippet}

{#snippet logins(
  list: string[],
  draft: string,
  setDraft: (value: string) => void,
  save: (list: string[]) => void,
  placeholder: string,
)}
  <div class="chips">
    {#each list as login, i (login)}
      <span class="chip">
        {login}
        <button type="button" onclick={() => save(without(list, i))} aria-label="Remove {login}"
          >✕</button
        >
      </span>
    {/each}
    <input
      class="text chip-input"
      value={draft}
      {placeholder}
      autocapitalize="off"
      spellcheck="false"
      oninput={(e) => setDraft(e.currentTarget.value)}
      onkeydown={(e) => {
        if (e.key !== 'Enter') return;
        e.preventDefault();
        const next = addLogin(list, draft);
        if (next) save(next);
        setDraft('');
      }}
    />
  </div>
{/snippet}

{#if section === 'highlights'}
  <section>
    <h3>Highlights</h3>

    <div class="check-row">
      <label class="check">
        <input
          type="checkbox"
          checked={chat.highlightMentions}
          onchange={(e) => settings.updateChat({ highlightMentions: e.currentTarget.checked })}
        />
        Mark messages that name you
      </label>
      <InfoHint label="being named">
        Your login, matched as a whole word — "ann" does not fire on "announcement". Your own
        messages never highlight. Whether it also notifies you is under Alerts.
      </InfoHint>
    </div>

    <div class="sub">
      <h4>
        Phrases
        <InfoHint label="highlight phrases">
          Plain text matches as a whole word, in any case. Tick <strong>Regex</strong> to write
          a regular expression instead, and <strong>Case</strong> to match capitals exactly.
          <strong>Notify</strong> raises a desktop notification as well.
        </InfoHint>
      </h4>

      {#each highlights.phrases as phrase, i (i)}
        <div class="rule">
          <input
            class="text grow"
            class:mono={phrase.regex}
            value={phrase.pattern}
            placeholder={phrase.regex ? '^!\\w+' : 'a word or phrase'}
            aria-label="Phrase"
            spellcheck="false"
            oninput={(e) => void check(`phrase:${i}`, e.currentTarget.value, phrase.regex)}
            onchange={(e) =>
              settings.updateHighlights({
                phrases: patched(highlights.phrases, i, { pattern: e.currentTarget.value }),
              })}
          />
          <label class="flag" title="Regular expression">
            <input
              type="checkbox"
              checked={phrase.regex}
              onchange={(e) =>
                settings.updateHighlights({
                  phrases: patched(highlights.phrases, i, { regex: e.currentTarget.checked }),
                })}
            />Regex
          </label>
          <label class="flag" title="Match capitals exactly">
            <input
              type="checkbox"
              checked={phrase.caseSensitive}
              onchange={(e) =>
                settings.updateHighlights({
                  phrases: patched(highlights.phrases, i, {
                    caseSensitive: e.currentTarget.checked,
                  }),
                })}
            />Case
          </label>
          <label class="flag" title="Desktop notification">
            <input
              type="checkbox"
              checked={phrase.notify}
              onchange={(e) =>
                settings.updateHighlights({
                  phrases: patched(highlights.phrases, i, { notify: e.currentTarget.checked }),
                })}
            />Notify
          </label>
          {@render colour(phrase.color, (color) =>
            settings.updateHighlights({ phrases: patched(highlights.phrases, i, { color }) }),
          )}
          {@render remove(() =>
            settings.updateHighlights({ phrases: without(highlights.phrases, i) }),
          )}
        </div>
        {#if patternErrors[`phrase:${i}`]}
          <p class="error">{patternErrors[`phrase:${i}`]}</p>
        {/if}
      {/each}

      <button
        class="add"
        type="button"
        onclick={() => settings.updateHighlights({ phrases: [...highlights.phrases, BLANK.phrase()] })}
      >
        + Add phrase
      </button>
    </div>

    <div class="sub">
      <h4>
        Chatters
        <InfoHint label="highlighting chatters">
          Everything one person says is marked — a friend, a mod you are keeping an eye on.
        </InfoHint>
      </h4>

      {#each highlights.users as user, i (i)}
        <div class="rule">
          <input
            class="text grow"
            value={user.login}
            placeholder="login"
            aria-label="Login"
            autocapitalize="off"
            spellcheck="false"
            onchange={(e) =>
              settings.updateHighlights({
                users: patched(highlights.users, i, { login: e.currentTarget.value }),
              })}
          />
          <label class="flag" title="Desktop notification">
            <input
              type="checkbox"
              checked={user.notify}
              onchange={(e) =>
                settings.updateHighlights({
                  users: patched(highlights.users, i, { notify: e.currentTarget.checked }),
                })}
            />Notify
          </label>
          {@render colour(user.color, (color) =>
            settings.updateHighlights({ users: patched(highlights.users, i, { color }) }),
          )}
          {@render remove(() => settings.updateHighlights({ users: without(highlights.users, i) }))}
        </div>
      {/each}

      <button
        class="add"
        type="button"
        onclick={() => settings.updateHighlights({ users: [...highlights.users, BLANK.user()] })}
      >
        + Add chatter
      </button>
    </div>

    <div class="sub">
      <h4>
        Badges
        <InfoHint label="highlighting badges">
          Everything said by someone wearing the badge — every moderator, every VIP.
        </InfoHint>
      </h4>

      {#each highlights.badges as badge, i (i)}
        <div class="rule">
          <select
            class="select grow"
            value={badge.badge}
            aria-label="Badge"
            onchange={(e) =>
              settings.updateHighlights({
                badges: patched(highlights.badges, i, { badge: e.currentTarget.value }),
              })}
          >
            {#each BADGES as [id, name] (id)}
              <option value={id}>{name}</option>
            {/each}
            {#if !BADGES.some(([id]) => id === badge.badge)}
              <option value={badge.badge}>{badge.badge}</option>
            {/if}
          </select>
          <label class="flag" title="Desktop notification">
            <input
              type="checkbox"
              checked={badge.notify}
              onchange={(e) =>
                settings.updateHighlights({
                  badges: patched(highlights.badges, i, { notify: e.currentTarget.checked }),
                })}
            />Notify
          </label>
          {@render colour(badge.color, (color) =>
            settings.updateHighlights({ badges: patched(highlights.badges, i, { color }) }),
          )}
          {@render remove(() =>
            settings.updateHighlights({ badges: without(highlights.badges, i) }),
          )}
        </div>
      {/each}

      <button
        class="add"
        type="button"
        onclick={() => settings.updateHighlights({ badges: [...highlights.badges, BLANK.badge()] })}
      >
        + Add badge
      </button>
    </div>

    <div class="sub">
      <h4>
        Never highlight
        <InfoHint label="chatters never highlighted">
          Their messages are never marked, whatever they say — the bot that repeats your name in
          every reply. Type a login and press Enter.
        </InfoHint>
      </h4>
      {@render logins(
        highlights.excludedUsers,
        excludeDraft,
        (v) => (excludeDraft = v),
        (list) => settings.updateHighlights({ excludedUsers: list }),
        'nightbot',
      )}
    </div>
  </section>
{:else if section === 'ignores'}
  <section>
    <h3>Ignores</h3>

    <div class="sub">
      <h4>
        Chatters
        <InfoHint label="ignored chatters">
          Their messages never reach the log, and nothing they say can notify you. Right-click a
          message to ignore its author. Type a login and press Enter to add one here.
        </InfoHint>
      </h4>
      {@render logins(
        ignores.users,
        ignoreDraft,
        (v) => (ignoreDraft = v),
        (list) => settings.updateIgnores({ users: list }),
        'login',
      )}
    </div>

    <div class="sub">
      <h4>
        Phrases
        <InfoHint label="ignored phrases">
          Plain text matches anywhere in a message, in any case. <strong>Hide</strong> drops the
          whole message; <strong>Replace</strong> swaps the match for your text and keeps the rest.
          With <strong>Regex</strong>, <code>$1</code> in the replacement is the first group. Your
          own messages are never ignored.
        </InfoHint>
      </h4>

      {#each ignores.phrases as phrase, i (i)}
        <div class="rule">
          <input
            class="text grow"
            class:mono={phrase.regex}
            value={phrase.pattern}
            placeholder={phrase.regex ? 'https?://\\S+' : 'text to match'}
            aria-label="Phrase"
            spellcheck="false"
            oninput={(e) => void check(`ignore:${i}`, e.currentTarget.value, phrase.regex)}
            onchange={(e) =>
              settings.updateIgnores({
                phrases: patched(ignores.phrases, i, { pattern: e.currentTarget.value }),
              })}
          />
          <label class="flag" title="Regular expression">
            <input
              type="checkbox"
              checked={phrase.regex}
              onchange={(e) =>
                settings.updateIgnores({
                  phrases: patched(ignores.phrases, i, { regex: e.currentTarget.checked }),
                })}
            />Regex
          </label>
          <label class="flag" title="Match capitals exactly">
            <input
              type="checkbox"
              checked={phrase.caseSensitive}
              onchange={(e) =>
                settings.updateIgnores({
                  phrases: patched(ignores.phrases, i, { caseSensitive: e.currentTarget.checked }),
                })}
            />Case
          </label>
          <select
            class="select"
            value={phrase.action}
            aria-label="What to do"
            onchange={(e) =>
              settings.updateIgnores({
                phrases: patched(ignores.phrases, i, {
                  action: e.currentTarget.value === 'replace' ? 'replace' : 'hide',
                }),
              })}
          >
            <option value="hide">Hide</option>
            <option value="replace">Replace</option>
          </select>
          {@render remove(() => settings.updateIgnores({ phrases: without(ignores.phrases, i) }))}
        </div>
        {#if phrase.action === 'replace'}
          <div class="rule indent">
            <span class="dim">with</span>
            <input
              class="text grow"
              value={phrase.replacement}
              placeholder="***"
              aria-label="Replacement"
              spellcheck="false"
              onchange={(e) =>
                settings.updateIgnores({
                  phrases: patched(ignores.phrases, i, { replacement: e.currentTarget.value }),
                })}
            />
          </div>
        {/if}
        {#if patternErrors[`ignore:${i}`]}
          <p class="error">{patternErrors[`ignore:${i}`]}</p>
        {/if}
      {/each}

      <button
        class="add"
        type="button"
        onclick={() => settings.updateIgnores({ phrases: [...ignores.phrases, BLANK.ignore()] })}
      >
        + Add phrase
      </button>
    </div>
  </section>
{:else if section === 'filters'}
  <section>
    <h3>
      Filters
      <InfoHint label="filters">
        A filter shows only the messages that match it. Tick one to apply it to the chat, or use
        the funnel in the chat header; with several ticked, a message has to match all of them.
      </InfoHint>
    </h3>

    {#each filters as filter, i (i)}
      <div class="rule">
        <input
          type="checkbox"
          checked={filter.enabled}
          aria-label="Apply this filter"
          title="Apply to the chat"
          onchange={(e) =>
            settings.update({ filters: patched(filters, i, { enabled: e.currentTarget.checked }) })}
        />
        <input
          class="text name"
          value={filter.name}
          placeholder="Name"
          aria-label="Filter name"
          onchange={(e) =>
            settings.update({ filters: patched(filters, i, { name: e.currentTarget.value }) })}
        />
        <input
          class="text grow mono"
          value={filter.expression}
          placeholder='author.badges contains "moderator"'
          aria-label="Filter expression"
          spellcheck="false"
          autocapitalize="off"
          oninput={(e) => (filterDrafts = { ...filterDrafts, [i]: e.currentTarget.value })}
          onchange={(e) => {
            const rest = { ...filterDrafts };
            delete rest[i];
            filterDrafts = rest;
            void settings.update({
              filters: patched(filters, i, { expression: e.currentTarget.value }),
            });
          }}
        />
        {@render remove(() => {
          filterDrafts = {};
          void settings.update({ filters: without(filters, i) });
        })}
      </div>
      {#if filterError(i, filter.expression)}
        <p class="error">
          {filterError(i, filter.expression)}{filter.enabled ? ' — not applied until fixed.' : ''}
        </p>
      {/if}
    {/each}

    <button
      class="add"
      type="button"
      onclick={() => settings.update({ filters: [...filters, BLANK.filter()] })}
    >
      + Add filter
    </button>

    <details class="reference">
      <summary>What a filter can use</summary>
      <dl>
        {#each Object.entries(VARIABLES) as [name, what] (name)}
          <dt><code>{name}</code></dt>
          <dd>{what}</dd>
        {/each}
      </dl>
      <p class="dim">
        Operators: <code>{OPERATORS}</code>. Text is written in double quotes and compared in any
        case; a regex is <code>r"…"</code>, or <code>ri"…"</code> to ignore case.
      </p>
      <p class="dim">
        Example: <code>author.subbed &amp;&amp; !(message.content match r"^!\w+")</code>
      </p>
    </details>
  </section>
{:else if section === 'commands'}
  <section>
    <h3>
      Commands
      <InfoHint label="custom commands">
        Type the trigger as the first word and Lurkn sends the template instead, filled in. What it
        produces is handled like anything you type, so it can be a whisper or a moderation
        command. Ticking <strong>Menu</strong> also puts it on a message's right-click menu, where
        it is about that message and its author.
      </InfoHint>
    </h3>

    {#each custom as command, i (i)}
      <div class="rule">
        <input
          class="text trigger mono"
          value={command.trigger}
          placeholder="/hi"
          aria-label="Trigger"
          spellcheck="false"
          autocapitalize="off"
          onchange={(e) =>
            settings.update({ commands: patched(custom, i, { trigger: e.currentTarget.value }) })}
        />
        <input
          class="text grow"
          value={command.template}
          placeholder={'Hello {1}, welcome to {channel.name}!'}
          aria-label="Template"
          spellcheck="false"
          onchange={(e) =>
            settings.update({ commands: patched(custom, i, { template: e.currentTarget.value }) })}
        />
        <label class="flag" title="Offer on a message's right-click menu">
          <input
            type="checkbox"
            checked={command.inMenu}
            onchange={(e) =>
              settings.update({ commands: patched(custom, i, { inMenu: e.currentTarget.checked }) })}
          />Menu
        </label>
        {@render remove(() => settings.update({ commands: without(custom, i) }))}
      </div>
    {/each}

    <button
      class="add"
      type="button"
      onclick={() => settings.update({ commands: [...custom, BLANK.command()] })}
    >
      + Add command
    </button>

    <details class="reference">
      <summary>What a template can use</summary>
      <dl>
        {#each Object.entries(COMMAND_VARIABLES) as [name, what] (name)}
          <dt><code>{name}</code></dt>
          <dd>{what}</dd>
        {/each}
      </dl>
      <p class="dim">
        A template that uses none of the numbered words gets them added at the end, so
        <code>!drops</code> as a template still passes on whatever followed the trigger.
      </p>
    </details>
  </section>
{/if}

<style>
  h3 {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin: 0 0 0.75rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }

  h4 {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin: 0 0 0.4rem;
    font-size: 0.875rem;
    font-weight: 600;
  }

  .sub {
    margin-top: 1.1rem;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.875rem;
    padding: 0.25rem 0;
    cursor: pointer;
  }

  /* One rule per line: the pattern takes the room, the switches sit after it
     in a fixed order so the eye can run down a column of them. */
  .rule {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-bottom: 0.35rem;
    min-width: 0;
  }

  .rule.indent {
    padding-left: 1rem;
  }

  .text,
  .select {
    font: inherit;
    font-size: 0.875rem;
    height: 1.75rem;
    padding: 0 0.45rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    min-width: 0;
  }

  .text:focus,
  .select:focus {
    outline: none;
    border-color: var(--accent);
  }

  .select option {
    background: var(--bg-raised);
    color: var(--text);
  }

  .grow {
    flex: 1 1 auto;
  }

  .name {
    flex: 0 0 7rem;
  }

  .trigger {
    flex: 0 0 6rem;
  }

  .mono {
    font-family: var(--mono);
    font-size: 0.8125rem;
  }

  .flag {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    font-size: 0.75rem;
    color: var(--text-dim);
    cursor: pointer;
  }

  .colour {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 0.1rem;
  }

  .colour input {
    width: 1.6rem;
    height: 1.4rem;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: none;
    cursor: pointer;
  }

  .mini {
    flex: 0 0 auto;
    font: inherit;
    font-size: 0.8125rem;
    width: 1.4rem;
    height: 1.4rem;
    padding: 0;
    border: none;
    border-radius: var(--radius);
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }

  .mini:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  .add {
    font: inherit;
    font-size: 0.8125rem;
    margin-top: 0.15rem;
    padding: 0.25rem 0.6rem;
    border-radius: var(--radius);
    border: 1px dashed var(--border);
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }

  .add:hover {
    color: var(--text);
    border-color: var(--text-dim);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3rem;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    font-size: 0.8125rem;
    padding: 0.1rem 0.2rem 0.1rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 999px;
  }

  .chip button {
    font: inherit;
    font-size: 0.6875rem;
    padding: 0 0.25rem;
    border: none;
    background: none;
    color: var(--text-dim);
    cursor: pointer;
  }

  .chip button:hover {
    color: var(--text);
  }

  .chip-input {
    flex: 1 1 8rem;
    max-width: 12rem;
  }

  .error {
    margin: -0.15rem 0 0.4rem;
    font-size: 0.75rem;
    color: var(--warn);
    overflow-wrap: anywhere;
  }

  .dim {
    color: var(--text-dim);
    font-size: 0.8125rem;
  }

  .reference {
    margin-top: 1rem;
    font-size: 0.8125rem;
  }

  .reference summary {
    cursor: pointer;
    color: var(--text-dim);
  }

  .reference dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.25rem 0.75rem;
    margin: 0.6rem 0;
  }

  .reference dd {
    margin: 0;
    color: var(--text-dim);
  }

  code {
    font-family: var(--mono);
    font-size: 0.75rem;
    user-select: text;
  }
</style>
