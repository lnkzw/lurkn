<script lang="ts">
  import { copyText } from '$lib/clipboard';
  import { auth } from '$lib/stores/auth.svelte';

  interface Props {
    /** Set when a previously saved session needs renewing, not a first login. */
    renewing?: boolean;
  }

  let { renewing = false }: Props = $props();

  const awaiting = $derived(auth.status.kind === 'awaitingUser' ? auth.status : null);
  const failed = $derived(auth.status.kind === 'failed' ? auth.status : null);

  type CopyState = 'idle' | 'copied' | 'failed';
  let copyState = $state<CopyState>('idle');
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  async function copyCode(code: string) {
    clearTimeout(copyTimer);
    copyState = (await copyText(code)) ? 'copied' : 'failed';
    copyTimer = setTimeout(() => (copyState = 'idle'), 2000);
  }

  // Put the code on the clipboard the moment it appears, so the fastest path is
  // "click Open Twitch, paste if the prefill did not take".
  $effect(() => {
    const code = awaiting?.userCode;
    if (!code) {
      copyState = 'idle';
      return;
    }
    void copyCode(code);
    return () => clearTimeout(copyTimer);
  });
</script>

<div class="login">
  {#if awaiting}
    <h2>Enter this code on Twitch</h2>

    <button
      class="code"
      title="Click to copy"
      aria-label={`Login code ${awaiting.userCode.split('').join(' ')}. Click to copy.`}
      onclick={() => copyCode(awaiting.userCode)}
    >
      {awaiting.userCode}
    </button>

    <p class="copystate" aria-live="polite">
      {#if copyState === 'copied'}
        <span class="ok">Copied to your clipboard</span>
      {:else if copyState === 'failed'}
        <span class="warn">Could not copy — select the code above</span>
      {:else}
        &nbsp;
      {/if}
    </p>

    <p class="dim">
      <strong>Open Twitch</strong> fills the code in for you. If it does not, paste it at
      twitch.tv/activate. This window continues on its own once you are done.
    </p>

    <div class="actions">
      <button class="primary" onclick={() => auth.openVerification()}>Open Twitch</button>
      <button onclick={() => copyCode(awaiting.userCode)}>Copy code</button>
      <button onclick={() => auth.cancel()} disabled={auth.busy}>Cancel</button>
    </div>

    {#if auth.actionError}
      <p class="error">{auth.actionError}</p>
    {/if}
  {:else}
    <h2>{renewing ? 'Your Twitch session expired' : 'Welcome to Lurkn'}</h2>

    <p class="dim">
      {#if renewing}
        Twitch sessions have to be renewed from time to time. Nothing else changes.
      {:else}
        Log in with Twitch to see who is live, read chat and join in.
      {/if}
    </p>

    {#if failed}
      <p class="error">{failed.message}</p>
    {/if}

    <div class="actions">
      <button class="primary" onclick={() => auth.login()} disabled={auth.busy}>
        {auth.busy ? 'Contacting Twitch…' : 'Log in with Twitch'}
      </button>
    </div>

    <p class="fineprint">
      Lurkn asks only to read who you follow and to read and send chat. It never sees your
      password, and video stays in your own browser.
    </p>
  {/if}
</div>

<style>
  .login {
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 2rem;
    max-width: 30rem;
    text-align: center;
  }

  h2 {
    margin: 0 0 0.75rem;
    font-size: 1.125rem;
  }

  .code {
    display: block;
    width: 100%;
    margin: 1.25rem 0 0.25rem;
    padding: 0.5rem 0;
    font-family: var(--mono);
    font-size: 2.25rem;
    font-weight: 600;
    letter-spacing: 0.25em;
    /* The tracking pads the right edge; pull it back so the code looks centred. */
    text-indent: 0.25em;
    color: var(--accent);
    background: transparent;
    border: 1px dashed var(--border);
    border-radius: var(--radius);
    cursor: pointer;
    user-select: text;
  }

  .code:hover {
    border-color: var(--accent);
  }

  .copystate {
    margin: 0 0 1rem;
    font-size: 0.75rem;
    min-height: 1.1em;
  }

  .dim {
    color: var(--text-dim);
    margin: 0;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    justify-content: center;
    margin-top: 1.5rem;
  }

  button {
    font: inherit;
    padding: 0.5rem 1.1rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    border-color: var(--text-dim);
  }

  button:disabled {
    opacity: 0.55;
    cursor: default;
  }

  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
    font-weight: 500;
  }

  button.primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .ok {
    color: var(--ok);
  }

  .warn,
  .error {
    color: var(--warn);
  }

  .error {
    margin: 1rem 0 0;
  }

  .fineprint {
    color: var(--text-dim);
    font-size: 0.75rem;
    margin: 1.5rem 0 0;
    line-height: 1.6;
  }
</style>
