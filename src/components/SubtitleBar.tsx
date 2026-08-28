import { useId, useState } from "react";

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
  const pathId = useId();
  const destinationId = useId();
  const [path, setPath] = useState("");
  const [destination, setDestination] = useState("");
  const trimmedPath = path.trim();
  const trimmedDestination = destination.trim();
  const detail = error === null ? null : subtitleErrorDetail(error);

  return (
    <>
      <div className="subbar">
        <label className="bar__label" htmlFor={pathId}>
          {en.subtitle.pathLabel}
        </label>
        <input
          id={pathId}
          className="subbar__input"
          type="text"
          value={path}
          placeholder={en.subtitle.pathPlaceholder}
          onChange={(event) => setPath(event.target.value)}
        />
        <button
          className="subbar__open"
          type="button"
          disabled={trimmedPath === ""}
          onClick={() => onOpen(trimmedPath)}
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
        <label className="bar__label" htmlFor={destinationId}>
          {en.subtitle.destinationLabel}
        </label>
        <input
          id={destinationId}
          className="subbar__dest"
          type="text"
          value={destination}
          placeholder={en.subtitle.destinationPlaceholder}
          onChange={(event) => setDestination(event.target.value)}
        />
        <button
          className="subbar__save"
          type="button"
          disabled={summary === null || trimmedDestination === ""}
          onClick={() => onSaveAs(trimmedDestination)}
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
