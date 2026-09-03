import { useEffect, useRef, useState } from "react";

import { en } from "../i18n/en";
import { type Waveform as Peaks } from "../hooks/useAudioPeaks";
import { useWaveformView } from "../hooks/useWaveformView";

/** The largest magnitude a 16-bit sample has: -32768 read as a distance from silence. */
const FULL_SCALE = 32768;

/** How much of the window one arrow press travels: an eighth, so eight presses cross it. */
const SCROLL_FRACTION = 8;

type WaveformProps = {
  peaks: Peaks;
  /** Where playback is, in milliseconds, for the playhead. */
  positionMs: number;
  /** The media's length in milliseconds, so the drawing keeps its scale while peaks arrive. */
  durationMs: number;
  /** The height the sash was left at, in CSS pixels, or nothing before the layout has been read. */
  height?: number;
};

/** The colours the canvas draws in, read from the tokens rather than written twice. */
function ink(element: HTMLElement, name: string): string {
  return getComputedStyle(element).getPropertyValue(name).trim();
}

/**
 * The waveform, in the tools column above the current line (M2.4 W5).
 *
 * It draws from the peaks as they arrive rather than when the job ends, so a long episode shows its
 * first seconds while the rest is still being read. There is no zoom and no toolbar: those are
 * M2.5, and drawing them empty is the placeholder the layout document refuses.
 */
export default function Waveform({ peaks, positionMs, durationMs, height }: WaveformProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [widthPx, setWidthPx] = useState(0);
  // The whole media's length, not what has arrived: peaks filling in must not rescale what is
  // already drawn under the playhead.
  const spanMs = Math.max(1, durationMs > 0 ? durationMs : peaks.filled);
  const { view, zoomBy, scrollBy } = useWaveformView(spanMs, widthPx);

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

    if (durationMs > 0) {
      const at = Math.round((Math.min(positionMs, durationMs) - view.fromMs) / view.msPerPixel);
      // Off the window at either end draws nothing rather than a line stuck to an edge, which
      // would read as a playhead that is where it is not.
      if (at >= 0 && at < width) {
        const head = ink(element, "--accent");
        context.fillStyle = head === "" ? "#4fb3d9" : head;
        context.fillRect(at, 0, 1, height);
      }
    }
  }, [peaks, positionMs, durationMs, view]);

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
