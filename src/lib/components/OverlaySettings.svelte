<script lang="ts">
  import InfoHint from '$lib/components/InfoHint.svelte';
  import { copyText } from '$lib/clipboard';
  import { commands, type OverlayStatus, type Rgb } from '$lib/ipc';
  import { settings } from '$lib/stores/settings.svelte';
  import {
    MAX_OVERLAY_FONT_SIZE,
    MAX_OVERLAY_MESSAGES,
    MAX_OVERLAY_RADIUS,
    MIN_OVERLAY_FONT_SIZE,
  } from '$lib/constants';

  interface Props {
    /** Installed font families, already read by the settings screen. */
    fonts: string[];
  }

  let { fonts }: Props = $props();

  const overlay = $derived(settings.current.overlay);

  let status = $state<OverlayStatus | null>(null);
  let notice = $state<string | null>(null);

  async function refresh() {
    const result = await commands.overlayStatus();
    if (result.status === 'ok') status = result.data;
  }

  // Polled while the section is on screen: whether OBS is connected changes
  // without anything in Lurkn happening, and the server starts in the
  // background after a settings change, so there is no single moment to ask.
  $effect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
  });

  function say(text: string) {
    notice = text;
    setTimeout(() => {
      if (notice === text) notice = null;
    }, 2500);
  }

  async function copy() {
    if (!status?.url) return;
    say((await copyText(status.url)) ? 'Copied.' : 'Could not copy; select the address instead.');
  }

  async function test() {
    const result = await commands.overlayTest();
    if (result.status === 'error') say(result.error);
    else if (result.data === 0) say('Sent, but nothing is connected to show it.');
    else say('Sent.');
  }

  async function preview() {
    const result = await commands.overlayPreview();
    if (result.status === 'error') say(result.error);
  }

  const PRESETS = ['system', 'sans', 'serif', 'monospace'] as const;
  const fontValue = $derived(overlay.font === 'custom' ? overlay.customFont : overlay.font);

  function chooseFont(value: string) {
    if ((PRESETS as readonly string[]).includes(value)) {
      void settings.updateOverlay({ font: value as (typeof PRESETS)[number] });
    } else {
      void settings.updateOverlay({ font: 'custom', customFont: value });
    }
  }

  function hex(color: Rgb): string {
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

  function setColour(key: 'textColor' | 'bubbleColor' | 'nameColor', value: string) {
    const color = rgb(value);
    if (color) void settings.updateOverlay({ [key]: color });
  }

  /** Held locally while typed, so a half-typed port is never bound. */
  let portDraft = $derived(String(overlay.port));

  const portValid = $derived(/^\d+$/.test(portDraft) && +portDraft >= 1024 && +portDraft <= 65535);

  function savePort() {
    if (!portValid || +portDraft === overlay.port) return;
    void settings.updateOverlay({ port: +portDraft });
  }

  const FADES = [
    [0, 'Never'],
    [10, 'After 10 s'],
    [20, 'After 20 s'],
    [30, 'After 30 s'],
    [60, 'After 1 min'],
    [120, 'After 2 min'],
    [300, 'After 5 min'],
  ] as const;

  let hiddenDraft = $state('');

  function addHidden() {
    const login = hiddenDraft.trim().replace(/^@/, '').toLowerCase();
    hiddenDraft = '';
    if (!/^[a-z0-9_]{1,25}$/.test(login) || overlay.hiddenUsers.includes(login)) return;
    void settings.updateOverlay({ hiddenUsers: [...overlay.hiddenUsers, login] });
  }

  function removeHidden(login: string) {
    void settings.updateOverlay({ hiddenUsers: overlay.hiddenUsers.filter((l) => l !== login) });
  }
</script>

<section>
  <h3>Chat overlay</h3>

  <div class="check-row">
    <label class="check">
      <input
        type="checkbox"
        checked={overlay.enabled}
        onchange={(e) => settings.updateOverlay({ enabled: e.currentTarget.checked })}
      />
      Serve my chat for OBS
    </label>
    <InfoHint label="the chat overlay">
      Add a <strong>Browser</strong> source in OBS and paste the address below. Chat messages
      only: no subs, raids or announcements. Changes here restyle it live, so keep the OBS
      preview open while you adjust. Everything on it is already public on Twitch, so the address needs no
      password; it only answers this computer.
    </InfoHint>
  </div>

  {#if overlay.enabled}
    {#if status?.error}
      <p class="error">{status.error}</p>
    {:else if status?.url}
      <div class="address">
        <input class="text mono" readonly value={status.url} aria-label="Overlay address" />
        <button class="ghost" type="button" onclick={copy}>Copy</button>
      </div>
      <p class="note dim">
        <span class="dot" class:on={status.listeners > 0}></span>
        {#if status.listeners === 0}
          Nothing connected yet.
        {:else if status.listeners === 1}
          One page connected.
        {:else}
          {status.listeners} pages connected.
        {/if}
        {#if status.channel}
          Showing <strong class="channel">{status.channel}</strong>.
        {:else if overlay.source === 'open'}
          Open a channel to show its chat.
        {:else}
          Log in to show your chat.
        {/if}
      </p>
      <div class="actions">
        <button class="ghost" type="button" onclick={test}>Send a test message</button>
        <button class="ghost" type="button" onclick={preview}>Preview in browser</button>
        {#if notice}<span class="note dim">{notice}</span>{/if}
      </div>
    {:else}
      <p class="note dim">Starting…</p>
    {/if}

    <div class="row">
      <span class="label">
        Shows
        <InfoHint label="whose chat the overlay shows">
          <strong>My channel</strong> stays on your chat whatever is open in Lurkn — clicking
          over to someone you are about to raid does not change what your stream shows.
          <strong>Open channel</strong> follows Lurkn instead, for watch-alongs and co-streams:
          every switch changes the overlay, live, and clears what was on it.
        </InfoHint>
      </span>
      <div class="segmented">
        {#each [['own', 'My channel'], ['open', 'Open channel']] as const as [value, label] (value)}
          <button
            class:active={overlay.source === value}
            onclick={() => settings.updateOverlay({ source: value })}
          >
            {label}
          </button>
        {/each}
      </div>
    </div>

    <div class="row">
      <label class="label" for="overlay-port">
        Port
        <InfoHint label="the overlay port">
          Part of the address OBS keeps, so it only needs changing if another program already
          uses it. Change it here and the address in OBS has to change with it.
        </InfoHint>
      </label>
      <input
        id="overlay-port"
        class="text port"
        class:invalid={!portValid}
        inputmode="numeric"
        value={portDraft}
        oninput={(e) => (portDraft = e.currentTarget.value)}
        onchange={savePort}
        onblur={savePort}
      />
    </div>

    <h4>Look</h4>

    <div class="row">
      <span class="label">Layout</span>
      <div class="segmented">
        {#each [['bubbles', 'Bubbles'], ['plain', 'Plain']] as const as [value, label] (value)}
          <button
            class:active={overlay.layout === value}
            onclick={() => settings.updateOverlay({ layout: value })}
          >
            {label}
          </button>
        {/each}
      </div>
    </div>

    <div class="row">
      <label class="label" for="overlay-font">Font</label>
      <select
        id="overlay-font"
        class="select"
        value={fontValue}
        onchange={(e) => chooseFont(e.currentTarget.value)}
      >
        <optgroup label="Built in">
          <option value="system">System</option>
          <option value="sans">Sans</option>
          <option value="serif">Serif</option>
          <option value="monospace">Monospace</option>
        </optgroup>
        {#if fonts.length > 0}
          <optgroup label="Installed">
            {#each fonts as family (family)}
              <option value={family}>{family}</option>
            {/each}
          </optgroup>
        {/if}
      </select>
    </div>

    <div class="row">
      <label class="label" for="overlay-size">Text size</label>
      <div class="slider">
        <input
          id="overlay-size"
          type="range"
          min={MIN_OVERLAY_FONT_SIZE}
          max={MAX_OVERLAY_FONT_SIZE}
          value={overlay.fontSize}
          oninput={(e) => settings.updateOverlay({ fontSize: Number(e.currentTarget.value) })}
        />
        <span class="value">{overlay.fontSize}px</span>
      </div>
    </div>

    <div class="row">
      <span class="label">Emotes</span>
      <div class="segmented">
        {#each [['small', 'Small'], ['medium', 'Medium'], ['large', 'Large']] as const as [value, label] (value)}
          <button
            class:active={overlay.emoteSize === value}
            onclick={() => settings.updateOverlay({ emoteSize: value })}
          >
            {label}
          </button>
        {/each}
      </div>
    </div>

    <div class="row">
      <label class="label" for="overlay-text">Text colour</label>
      <input
        id="overlay-text"
        class="swatch"
        type="color"
        value={hex(overlay.textColor)}
        onchange={(e) => setColour('textColor', e.currentTarget.value)}
      />
    </div>

    {#if overlay.layout === 'bubbles'}
      <div class="row">
        <label class="label" for="overlay-bubble">Bubble</label>
        <div class="slider">
          <input
            id="overlay-bubble"
            class="swatch"
            type="color"
            value={hex(overlay.bubbleColor)}
            onchange={(e) => setColour('bubbleColor', e.currentTarget.value)}
          />
          <input
            type="range"
            min="0"
            max="100"
            value={overlay.bubbleOpacity}
            aria-label="Bubble opacity"
            oninput={(e) =>
              settings.updateOverlay({ bubbleOpacity: Number(e.currentTarget.value) })}
          />
          <span class="value">{overlay.bubbleOpacity}%</span>
        </div>
      </div>

      <div class="row">
        <label class="label" for="overlay-radius">Corners</label>
        <div class="slider">
          <input
            id="overlay-radius"
            type="range"
            min="0"
            max={MAX_OVERLAY_RADIUS}
            value={overlay.cornerRadius}
            oninput={(e) =>
              settings.updateOverlay({ cornerRadius: Number(e.currentTarget.value) })}
          />
          <span class="value">{overlay.cornerRadius}px</span>
        </div>
      </div>
    {/if}

    <div class="row">
      <div class="check-row">
        <label class="check">
          <input
            type="checkbox"
            checked={overlay.twitchNameColors}
            onchange={(e) => settings.updateOverlay({ twitchNameColors: e.currentTarget.checked })}
          />
          Twitch name colours
        </label>
        <InfoHint label="name colours">
          Each chatter's own colour, lifted just enough to read against the bubble — Twitch
          lets anyone pick a dark blue that disappears on stream. Off, every name gets the
          colour on the right.
        </InfoHint>
      </div>
      {#if !overlay.twitchNameColors}
        <input
          class="swatch"
          type="color"
          value={hex(overlay.nameColor)}
          aria-label="Name colour"
          onchange={(e) => setColour('nameColor', e.currentTarget.value)}
        />
      {/if}
    </div>

    <label class="check">
      <input
        type="checkbox"
        checked={overlay.outline}
        onchange={(e) => settings.updateOverlay({ outline: e.currentTarget.checked })}
      />
      Outline the text
    </label>
    <label class="check">
      <input
        type="checkbox"
        checked={overlay.showBadges}
        onchange={(e) => settings.updateOverlay({ showBadges: e.currentTarget.checked })}
      />
      Show badges
    </label>
    <label class="check">
      <input
        type="checkbox"
        checked={overlay.animate}
        onchange={(e) => settings.updateOverlay({ animate: e.currentTarget.checked })}
      />
      Slide new messages in
    </label>

    <h4>Messages</h4>

    <div class="row">
      <label class="label" for="overlay-max">At most</label>
      <div class="slider">
        <input
          id="overlay-max"
          type="range"
          min="1"
          max={MAX_OVERLAY_MESSAGES}
          value={overlay.maxMessages}
          oninput={(e) => settings.updateOverlay({ maxMessages: Number(e.currentTarget.value) })}
        />
        <span class="value">{overlay.maxMessages}</span>
      </div>
    </div>

    <div class="row">
      <label class="label" for="overlay-fade">Fade out</label>
      <select
        id="overlay-fade"
        class="select"
        value={String(overlay.fadeAfterSeconds)}
        onchange={(e) =>
          settings.updateOverlay({ fadeAfterSeconds: Number(e.currentTarget.value) })}
      >
        {#each FADES as [seconds, label] (seconds)}
          <option value={String(seconds)}>{label}</option>
        {/each}
        {#if !FADES.some(([s]) => s === overlay.fadeAfterSeconds)}
          <option value={String(overlay.fadeAfterSeconds)}>After {overlay.fadeAfterSeconds} s</option>
        {/if}
      </select>
    </div>

    <label class="check">
      <input
        type="checkbox"
        checked={overlay.hideCommands}
        onchange={(e) => settings.updateOverlay({ hideCommands: e.currentTarget.checked })}
      />
      Leave out <code>!commands</code>
    </label>

    <div class="sub">
      <span class="label">
        Leave out these chatters
        <InfoHint label="hidden chatters">
          Kept off the overlay only; they still show in Lurkn. Your ignores apply to the overlay
          as well, so anyone you ignore is already gone from it.
        </InfoHint>
      </span>
      <div class="chips">
        {#each overlay.hiddenUsers as login (login)}
          <span class="chip">
            {login}
            <button type="button" onclick={() => removeHidden(login)} aria-label="Remove {login}"
              >✕</button
            >
          </span>
        {/each}
        <input
          class="text chip-input"
          bind:value={hiddenDraft}
          placeholder="add a login"
          autocapitalize="off"
          spellcheck="false"
          onkeydown={(e) => {
            if (e.key !== 'Enter') return;
            e.preventDefault();
            addHidden();
          }}
        />
      </div>
    </div>

    <p class="note dim fine">
      For anything not here, OBS's own <strong>Custom CSS</strong> field on the source works:
      <code>.msg</code>, <code>.name</code>, <code>.body</code>, <code>.emote</code> and
      <code>.badge</code> are stable.
    </p>
  {/if}
</section>

<style>
  h3 {
    margin: 0 0 0.75rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }

  h4 {
    margin: 1.1rem 0 0.6rem;
    font-size: 0.875rem;
    font-weight: 600;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 0.75rem;
  }

  .label {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.875rem;
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

  .segmented {
    display: flex;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .segmented button {
    font: inherit;
    font-size: 0.8125rem;
    padding: 0.3rem 0.7rem;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }

  .segmented button.active {
    background: var(--accent);
    color: var(--on-accent);
  }

  .segmented button:hover:not(.active) {
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
  }

  .slider {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .value {
    font-size: 0.8125rem;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    min-width: 2.5rem;
    text-align: right;
  }

  .text,
  .select {
    font: inherit;
    font-size: 0.875rem;
    padding: 0.3rem 0.5rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    min-width: 0;
  }

  .select {
    max-width: 14rem;
  }

  .text:focus,
  .select:focus {
    outline: none;
    border-color: var(--accent);
  }

  .select option,
  .select optgroup {
    background: var(--bg-raised);
    color: var(--text);
  }

  .mono {
    font-family: var(--mono);
    font-size: 0.8125rem;
  }

  .port {
    width: 5.5rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .invalid {
    border-color: var(--warn);
  }

  .swatch {
    width: 1.9rem;
    height: 1.5rem;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: none;
    cursor: pointer;
  }

  .address {
    display: flex;
    gap: 0.4rem;
    margin: 0.4rem 0 0.35rem;
  }

  .address .text {
    flex: 1;
    user-select: text;
  }

  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-bottom: 1rem;
  }

  .actions .note {
    margin: 0;
  }

  .ghost {
    font: inherit;
    font-size: 0.8125rem;
    padding: 0.25rem 0.7rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    cursor: pointer;
  }

  .ghost:hover {
    border-color: var(--text-dim);
  }

  .note {
    margin: 0 0 0.5rem;
    font-size: 0.8125rem;
    line-height: 1.6;
  }

  .dim {
    color: var(--text-dim);
  }

  .channel {
    color: var(--text);
    font-weight: 600;
  }

  .fine {
    margin-top: 1rem;
    font-size: 0.75rem;
  }

  .dot {
    display: inline-block;
    width: 0.5rem;
    height: 0.5rem;
    margin-right: 0.3rem;
    border-radius: 50%;
    background: var(--text-dim);
    opacity: 0.5;
  }

  .dot.on {
    background: var(--ok);
    opacity: 1;
  }

  .sub {
    margin-top: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
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

  code {
    font-family: var(--mono);
    font-size: 0.9em;
  }

  .error {
    color: var(--warn);
    font-size: 0.8125rem;
    margin: 0.5rem 0;
  }
</style>
