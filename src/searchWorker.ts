/**
 * The thread the user's own expression runs on.
 *
 * Everything a search does happens here, plain text included. Two implementations of one search is
 * how the two drift, and the cost of sending the cues across is paid once per search rather than
 * per keystroke. A pattern that backtracks for ever hangs this thread and nothing else; the window
 * kills it and starts another. See docs/find-replace-tasks.md F4a.
 */
import { nextMatch, replaceEverywhere, type Match, type Query } from "./search";
import { type CueRow } from "./types/subtitle";

export type SearchRequest =
  | { id: number; kind: "find"; cues: CueRow[]; query: Query; after: Match | null }
  | { id: number; kind: "replace-all"; cues: CueRow[]; query: Query; replacement: string };

export type SearchReply =
  | { id: number; kind: "found"; match: Match | null }
  | { id: number; kind: "replaced"; edits: { cue: number; text: string }[]; count: number }
  /** The expression did not compile. The message is the engine's own, for the band to draw. */
  | { id: number; kind: "bad-pattern"; detail: string };

self.onmessage = (event: MessageEvent<SearchRequest>) => {
  const request = event.data;
  try {
    if (request.kind === "find") {
      const reply: SearchReply = {
        id: request.id,
        kind: "found",
        match: nextMatch(request.cues, request.query, request.after),
      };
      self.postMessage(reply);
      return;
    }
    const { edits, count } = replaceEverywhere(request.cues, request.query, request.replacement);
    const reply: SearchReply = { id: request.id, kind: "replaced", edits, count };
    self.postMessage(reply);
  } catch (failure) {
    // The one expected failure is a pattern the engine refuses. Anything else is a defect and is
    // reported the same way rather than left as a request that never answers.
    const reply: SearchReply = {
      id: request.id,
      kind: "bad-pattern",
      detail: failure instanceof Error ? failure.message : String(failure),
    };
    self.postMessage(reply);
  }
};
