import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

/** Kept in step with `src-tauri/src/layout.rs`; the file it comes from carries the same names. */
export type Layout = {
  waveformHeight: number;
  /** The video panel's share of the top row, not a width: see the note in `layout.rs`. */
  videoFraction: number;
  topHeight: number;
  /** A multiplier on the root font size, not a panel bound: see `layout.rs`. */
  interfaceScale: number;
};

/**
 * Where the panels were left, and how to say where they are now.
 *
 * The read happens once, when the shell mounts, and the write only when a drag ends: a write per
 * pointer move would put the disk inside the frame budget M2.3 measured. Losing the file costs one
 * drag, so nothing here fails loudly — the backend answers with the default and says so in its log.
 *
 * Both setters take one edge and send all three: a write that named only the edge that moved would
 * reset the other two to whatever the caller last rendered.
 */
export function useLayout(): {
  layout: Layout | null;
  changeLayout: (edge: Partial<Layout>) => void;
  storeLayout: (edge: Partial<Layout>) => void;
} {
  const [layout, setLayout] = useState<Layout | null>(null);
  // The same value as the state, readable during a drag: a release that merged inside the state
  // updater would send its write twice under StrictMode.
  const current = useRef<Layout | null>(null);

  useEffect(() => {
    let alive = true;
    void invoke<Layout>("layout_read")
      .then((stored) => {
        if (alive) {
          current.current = stored;
          setLayout(stored);
        }
      })
      .catch(() => {
        // The command cannot fail; a rejection here means the shell is going away mid-read, and a
        // layout nobody will draw is not worth reporting.
      });
    return () => {
      alive = false;
    };
  }, []);

  const apply = useCallback((edge: Partial<Layout>): Layout | null => {
    const next = current.current === null ? null : { ...current.current, ...edge };
    current.current = next;
    setLayout(next);
    return next;
  }, []);

  const changeLayout = useCallback(
    (edge: Partial<Layout>) => {
      apply(edge);
    },
    [apply],
  );

  const storeLayout = useCallback(
    (edge: Partial<Layout>) => {
      const next = apply(edge);
      if (next === null) {
        return;
      }
      void invoke("layout_write", { layout: next }).catch(() => {
        // Same as above: the backend logs what it could not store and opens at the default next
        // time.
      });
    },
    [apply],
  );

  return { layout, changeLayout, storeLayout };
}
