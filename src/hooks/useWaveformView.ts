import { useCallback, useEffect, useState } from "react";

/** What part of the media the panel is drawing, and at what scale. */
export type WaveformView = {
  /** The time at the left edge, in milliseconds. */
  fromMs: number;
  /** Milliseconds of media per device pixel. Smaller is deeper. */
  msPerPixel: number;
};

/**
 * One bucket of peaks is one millisecond, so a pixel that covers less than that has nothing more to
 * show: this is the deepest zoom there is rather than a number chosen for comfort (M2.4 W7).
 */
export const DEEPEST_MS_PER_PIXEL = 1;

/** One press or one wheel notch. Six of them cross a minute-long file from whole to deepest. */
const ZOOM_FACTOR = 2;

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), Math.max(low, high));
}

/** The scale at which the whole media fits the panel, which is also the shallowest zoom allowed. */
function fitting(spanMs: number, widthPx: number): number {
  return widthPx > 0 ? Math.max(DEEPEST_MS_PER_PIXEL, spanMs / widthPx) : DEEPEST_MS_PER_PIXEL;
}

/**
 * The window on the media the waveform draws, with its bounds.
 *
 * It opens on the whole file and returns there whenever the media or the panel changes size, so a
 * new episode is never inherited at somebody else's zoom. Neither edge can be scrolled past: the
 * left stops at zero and the right at the end of the media, which is what keeps the drawing from
 * running off into a region that has no peaks and no meaning.
 */
export function useWaveformView(
  spanMs: number,
  widthPx: number,
): {
  view: WaveformView;
  zoomBy: (steps: number, atMs?: number) => void;
  scrollBy: (pixels: number) => void;
} {
  const [view, setView] = useState<WaveformView>({ fromMs: 0, msPerPixel: DEEPEST_MS_PER_PIXEL });

  useEffect(() => {
    setView({ fromMs: 0, msPerPixel: fitting(spanMs, widthPx) });
  }, [spanMs, widthPx]);

  const zoomBy = useCallback(
    (steps: number, atMs?: number) => {
      setView((current) => {
        const shallowest = fitting(spanMs, widthPx);
        const msPerPixel = clamp(
          current.msPerPixel / ZOOM_FACTOR ** steps,
          DEEPEST_MS_PER_PIXEL,
          shallowest,
        );
        // The time under the pointer stays under the pointer; with no pointer, the middle holds.
        const anchor = atMs ?? current.fromMs + (widthPx / 2) * current.msPerPixel;
        const offset = (anchor - current.fromMs) / current.msPerPixel;
        const fromMs = clamp(anchor - offset * msPerPixel, 0, spanMs - widthPx * msPerPixel);
        return { fromMs, msPerPixel };
      });
    },
    [spanMs, widthPx],
  );

  const scrollBy = useCallback(
    (pixels: number) => {
      setView((current) => ({
        ...current,
        fromMs: clamp(
          current.fromMs + pixels * current.msPerPixel,
          0,
          spanMs - widthPx * current.msPerPixel,
        ),
      }));
    },
    [spanMs, widthPx],
  );

  return { view, zoomBy, scrollBy };
}
