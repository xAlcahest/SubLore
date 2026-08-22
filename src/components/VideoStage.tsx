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
      const rect = element.getBoundingClientRect();
      onRegionChange({
        x: rect.left,
        y: rect.top,
        width: rect.width,
        height: rect.height,
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
