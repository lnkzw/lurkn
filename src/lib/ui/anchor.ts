/**
 * Places a floating box next to an element, in viewport coordinates.
 *
 * Both callers (the settings hints and the channel preview) sit inside a
 * scrolling container, which clips anything absolutely positioned. Fixed
 * positioning escapes that, but then the coordinates have to be worked out
 * rather than declared — that arithmetic lives here so it is testable without
 * a browser, and so both callers flip and clamp the same way.
 */

export type Side = 'top' | 'right' | 'bottom' | 'left';

export interface Box {
  width: number;
  height: number;
}

export interface Point {
  left: number;
  top: number;
}

export interface PlaceOptions {
  /** Where to put the box if it fits there. */
  side: Side;
  /**
   * Which end of the anchor the box lines up with on the other axis. `end`
   * right-aligns a box below its anchor, keeping it inside a narrow panel.
   */
  align?: 'start' | 'end';
  /** Space between anchor and box. */
  gap?: number;
  /** Space kept clear of the window edge. */
  edge?: number;
  viewport?: Box;
}

const OPPOSITE: Record<Side, Side> = {
  top: 'bottom',
  bottom: 'top',
  left: 'right',
  right: 'left',
};

function cornerFor(side: Side, align: 'start' | 'end', anchor: DOMRectLike, box: Box): Point {
  switch (side) {
    case 'right':
      return { left: anchor.right, top: align === 'end' ? anchor.bottom - box.height : anchor.top };
    case 'left':
      return {
        left: anchor.left - box.width,
        top: align === 'end' ? anchor.bottom - box.height : anchor.top,
      };
    case 'bottom':
      return { left: align === 'end' ? anchor.right - box.width : anchor.left, top: anchor.bottom };
    case 'top':
      return {
        left: align === 'end' ? anchor.right - box.width : anchor.left,
        top: anchor.top - box.height,
      };
  }
}

function withGap(side: Side, point: Point, gap: number): Point {
  switch (side) {
    case 'right':
      return { ...point, left: point.left + gap };
    case 'left':
      return { ...point, left: point.left - gap };
    case 'bottom':
      return { ...point, top: point.top + gap };
    case 'top':
      return { ...point, top: point.top - gap };
  }
}

export interface DOMRectLike {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/** Whether the box lands fully inside the viewport on the side's own axis. */
function fitsOnAxis(side: Side, point: Point, box: Box, viewport: Box, edge: number): boolean {
  if (side === 'left' || side === 'right') {
    return point.left >= edge && point.left + box.width <= viewport.width - edge;
  }
  return point.top >= edge && point.top + box.height <= viewport.height - edge;
}

function clamp(value: number, size: number, limit: number, edge: number): number {
  // When the box is larger than the window, the edge nearest the start wins:
  // clipping the far end is better than starting off screen.
  return Math.max(edge, Math.min(value, limit - size - edge));
}

export function place(anchor: DOMRectLike, box: Box, options: PlaceOptions): Point {
  const { side, align = 'start', gap = 6, edge = 8 } = options;
  const viewport = options.viewport ?? {
    width: window.innerWidth,
    height: window.innerHeight,
  };

  let point = withGap(side, cornerFor(side, align, anchor, box), gap);

  // Only flip when the other side actually has room; flipping into a second
  // bad fit just moves the problem.
  if (!fitsOnAxis(side, point, box, viewport, edge)) {
    const flipped = OPPOSITE[side];
    const alternative = withGap(flipped, cornerFor(flipped, align, anchor, box), gap);
    if (fitsOnAxis(flipped, alternative, box, viewport, edge)) {
      point = alternative;
    }
  }

  return {
    left: clamp(point.left, box.width, viewport.width, edge),
    top: clamp(point.top, box.height, viewport.height, edge),
  };
}
