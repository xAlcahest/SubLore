import { useState } from "react";

import { choosePath } from "../chooser";
import {
  subtitleErrorDetail,
  subtitleErrorMessage,
  subtitleSavedLine,
  subtitleStatusLine,
} from "../hooks/useSubtitleFile";
import { en } from "../i18n/en";
import { type SubtitleError, type SubtitleSaved, type SubtitleSummary } from "../types/subtitle";

type SubtitleBarProps = {
  summary: SubtitleSummary | null;
  saved: SubtitleSaved | null;
  savedInPlace: boolean;
  error: SubtitleError | null;
  dirty: boolean;
  truncated: boolean;
  canUndo: boolean;
  canRedo: boolean;
  /** Set when an open was refused because the file on screen has unsaved edits. */
  blocked: boolean;
  onOpen: (path: string) => void;
  onDiscard: () => void;
  onSave: () => void;
  onSaveAs: (destination: string) => void;
  onUndo: () => void;
  onRedo: () => void;
};

/** Open a subtitle file, save it, and say what state it is in. The list itself is CueList. */
export default function SubtitleBar({
  summary,
  saved,
  savedInPlace,
  error,
  dirty,
  truncated,
  canUndo,
  canRedo,
  blocked,
  onOpen,
  onDiscard,
  onSave,
  onSaveAs,
  onUndo,
  onRedo,
}: SubtitleBarProps) {
  // The chooser is modal and answers on its own thread, so a second click while it is up would
  // raise a second one behind the first.
  const [choosing, setChoosing] = useState(false);
  const detail = error === null ? null : subtitleErrorDetail(error);

  async function pick(kind: "subtitle" | "subtitle-save", act: (path: string) => void) {
    setChoosing(true);
    try {
      // A save chooser opens on the open file's own name, which is what a copy is usually called.
      const from = kind === "subtitle-save" && summary !== null ? summary.path : undefined;
      const path = await choosePath(kind, from);
      // Cancelled is an outcome, not a failure: nothing opens, nothing is written, nothing is said.
      if (path !== null) {
        act(path);
      }
    } finally {
      setChoosing(false);
    }
  }

  return (
    <>
      <div className="subbar">
        <button
          className="subbar__open"
          type="button"
          disabled={choosing}
          onClick={() => void pick("subtitle", onOpen)}
        >
          {en.subtitle.open}
        </button>
        <button
          className="subbar__savefile"
          type="button"
          disabled={summary === null || !dirty}
          onClick={onSave}
        >
          {en.subtitle.saveFile}
        </button>
        <button className="subbar__undo" type="button" disabled={!canUndo} onClick={onUndo}>
          {en.subtitle.undo}
        </button>
        <button className="subbar__redo" type="button" disabled={!canRedo} onClick={onRedo}>
          {en.subtitle.redo}
        </button>
        {blocked && (
          <button className="subbar__discard" type="button" onClick={onDiscard}>
            {en.subtitle.discard}
          </button>
        )}
        <button
          className="subbar__save"
          type="button"
          disabled={summary === null || choosing}
          onClick={() => void pick("subtitle-save", onSaveAs)}
        >
          {en.subtitle.save}
        </button>
      </div>
      <p className="subbar__status">
        <span>{summary === null ? en.subtitle.noFile : subtitleStatusLine(summary)}</span>
        {dirty && <span className="subbar__dirty">{en.subtitle.dirty}</span>}
        {truncated && <span className="subbar__truncated">{en.subtitle.truncated}</span>}
        {saved !== null && <span>{subtitleSavedLine(saved, savedInPlace)}</span>}
      </p>
      {error !== null && (
        <p className="subbar__error" role="alert">
          <span>{subtitleErrorMessage(error)}</span>
          {detail !== null && <span>{detail}</span>}
        </p>
      )}
    </>
  );
}
