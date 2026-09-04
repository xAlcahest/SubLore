/**
 * Finding a string in the open document's cue text. No IPC: the shell already holds every cue, so
 * a search reads what is on screen rather than asking the backend for it again.
 * See docs/find-replace-tasks.md F2.
 */
import { type CueRow } from "./types/subtitle";

/** Where a match sits: the cue it is in, and the offsets into that cue's text. */
export type Match = { cue: number; start: number; end: number };

export type Query = {
  needle: string;
  matchCase: boolean;
};

/** The fourteen characters a regular expression reads as syntax, so a typed one is taken literally. */
const SYNTAX = /[.*+?^${}()|[\]\\]/g;

/**
 * The pattern a query searches with, or null for a query that matches nothing.
 *
 * Case folding goes through the expression engine rather than through `toLowerCase`, and that is a
 * correctness decision rather than a convenience: `"İ".toLowerCase()` is two characters, so a folded
 * haystack carries offsets the real one does not and a highlight computed in it lands in the wrong
 * place. An empty needle matches nothing rather than matching at every position.
 */
function pattern(query: Query): RegExp | null {
  if (query.needle === "") {
    return null;
  }
  return new RegExp(query.needle.replace(SYNTAX, "\\$&"), query.matchCase ? "gu" : "giu");
}

/**
 * The next match after `after`, wrapping the whole file exactly once. Null when the pattern is in
 * no cue at all, which is a result the caller reports rather than an error.
 */
export function nextMatch(
  cues: readonly CueRow[],
  query: Query,
  after: Match | null,
): Match | null {
  const expression = pattern(query);
  if (expression === null || cues.length === 0) {
    return null;
  }
  // A match in a cue the document no longer has is a stale cursor, not a place to resume from.
  const resume = after !== null && after.cue < cues.length ? after : null;
  const first = resume?.cue ?? 0;
  // One step past the cue count, so the cue the search started in is looked at again: a match
  // sitting before the cursor in that same cue is the one a wrap must find.
  for (let step = 0; step <= cues.length; step += 1) {
    const index = (first + step) % cues.length;
    const text = cues[index]?.text ?? "";
    expression.lastIndex = step === 0 && resume !== null ? resume.end : 0;
    const found = expression.exec(text);
    if (found !== null) {
      return { cue: index, start: found.index, end: found.index + found[0].length };
    }
  }
  return null;
}
