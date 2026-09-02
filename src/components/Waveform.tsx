import { useEffect, useRef } from "react";

import { en } from "../i18n/en";
import { type Waveform as Peaks } from "../hooks/useAudioPeaks";

/** The largest magnitude a 16-bit sample has: -32768 read as a distance from silence. */
const FULL_SCALE = 32768;

type WaveformProps = {
  peaks: Peaks;
  /** Where playback is, in milliseconds, for the playhead. */
  positionMs: number;
  /** The media's length in milliseconds, so the drawing keeps its scale while peaks arrive. */
  durationMs: number;
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
export default function Waveform({ peaks, positionMs, durationMs }: WaveformProps) {
  const canvas = useRef<HTMLCanvasElement>(null);

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

    // The whole media's length, not what has arrived: peaks filling in must not rescale what is
    // already drawn under the playhead.
    const span = Math.max(1, durationMs > 0 ? durationMs : peaks.filled);
    const middle = height / 2;
    const wave = ink(element, "--ink-dim");
    context.fillStyle = wave === "" ? "#9a9aa6" : wave;

    for (let x = 0; x < width; x += 1) {
      const from = Math.floor((x / width) * span);
      const to = Math.max(from + 1, Math.floor(((x + 1) / width) * span));
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
      const at = Math.round((Math.min(positionMs, durationMs) / durationMs) * width);
      const head = ink(element, "--accent");
      context.fillStyle = head === "" ? "#4fb3d9" : head;
      context.fillRect(Math.min(at, width - 1), 0, 1, height);
    }
  }, [peaks, positionMs, durationMs]);

  return (
    <section className="waveform" aria-label={en.waveform.label}>
      <canvas ref={canvas} className="waveform__canvas" />
    </section>
  );
}
