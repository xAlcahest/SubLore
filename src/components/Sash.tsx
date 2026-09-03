import { useCallback, useEffect, useRef, useState } from "react";

type SashProps = {
  /** The height the panel above has now, in CSS pixels. */
  height: number;
  min: number;
  max: number;
  label: string;
  /** Every step of a drag. Cheap: the caller keeps this in React state and stores nothing. */
  onResize: (height: number) => void;
  /** The end of a drag or one keyboard step, which is when the height is worth storing. */
  onRelease: (height: number) => void;
};

/** One keyboard press. Small enough to place the edge, large enough to cross the range. */
const STEP = 8;

function clamp(height: number, min: number, max: number): number {
  return Math.min(Math.max(height, min), Math.max(min, max));
}

/**
 * The waveform's bottom edge, and the one draggable thing in v1 (decision 24 A5).
 *
 * The move and release are listened for on the window rather than through pointer capture on this
 * four-pixel strip. Capture is the tidier API and it does not hold here: under WebKitGTK the strip
 * stopped hearing anything once the pointer left it, so a drag to the top of the window ended
 * wherever the pointer crossed the edge and the release that stores the height never arrived.
 *
 * The keyboard route is not decoration: a separator only a mouse can place is one a keyboard user
 * cannot use at all.
 */
export default function Sash({ height, min, max, label, onResize, onRelease }: SashProps) {
  const [dragging, setDragging] = useState(false);
  // Read by the window listeners, which are installed once per drag and must not see a stale
  // starting point or a stale range.
  const from = useRef({ y: 0, height: 0 });
  const bounds = useRef({ min, max });
  bounds.current = { min, max };

  const heightAt = useCallback(
    (clientY: number) =>
      clamp(
        from.current.height + (clientY - from.current.y),
        bounds.current.min,
        bounds.current.max,
      ),
    [],
  );

  useEffect(() => {
    if (!dragging) {
      return;
    }
    const move = (event: PointerEvent) => onResize(heightAt(event.clientY));
    const up = (event: PointerEvent) => {
      setDragging(false);
      onRelease(heightAt(event.clientY));
    };
    // A cancelled drag keeps where it got to: the panel has been drawn there throughout, and the
    // release is the only thing that did not happen.
    const cancel = (event: PointerEvent) => {
      setDragging(false);
      onRelease(heightAt(event.clientY));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
    };
  }, [dragging, heightAt, onResize, onRelease]);

  return (
    <div
      className={`sash${dragging ? " sash--dragging" : ""}`}
      role="separator"
      aria-orientation="horizontal"
      aria-label={label}
      aria-valuenow={Math.round(height)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(Math.max(min, max))}
      tabIndex={0}
      onPointerDown={(event) => {
        if (event.button !== 0) {
          return;
        }
        event.preventDefault();
        from.current = { y: event.clientY, height };
        setDragging(true);
      }}
      onKeyDown={(event) => {
        const by = event.key === "ArrowDown" ? STEP : event.key === "ArrowUp" ? -STEP : 0;
        if (by === 0) {
          return;
        }
        event.preventDefault();
        onRelease(clamp(height + by, min, max));
      }}
    />
  );
}
