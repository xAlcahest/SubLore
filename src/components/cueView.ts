/**
 * What the two views of a cue share: how its numbers are drawn, and the mark that says a field
 * edits the open document rather than owning its own keyboard.
 *
 * The grid row and the current-line box are two views of one row, not two states (decision 5), so
 * neither restates any of this and the two cannot drift apart. See T5.
 */
import { type CueRow } from "../types/subtitle";

/** Reading rate a line is flagged above, fixed and not configurable in v1. Decision 24 A8. */
export const CPS_LIMIT = 21;
/** The markup A8 does not count: ASS override blocks and HTML-style tags. */
const MARKUP = /\{[^}]*\}|<[^>]*>/g;
/** Line breaks in both spellings a cue holds: a real one, and the `\N` of an ASS field. */
const LINE_BREAKS = /\r\n|[\r\n]|\\[Nn]/g;

/**
 * Whether a field edits the open document. Both editors carry `data-document-editor`, so Ctrl+Z
 * inside either is the document's undo and never the webview's own text undo, which would fork the
 * two histories. See BACKLOG.md M2.3 and T5.
 */
export function isDocumentEditor(element: HTMLElement): boolean {
  return element.dataset.documentEditor !== undefined;
}

/**
 * Characters per second: spaces counted, line breaks not, over text with its markup stripped.
 * Null when the cue has no duration to divide by. Decision 24 A8.
 *
 * CodeQL reads the strip below as an incomplete HTML sanitizer and is wrong about what it is: the
 * stripped string is never bound, `.length` consumes it here, and nothing renders it. Dismissed
 * twice, once per address it has lived at; the measurements are in #49.
 */
export function readingRate(cue: CueRow): number | null {
  const seconds = (cue.endMs - cue.startMs) / 1000;
  if (!(seconds > 0)) {
    return null;
  }
  return cue.text.replace(MARKUP, "").replace(LINE_BREAKS, "").length / seconds;
}

/** hh:mm:ss.mmm. Separators are punctuation, not translatable copy. */
export function timecode(milliseconds: number): string {
  const safe = Number.isFinite(milliseconds) && milliseconds > 0 ? Math.floor(milliseconds) : 0;
  const millis = safe % 1000;
  const seconds = Math.floor(safe / 1000) % 60;
  const minutes = Math.floor(safe / 60_000) % 60;
  const hours = Math.floor(safe / 3_600_000);
  const pad = (value: number, width: number) => value.toString().padStart(width, "0");
  return `${pad(hours, 2)}:${pad(minutes, 2)}:${pad(seconds, 2)}.${pad(millis, 3)}`;
}

/**
 * A time a person typed, back into milliseconds. Hours and minutes are optional and either
 * separator introduces the milliseconds, because a translator types `9.1` and pastes `00:00:09,100`
 * for the same instant. Null when the string is not a time, and a null is never committed.
 *
 * The digit counts are the bound on the result: three digits of the leading unit is at most
 * 999:59:59.999, which is inside the `u32` the command takes. See M2.7 E1.
 */
const TYPED_TIME = /^(\d{1,3}(?::\d{1,2}){0,2})[.,](\d{1,3})$/;

export function parseTimecode(value: string): number | null {
  const match = TYPED_TIME.exec(value.trim());
  if (match === null) {
    return null;
  }
  const units = match[1].split(":").map(Number);
  // Everything but the leading unit is a sexagesimal digit: `1:75.000` is not a time.
  if (units.slice(1).some((unit) => unit > 59)) {
    return null;
  }
  const millis = Number(match[2].padEnd(3, "0"));
  // Read from the right, so that seconds, minutes and hours all land in the right place.
  const seconds = units.pop() ?? 0;
  const minutes = units.pop() ?? 0;
  const hours = units.pop() ?? 0;
  return hours * 3_600_000 + minutes * 60_000 + seconds * 1000 + millis;
}

/**
 * A cue's length in seconds, to the millisecond the product reasons in (decision 11). Shown as
 * seconds rather than as a timecode: a length is judged against the second, not against the hour.
 */
export function lengthLabel(cue: CueRow): string {
  const milliseconds = cue.endMs - cue.startMs;
  return (Number.isFinite(milliseconds) ? milliseconds / 1000 : 0).toFixed(3);
}
