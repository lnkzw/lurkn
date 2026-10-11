/**
 * A message that clears itself.
 *
 * For the composer's error line. A failure such as AutoMod holding a message
 * is worth saying once, but it describes a moment: left on screen it goes on
 * claiming the last message failed long after the reader has moved on, and
 * nothing they do makes it leave. Setting a new value restarts the clock;
 * setting `null` clears it at once.
 */
export class Transient {
  #value = $state<string | null>(null);
  #timer: ReturnType<typeof setTimeout> | null = null;
  readonly #ms: number;

  constructor(ms: number) {
    this.#ms = ms;
  }

  get value(): string | null {
    return this.#value;
  }

  set value(next: string | null) {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = null;
    this.#value = next;

    if (next !== null) {
      this.#timer = setTimeout(() => {
        this.#value = null;
        this.#timer = null;
      }, this.#ms);
    }
  }
}

/** Long enough to read a sentence twice; short enough not to outlive it. */
export const ERROR_MS = 8000;
