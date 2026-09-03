import { useCallback, useEffect, useRef, useState } from "react";

/** Which way the edge travels: `x` moves a width, `y` moves a height. */
type SashAxis = "x" | "y";

/** Names the edge in the DOM, so a check and a stylesheet can point at one of the three. */
type SashEdge = "video" | "waveform" | "grid";

type SashProps = {
  axis: SashAxis;
  edge: SashEdge;
  /** The size the panel before this edge has now, in CSS pixels: a width on `x`, a height on `y`. */
  size: number;
  min: number;
  max: number;
  label: string;
  /** Every step of a drag. Cheap: the caller keeps this in React state and stores nothing. */
  onResize: (size: number) => void;
  /** The end of a drag or one keyboard step, which is when the size is worth storing. */
  onRelease: (size: number) => void;
};

/** One keyboard press. Small enough to place the edge, large enough to cross the range. */
const STEP = 8;

function clamp(size: number, min: number, max: number): number {
  return Math.min(Math.max(size, min), Math.max(min, max));
}

/**
 * A draggable edge between two panels (D1). Three of them: the video's right side, the waveform's
 * bottom and the top block's bottom.
 *
 * The move and release are listened for on the window rather than through pointer capture on this
 * four-pixel strip. Capture is the tidier API and it does not hold here: under WebKitGTK the strip
 * stopped hearing anything once the pointer left it, so a drag to the top of the window ended
 * wherever the pointer crossed the edge and the release that stores the size never arrived.
 *
 * The keyboard route is not decoration: a separator only a mouse can place is one a keyboard user
 * cannot use at all.
 */
export default function Sash({
  axis,
  edge,
  size,
  min,
  max,
  label,
  onResize,
  onRelease,
}: SashProps) {
  const [dragging, setDragging] = useState(false);
  // Read by the window listeners, which are installed once per drag and must not see a stale
  // starting point or a stale range.
  const from = useRef({ at: 0, size: 0 });
  const bounds = useRef({ min, max });
  bounds.current = { min, max };

  // The pointer's coordinate along this edge's own axis, and nothing about the other one.
  const along = useCallback(
    (at: { clientX: number; clientY: number }) => (axis === "x" ? at.clientX : at.clientY),
    [axis],
  );

  const sizeAt = useCallback(
    (at: number) =>
      clamp(from.current.size + (at - from.current.at), bounds.current.min, bounds.current.max),
    [],
  );

  useEffect(() => {
    if (!dragging) {
      return;
    }
    const move = (event: PointerEvent) => onResize(sizeAt(along(event)));
    const up = (event: PointerEvent) => {
      setDragging(false);
      onRelease(sizeAt(along(event)));
    };
    // A cancelled drag keeps where it got to: the panel has been drawn there throughout, and the
    // release is the only thing that did not happen.
    const cancel = (event: PointerEvent) => {
      setDragging(false);
      onRelease(sizeAt(along(event)));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
    };
  }, [dragging, along, sizeAt, onResize, onRelease]);

  return (
    <div
      className={`sash sash--${axis} sash--${edge}${dragging ? " sash--dragging" : ""}`}
      role="separator"
      // A bar between a left and a right panel is a vertical separator; one between a top and a
      // bottom is horizontal.
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      aria-label={label}
      aria-valuenow={Math.round(size)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(Math.max(min, max))}
      tabIndex={0}
      onPointerDown={(event) => {
        if (event.button !== 0) {
          return;
        }
        event.preventDefault();
        from.current = { at: along(event), size };
        setDragging(true);
      }}
      onKeyDown={(event) => {
        const forward = axis === "x" ? "ArrowRight" : "ArrowDown";
        const back = axis === "x" ? "ArrowLeft" : "ArrowUp";
        const by = event.key === forward ? STEP : event.key === back ? -STEP : 0;
        if (by === 0) {
          return;
        }
        event.preventDefault();
        onRelease(clamp(size + by, min, max));
      }}
    />
  );
}
