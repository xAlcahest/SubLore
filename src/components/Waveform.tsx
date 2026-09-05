import { useCallback, useEffect, useRef, useState } from "react";

import { en } from "../i18n/en";
import { type Waveform as Peaks } from "../hooks/useAudioPeaks";
import { useSmoothPosition } from "../hooks/useSmoothPosition";
import { useWaveformView } from "../hooks/useWaveformView";
import { type CueRow } from "../types/subtitle";

/** The largest magnitude a 16-bit sample has: -32768 read as a distance from silence. */
const FULL_SCALE = 32768;

/** How much of the window one arrow press travels: an eighth, so eight presses cross it. */
const SCROLL_FRACTION = 8;

/** How wide a boundary is drawn, in device pixels (interface-spec 5.6). */
const MARKER_PX = 2;

/**
 * How far from a drawn boundary a press still grabs it, in device pixels, converted to milliseconds
 * at whatever zoom the view is at. The reference's own drag-sensitivity default (interface-spec
 * 5.6), not a number chosen here. See M2.5.
 */
const GRAB_PX = 8;

/** Which boundary a gesture holds. The two travel in one command, so a drag names one of them. */
type Boundary = "start" | "end";

type WaveformProps = {
  peaks: Peaks;
  /** Where playback is, in milliseconds, for the playhead. */
  positionMs: number;
  /** The media's length in milliseconds, so the drawing keeps its scale while peaks arrive. */
  durationMs: number;
  /** The height the sash was left at, in CSS pixels, or nothing before the layout has been read. */
  height?: number;
  /** Playback stopped: the playhead neither moves on its own nor drags the view along.  */
  paused: boolean;
  /** The row the cursor is on, held here so a release can name it after the cursor has moved. */
  cueIndex: number | null;
  /** The cursor's cue, whose two boundaries are drawn and can be dragged, or null for none. */
  cue: CueRow | null;
  /** The end of a drag: the pair the cue takes. Not called for a drag that is refused (M2.5 G4). */
  onDragTimes: (cue: number, startMs: number, endMs: number) => void;
};

/** The colours the canvas draws in, read from the tokens rather than written twice. */
function ink(element: HTMLElement, name: string): string {
  return getComputedStyle(element).getPropertyValue(name).trim();
}

/** Where a pointer is on the panel, in the device pixels the view is scaled in. */
function atPixel(event: { clientX: number }, element: HTMLCanvasElement): number {
  return (event.clientX - element.getBoundingClientRect().x) * window.devicePixelRatio;
}

/**
 * The waveform, in the tools column above the current line (M2.4 W5).
 *
 * It draws from the peaks as they arrive rather than when the job ends, so a long episode shows its
 * first seconds while the rest is still being read. There is no toolbar: drawing one empty is the
 * placeholder the layout document refuses.
 *
 * The cursor's cue puts two markers on it and either can be dragged (M2.5 G1 to G4). What the
 * document is told is the release, never the travel: the editor sends finished edits, so a drag
 * that reported every pointer move would be one gesture and a hundred undo steps.
 */
export default function Waveform({
  peaks,
  positionMs,
  durationMs,
  height,
  paused,
  cueIndex,
  cue,
  onDragTimes,
}: WaveformProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [widthPx, setWidthPx] = useState(0);
  /** Which boundary the pointer holds, and where it has taken it. Null while nothing is held. */
  const [held, setHeld] = useState<Boundary | null>(null);
  const [heldMs, setHeldMs] = useState(0);
  // The whole media's length, not what has arrived: peaks filling in must not rescale what is
  // already drawn under the playhead.
  const spanMs = Math.max(1, durationMs > 0 ? durationMs : peaks.filled);
  const { view, zoomBy, scrollBy, followTo, startFollowing } = useWaveformView(spanMs, widthPx);
  const headMs = useSmoothPosition(positionMs, paused, durationMs);

  const pixelOf = useCallback((ms: number) => (ms - view.fromMs) / view.msPerPixel, [view]);
  // Never below zero: a boundary dragged off the front of the media is not a time (M2.5 G4).
  const msAt = useCallback(
    (px: number) => Math.max(0, Math.round(view.fromMs + px * view.msPerPixel)),
    [view],
  );

  /** The boundary a press at this pixel grabs: the nearer of the two, inside the hit area. */
  const grabbed = useCallback(
    (px: number): Boundary | null => {
      if (cue === null) {
        return null;
      }
      const toStart = Math.abs(pixelOf(cue.startMs) - px);
      const toEnd = Math.abs(pixelOf(cue.endMs) - px);
      if (Math.min(toStart, toEnd) > GRAB_PX) {
        return null;
      }
      return toStart <= toEnd ? "start" : "end";
    },
    [cue, pixelOf],
  );

  // What the panel draws: the document's own times, except for the one a hand is holding.
  const shownStartMs = held === "start" ? heldMs : (cue?.startMs ?? 0);
  const shownEndMs = held === "end" ? heldMs : (cue?.endMs ?? 0);

  // Playback starting takes the view back from wherever a hand left it, and every frame after that
  // keeps the head on screen.
  useEffect(() => {
    if (!paused) {
      startFollowing();
    }
  }, [paused, startFollowing]);

  useEffect(() => {
    if (!paused) {
      followTo(headMs);
    }
  }, [paused, headMs, followTo]);

  // In device pixels, which is what the view is scaled in and what the backing store is sized in.
  useEffect(() => {
    const element = canvas.current;
    if (element === null) {
      return;
    }
    const measure = () =>
      setWidthPx(Math.max(1, Math.round(element.clientWidth * window.devicePixelRatio)));
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const element = canvas.current;
    if (element === null) {
      return;
    }
    const context = element.getContext("2d");
    if (context === null) {
      return;
    }

    // The backing store is in device pixels: a canvas sized in CSS pixels on a scaled display is
    // drawn once and then stretched, and the check reads this buffer.
    const ratio = window.devicePixelRatio;
    const width = Math.max(1, Math.round(element.clientWidth * ratio));
    const height = Math.max(1, Math.round(element.clientHeight * ratio));
    if (element.width !== width || element.height !== height) {
      element.width = width;
      element.height = height;
    }

    const background = ink(element, "--bg-media");
    context.fillStyle = background === "" ? "#0c0c0e" : background;
    context.fillRect(0, 0, width, height);

    const middle = height / 2;
    const wave = ink(element, "--ink-dim");
    context.fillStyle = wave === "" ? "#9a9aa6" : wave;

    for (let x = 0; x < width; x += 1) {
      const from = Math.floor(view.fromMs + x * view.msPerPixel);
      const to = Math.max(from + 1, Math.floor(view.fromMs + (x + 1) * view.msPerPixel));
      // The window only ever moves forward across the buckets, so the first pixel past what has
      // arrived is the last one worth drawing.
      if (from >= peaks.filled) {
        break;
      }
      let low = 0;
      let high = 0;
      for (let bucket = from; bucket < to && bucket < peaks.filled; bucket += 1) {
        low = Math.min(low, peaks.min[bucket]);
        high = Math.max(high, peaks.max[bucket]);
      }
      // Always at least one device pixel: silence is a line through the middle, never a gap that
      // reads the same as audio that has not arrived.
      const top = middle - (high / FULL_SCALE) * middle;
      const bottom = middle - (low / FULL_SCALE) * middle;
      context.fillRect(x, top, 1, Math.max(1, bottom - top));
    }

    // Under the playhead, which is the paint order interface-spec 5.4 keeps: a head sitting exactly
    // on a boundary has to stay readable.
    if (cue !== null) {
      const markers: [number, string, string][] = [
        [shownStartMs, "--marker-start", "#6fd08c"],
        [shownEndMs, "--marker-end", "#e8935f"],
      ];
      for (const [ms, token, fallback] of markers) {
        const at = Math.round(pixelOf(ms));
        if (at >= 0 && at < width) {
          const paint = ink(element, token);
          context.fillStyle = paint === "" ? fallback : paint;
          context.fillRect(at, 0, MARKER_PX, height);
        }
      }
    }

    if (durationMs > 0) {
      const at = Math.round(pixelOf(Math.min(headMs, durationMs)));
      // Off the window at either end draws nothing rather than a line stuck to an edge, which
      // would read as a playhead that is where it is not.
      if (at >= 0 && at < width) {
        const head = ink(element, "--accent");
        context.fillStyle = head === "" ? "#4fb3d9" : head;
        context.fillRect(at, 0, 1, height);
      }
    }
  }, [peaks, headMs, durationMs, view, pixelOf, cue, shownStartMs, shownEndMs]);

  // On the window rather than through pointer capture, for the reason Sash.tsx records: under
  // WebKitGTK a captured element stops hearing the pointer the moment it leaves.
  useEffect(() => {
    const element = canvas.current;
    if (held === null || element === null) {
      return;
    }
    // The row went out from under the gesture. Let it go here rather than leave a drag that rearms
    // itself on the next render and commits on a button nobody is holding.
    if (cue === null || cueIndex === null) {
      setHeld(null);
      return;
    }
    const move = (event: PointerEvent) => setHeldMs(msAt(atPixel(event, element)));
    const finish = (event: PointerEvent) => {
      setHeld(null);
      const ms = msAt(atPixel(event, element));
      const startMs = held === "start" ? ms : cue.startMs;
      const endMs = held === "end" ? ms : cue.endMs;
      // Refused rather than clamped: nothing below refuses it either, and a cue whose length
      // nobody chose is worse than a marker that goes back where it was (M2.5 G4).
      if (endMs > startMs) {
        onDragTimes(cueIndex, startMs, endMs);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
    };
  }, [held, cue, cueIndex, msAt, onDragTimes]);

  // A native listener, not React's `onWheel`: React attaches wheel passively at the root, so the
  // handler cannot stop the page from taking the gesture as its own.
  useEffect(() => {
    const element = canvas.current;
    if (element === null) {
      return;
    }
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const along = event.deltaX !== 0 ? event.deltaX : event.deltaY;
      if (event.ctrlKey) {
        const box = element.getBoundingClientRect();
        const atPx = (event.clientX - box.x) * window.devicePixelRatio;
        zoomBy(along > 0 ? -1 : 1, view.fromMs + atPx * view.msPerPixel);
        return;
      }
      scrollBy(along);
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [zoomBy, scrollBy, view]);

  return (
    <section
      className="waveform"
      aria-label={en.waveform.label}
      style={height === undefined ? undefined : { height }}
    >
      <canvas
        ref={canvas}
        className="waveform__canvas"
        tabIndex={0}
        aria-label={en.waveform.canvas}
        onPointerDown={(event) => {
          if (event.button !== 0 || cue === null) {
            return;
          }
          const which = grabbed(atPixel(event, event.currentTarget));
          if (which === null) {
            return;
          }
          event.preventDefault();
          setHeld(which);
          setHeldMs(which === "start" ? cue.startMs : cue.endMs);
        }}
        onPointerMove={(event) => {
          // Straight onto the element: the shape that says a press would grab must not cost the
          // panel a redraw (interface-spec 5.7).
          if (held === null) {
            event.currentTarget.style.cursor =
              grabbed(atPixel(event, event.currentTarget)) === null ? "" : "ew-resize";
          }
        }}
        onKeyDown={(event) => {
          const zoom = event.key === "+" || event.key === "=" ? 1 : event.key === "-" ? -1 : 0;
          if (zoom !== 0) {
            event.preventDefault();
            zoomBy(zoom);
            return;
          }
          const along = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
          if (along !== 0) {
            event.preventDefault();
            scrollBy(along * Math.max(1, Math.round(widthPx / SCROLL_FRACTION)));
          }
        }}
      />
    </section>
  );
}
