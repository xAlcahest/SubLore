import { useCallback, useEffect, useRef } from "react";

import { type Match, type Query } from "../search";
import { type SearchReply, type SearchRequest } from "../searchWorker";
import { type CueRow } from "../types/subtitle";

/**
 * How long a search may take before the window stops waiting for it.
 *
 * Measured rather than picked: a replace all over the 2000 cue fixture, which is the largest thing
 * the harness opens, answers in single-digit milliseconds. A hundred times that is far outside any
 * legitimate search and still short enough that a user reads the refusal as an answer rather than
 * as the app having gone away. Written here as the one number this file has.
 */
const PATIENCE_MS = 1000;

/** What a search came back with. `slow` is the pattern that never finished inside the bound. */
export type SearchOutcome =
  | { kind: "found"; match: Match | null }
  | { kind: "replaced"; edits: { cue: number; text: string }[]; count: number }
  | { kind: "bad-pattern"; detail: string }
  | { kind: "slow" };

/** `Omit` over a union keeps only the shared keys; this drops the id from each member instead. */
type WithoutId<T> = T extends { id: number } ? Omit<T, "id"> : never;

/** `only` is the cues to stay inside, and null for the whole document. See F4b. */
export type Search = {
  find: (
    cues: CueRow[],
    only: number[] | null,
    query: Query,
    after: Match | null,
  ) => Promise<SearchOutcome>;
  replaceAll: (
    cues: CueRow[],
    only: number[] | null,
    query: Query,
    replacement: string,
  ) => Promise<SearchOutcome>;
};

/**
 * The search worker, and the promise that it always answers.
 *
 * One worker for the window's life, started on the first search: the probe measured 42 ms to boot
 * one, which is worth paying once and not per keystroke. A request that outlives `PATIENCE_MS` is
 * a pattern the engine will never finish, so the worker is killed and the next search starts a new
 * one. See docs/find-replace-tasks.md F4a.
 */
export function useSearch(): Search {
  const worker = useRef<Worker | null>(null);
  const nextId = useRef(0);

  useEffect(
    () => () => {
      worker.current?.terminate();
      worker.current = null;
    },
    [],
  );

  const ask = useCallback((request: WithoutId<SearchRequest>): Promise<SearchOutcome> => {
    // A classic worker, which is the shape the probe proved runs in this webview.
    worker.current ??= new Worker(new URL("../searchWorker.ts", import.meta.url));
    const live = worker.current;
    nextId.current += 1;
    const id = nextId.current;

    return new Promise<SearchOutcome>((resolve) => {
      let answered = false;
      const done = (outcome: SearchOutcome) => {
        if (answered) {
          return;
        }
        answered = true;
        window.clearTimeout(timer);
        live.removeEventListener("message", onMessage);
        live.removeEventListener("error", onError);
        resolve(outcome);
      };
      const onMessage = (event: MessageEvent<SearchReply>) => {
        // An older request's reply arriving late is not this one's answer.
        if (event.data.id !== id) {
          return;
        }
        done(
          event.data.kind === "found"
            ? { kind: "found", match: event.data.match }
            : event.data.kind === "replaced"
              ? { kind: "replaced", edits: event.data.edits, count: event.data.count }
              : { kind: "bad-pattern", detail: event.data.detail },
        );
      };
      // A worker that failed to load or threw outside the handler must not leave a search hanging.
      const onError = () => done({ kind: "bad-pattern", detail: "the search thread stopped" });
      const timer = window.setTimeout(() => {
        // It is still backtracking and it always will be. Nothing else can reach this thread.
        live.terminate();
        if (worker.current === live) {
          worker.current = null;
        }
        done({ kind: "slow" });
      }, PATIENCE_MS);

      live.addEventListener("message", onMessage);
      live.addEventListener("error", onError);
      live.postMessage({ ...request, id } as SearchRequest);
    });
  }, []);

  const find = useCallback(
    (cues: CueRow[], only: number[] | null, query: Query, after: Match | null) =>
      ask({ kind: "find", cues, only, query, after }),
    [ask],
  );

  const replaceAll = useCallback(
    (cues: CueRow[], only: number[] | null, query: Query, replacement: string) =>
      ask({ kind: "replace-all", cues, only, query, replacement }),
    [ask],
  );

  return { find, replaceAll };
}
