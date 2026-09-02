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
 * A cue's length in seconds, to the millisecond the product reasons in (decision 11). Shown as
 * seconds rather than as a timecode: a length is judged against the second, not against the hour.
 */
export function lengthLabel(cue: CueRow): string {
  const milliseconds = cue.endMs - cue.startMs;
  return (Number.isFinite(milliseconds) ? milliseconds / 1000 : 0).toFixed(3);
}
