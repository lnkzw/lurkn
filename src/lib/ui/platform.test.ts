import { describe, expect, it } from 'vitest';

import { platformOf } from './platform';

describe('platformOf', () => {
  it('tells the three WebViews apart', () => {
    expect(
      platformOf(
        'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15',
      ),
    ).toBe('linux');
    expect(
      platformOf(
        'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)',
      ),
    ).toBe('macos');
    expect(
      platformOf(
        'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0',
      ),
    ).toBe('windows');
  });

  it('falls back to Linux for anything it does not recognise', () => {
    expect(platformOf('')).toBe('linux');
  });
});
