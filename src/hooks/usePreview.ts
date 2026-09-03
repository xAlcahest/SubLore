import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Preview = {
  /** Whether View has the document on the video. On from the start (decision 7). */
  shown: boolean;
  /** Set while the backend could not put the document on the frame. */
  failed: boolean;
  toggle: () => void;
};

/**
 * The open document on the video frame (decision 7).
 *
 * Nothing here holds the document or the shadow copy the backend writes for mpv: this is the View
 * toggle and the one line the status bar shows when the picture could not be drawn.
 */
export function usePreview(): Preview {
  const [shown, setShown] = useState(true);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const listeners = Promise.all([
      listen("preview://failed", () => setFailed(true)),
      listen("preview://drawn", () => setFailed(false)),
    ]);

    return () => {
      void listeners.then((unlisteners) => {
        for (const unlisten of unlisteners) {
          unlisten();
        }
      });
    };
  }, []);

  // Sent on mount too, so the backend's default and this one cannot drift apart: with nothing open
  // it costs a line in the log and no work.
  useEffect(() => {
    invoke("preview_set_shown", { shown }).catch(() => setFailed(true));
  }, [shown]);

  const toggle = useCallback(() => setShown((was) => !was), []);

  return { shown, failed, toggle };
}
