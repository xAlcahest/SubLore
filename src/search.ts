/**
 * Finding a string in the open document's cue text. No IPC: the shell already holds every cue, so
 * a search reads what is on screen rather than asking the backend for it again.
 *
 * Everything here is pure and synchronous, and it runs inside the worker rather than on the
 * window's own thread: a pattern the user types is the one input this app runs as code, and a bad
 * one backtracks forever. See docs/find-replace-tasks.md F4a and `searchWorker.ts`.
 */
import { type CueRow } from "./types/subtitle";

/**
 * Where a match sits: the cue it is in, the offsets into that cue's text, and what it captured.
 *
 * `found` is the whole match followed by its groups, with a group that took part in nothing written
 * as an empty string. It is carried rather than recomputed so that replacing one match can expand
 * `$1` without running the user's expression again on the window's own thread.
 */
export type Match = { cue: number; start: number; end: number; found: string[] };

export type Query = {
  needle: string;
  matchCase: boolean;
  /** Off: the needle is taken literally. On: it is the user's own expression, hazards and all. */
  regex: boolean;
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
 *
 * Throws for an expression the engine will not compile, which the caller reports rather than
 * swallows: an unclosed bracket is a typo, not an empty result.
 */
function pattern(query: Query): RegExp | null {
  if (query.needle === "") {
    return null;
  }
  const source = query.regex ? query.needle : query.needle.replace(SYNTAX, "\\$&");
  return new RegExp(source, query.matchCase ? "gu" : "giu");
}

/** One cue the search may look in: its text, and where it sits in the document. */
type Scoped = { cue: number; text: string };

/**
 * The cues a search may look in, in file order and each one once.
 *
 * `only` is the selection when the band is asked to stay inside it, and null for the whole file.
 * The indices it returns are the document's own, never positions in this list: a match carries the
 * cue it is in, and the cursor and the edit both read that number.
 */
function scopeOf(cues: readonly CueRow[], only: readonly number[] | null): Scoped[] {
  if (only === null) {
    return cues.map((cue, index) => ({ cue: index, text: cue.text }));
  }
  const wanted = new Set(only);
  return cues.flatMap((cue, index) => (wanted.has(index) ? [{ cue: index, text: cue.text }] : []));
}

/**
 * The next match after `after`, wrapping the scope exactly once. Null when the pattern is in no cue
 * of it at all, which is a result the caller reports rather than an error.
 */
export function nextMatch(
  cues: readonly CueRow[],
  only: readonly number[] | null,
  query: Query,
  after: Match | null,
): Match | null {
  const expression = pattern(query);
  const scope = scopeOf(cues, only);
  if (expression === null || scope.length === 0) {
    return null;
  }
  // A match in a cue the scope no longer holds is a stale cursor, not a place to resume from: the
  // selection may have moved under it, or the document may have.
  const at = after === null ? -1 : scope.findIndex((one) => one.cue === after.cue);
  const resume = at < 0 ? null : after;
  const first = at < 0 ? 0 : at;
  // One step past the scope's size, so the cue the search started in is looked at again: a match
  // sitting before the cursor in that same cue is the one a wrap must find.
  for (let step = 0; step <= scope.length; step += 1) {
    const here = scope[(first + step) % scope.length];
    if (here === undefined) {
      return null;
    }
    const { cue: index, text } = here;
    expression.lastIndex = step === 0 && resume !== null ? resume.end : 0;
    const found = expression.exec(text);
    if (found !== null) {
      // An expression that matches nothing consumes nothing: the caller resumes from `end`, so an
      // empty match would hand back the same one for ever. Reachable only with regex on.
      if (found[0].length === 0) {
        return null;
      }
      return {
        cue: index,
        start: found.index,
        end: found.index + found[0].length,
        found: captured(found),
      };
    }
  }
  return null;
}

/**
 * Every match rewritten, as one edit per cue that has any, plus how many were replaced.
 *
 * Built by walking the matches rather than through `String.replace`, because the two replacement
 * forms cannot be mixed: with regex off the replacement has to land exactly as typed, and with it
 * on `$1` has to name what the expression captured. `String.replace` offers one or the other.
 */
export function replaceEverywhere(
  cues: readonly CueRow[],
  only: readonly number[] | null,
  query: Query,
  replacement: string,
): { edits: { cue: number; text: string }[]; count: number } {
  const expression = pattern(query);
  const edits: { cue: number; text: string }[] = [];
  let count = 0;
  if (expression === null) {
    return { edits, count };
  }
  for (const { cue: index, text } of scopeOf(cues, only)) {
    // A zero-length match is skipped rather than replaced at every position, which is what the
    // single search does with one too.
    const hits = [...text.matchAll(expression)].filter((hit) => hit[0].length > 0);
    if (hits.length === 0) {
      continue;
    }
    let rewritten = "";
    let cursor = 0;
    for (const hit of hits) {
      rewritten += text.slice(cursor, hit.index) + written(query, replacement, captured(hit));
      cursor = hit.index + hit[0].length;
    }
    edits.push({ cue: index, text: rewritten + text.slice(cursor) });
    count += hits.length;
  }
  return { edits, count };
}

/** The whole match and its groups, with a group that took part in nothing written as empty. */
function captured(hit: RegExpExecArray | RegExpMatchArray): string[] {
  return Array.from(hit, (group) => group ?? "");
}

/** What one match is replaced by: as typed with regex off, expanded with it on. */
export function written(query: Query, replacement: string, found: readonly string[]): string {
  return query.regex ? expand(replacement, found) : replacement;
}

/**
 * `$$`, `$&` and `$1` to `$9` in a replacement, against one match's own replacer arguments.
 *
 * Only the sequences a translator reaches for are honoured; anything else is left as typed rather
 * than silently eaten, which is the safer direction for text somebody is about to write into a file.
 */
function expand(replacement: string, found: readonly string[]): string {
  return replacement.replace(/\$(\$|&|[1-9])/g, (_written: string, what: string) => {
    if (what === "$") {
      return "$";
    }
    if (what === "&") {
      return found[0] ?? "";
    }
    // A group the expression does not have, or one that matched nothing, contributes nothing.
    return found[Number(what)] ?? "";
  });
}

/**
 * One match rewritten in place, for the replace that walks the file one hit at a time.
 *
 * Safe on the window's own thread: it splices a string and expands what the match already carried,
 * so no expression of the user's runs here.
 */
export function replaceOne(text: string, at: Match, query: Query, replacement: string): string {
  return text.slice(0, at.start) + written(query, replacement, at.found) + text.slice(at.end);
}
