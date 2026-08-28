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
  busy: boolean;
  summary: SubtitleSummary | null;
  saved: SubtitleSaved | null;
  error: SubtitleError | null;
  onOpen: (path: string) => void;
  onSave: (destination: string) => void;
};

/** Open a subtitle file and save a copy of it. No editor: M1 only reads and writes whole files. */
export default function SubtitleBar({
  busy,
  summary,
  saved,
  error,
  onOpen,
  onSave,
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
          disabled={trimmedPath === "" || busy}
          onClick={() => onOpen(trimmedPath)}
        >
          {en.subtitle.open}
        </button>
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
          disabled={summary === null || trimmedDestination === "" || busy}
          onClick={() => onSave(trimmedDestination)}
        >
          {en.subtitle.save}
        </button>
      </div>
      <p className="subbar__status">
        <span>{summary === null ? en.subtitle.noFile : subtitleStatusLine(summary)}</span>
        {saved !== null && <span>{subtitleSavedLine(saved)}</span>}
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
