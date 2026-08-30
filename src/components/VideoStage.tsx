import { useEffect, useRef } from "react";

import { en } from "../i18n/en";
import { type VideoRegion } from "../types/video";

const HIDDEN: VideoRegion = { x: 0, y: 0, width: 0, height: 0 };

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
