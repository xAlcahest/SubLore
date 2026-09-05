import { useCallback, useEffect, useRef, useState, type ReactNode, type RefObject } from "react";

import { en } from "../i18n/en";
import { type Waveform as Peaks } from "../hooks/useAudioPeaks";
import { useSmoothPosition } from "../hooks/useSmoothPosition";
import { useWaveformView } from "../hooks/useWaveformView";
import { type CueRow } from "../types/subtitle";
import { timecode } from "./cueView";
import { rulerTicks } from "./waveRuler";

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

/**
 * The sizes below are the reference's own, and the reference draws into a logical device context,
 * so they are CSS pixels. Both canvases here are backed in device pixels, so each is multiplied by
 * the display's ratio where it is used and none of them is written into the drawing as it stands.
 */
/** A boundary's triangular foot (`src/audio_display.h:127`). */
const FOOT_CSS_PX = 6;
/** How tall a ruler tick is drawn, minor and major (`src/audio_display.cpp:435-438`). */
const MINOR_TICK_CSS_PX = 3;
const MAJOR_TICK_CSS_PX = 5;
/** The room the ruler keeps around its line of type (`src/audio_display.cpp:308-310`). */
const RULER_PADDING_CSS_PX = 4;
/** The room the hover label keeps from the panel's edges and from its top
 * (`src/audio_display.cpp:981-983`). */
const LABEL_MARGIN_CSS_PX = 2;
/** The halo drawn round the hover label, one pixel on every side (`src/audio_display.cpp:988-993`). */
const LABEL_OUTLINE_CSS_PX = 2;

/** The glyphs a ruler label can hold, measured to give the band its height. Not user-facing copy. */
const RULER_SAMPLE = "0123456789:.";

/**
 * A held boundary out of the window brings the window after it: one 50 ms shot, armed when the
 * boundary moves out of view, which scrolls the window and never moves the boundary. The scroll
 * overshoots the edge by a twentieth of the panel (`src/audio_display.cpp:1327-1340,1349-1366`).
 */
const EDGE_STEP_MS = 50;
const EDGE_MARGIN_DIVISOR = 20;

/** Which boundary a gesture holds. The two travel in one command, so a drag names one of them. */
type Boundary = "start" | "end";

/** The pair a hand is holding: which boundary moves, and where the two of them are right now. */
type Drag = { moving: Boundary; startMs: number; endMs: number };

/** What the panel is holding, or nothing, published so the chrome can play what a hand is on. */
export type LiveTimes = { startMs: number; endMs: number } | null;

type WaveformProps = {
  peaks: Peaks;
  /** Where playback is, in milliseconds, for the playhead. */
  positionMs: number;
  /** The media's length in milliseconds, so the drawing keeps its scale while peaks arrive. */
  durationMs: number;
  /** The height the sash was left at, in CSS pixels, or nothing before the layout has been read. */
  height?: number;
  /** The interface size, which is the size of the type the ruler's band is measured against (S2). */
  scale: number;
  /** Playback stopped: the playhead neither moves on its own nor drags the view along.  */
  paused: boolean;
  /** The row the cursor is on, held here so a release can name it after the cursor has moved. */
  cueIndex: number | null;
  /** The cursor's cue, whose two boundaries are drawn and can be dragged, or null for none. */
  cue: CueRow | null;
  /** Every cue in the document: the neighbours are drawn, tinted and snapped to. */
  cues: CueRow[];
  /** Which rows are in the selection, which is the third of the four background tints. */
  selected: ReadonlySet<number>;
  /** Whether the cursor's cue is brought into view whenever its range changes. */
  autoscroll: boolean;
  /** Filled with this panel's own centring, so the command in the chrome can run it. */
  centreRef: RefObject<() => void>;
  /** Filled with the pair a hand is holding, so playing the selection plays where it is now. */
  liveRef: RefObject<LiveTimes>;
  /** The end of a drag: the pair the cue takes. Not called for a drag that is refused (M2.5 G4). */
  onDragTimes: (cue: number, startMs: number, endMs: number) => void;
  /** The middle press: the video goes to the millisecond under the pointer. */
  onSeek: (seconds: number) => void;
  /**
   * The panel's own strip of controls, drawn under the wave and inside the panel, which is where
   * the reference puts it (`src/audio_box.cpp:104-107`). It arrives as a child rather than being
   * built here so that the registry stays in the shell and the panel keeps drawing audio.
   */
  children?: ReactNode;
};

/** The colours the canvas draws in, read from one settled cascade rather than written twice. */
function ink(style: CSSStyleDeclaration, name: string, fallback: string): string {
  const value = style.getPropertyValue(name).trim();
  return value === "" ? fallback : value;
}

/**
 * The font a canvas draws in, sized in the device pixels its backing store is sized in.
 *
 * The two parts are joined by hand rather than read off the `font` shorthand: WebKit serialises that
 * shorthand as the empty string whenever it cannot express the whole of the cascade in it, and
 * assigning an empty string to `context.font` leaves the canvas default.
 */
function fontOf(style: CSSStyleDeclaration, ratio: number): string {
  const size = Number.parseFloat(style.fontSize);
  // A computed font size is a pixel length; anything else is drawn as given rather than guessed at.
  const sized = Number.isFinite(size) && size > 0 ? `${size * ratio}px` : style.fontSize;
  return `${sized} ${style.fontFamily}`;
}

/** Where a pointer is on the panel, in the device pixels the view is scaled in. */
function atPixel(event: { clientX: number }, element: HTMLCanvasElement): number {
  return (event.clientX - element.getBoundingClientRect().x) * window.devicePixelRatio;
}

/** The backing store sized in device pixels, so a scaled display draws once rather than stretching
 * a smaller drawing. Returns the size it settled on. */
function fitBackingStore(element: HTMLCanvasElement, heightPx?: number): [number, number] {
  const ratio = window.devicePixelRatio;
  const width = Math.max(1, Math.round(element.clientWidth * ratio));
  const height = heightPx ?? Math.max(1, Math.round(element.clientHeight * ratio));
  if (element.width !== width || element.height !== height) {
    element.width = width;
    element.height = height;
  }
  return [width, height];
}

/**
 * The waveform, in the tools column above the current line (M2.4 W5).
 *
 * It draws from the peaks as they arrive rather than when the job ends, so a long episode shows its
 * first seconds while the rest is still being read. Its ruler is a canvas of its own above the body,
 * so the body's own height stays the wave's and nothing measured against it moves.
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
  scale,
  paused,
  cueIndex,
  cue,
  cues,
  selected,
  autoscroll,
  centreRef,
  liveRef,
  onDragTimes,
  onSeek,
  children,
}: WaveformProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const ruler = useRef<HTMLCanvasElement>(null);
  const [widthPx, setWidthPx] = useState(0);
  const [heightPx, setHeightPx] = useState(0);
  /** Flipped once the type has settled, because every measurement below is of some type. */
  const [typeSettled, setTypeSettled] = useState(false);
  /** The ruler band's height in device pixels, measured off its own type rather than written down. */
  const [rulerPx, setRulerPx] = useState(0);
  /** The pair a hand is holding, or null while nothing is held. */
  const [drag, setDragState] = useState<Drag | null>(null);
  const dragRef = useRef<Drag | null>(null);
  /** Where the hover line stands, or null while the pointer is not over the panel. */
  const [hoverPx, setHoverPx] = useState<number | null>(null);
  /** Where a pan on the ruler was last seen, in CSS pixels, or null while none is running. */
  const [panning, setPanning] = useState(false);
  const panFrom = useRef(0);
  /**
   * The window's own drawing, kept on a canvas of its own: the tints, the wave and every other
   * line's boundaries. A pointer move or a playback frame blits it instead of rescanning every peak
   * bucket, which is the reference's rule that a moving cursor invalidates two thin rectangles and
   * never the audio (`src/audio_display.cpp:1024-1049`). See CLAUDE.md section 7.
   */
  const layer = useRef<HTMLCanvasElement | null>(null);
  const layerFor = useRef<{
    key: string;
    cues: CueRow[];
    selected: ReadonlySet<number>;
  } | null>(null);
  /** Where the held boundary is, in device pixels, read by the edge shot after it fires. */
  const heldPx = useRef(0);
  const edgeShot = useRef<number | null>(null);
  // The whole media's length, not what has arrived: peaks filling in must not rescale what is
  // already drawn under the playhead.
  const spanMs = Math.max(1, durationMs > 0 ? durationMs : peaks.filled);
  const { view, zoomBy, scrollBy, showRange, followTo, startFollowing } = useWaveformView(
    spanMs,
    widthPx,
  );
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

  /**
   * A boundary a press or a drag puts down lands on the nearest boundary of another line inside the
   * snap distance. Shift inverts the setting for that gesture, which is the reference's own escape
   * hatch, and the reference snaps a press exactly as it snaps a drag
   * (`src/audio_timing_dialogue.cpp:606-635`).
   */
  const snapAt = useCallback(
    (ms: number, snapping: boolean): number => {
      if (!snapping) {
        return ms;
      }
      let landing = ms;
      let nearest = GRAB_PX * view.msPerPixel;
      for (let index = 0; index < cues.length; index += 1) {
        if (index === cueIndex) {
          continue;
        }
        for (const edge of [cues[index].startMs, cues[index].endMs]) {
          const gap = Math.abs(edge - ms);
          if (gap <= nearest) {
            nearest = gap;
            landing = edge;
          }
        }
      }
      return landing;
    },
    [cues, cueIndex, view],
  );

  /** One place writes the drag, so the ref the release reads and what the panel paints agree. */
  const setDrag = useCallback(
    (next: Drag | null) => {
      dragRef.current = next;
      liveRef.current = next === null ? null : { startMs: next.startMs, endMs: next.endMs };
      setDragState(next);
    },
    [liveRef],
  );

  // What the panel draws: the document's own times, except for the pair a hand is holding.
  const shownStartMs = drag === null ? (cue?.startMs ?? 0) : drag.startMs;
  const shownEndMs = drag === null ? (cue?.endMs ?? 0) : drag.endMs;
  // The row the panel draws as the current one. Null when the cursor has no cue behind it, so a
  // cursor past the end of the document tints nothing and hides no neighbour.
  const activeRow = cue === null ? null : cueIndex;

  // The command in the chrome runs the panel's own centring: only the panel knows where its window
  // is. Rewritten every render so the cue it closes over is the one on screen.
  useEffect(() => {
    centreRef.current = () => {
      if (cue !== null) {
        showRange(cue.startMs, cue.endMs);
      }
    };
  });

  // The cursor's line is brought into view whenever its range changes, which is what makes the
  // panel usable on a file longer than its own window (interface-spec 5.2).
  useEffect(() => {
    if (autoscroll && cue !== null) {
      showRange(cue.startMs, cue.endMs);
    }
  }, [autoscroll, cue?.startMs, cue?.endMs, showRange]);

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
    const measure = () => {
      const ratio = window.devicePixelRatio;
      setWidthPx(Math.max(1, Math.round(element.clientWidth * ratio)));
      setHeightPx(Math.max(1, Math.round(element.clientHeight * ratio)));
    };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    // Type can arrive after the first paint, and the ruler's band is the height of some type. The
    // promise has no failure: a page whose fonts never settle keeps the reading taken above.
    void document.fonts.ready.then(
      () => setTypeSettled(true),
      () => {},
    );
    return () => observer.disconnect();
  }, []);

  // The ruler: its own band, its own canvas, its height measured off the type it draws.
  useEffect(() => {
    const element = ruler.current;
    if (element === null) {
      return;
    }
    const context = element.getContext("2d");
    if (context === null) {
      return;
    }
    const ratio = window.devicePixelRatio;
    const style = getComputedStyle(element);
    context.font = fontOf(style, ratio);
    const box = context.measureText(RULER_SAMPLE);
    const ascent = Number.isFinite(box.fontBoundingBoxAscent)
      ? box.fontBoundingBoxAscent
      : box.actualBoundingBoxAscent;
    const descent = Number.isFinite(box.fontBoundingBoxDescent)
      ? box.fontBoundingBoxDescent
      : box.actualBoundingBoxDescent;
    const line = ascent + descent;
    // A band read against type that has not arrived would be a band read once, so it is taken on
    // every paint and the state settles on the first that differs.
    if (Number.isFinite(line) && line > 0) {
      const wanted = Math.ceil(line) + Math.round(RULER_PADDING_CSS_PX * ratio);
      if (wanted !== rulerPx) {
        setRulerPx(wanted);
        return;
      }
    }
    if (rulerPx <= 0) {
      return;
    }
    const [width, bandHeight] = fitBackingStore(element, rulerPx);
    // The font is lost with the backing store, so it goes back on after the resize above.
    context.font = fontOf(style, ratio);
    const rulePx = Math.max(1, Math.round(ratio));
    const minorPx = Math.max(1, Math.round(MINOR_TICK_CSS_PX * ratio));
    const majorPx = Math.max(1, Math.round(MAJOR_TICK_CSS_PX * ratio));
    context.fillStyle = ink(style, "--bg-panel", "#202026");
    context.fillRect(0, 0, width, bandHeight);
    context.fillStyle = ink(style, "--line-strong", "#34343c");
    context.fillRect(0, bandHeight - rulePx, width, rulePx);
    context.fillStyle = ink(style, "--ink-muted", "#b6b6c0");
    context.textBaseline = "top";
    const ticks = rulerTicks(
      view.fromMs,
      view.msPerPixel,
      width,
      durationMs,
      (label) => context.measureText(label).width,
    );
    for (const tick of ticks) {
      const tall = tick.major ? majorPx : minorPx;
      context.fillRect(Math.round(tick.atPx), bandHeight - rulePx - tall, rulePx, tall);
      if (tick.label !== null) {
        // On the tick, not beside it: the crowding rule above measures from the tick's own pixel
        // (`src/audio_display.cpp:463,475`), so a label drawn anywhere else is measured wrong.
        context.fillText(tick.label, Math.round(tick.atPx), 0);
      }
    }
  }, [view, widthPx, durationMs, rulerPx, typeSettled, scale]);

  useEffect(() => {
    const element = canvas.current;
    if (element === null) {
      return;
    }
    const context = element.getContext("2d");
    if (context === null) {
      return;
    }

    const ratio = window.devicePixelRatio;
    // One read of the cascade per repaint: a getComputedStyle call per token was ten of them a
    // frame, and this effect runs on every pointer move.
    const style = getComputedStyle(element);
    const [width, canvasHeight] = fitBackingStore(element);
    context.font = fontOf(style, ratio);
    const background = ink(style, "--bg-media", "#0c0c0e");
    const footPx = Math.max(1, Math.round(FOOT_CSS_PX * ratio));
    const middle = canvasHeight / 2;

    /**
     * One boundary: the bar, and a triangular foot at each end pointing into its own line's span.
     * Every drawn line's boundaries carry the same shape and the same thickness in the reference,
     * and only the colour tells the current line's from the rest
     * (`src/audio_display.cpp:915-942`; `src/audio_timing_dialogue.cpp:181-187,292-296`).
     */
    const paintMarker = (target: CanvasRenderingContext2D, atPx: number, into: number) => {
      target.fillRect(atPx, 0, MARKER_PX, canvasHeight);
      const ends: [number, number][] = [
        [0, 1],
        [canvasHeight, -1],
      ];
      for (const [edge, down] of ends) {
        target.beginPath();
        target.moveTo(atPx, edge);
        target.lineTo(atPx + footPx * into, edge);
        target.lineTo(atPx, edge + footPx * down);
        target.closePath();
        target.fill();
      }
    };

    // Everything that only changes with the window goes on the cached layer. The current line's own
    // times are in the key because its tint moves with a hand on a boundary.
    const key = [
      width,
      canvasHeight,
      view.fromMs,
      view.msPerPixel,
      peaks.filled,
      activeRow,
      shownStartMs,
      shownEndMs,
    ].join("|");
    if (layer.current === null) {
      layer.current = document.createElement("canvas");
    }
    const spare = layer.current;
    const cached = layerFor.current;
    if (
      cached === null ||
      cached.key !== key ||
      cached.cues !== cues ||
      cached.selected !== selected
    ) {
      spare.width = width;
      spare.height = canvasHeight;
      const paint = spare.getContext("2d");
      if (paint === null) {
        return;
      }
      paint.fillStyle = background;
      paint.fillRect(0, 0, width, canvasHeight);

      // The four background tints, painted in role order so the higher role wins outright wherever
      // two lines overlap (interface-spec 5.5). The cursor's own line is drawn where a hand has it.
      const tints: [string, string][] = [
        ["--wave-other", "#16161d"],
        ["--wave-other-selected", "#1d2430"],
        ["--wave-current", "#26303d"],
      ];
      for (let role = 0; role < tints.length; role += 1) {
        paint.fillStyle = ink(style, tints[role][0], tints[role][1]);
        for (let index = 0; index < cues.length; index += 1) {
          const own = index === activeRow;
          const rank = own ? 2 : selected.has(index) ? 1 : 0;
          if (rank !== role) {
            continue;
          }
          const row = cues[index];
          const from = Math.round(pixelOf(own ? shownStartMs : row.startMs));
          const to = Math.round(pixelOf(own ? shownEndMs : row.endMs));
          if (to < 0 || from >= width) {
            continue;
          }
          const left = Math.max(0, from);
          paint.fillRect(left, 0, Math.max(1, Math.min(width, to) - left), canvasHeight);
        }
      }

      paint.fillStyle = ink(style, "--ink-dim", "#9a9aa6");
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
        paint.fillRect(x, top, 1, Math.max(1, bottom - top));
      }

      // The neighbours: where the lines either side of this one are. They are not what a drag
      // grabs, and the colour is the only thing that says so (interface-spec 5.4).
      paint.fillStyle = ink(style, "--marker-other", "#6a6a7a");
      for (let index = 0; index < cues.length; index += 1) {
        if (index === activeRow) {
          continue;
        }
        const edges: [number, number][] = [
          [cues[index].startMs, 1],
          [cues[index].endMs, -1],
        ];
        for (const [ms, into] of edges) {
          const at = Math.round(pixelOf(ms));
          if (at >= 0 && at < width) {
            paintMarker(paint, at, into);
          }
        }
      }
      layerFor.current = { key, cues, selected };
    }
    context.drawImage(spare, 0, 0);

    // Under the playhead, which is the paint order interface-spec 5.4 keeps: a head sitting exactly
    // on a boundary has to stay readable.
    if (cue !== null) {
      const markers: [number, string, string, number][] = [
        [shownStartMs, "--marker-start", "#6fd08c", 1],
        [shownEndMs, "--marker-end", "#e8935f", -1],
      ];
      for (const [ms, token, fallback, into] of markers) {
        const at = Math.round(pixelOf(ms));
        if (at >= 0 && at < width) {
          context.fillStyle = ink(style, token, fallback);
          paintMarker(context, at, into);
        }
      }
    }

    if (durationMs > 0) {
      const at = Math.round(pixelOf(Math.min(headMs, durationMs)));
      // Off the window at either end draws nothing rather than a line stuck to an edge, which
      // would read as a playhead that is where it is not.
      if (at >= 0 && at < width) {
        context.fillStyle = ink(style, "--accent", "#4fb3d9");
        context.fillRect(at, 0, 1, canvasHeight);
      }
    }

    // The hover line and the time it carries. Only while nothing is playing: during playback the
    // playhead is the line that means something (interface-spec 5.7).
    if (paused && hoverPx !== null && hoverPx >= 0 && hoverPx < width) {
      const at = Math.round(hoverPx);
      const marginPx = Math.max(1, Math.round(LABEL_MARGIN_CSS_PX * ratio));
      context.fillStyle = ink(style, "--ink-muted", "#b6b6c0");
      context.fillRect(at, 0, 1, canvasHeight);
      const label = timecode(msAt(hoverPx));
      const labelWidth = context.measureText(label).width;
      // Centred on the line and kept off both edges, which is where the reference puts it
      // (`src/audio_display.cpp:981-983`); a panel too narrow to hold it keeps the left margin.
      const textAt = Math.max(
        marginPx,
        Math.min(at - labelWidth / 2, width - labelWidth - marginPx),
      );
      context.textBaseline = "top";
      context.lineWidth = Math.max(1, Math.round(LABEL_OUTLINE_CSS_PX * ratio));
      context.strokeStyle = background;
      context.strokeText(label, textAt, marginPx);
      context.fillText(label, textAt, marginPx);
    }
  }, [
    peaks,
    headMs,
    durationMs,
    view,
    pixelOf,
    msAt,
    cue,
    activeRow,
    cues,
    selected,
    shownStartMs,
    shownEndMs,
    hoverPx,
    paused,
    widthPx,
    heightPx,
  ]);

  // On the window rather than through pointer capture, for the reason Sash.tsx records: under
  // WebKitGTK a captured element stops hearing the pointer the moment it leaves.
  const dragging = drag !== null;
  useEffect(() => {
    const element = canvas.current;
    if (!dragging || element === null) {
      return;
    }
    // The row went out from under the gesture. Let it go here rather than leave a drag that rearms
    // itself on the next render and commits on a button nobody is holding.
    if (cue === null || cueIndex === null) {
      setDrag(null);
      return;
    }
    const moveTo = (px: number, snapping: boolean) => {
      const held = dragRef.current;
      if (held === null) {
        return;
      }
      const ms = snapAt(msAt(px), snapping);
      setDrag(held.moving === "start" ? { ...held, startMs: ms } : { ...held, endMs: ms });
    };
    const move = (event: PointerEvent) => moveTo(atPixel(event, element), !event.shiftKey);
    const finish = (event: PointerEvent) => {
      moveTo(atPixel(event, element), !event.shiftKey);
      const held = dragRef.current;
      setDrag(null);
      // Refused rather than clamped: nothing below refuses it either, and a cue whose length
      // nobody chose is worse than a marker that goes back where it was (M2.5 G4).
      if (held !== null && held.endMs > held.startMs) {
        onDragTimes(cueIndex, held.startMs, held.endMs);
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
  }, [dragging, cue, cueIndex, msAt, snapAt, onDragTimes, setDrag]);

  // The edge shot, armed by the held boundary leaving the window and by nothing else, so a hand
  // that stops moving outside the panel stops taking the window with it.
  useEffect(() => {
    if (drag === null) {
      return;
    }
    heldPx.current = pixelOf(drag.moving === "start" ? drag.startMs : drag.endMs);
    if (heldPx.current >= 0 && heldPx.current < widthPx) {
      return;
    }
    // A shot already in flight is left in flight rather than restarted: the reference arms its
    // timer only when none is running, so a hand moving without pause still gets its scroll.
    if (edgeShot.current !== null) {
      return;
    }
    edgeShot.current = window.setTimeout(() => {
      edgeShot.current = null;
      const at = heldPx.current;
      const margin = widthPx / EDGE_MARGIN_DIVISOR;
      if (at < 0) {
        scrollBy(at - margin);
      } else if (at >= widthPx) {
        scrollBy(at - widthPx + margin);
      }
    }, EDGE_STEP_MS);
  }, [drag, pixelOf, widthPx, scrollBy]);

  // The shot never outlives the gesture that armed it or the panel it would scroll.
  useEffect(() => {
    return () => {
      if (edgeShot.current !== null) {
        window.clearTimeout(edgeShot.current);
        edgeShot.current = null;
      }
    };
  }, [dragging]);

  // Nothing is held and nothing is centred once the panel goes: a pair left behind here would be
  // played by the strip, and a centring left behind would run against a window that is not there.
  useEffect(() => {
    return () => {
      liveRef.current = null;
      centreRef.current = () => {};
    };
  }, [liveRef, centreRef]);

  // Dragging the ruler pans the window one for one with the hand (`src/audio_display.cpp:380-400`).
  useEffect(() => {
    if (!panning) {
      return;
    }
    const move = (event: PointerEvent) => {
      const travel = (event.clientX - panFrom.current) * window.devicePixelRatio;
      panFrom.current = event.clientX;
      scrollBy(-travel);
    };
    const finish = () => setPanning(false);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
    };
  }, [panning, scrollBy]);

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
        ref={ruler}
        className="waveform__ruler"
        aria-hidden="true"
        style={{ height: rulerPx === 0 ? 0 : rulerPx / window.devicePixelRatio }}
        onPointerDown={(event) => {
          if (event.button !== 0) {
            return;
          }
          event.preventDefault();
          panFrom.current = event.clientX;
          setPanning(true);
        }}
      />
      <canvas
        ref={canvas}
        className="waveform__canvas"
        tabIndex={0}
        aria-label={en.waveform.canvas}
        onContextMenu={(event) => event.preventDefault()}
        onPointerDown={(event) => {
          const element = event.currentTarget;
          const px = atPixel(event, element);
          // Every branch below prevents the default, which is also what would have moved the
          // keyboard here, so the panel takes focus itself and its own keys keep working.
          element.focus();
          // The middle press seeks and grabs nothing (`src/audio_display.cpp:1098-1102`).
          if (event.button === 1) {
            event.preventDefault();
            onSeek(msAt(px) / 1000);
            return;
          }
          if (cue === null || cueIndex === null) {
            return;
          }
          const landing = snapAt(msAt(px), !event.shiftKey);
          // The right press takes the end to where it landed, whichever boundary is nearer
          // (`src/audio_timing_dialogue.cpp:638-644`).
          if (event.button === 2) {
            event.preventDefault();
            setDrag({ moving: "end", startMs: cue.startMs, endMs: landing });
            return;
          }
          if (event.button !== 0) {
            return;
          }
          event.preventDefault();
          const which = grabbed(px);
          // A press near the end grabs it and moves nothing; a press near the start grabs it and
          // takes it to the press as well (`src/audio_timing_dialogue.cpp:617-635`).
          if (which === "end") {
            setDrag({ moving: "end", startMs: cue.startMs, endMs: cue.endMs });
            return;
          }
          if (which === "start") {
            setDrag({ moving: "start", startMs: landing, endMs: cue.endMs });
            return;
          }
          // Far from either boundary: the start goes to the press and the end is handed to the
          // drag from where the document still has it, so one gesture times the line from scratch
          // (`src/audio_timing_dialogue.cpp:604-615`).
          setDrag({ moving: "end", startMs: landing, endMs: cue.endMs });
        }}
        onPointerMove={(event) => {
          const px = atPixel(event, event.currentTarget);
          // Straight onto the element: the shape that says a press would grab must not cost the
          // panel a redraw (interface-spec 5.7). The line and its time cost one blit.
          if (drag === null) {
            event.currentTarget.style.cursor = grabbed(px) === null ? "" : "ew-resize";
          }
          setHoverPx(px);
        }}
        onPointerLeave={() => setHoverPx(null)}
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
      {children}
    </section>
  );
}
