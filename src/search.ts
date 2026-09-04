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

/**
 * Every match rewritten, as one edit per cue that has any, plus how many were replaced.
 *
 * The replacement is applied through a function rather than a string, because `String.replace`
 * reads `$&` and `$1` out of a string replacement: F3 replaces text literally, and capture groups
 * are F4's to add on purpose rather than by accident.
 */
export function replaceEverywhere(
  cues: readonly CueRow[],
  query: Query,
  replacement: string,
): { edits: { cue: number; text: string }[]; count: number } {
  const expression = pattern(query);
  const edits: { cue: number; text: string }[] = [];
  let count = 0;
  if (expression === null) {
    return { edits, count };
  }
  cues.forEach((cue, index) => {
    let hits = 0;
    const rewritten = cue.text.replace(expression, () => {
      hits += 1;
      return replacement;
    });
    if (hits > 0) {
      edits.push({ cue: index, text: rewritten });
      count += hits;
    }
  });
  return { edits, count };
}

/** One match rewritten in place, for the replace that walks the file one hit at a time. */
export function replaceOne(text: string, at: Match, replacement: string): string {
  return text.slice(0, at.start) + replacement + text.slice(at.end);
}
