import { useEffect, useRef, useState } from "react";

/**
 * The playback position between the events that report it.
 *
 * `video://position` arrives ten times a second, and a playhead drawn straight from it moves in ten
 * steps a second, which reads as a stutter rather than as playback. This carries it forward on the
 * clock between events and resets to the reported figure whenever one arrives, so the drawing is
 * never more than one event's worth of guessing away from what the player says (M2.4 W7).
 *
 * Paused, it is the reported figure and nothing else: a seek is on screen the moment it is asked
 * for, and a still playhead must not drift.
 */
export function useSmoothPosition(positionMs: number, paused: boolean, durationMs: number): number {
  const [smooth, setSmooth] = useState(positionMs);
  const from = useRef({ positionMs, at: 0 });

  useEffect(() => {
    from.current = { positionMs, at: performance.now() };
    setSmooth(positionMs);
  }, [positionMs, paused]);

  useEffect(() => {
    if (paused || durationMs <= 0) {
      return;
    }
    let frame = 0;
    const step = () => {
      const ahead = from.current.positionMs + (performance.now() - from.current.at);
      setSmooth(Math.min(ahead, durationMs));
      frame = window.requestAnimationFrame(step);
    };
    frame = window.requestAnimationFrame(step);
    return () => window.cancelAnimationFrame(frame);
  }, [paused, durationMs]);

  return smooth;
}
