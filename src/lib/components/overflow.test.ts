import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * Narrow tiled windows are the failure case: a single unbroken run of text
 * pushed messages past the panel border. These assertions pin the rules that
 * prevent it, since the symptom only shows at a specific window width and no
 * component test would reproduce it.
 */
function styles(component: string): string {
  return readFileSync(join(process.cwd(), 'src/lib/components', component), 'utf8');
}

describe('chat layout cannot overflow a narrow window', () => {
  it('breaks long words inside a message', () => {
    const css = styles('ChatMessage.svelte');
    expect(css).toContain('overflow-wrap: anywhere');
    // break-word leaves an unbroken URL sticking out; anywhere does not.
    expect(css).not.toContain('word-break: break-word');
  });

  it('lets the chat panel shrink below its content', () => {
    const css = styles('Chat.svelte');
    expect(css).toMatch(/\.chat\s*\{[^}]*min-width:\s*0/s);
    expect(css).toMatch(/\.chat\s*\{[^}]*flex:\s*1/s);
    // A fixed width cannot shrink, which is what broke tiling before.
    expect(css).not.toMatch(/\.chat\s*\{[^}]*width:\s*\d+px/s);
  });

  it('never scrolls the chat log sideways', () => {
    expect(styles('Chat.svelte')).toMatch(/\.log\s*\{[^}]*overflow-x:\s*hidden/s);
  });

  it('keeps the drawer inside the window', () => {
    const css = styles('Sidebar.svelte');
    expect(css).toMatch(/\.drawer\s*\{[^}]*max-width:\s*100%/s);
    expect(css).toMatch(/\.drawer\s*\{[^}]*max-height:\s*100%/s);
  });
});

describe('the avatar rail leaves no room for a scrollbar', () => {
  it('hides the scrollbar when collapsed', () => {
    const css = readFileSync(
      join(process.cwd(), 'src/lib/components', 'Sidebar.svelte'),
      'utf8',
    );
    // Both properties: Firefox honours scrollbar-width, WebKitGTK the size token
    // that drives its ::-webkit-scrollbar rule in app.css.
    expect(css).toMatch(/\.collapsed \.scroll\s*\{[^}]*scrollbar-width:\s*none/s);
    expect(css).toMatch(/\.collapsed \.scroll\s*\{[^}]*--scrollbar-size:\s*0/s);
  });

  it('centres rows in the rail rather than reserving side padding', () => {
    const css = readFileSync(
      join(process.cwd(), 'src/lib/components', 'Sidebar.svelte'),
      'utf8',
    );
    expect(css).toMatch(/\.collapsed \.row[^{]*\{[^}]*justify-content:\s*center/s);
  });
});

describe('tall emotes grow their line instead of overlapping', () => {
  const css = () =>
    readFileSync(join(process.cwd(), 'src/lib/components', 'ChatMessage.svelte'), 'utf8');

  /**
   * A negative vertical margin lifts the image out of the line box, so once the
   * emote is taller than the line height it draws over the row above.
   */
  it('gives emotes no negative vertical margin', () => {
    const emote = /\.emote\s*\{([^}]*)\}/.exec(css())?.[1] ?? '';
    expect(emote).toContain('height: var(--emote-height');
    expect(emote).not.toMatch(/margin:\s*-/);
    expect(emote).not.toMatch(/margin-top:\s*-/);
  });

  it('gives badges no negative vertical margin either', () => {
    const badge = /\.badge\s*\{([^}]*)\}/.exec(css())?.[1] ?? '';
    expect(badge).not.toMatch(/margin-top:\s*-/);
  });

  /** The horizontal pull-back is what stacks overlay emotes; it must survive. */
  it('keeps the horizontal offset that stacks zero-width emotes', () => {
    const zero = /\.emote\.zero-width\s*\{([^}]*)\}/.exec(css())?.[1] ?? '';
    expect(zero).toMatch(/margin-left:\s*calc\(/);
  });
});

describe('the composer controls match in size', () => {
  const css = () =>
    readFileSync(join(process.cwd(), 'src/lib/components', 'Chat.svelte'), 'utf8');

  it('sizes the input and the send button from one shared height', () => {
    expect(css()).toMatch(/--control-height:/);
    expect(/input\s*\{([^}]*)\}/.exec(css())?.[1] ?? '').toContain(
      'height: var(--control-height)',
    );
    expect(/button\[type='submit'\]\s*\{([^}]*)\}/.exec(css())?.[1] ?? '').toContain(
      'height: var(--control-height)',
    );
  });
});

describe('the composer does not trap the command menu', () => {
  /**
   * The menu is `position: fixed` and placed in viewport coordinates. Any
   * transform, filter or containment on an ancestor would make that ancestor
   * the containing block instead, and the menu would open off screen — which it
   * did, once.
   */
  it('centres the in-field trigger without a transform', () => {
    const css = readFileSync(join(process.cwd(), 'src/lib/components', 'Chat.svelte'), 'utf8');
    const trailing = /\.trailing\s*\{([^}]*)\}/.exec(css)?.[1] ?? '';

    expect(trailing).toContain('position: absolute');
    expect(trailing).not.toMatch(/transform:|filter:|contain:|backdrop-filter:/);
  });
});

describe('the chat header and the avatar rail line up', () => {
  /**
   * Both read the same token rather than each choosing its own padding, so the
   * first avatar stays level with the header when either is adjusted.
   */
  it('sizes the chat header from the shared rail row height', () => {
    const css = readFileSync(join(process.cwd(), 'src/lib/components', 'Chat.svelte'), 'utf8');
    expect(/header\s*\{([^}]*)\}/.exec(css)?.[1] ?? '').toContain(
      'height: var(--rail-item-height)',
    );
  });

  it('sizes the rail rows from the same token', () => {
    const css = readFileSync(join(process.cwd(), 'src/lib/components', 'Sidebar.svelte'), 'utf8');
    expect(css).toMatch(/\.collapsed \.row[^{]*\{[^}]*height:\s*var\(--rail-item-height\)/s);
  });

  /** A padded list would push the first avatar below the header. */
  it('starts the rail flush with the top', () => {
    const css = readFileSync(join(process.cwd(), 'src/lib/components', 'Sidebar.svelte'), 'utf8');
    expect(css).toMatch(/\.collapsed ul\s*\{[^}]*padding-top:\s*0/s);
  });

  it('separates the avatars with the shared gap', () => {
    const css = readFileSync(join(process.cwd(), 'src/lib/components', 'Sidebar.svelte'), 'utf8');
    expect(css).toMatch(/\.collapsed ul\s*\{[^}]*gap:\s*var\(--rail-gap\)/s);
  });
});


describe('the show-scrollbars setting reaches every scrollable surface', () => {
  const appCss = () => readFileSync(join(process.cwd(), 'src', 'app.css'), 'utf8');

  /**
   * Inherited custom properties rather than a global override: Svelte's scoped
   * class raises component specificity, so a blanket rule would lose.
   */
  it('turns the setting into inherited tokens', () => {
    expect(appCss()).toMatch(/:root\[data-scrollbars='hidden'\]\s*\{[^}]*--scrollbar-width:\s*none/s);
    expect(appCss()).toMatch(/:root\[data-scrollbars='hidden'\]\s*\{[^}]*--scrollbar-size:\s*0/s);
  });

  it('has the scrollable components read those tokens', () => {
    const sidebar = readFileSync(
      join(process.cwd(), 'src/lib/components', 'Sidebar.svelte'),
      'utf8',
    );
    const chat = readFileSync(join(process.cwd(), 'src/lib/components', 'Chat.svelte'), 'utf8');

    expect(sidebar).toMatch(/\.scroll\s*\{[^}]*scrollbar-width:\s*var\(--scrollbar-width\)/s);
    expect(chat).toMatch(/\.log\s*\{[^}]*scrollbar-width:\s*var\(--scrollbar-width\)/s);
  });

  /** The rail has no room for one whatever the setting says. */
  it('keeps the avatar rail scrollbar hidden regardless', () => {
    const sidebar = readFileSync(
      join(process.cwd(), 'src/lib/components', 'Sidebar.svelte'),
      'utf8',
    );
    expect(sidebar).toMatch(/\.collapsed \.scroll\s*\{[^}]*scrollbar-width:\s*none/s);
  });
});

describe('chat links leave the WebView', () => {
  /**
   * `target="_blank"` does nothing in a Tauri WebView, and following the href
   * would navigate away from Lurkn itself. The click has to be handed to the
   * backend launcher instead.
   */
  it('routes link clicks through the backend rather than the href', () => {
    const css = readFileSync(
      join(process.cwd(), 'src/lib/components', 'ChatMessage.svelte'),
      'utf8',
    );
    expect(css).toContain('commands.openLink');
    expect(css).toContain('event.preventDefault()');
    expect(css).not.toContain('target="_blank"');
  });
});

describe('the stream title truncates instead of pushing the icons out', () => {
  const css = () =>
    readFileSync(join(process.cwd(), 'src/lib/components', 'Chat.svelte'), 'utf8');

  it('lets the title shrink to an ellipsis', () => {
    const title = /\.title\s*\{([^}]*)\}/.exec(css())?.[1] ?? '';
    expect(title).toContain('text-overflow: ellipsis');
    expect(title).toContain('min-width: 0');
    // Growing would steal the space the spacer needs to right-align the icons.
    expect(title).toMatch(/flex:\s*0 1 auto/);
  });

  it('keeps the spacer able to grow so icons stay right-aligned', () => {
    const spacer = /\.spacer\s*\{([^}]*)\}/.exec(css())?.[1] ?? '';
    expect(spacer).toMatch(/flex:\s*1 1 auto/);
  });
});

describe('the channel name is never truncated', () => {
  /**
   * The channel you are reading has to stay legible; the stream title is what
   * gives way when the header runs out of room.
   */
  it('stops the heading from shrinking', () => {
    const css = readFileSync(join(process.cwd(), 'src/lib/components', 'Chat.svelte'), 'utf8');
    const heading = /\n {2}h2\s*\{([^}]*)\}/.exec(css)?.[1] ?? '';

    expect(heading).toMatch(/flex:\s*0 0 auto/);
    expect(heading).toContain('white-space: nowrap');
    expect(heading).not.toContain('text-overflow: ellipsis');
  });
});

describe('native widgets follow the theme', () => {
  const appCss = () => readFileSync(join(process.cwd(), 'src', 'app.css'), 'utf8');

  /**
   * A select's dropdown, checkboxes and the default scrollbars are drawn by the
   * engine, not by page CSS. Only `color-scheme` tells it which palette to use,
   * so without this they stay light on a dark page.
   */
  it('declares a colour scheme for both themes', () => {
    expect(appCss()).toMatch(/:root\s*\{[^}]*color-scheme:\s*dark/s);
    expect(appCss()).toMatch(/:root\[data-theme='light'\]\s*\{[^}]*color-scheme:\s*light/s);
  });
});
