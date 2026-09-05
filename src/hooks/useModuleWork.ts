import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** One activation beginning, so the band is on screen before the module answers. */
type ModuleBegan = { callId: number; module: number; item: number };
type ModuleStatus = { callId: number; message: string };
type ModuleProgress = { callId: number; done: number; total: number };
type ModuleEnded = { callId: number; code: number };

/** How far a module says it has got. Both numbers are the module's own. */
export type ModuleProgressReading = { done: number; total: number };

/**
 * What a module's own work looks like while it runs, and the one control that can stop it.
 *
 * The four events are the only way any of this could arrive. `module_invoke` runs the module on a
 * blocking thread and its promise does not resolve until the module returns, so a status or a
 * progress carried back on the result would describe work that had already finished. See
 * docs/module-host-tasks.md H8.
 */
export type ModuleWork = {
  /** An activation is in flight, so the band is drawn and the Stop with it. */
  running: boolean;
  /** The last line the module put on screen, or null when it has not put one there. */
  message: string | null;
  progress: ModuleProgressReading | null;
  /** Ask the running activation to stop. A request: nothing forces a module to obey it. */
  stop: () => void;
  /**
   * Take the band down.
   *
   * Called when an activation's own promise settles, which is the second of two independent
   * reasons the band ends. Neither waits on the other: the event can arrive first, and on a path
   * where no event arrives at all the promise still settles.
   */
  clear: () => void;
};

export function useModuleWork(): ModuleWork {
  const [callId, setCallId] = useState<number | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [progress, setProgress] = useState<ModuleProgressReading | null>(null);
  /** Read by the listeners, which are installed once and outlive every render. */
  const running = useRef<number | null>(null);

  useEffect(() => {
    /**
     * Whether an event belongs to the activation on screen.
     *
     * One runs at a time, because the backend holds the module lock across the whole call, so an
     * event that arrives before this window has learned the id is still this one's.
     */
    const mine = (id: number) => running.current === null || running.current === id;

    const listeners = Promise.all([
      listen<ModuleBegan>("module://began", (event) => {
        running.current = event.payload.callId;
        setCallId(event.payload.callId);
        // A new activation says nothing yet, and shows no progress until it reports one.
        setMessage(null);
        setProgress(null);
      }),
      listen<ModuleStatus>("module://status", (event) => {
        if (mine(event.payload.callId)) {
          setMessage(event.payload.message);
        }
      }),
      listen<ModuleProgress>("module://progress", (event) => {
        if (mine(event.payload.callId)) {
          setProgress({ done: event.payload.done, total: event.payload.total });
        }
      }),
      listen<ModuleEnded>("module://ended", (event) => {
        if (!mine(event.payload.callId)) {
          return;
        }
        running.current = null;
        setCallId(null);
        setMessage(null);
        setProgress(null);
      }),
    ]);

    return () => {
      void listeners.then((unlisteners) => {
        for (const unlisten of unlisteners) {
          unlisten();
        }
      });
    };
  }, []);

  const stop = useCallback(() => {
    if (callId === null) {
      return;
    }
    // The button stays drawn after this. Whether the work stops is the module's answer to
    // `should_cancel`, and pretending otherwise would be the core claiming a power it has not got.
    void invoke("module_cancel", { callId }).catch((failure: unknown) => {
      console.error("a module's work could not be asked to stop", failure);
    });
  }, [callId]);

  const clear = useCallback(() => {
    running.current = null;
    setCallId(null);
    setMessage(null);
    setProgress(null);
  }, []);

  return { running: callId !== null, message, progress, stop, clear };
}
