import { useEffect, useRef } from "react";

import { en } from "../i18n/en";
import { type VideoRegion } from "../types/video";

const HIDDEN: VideoRegion = { x: 0, y: 0, width: 0, height: 0 };

/** Between the integer scale factors a window system hands out, so a move from one to the next
 * crosses exactly one threshold. */
const RATIO_THRESHOLDS = [1.5, 2.5, 3.5];

type VideoStageProps = {
  hasVideo: boolean;
  onRegionChange: (region: VideoRegion) => void;
};

/** Measures where the native video surface belongs. The surface covers this element exactly. */
export default function VideoStage({ hasVideo, onRegionChange }: VideoStageProps) {
  const surfaceRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = surfaceRef.current;
    if (!element) {
      return;
    }

    let frame = 0;
    const report = () => {
      frame = 0;
      // Resolved here, in native pixels, because this is the only place the ratio is known: the
      // backend's `scale_factor()` is an integer and reports 1 on a fractionally scaled display
      // (docs/reports/n2c-p3-scala.md). Sending the ratio alongside would put the same fact in two
      // places that have to agree.
      const rect = element.getBoundingClientRect();
      const ratio = window.devicePixelRatio;
      // Edges first, then the size from them, so a rectangle never gains or loses a pixel to
      // rounding each side independently.
      const x = Math.round(rect.left * ratio);
      const y = Math.round(rect.top * ratio);
      onRegionChange({
        x,
        y,
        width: Math.round(rect.right * ratio) - x,
        height: Math.round(rect.bottom * ratio) - y,
      });
    };
    // Coalesce the observer, the window resize and the mount into one update per frame.
    const schedule = () => {
      if (frame === 0) {
        frame = window.requestAnimationFrame(report);
      }
    };

    // A scale-factor change moves the ratio and leaves the CSS box exactly where it was, so the
    // two listeners below never fire. Thresholds rather than the ratio itself: `resolution` is the
    // device's own factor, which on a fractionally scaled display is not `devicePixelRatio`.
    // One at a time, so a webview missing either call costs these listeners and not the page.
    const ratioQueries: MediaQueryList[] = [];
    for (const threshold of RATIO_THRESHOLDS) {
      try {
        const query = window.matchMedia(`(min-resolution: ${threshold}dppx)`);
        query.addEventListener("change", schedule);
        ratioQueries.push(query);
      } catch (error) {
        // Degradation, not failure: the surface then keeps its size until the next resize.
        console.warn(`video stage: no listener for ${threshold}dppx`, error);
      }
    }

    const observer = new ResizeObserver(schedule);
    observer.observe(element);
    window.addEventListener("resize", schedule);
    schedule();

    return () => {
      if (frame !== 0) {
        window.cancelAnimationFrame(frame);
      }
      observer.disconnect();
      window.removeEventListener("resize", schedule);
      for (const query of ratioQueries) {
        query.removeEventListener("change", schedule);
      }
      onRegionChange(HIDDEN);
    };
  }, [onRegionChange]);

  return (
    <div className="stage">
      <div className="stage__surface" ref={surfaceRef} />
      {!hasVideo && <p className="stage__empty">{en.video.noFile}</p>}
    </div>
  );
}
