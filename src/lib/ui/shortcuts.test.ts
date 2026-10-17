import { describe, expect, it } from 'vitest';

import { actionFor, isTyping, nativeMenu } from './shortcuts';

describe('actionFor', () => {
  it('ignores a plain key', () => {
    expect(actionFor({ key: 'k' })).toBeNull();
    expect(actionFor({ key: 'ArrowDown' })).toBeNull();
  });

  it('opens the channel list, however the modifier is spelled', () => {
    expect(actionFor({ key: 'k', ctrlKey: true })).toBe('channels');
    expect(actionFor({ key: 'K', metaKey: true })).toBe('channels');
  });

  /** Ctrl+W closes a window everywhere else; taking it would cost a session. */
  it('needs shift for whispers, and leaves plain Ctrl+W alone', () => {
    expect(actionFor({ key: 'w', ctrlKey: true, shiftKey: true })).toBe('whispers');
    expect(actionFor({ key: 'w', ctrlKey: true })).toBeNull();
  });

  it('steps channels with alt and the arrows', () => {
    expect(actionFor({ key: 'ArrowDown', altKey: true })).toBe('next-channel');
    expect(actionFor({ key: 'ArrowUp', altKey: true })).toBe('previous-channel');
  });

  /** Alt+Ctrl+Arrow is a workspace switch on most desktops, not ours. */
  it('leaves a combination that carries both modifiers alone', () => {
    expect(actionFor({ key: 'ArrowDown', altKey: true, ctrlKey: true })).toBeNull();
    expect(actionFor({ key: 'k', ctrlKey: true, altKey: true })).toBeNull();
  });

  it('sizes the chat text with ctrl and plus or minus, on any layout', () => {
    expect(actionFor({ key: '+', ctrlKey: true })).toBe('font-bigger');
    expect(actionFor({ key: '+', ctrlKey: true, shiftKey: true })).toBe('font-bigger');
    expect(actionFor({ key: '=', ctrlKey: true })).toBe('font-bigger');
    expect(actionFor({ key: '-', ctrlKey: true })).toBe('font-smaller');
    expect(actionFor({ key: '_', ctrlKey: true, shiftKey: true })).toBe('font-smaller');
    // Typing them is typing them.
    expect(actionFor({ key: '+' })).toBeNull();
    expect(actionFor({ key: '-' })).toBeNull();
  });

  it('watches the open channel with ctrl and o', () => {
    expect(actionFor({ key: 'o', ctrlKey: true })).toBe('watch');
    expect(actionFor({ key: 'o' })).toBeNull();
  });

  it('refreshes the live list with F5 or ctrl and r, as a browser reloads', () => {
    expect(actionFor({ key: 'F5' })).toBe('refresh');
    expect(actionFor({ key: 'r', ctrlKey: true })).toBe('refresh');
    expect(actionFor({ key: 'r', metaKey: true })).toBe('refresh');
    expect(actionFor({ key: 'r' })).toBeNull();
    expect(actionFor({ key: 'F5', altKey: true })).toBeNull();
  });

  it('knows the rest of the table', () => {
    expect(actionFor({ key: ',', ctrlKey: true })).toBe('settings');
    expect(actionFor({ key: 'End', ctrlKey: true })).toBe('newest');
  });
});

describe('isTyping', () => {
  it('treats text fields and editable elements as typing', () => {
    expect(isTyping('INPUT', false)).toBe(true);
    expect(isTyping('textarea', false)).toBe(true);
    expect(isTyping('div', true)).toBe(true);
  });

  it('treats everything else as not typing', () => {
    expect(isTyping('div', false)).toBe(false);
    expect(isTyping(undefined, false)).toBe(false);
  });
});

describe('nativeMenu', () => {
  /** Paste and spelling live there; the browser's Reload and Inspect do not belong anywhere. */
  it('allows the WebView menu in text fields only', () => {
    expect(nativeMenu({ tagName: 'INPUT' })).toBe(true);
    expect(nativeMenu({ tagName: 'INPUT', type: 'search' })).toBe(true);
    expect(nativeMenu({ tagName: 'INPUT', type: 'checkbox' })).toBe(false);
    expect(nativeMenu({ tagName: 'INPUT', type: 'range' })).toBe(false);
    expect(nativeMenu({ tagName: 'TEXTAREA' })).toBe(true);
    expect(nativeMenu({ tagName: 'DIV', isContentEditable: true })).toBe(true);
    expect(nativeMenu({ tagName: 'BUTTON' })).toBe(false);
    expect(nativeMenu({ tagName: 'IMG' })).toBe(false);
    expect(nativeMenu({ tagName: 'SELECT' })).toBe(false);
    expect(nativeMenu(null)).toBe(false);
  });
});
