import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";

import { en } from "../i18n/en";
import { type CueRow } from "../types/subtitle";
import { CPS_LIMIT, lengthLabel, readingRate, timecode } from "./cueView";

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
  onCommit: (cue: number, text: string) => Promise<void>;
};

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
  onCommit,
}: CurrentLineProps) {
  const text = cue?.text ?? "";
  const [draft, setDraft] = useState(text);
  /** What the box last drew, so a cursor move or a change from elsewhere re-seeds it. */
  const [shown, setShown] = useState({ index, text });
  /**
   * What the box holds and the row it belongs to. A ref, because the blur that commits it arrives
   * after the click that caused it has already moved the cursor.
   */
  const pending = useRef<{ index: number; was: string; text: string } | null>(null);

  // The box and the grid's inline editor are two views of the active row, not two states: the one
  // without the keyboard shows what the document holds (decision 5).
  if (shown.index !== index || shown.text !== text) {
    setShown({ index, text });
    setDraft(text);
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

  // The window shortcuts and the toolbar flush both editors, so "save" means one thing wherever it
  // was asked for.
  useEffect(() => {
    flushRef.current = commit;
  });

  useEffect(() => {
    onDraftChange(draft !== text);
  }, [draft, text, onDraftChange]);

  function onType(value: string) {
    setDraft(value);
    // Guarded rather than assumed: the box is only enabled over a row, and the ref outlives the
    // render that produced it.
    if (index !== null) {
      pending.current = { index, was: text, text: value };
    }
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

  return (
    <section className="currentline" aria-label={en.subtitle.currentLine.label}>
      <div className="currentline__times">
        <span className="currentline__field">
          <span className="currentline__label">{en.subtitle.currentLine.start}</span>
          <span className="currentline__start">{timecode(cue.startMs)}</span>
        </span>
        <span className="currentline__field">
          <span className="currentline__label">{en.subtitle.currentLine.end}</span>
          <span className="currentline__end">{timecode(cue.endMs)}</span>
        </span>
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
        onChange={(event) => onType(event.target.value)}
        onKeyDown={onEditorKeyDown}
        onBlur={() => void commit()}
      />
    </section>
  );
}
