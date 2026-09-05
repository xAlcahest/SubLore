/**
 * What a row of controls asks for, read off the page the app is drawing rather than written down
 * once (S2).
 *
 * A layout minimum that is a number in this repo is a number somebody measured on one machine, and
 * every width in a row of controls is a width the font decides: the same row is about a tenth wider
 * under DejaVu Sans than under the families this was calibrated on, which is enough to wrap it.
 * Nothing here knows the name of a control or a size in pixels; it reads the row it is given.
 */

/** One reading a row can show, applied to a copy of it before that copy is measured. */
export type RowReading = (row: HTMLElement) => void;

/** A row that shows one thing only is measured showing it, rather than measuring nothing at all. */
const AS_DRAWN: readonly RowReading[] = [() => {}];

/**
 * The widest a row is on one row, across the readings it can show. Null only when the row is not in
 * the page, in which case there is no row to keep on one line either.
 *
 * The copy is appended, read and removed without yielding the thread, so nothing else, no observer
 * and no sweep over the page, ever sees a second row.
 */
export function widestRow(row: HTMLElement, readings: readonly RowReading[]): number | null {
  const host = row.parentElement;
  if (host === null) {
    return null;
  }
  const copy = row.cloneNode(true) as HTMLElement;
  // Inside the row's own parent, so it inherits the same font and the same rem; out of the flow and
  // never painted, so it moves nothing; `max-content` so it is one line and nothing is shrunk.
  copy.style.position = "absolute";
  copy.style.top = "0";
  copy.style.left = "0";
  copy.style.width = "max-content";
  copy.style.visibility = "hidden";
  copy.style.pointerEvents = "none";
  copy.setAttribute("aria-hidden", "true");
  host.append(copy);
  try {
    let widest = 0;
    for (const reading of readings.length === 0 ? AS_DRAWN : readings) {
      reading(copy);
      widest = Math.max(widest, unwrappedWidth(copy));
    }
    return widest;
  } finally {
    copy.remove();
  }
}

/**
 * The row's own width with everything on one line: its padding, its borders, its gaps, and each
 * child at the width that child asks for.
 */
function unwrappedWidth(row: HTMLElement): number {
  const style = window.getComputedStyle(row);
  const children = Array.from(row.children);
  let total =
    px(style.paddingLeft) +
    px(style.paddingRight) +
    px(style.borderLeftWidth) +
    px(style.borderRightWidth) +
    px(style.columnGap) * Math.max(0, children.length - 1);
  for (const child of children) {
    total += outerWidth(child);
  }
  return total;
}

/** What one child asks for: the width it is drawn at, or, for a child that takes the row's slack,
 * the width its own rule says it never goes under. */
function outerWidth(child: Element): number {
  const style = window.getComputedStyle(child);
  const declared = style.minWidth.endsWith("px") ? Number.parseFloat(style.minWidth) : Number.NaN;
  const flexible = px(style.flexGrow) > 0;
  const own =
    flexible && Number.isFinite(declared) ? declared : child.getBoundingClientRect().width;
  return own + px(style.marginLeft) + px(style.marginRight);
}

/** A computed length in pixels, and 0 for the keywords that name no length. */
function px(value: string): number {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : 0;
}
