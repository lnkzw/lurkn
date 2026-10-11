import { describe, expect, it } from 'vitest';
import { place, type DOMRectLike } from './anchor';

const VIEWPORT = { width: 1000, height: 800 };

function rect(left: number, top: number, width: number, height: number): DOMRectLike {
  return { left, top, right: left + width, bottom: top + height };
}

const BOX = { width: 200, height: 100 };

describe('place', () => {
  it('sits beside the anchor on the side asked for', () => {
    const at = place(rect(100, 300, 40, 20), BOX, { side: 'right', viewport: VIEWPORT });
    expect(at).toEqual({ left: 146, top: 300 });
  });

  it('flips to the other side when there is no room', () => {
    // A rail on the right edge: the card must land to its left.
    const at = place(rect(950, 300, 40, 20), BOX, { side: 'right', viewport: VIEWPORT });
    expect(at).toEqual({ left: 744, top: 300 });
  });

  it('flips above when the box would pass the bottom edge', () => {
    const at = place(rect(100, 760, 14, 14), BOX, { side: 'bottom', viewport: VIEWPORT });
    expect(at.top).toBe(654);
  });

  it('right-aligns to the anchor when asked', () => {
    const at = place(rect(400, 100, 14, 14), BOX, {
      side: 'bottom',
      align: 'end',
      viewport: VIEWPORT,
    });
    expect(at.left).toBe(414 - BOX.width);
  });

  it('pulls the box back inside the window on the cross axis', () => {
    // An icon flush against the right edge: right-aligning alone would put the
    // box's far end past the window.
    const at = place(rect(986, 10, 14, 14), BOX, {
      side: 'bottom',
      align: 'end',
      viewport: VIEWPORT,
    });
    expect(at.left).toBe(VIEWPORT.width - BOX.width - 8);
  });

  it('keeps the near edge visible when the box is taller than the window', () => {
    const tall = { width: 200, height: 900 };
    const at = place(rect(100, 400, 40, 20), tall, { side: 'right', viewport: VIEWPORT });
    expect(at.top).toBe(8);
  });

  it('does not flip when neither side fits', () => {
    const wide = { width: 900, height: 100 };
    const at = place(rect(480, 300, 40, 20), wide, { side: 'right', viewport: VIEWPORT });
    // Clamped against the right edge rather than thrown off to the left.
    expect(at.left).toBe(VIEWPORT.width - wide.width - 8);
  });
});
