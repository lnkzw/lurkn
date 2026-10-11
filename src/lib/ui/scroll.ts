/**
 * Keeping the chat log still while the reader is scrolled up.
 *
 * Two things move a log under someone reading back. New lines arrive at the
 * bottom, which is harmless, and old lines are trimmed off the top, which is
 * not: the content shifts up by whatever was removed while the scroll offset
 * stays numerically the same, so the view slides on towards newer messages.
 * The fix is to remember which row was at the top before an update and put it
 * back where it was afterwards.
 */

/** The row at the top of the view, and how far its top edge sat from the view's. */
export interface ScrollAnchor {
  id: string;
  offset: number;
}

const ROW = '[data-message-id]';

/** The first message row still visible at the top of the log. */
export function captureAnchor(log: HTMLElement): ScrollAnchor | null {
  const rows = log.querySelectorAll<HTMLElement>(ROW);
  if (rows.length === 0) return null;

  const top = log.getBoundingClientRect().top;

  // Rows are in document order, so the first one reaching below the top edge
  // can be found by halving rather than by measuring all 500.
  let lo = 0;
  let hi = rows.length - 1;
  let found: HTMLElement | null = null;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const row = rows[mid];
    if (!row) break;
    if (row.getBoundingClientRect().bottom > top) {
      found = row;
      hi = mid - 1;
    } else {
      lo = mid + 1;
    }
  }

  const id = found?.dataset.messageId;
  if (!found || !id) return null;
  return { id, offset: found.getBoundingClientRect().top - top };
}

/**
 * Scroll so the anchored row is back where it was.
 *
 * Returns false when the row is gone — trimmed away — in which case everything
 * above it went too, and the caller should show the oldest line left.
 */
export function restoreAnchor(log: HTMLElement, anchor: ScrollAnchor): boolean {
  const row = log.querySelector(`[data-message-id="${CSS.escape(anchor.id)}"]`);
  if (!row) return false;

  const offset = row.getBoundingClientRect().top - log.getBoundingClientRect().top;
  log.scrollTop += offset - anchor.offset;
  return true;
}

/**
 * Whether the log should keep following the newest line, after a scroll.
 *
 * Any upward movement stops following, however small: the only rule that
 * survives a busy channel. A tolerance near the bottom meant
 * the first few pixels of a wheel turn still counted as "at the bottom", and
 * the next batch, 80 ms later, snapped the log straight back down. Following
 * resumes only once the reader is back at the very end.
 */
export function stillFollowing(following: boolean, distance: number, movedUp: boolean): boolean {
  // Sub-pixel scroll offsets under display scaling never quite reach zero.
  if (distance < 2) return true;
  if (movedUp) return false;
  return following;
}
