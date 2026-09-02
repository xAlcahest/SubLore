import { useCallback, useRef, useState } from "react";

/** What a gesture does to the selection while it moves the cursor. See decision 5. */
export type CueMove = "plain" | "extend" | "cursorOnly";

export type CueSelection = {
  /** The cursor: one row, or null while the document has no rows. */
  active: number | null;
  /** Membership, which is what a bulk operation acts on. Never empty while rows stand. */
  selected: ReadonlySet<number>;
  move: (index: number, how: CueMove) => void;
  /** Ctrl+Space and ctrl-click: the cursor goes to the row and that row's membership flips. */
  toggle: (index: number) => void;
  selectAll: () => void;
  /** Escape: the selection falls back onto the cursor. */
  collapse: () => void;
};

/** The rows between two indices, inclusive, whichever order they arrive in. */
function runBetween(from: number, to: number): Set<number> {
  const rows = new Set<number>();
  for (let index = Math.min(from, to); index <= Math.max(from, to); index += 1) {
    rows.add(index);
  }
  return rows;
}

/**
 * The active line and the selection as two states, decision 5. A document with rows opens on its
 * first row, selected alone; the grid remounts per document, so a new document starts there too.
 */
export function useCueSelection(count: number): CueSelection {
  const [active, setActive] = useState<number | null>(count === 0 ? null : 0);
  const [selected, setSelected] = useState<ReadonlySet<number>>(() =>
    count === 0 ? new Set<number>() : new Set<number>([0]),
  );
  /** Where a range extension starts. Nothing renders from it, so it is a ref. */
  const anchor = useRef(0);

  const move = useCallback((index: number, how: CueMove) => {
    setActive(index);
    // cursorOnly leaves the selection where it is: the only way the keyboard can build a
    // scattered set, since Ctrl+Space only ever toggles the row under the cursor (decision 5).
    if (how === "extend") {
      setSelected(runBetween(anchor.current, index));
    } else if (how === "plain") {
      anchor.current = index;
      setSelected(new Set([index]));
    }
  }, []);

  const toggle = useCallback((index: number) => {
    setActive(index);
    anchor.current = index;
    setSelected((current) => {
      const next = new Set(current);
      // The selection never empties while rows stand, so the last member cannot be toggled off.
      if (next.has(index) && next.size > 1) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  }, []);

  const selectAll = useCallback(() => {
    setSelected(new Set(Array.from({ length: count }, (_, index) => index)));
  }, [count]);

  const collapse = useCallback(() => {
    setSelected(active === null ? new Set<number>() : new Set([active]));
  }, [active]);

  return { active, selected, move, toggle, selectAll, collapse };
}
