import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";

import { en } from "../i18n/en";
import { type CueRow } from "../types/subtitle";
import { CPS_LIMIT, lengthLabel, parseTimecode, readingRate, timecode } from "./cueView";

type CurrentLineProps = {
  /** The row the cursor is on, or null while the document has none. */
  index: number | null;
  cue: CueRow | null;
  /** ASS writes line breaks as `\N` inside one field, so a real one cannot be committed there. */
  multiline: boolean;
  /**
   * Filled with a function that sends whatever the box holds. A save must write what the user
   * typed, whichever way the save was asked for. See BACKLOG.md M2.3.
   */
  flushRef: { current: () => Promise<void> };
  /** Told whenever the box holds text the document does not: that is unsaved work too. */
  onDraftChange: (pending: boolean) => void;
  /**
   * Where the caret is in the text box, as a UTF-8 byte offset, which is what a split counts in.
   * Reported rather than read back later because the click that splits blurs the box first.
   */
  onCaret: (offset: number) => void;
  onCommit: (cue: number, text: string) => Promise<void>;
  onCommitTimes: (cue: number, startMs: number, endMs: number) => Promise<void>;
};

/** Which of the two time fields a gesture is in. Duration and CPS are derived and stay read-only. */
type TimeField = "start" | "end";

const encoder = new TextEncoder();

/** A caret at `at` UTF-16 units into `text`, counted in the bytes the backend indexes text by. */
function byteOffset(text: string, at: number): number {
  return encoder.encode(text.slice(0, at)).length;
}

/**
 * The current line, in the tools column under where the waveform will be. It edits whichever row
 * carries the cursor, through the one command the grid's own editor commits with (T5).
 *
 * The waveform is not above it and no placeholder stands in for it: there is no audio provider
 * before M2.4, and a panel with no provider takes no space.
 */
export default function CurrentLine({
  index,
  cue,
  multiline,
  flushRef,
  onDraftChange,
  onCaret,
  onCommit,
  onCommitTimes,
}: CurrentLineProps) {
  const text = cue?.text ?? "";
  const startMs = cue?.startMs ?? 0;
  const endMs = cue?.endMs ?? 0;
  const [draft, setDraft] = useState(text);
  const [times, setTimes] = useState({ start: timecode(startMs), end: timecode(endMs) });
  /** What the box last drew, so a cursor move or a change from elsewhere re-seeds it. */
  const [shown, setShown] = useState({ index, text });
  /** The same for the two time fields, tracked apart so committing one never re-seeds the other. */
  const [shownTimes, setShownTimes] = useState({ index, startMs, endMs });
  /**
   * What the box holds and the row it belongs to. A ref, because the blur that commits it arrives
   * after the click that caused it has already moved the cursor.
   */
  const pending = useRef<{ index: number; was: string; text: string } | null>(null);
  /** The same, for the pair of times: they travel together, in the one command that takes both. */
  const pendingTimes = useRef<{ index: number; startMs: number; endMs: number } | null>(null);

  // The box and the grid's inline editor are two views of the active row, not two states: the one
  // without the keyboard shows what the document holds (decision 5).
  if (shown.index !== index || shown.text !== text) {
    setShown({ index, text });
    setDraft(text);
  }
  if (shownTimes.index !== index || shownTimes.startMs !== startMs || shownTimes.endMs !== endMs) {
    setShownTimes({ index, startMs, endMs });
    setTimes({ start: timecode(startMs), end: timecode(endMs) });
  }

  /** Send what the box holds, if it belongs to a row and actually differs from it. */
  const commit = useCallback(async () => {
    const held = pending.current;
    pending.current = null;
    if (held === null || held.text === held.was) {
      return;
    }
    await onCommit(held.index, held.text);
  }, [onCommit]);

  /** Send the pair, if both fields are times and at least one of them moved. */
  const commitTimes = useCallback(async () => {
    const held = pendingTimes.current;
    pendingTimes.current = null;
    if (held === null) {
      return;
    }
    await onCommitTimes(held.index, held.startMs, held.endMs);
  }, [onCommitTimes]);

  // The window shortcuts and the toolbar flush both editors, so "save" means one thing wherever it
  // was asked for. Times as well as text: an uncommitted time is unsaved work the same way.
  useEffect(() => {
    flushRef.current = async () => {
      await commit();
      await commitTimes();
    };
  });

  const timesEdited = times.start !== timecode(startMs) || times.end !== timecode(endMs);
  useEffect(() => {
    onDraftChange(draft !== text || timesEdited);
  }, [draft, text, timesEdited, onDraftChange]);

  /** A range reports where it starts, which is where the text would divide. */
  function reportCaret(box: HTMLTextAreaElement) {
    onCaret(byteOffset(box.value, box.selectionStart));
  }

  function onType(value: string) {
    setDraft(value);
    // Guarded rather than assumed: the box is only enabled over a row, and the ref outlives the
    // render that produced it.
    if (index !== null) {
      pending.current = { index, was: text, text: value };
    }
  }

  function onTypeTime(field: TimeField, value: string) {
    const next = { ...times, [field]: value };
    setTimes(next);
    const start = parseTimecode(next.start);
    const end = parseTimecode(next.end);
    // A field that is not a time is never sent: it leaves the document alone and shows itself.
    if (index === null || start === null || end === null || (start === startMs && end === endMs)) {
      pendingTimes.current = null;
      return;
    }
    pendingTimes.current = { index, startMs: start, endMs: end };
  }

  function onEditorKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      pending.current = null;
      setDraft(text);
      return;
    }
    if (event.key === "Enter") {
      if (event.shiftKey && multiline) {
        return;
      }
      event.preventDefault();
      void commit();
    }
  }

  function onTimeKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      pendingTimes.current = null;
      setTimes({ start: timecode(startMs), end: timecode(endMs) });
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      void commitTimes();
    }
  }

  if (cue === null) {
    return (
      <section className="currentline" aria-label={en.subtitle.currentLine.label}>
        <p className="currentline__empty">{en.subtitle.currentLine.none}</p>
      </section>
    );
  }

  const rate = readingRate(cue);
  const cpsClasses = ["currentline__cps"];
  if (rate !== null && rate > CPS_LIMIT) {
    cpsClasses.push("currentline__cps--over");
  }

  /** A field that is not a time says so where it is, rather than in the line across the bottom. */
  function timeField(field: TimeField, label: string) {
    const value = times[field];
    const bad = parseTimecode(value) === null;
    const classes = ["currentline__time", `currentline__${field}`];
    if (bad) {
      classes.push("currentline__time--invalid");
    }
    return (
      <span className="currentline__field">
        <span className="currentline__label">{label}</span>
        <input
          className={classes.join(" ")}
          aria-label={label}
          aria-invalid={bad}
          data-document-editor=""
          value={value}
          spellCheck={false}
          onChange={(event) => onTypeTime(field, event.target.value)}
          onKeyDown={onTimeKeyDown}
          onBlur={() => void commitTimes()}
        />
      </span>
    );
  }

  return (
    <section className="currentline" aria-label={en.subtitle.currentLine.label}>
      <div className="currentline__times">
        {timeField("start", en.subtitle.currentLine.start)}
        {timeField("end", en.subtitle.currentLine.end)}
        <span className="currentline__field">
          <span className="currentline__label">{en.subtitle.currentLine.duration}</span>
          <span className="currentline__duration">{lengthLabel(cue)}</span>
        </span>
        <span className="currentline__field">
          <span className="currentline__label">{en.subtitle.currentLine.cps}</span>
          <span className={cpsClasses.join(" ")}>{rate === null ? "" : Math.round(rate)}</span>
        </span>
      </div>
      <textarea
        className="currentline__text"
        aria-label={en.subtitle.currentLine.text}
        data-document-editor=""
        value={draft}
        spellCheck={false}
        onChange={(event) => {
          onType(event.target.value);
          reportCaret(event.target);
        }}
        onSelect={(event) => reportCaret(event.currentTarget)}
        onKeyDown={onEditorKeyDown}
        onBlur={() => void commit()}
      />
    </section>
  );
}
