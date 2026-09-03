import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import {
  type AudioDone,
  type AudioFailed,
  type AudioJobStarted,
  type AudioPeaks,
} from "../types/audio";

/** What the canvas draws: the buckets received so far, and how far the job says it will go. */
export type Waveform = {
  /** The smallest and largest sample per millisecond, filled from the start. */
  min: Int16Array;
  max: Int16Array;
  /** How many buckets of the two arrays above hold peaks. The rest is not audio, it is capacity. */
  filled: number;
  /** Milliseconds the job peaked in total, once it has said so. Null while it is still running. */
  total: number | null;
  /** A sentence the user can act on, or null. A cancel is never one: every media change cancels. */
  error: AudioFailed | null;
  /**
   * The open media carries no audio at all (decision 24 E3). Distinct from `filled === 0`, which is
   * also true of a job that has simply not produced anything yet.
   */
  silent: boolean;
};

const EMPTY: Waveform = {
  min: new Int16Array(0),
  max: new Int16Array(0),
  filled: 0,
  total: null,
  error: null,
  silent: false,
};

/** Grow to hold `wanted` buckets, doubling, so a long episode costs a handful of copies. */
function grown(buffer: Int16Array, wanted: number): Int16Array {
  if (wanted <= buffer.length) {
    return buffer;
  }
  let size = Math.max(buffer.length, 1 << 16);
  while (size < wanted) {
    size *= 2;
  }
  const bigger = new Int16Array(size);
  bigger.set(buffer);
  return bigger;
}

/**
 * The peaks of the job the backend is running for the open media (M2.4 W5).
 *
 * Nothing here starts a job: `video_open` does that for whichever track is playing, and this hook
 * listens. Every message carries its job id and anything from another job is dropped, because a
 * chunk of the media just closed can still be in flight when its replacement has started.
 */
export function useAudioPeaks(): Waveform {
  const [waveform, setWaveform] = useState<Waveform>(EMPTY);
  // The job whose messages count. Held in a ref because a chunk arriving in the same tick as the
  // start must already see it.
  const current = useRef<number | null>(null);
  const min = useRef(EMPTY.min);
  const max = useRef(EMPTY.max);
  const filled = useRef(0);

  useEffect(() => {
    const listeners = Promise.all([
      // A media with no audio, which arrives instead of a job rather than after one.
      listen("audio://none", () => {
        current.current = null;
        min.current = EMPTY.min;
        max.current = EMPTY.max;
        filled.current = 0;
        setWaveform({ ...EMPTY, silent: true });
      }),
      listen<AudioJobStarted>("audio://started", (event) => {
        current.current = event.payload.jobId;
        min.current = EMPTY.min;
        max.current = EMPTY.max;
        filled.current = 0;
        setWaveform(EMPTY);
      }),
      listen<AudioPeaks>("audio://peaks", (event) => {
        const chunk = event.payload;
        if (chunk.jobId !== current.current) {
          return;
        }
        const end = chunk.firstMs + chunk.min.length;
        min.current = grown(min.current, end);
        max.current = grown(max.current, end);
        min.current.set(chunk.min, chunk.firstMs);
        max.current.set(chunk.max, chunk.firstMs);
        filled.current = Math.max(filled.current, end);
        setWaveform((was) => ({
          ...was,
          min: min.current,
          max: max.current,
          filled: filled.current,
        }));
      }),
      listen<AudioDone>("audio://done", (event) => {
        if (event.payload.jobId !== current.current) {
          return;
        }
        setWaveform((was) => ({ ...was, total: event.payload.buckets }));
      }),
      listen<AudioFailed>("audio://error", (event) => {
        if (event.payload.jobId !== current.current) {
          return;
        }
        // A cancel is what closing or replacing the media does, so it empties the panel and says
        // nothing. Anything else is a sentence the translator can act on.
        current.current = null;
        setWaveform(
          event.payload.code === "cancelled" ? EMPTY : { ...EMPTY, error: event.payload },
        );
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

  return waveform;
}
