import { describe, expect, it } from 'vitest';

import { commands, events } from './index';

/**
 * Guards the generated binding surface. If a Rust signature changes without the
 * frontend following, this is where it surfaces — see docs/architecture.md §8.2.
 */
describe('generated IPC bindings', () => {
  it('exposes the app_info command as camelCase', () => {
    expect(typeof commands.appInfo).toBe('function');
  });

  it('exposes events with a listener surface', () => {
    expect(typeof events.settingsChanged.listen).toBe('function');
    expect(typeof events.settingsChanged.once).toBe('function');
  });
});
