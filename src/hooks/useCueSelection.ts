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
  /**
   * Rows changed under the two states: `removed` of them vanished at `at` and `inserted` took their
   * place. Both states are indexed by row, so without this an insert above the cursor leaves it
   * pointing at a different line and a delete leaves the row that moved up wearing the selection of
   * the row that went. See BACKLOG.md M2.7.
   */
  rowsMoved: (at: number, removed: number, inserted: number) => void;
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
 * first row, selected alone.
 *
 * `openId` counts opens: the shell owns these states now that the tools column reads the cursor
 * too (T5), so a new document can no longer reset them by remounting the grid.
 */
export function useCueSelection(count: number, openId: number): CueSelection {
  const [active, setActive] = useState<number | null>(count === 0 ? null : 0);
  const [selected, setSelected] = useState<ReadonlySet<number>>(() =>
    count === 0 ? new Set<number>() : new Set<number>([0]),
  );
  /** Where a range extension starts. Nothing renders from it, so it is a ref. */
  const anchor = useRef(0);
  const [openedAt, setOpenedAt] = useState(openId);

  // A new document starts on its first row. The rows and the count arrive in the same update, so
  // the branch below already sees the document it is resetting onto.
  if (openedAt !== openId) {
    setOpenedAt(openId);
    setActive(count === 0 ? null : 0);
    setSelected(count === 0 ? new Set<number>() : new Set<number>([0]));
    anchor.current = 0;
  }

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

  const rowsMoved = useCallback(
    (at: number, removed: number, inserted: number) => {
      const delta = inserted - removed;
      if (delta === 0) {
        return;
      }
      // `count` is what stood before this call: the patch that moved the rows and this run in one
      // update, so the new length is that plus the delta rather than anything read from state.
      const left = count + delta;
      // A row inside the replaced range no longer exists. The cursor takes the first row standing
      // in its place, clamped when the last row was the one that went; a selected row simply goes,
      // because nothing it named is on screen any more.
      const after = at + removed;
      const moved = (index: number) => (index < at ? index : index + delta);
      const survives = (index: number) => index < at || index >= after;
      const settle = (index: number) => Math.min(survives(index) ? moved(index) : at, left - 1);

      // Worked out before both updaters rather than inside them: the selection's fallback is the
      // cursor's new row, and an updater cannot see where the other one landed.
      const cursor = active === null || left === 0 ? null : settle(active);
      setActive(cursor);
      setSelected((current) => {
        const next = new Set<number>();
        for (const index of current) {
          if (survives(index)) {
            next.add(moved(index));
          }
        }
        // The selection never empties while rows stand, which is this hook's own invariant: when
        // every row it named is one that went, it comes down onto the cursor.
        if (next.size === 0 && cursor !== null) {
          next.add(cursor);
        }
        return next;
      });
      anchor.current = left === 0 ? 0 : settle(anchor.current);
    },
    [active, count],
  );

  return { active, selected, move, toggle, selectAll, collapse, rowsMoved };
}
