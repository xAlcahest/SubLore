import { useCallback, useEffect, useRef, useState } from "react";

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

/** Where the playhead is put when the view has to move on, and the band it may wander in. */
const EDGE_FRACTION = 0.1;

/** How much of the panel each edge is kept clear when a range is brought into view: the
 * reference's own margin (`src/audio_display.cpp:650-652`). */
const MARGIN_FRACTION = 0.05;

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
  showRange: (fromRangeMs: number, toRangeMs: number) => void;
  followTo: (atMs: number) => void;
  startFollowing: () => void;
} {
  const [view, setView] = useState<WaveformView>({ fromMs: 0, msPerPixel: DEEPEST_MS_PER_PIXEL });
  // Not state: it changes on every drag and every frame of playback, and nothing draws it.
  const following = useRef(true);

  useEffect(() => {
    setView({ fromMs: 0, msPerPixel: fitting(spanMs, widthPx) });
  }, [spanMs, widthPx]);

  const zoomBy = useCallback(
    (steps: number, atMs?: number) => {
      following.current = false;
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
      following.current = false;
      setView((current) => {
        const fromMs = clamp(
          current.fromMs + pixels * current.msPerPixel,
          0,
          spanMs - widthPx * current.msPerPixel,
        );
        // The same window is the same object: a scroll that the clamp swallowed must not repaint
        // the panel, and the edge shot in Waveform.tsx is armed by the window changing.
        return fromMs === current.fromMs ? current : { ...current, fromMs };
      });
    },
    [spanMs, widthPx],
  );

  /**
   * Bring a stretch of the media into view, which is what the current line's range asks for when
   * the cursor moves onto it and what the centre-on-line command runs by hand.
   *
   * Four cases, and they are the reference's (`src/audio_display.cpp:643-681`): a range already on
   * screen inside the margins moves nothing; one that fits is centred; one longer than the window
   * that is already straddling it is left alone, so a hand reading the middle of a long line is not
   * yanked; otherwise the edge that is nearest to being in view is brought just inside the margin.
   *
   * Following is not cancelled here. This is not a hand on the panel: during playback the follow
   * owns the window and puts it back on the next frame, which is what the reference does too.
   */
  const showRange = useCallback(
    (fromRangeMs: number, toRangeMs: number) => {
      if (!(widthPx > 0)) {
        return;
      }
      setView((current) => {
        const marginPx = widthPx * MARGIN_FRACTION;
        const innerPx = widthPx - 2 * marginPx;
        const beginPx = (fromRangeMs - current.fromMs) / current.msPerPixel;
        const endPx = (toRangeMs - current.fromMs) / current.msPerPixel;
        const rangePx = endPx - beginPx;
        if (beginPx >= marginPx && endPx <= marginPx + innerPx) {
          return current;
        }
        let leftPx: number;
        if (rangePx < innerPx) {
          leftPx = beginPx - (innerPx - rangePx) / 2 - marginPx;
        } else if (beginPx < marginPx && endPx > marginPx + innerPx) {
          return current;
        } else if (endPx >= marginPx && endPx < marginPx + innerPx) {
          leftPx = endPx - innerPx - marginPx;
        } else {
          leftPx = beginPx - marginPx;
        }
        const fromMs = clamp(
          current.fromMs + leftPx * current.msPerPixel,
          0,
          spanMs - widthPx * current.msPerPixel,
        );
        return fromMs === current.fromMs ? current : { ...current, fromMs };
      });
    },
    [spanMs, widthPx],
  );

  /**
   * Keep the playhead on screen while playback runs, by the page rather than by the pixel.
   *
   * A window that slides under a still playhead is a moving background, and it costs a redraw of
   * the whole panel every frame. This leaves the drawing alone until the head reaches the last
   * tenth and then moves it on by most of a page, which is the gesture a reader already knows.
   */
  const followTo = useCallback(
    (atMs: number) => {
      if (!following.current) {
        return;
      }
      setView((current) => {
        const window = widthPx * current.msPerPixel;
        const ahead = current.fromMs + window * (1 - EDGE_FRACTION);
        const behind = current.fromMs + window * EDGE_FRACTION;
        if (atMs <= ahead && atMs >= behind) {
          return current;
        }
        const fromMs = clamp(atMs - window * EDGE_FRACTION, 0, spanMs - window);
        return fromMs === current.fromMs ? current : { ...current, fromMs };
      });
    },
    [spanMs, widthPx],
  );

  /** Playback starting takes the view back, whatever the hand did while it was stopped. */
  const startFollowing = useCallback(() => {
    following.current = true;
  }, []);

  return { view, zoomBy, scrollBy, showRange, followTo, startFollowing };
}
