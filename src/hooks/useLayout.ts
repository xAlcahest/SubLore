import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";

/** Kept in step with `src-tauri/src/layout.rs`; the file it comes from carries the same names. */
export type Layout = {
  waveformHeight: number;
};

/**
 * Where the panels were left, and how to say where they are now.
 *
 * The read happens once, when the shell mounts, and the write only when a drag ends: a write per
 * pointer move would put the disk inside the frame budget M2.3 measured. Losing the file costs one
 * drag, so nothing here fails loudly — the backend answers with the default and says so in its log.
 */
export function useLayout(): {
  layout: Layout | null;
  setWaveformHeight: (height: number) => void;
  storeWaveformHeight: (height: number) => void;
} {
  const [layout, setLayout] = useState<Layout | null>(null);

  useEffect(() => {
    let alive = true;
    void invoke<Layout>("layout_read")
      .then((stored) => {
        if (alive) {
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

  const setWaveformHeight = useCallback((waveformHeight: number) => {
    setLayout({ waveformHeight });
  }, []);

  const storeWaveformHeight = useCallback((waveformHeight: number) => {
    setLayout({ waveformHeight });
    void invoke("layout_write", { waveformHeight }).catch(() => {
      // Same as above: the backend logs what it could not store and opens at the default next time.
    });
  }, []);

  return { layout, setWaveformHeight, storeWaveformHeight };
}
