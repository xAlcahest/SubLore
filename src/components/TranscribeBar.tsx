import { useId } from "react";

import {
  asrErrorMessage,
  backendName,
  cueCountLine,
  downloadLine,
  modelOptionLabel,
  runStatusLine,
  type Transcription,
} from "../hooks/useTranscription";
import { en } from "../i18n/en";

type TranscribeBarProps = {
  /** The video that is open, or null. Nothing can be transcribed without one. */
  mediaPath: string | null;
  transcription: Transcription;
  /** The run whose cues are the open document, or null. See BACKLOG.md M3.5. */
  adoptedRunId: number | null;
  onUse: (runId: number) => void;
};

/** m:ss, the same shape the playback controls use. Punctuation, not translatable copy. */
function timestamp(milliseconds: number): string {
  const seconds = Math.floor(milliseconds / 1000);
  return `${Math.floor(seconds / 60)}:${(seconds % 60).toString().padStart(2, "0")}`;
}

/** The one sentence on the status line, in the order the states rank. */
function statusText({ running, phase, percent, notice, result }: Transcription): string {
  if (running) {
    return runStatusLine(phase, percent);
  }
  if (notice !== null) {
    return asrErrorMessage(notice);
  }
  return result === null ? en.asr.idle : cueCountLine(result.cues.length);
}

/**
 * How full the bar is, or nothing for an indeterminate one: ffmpeg reports no percentage worth
 * showing, and a bar that admits it beats one that invents a number.
 */
function progressValue({ running, phase, percent, download }: Transcription): number | undefined {
  if (download !== null) {
    return download.totalBytes === 0
      ? undefined
      : Math.floor((download.receivedBytes / download.totalBytes) * 100);
  }
  return running && phase === "transcribing" ? percent : undefined;
}

/**
 * Choose a model, start a transcription, watch it, stop it. The cue list underneath is what the run
 * produced; the document those cues became is the grid. See BACKLOG.md M3.4 and M3.5.
 */
export default function TranscribeBar({
  mediaPath,
  transcription,
  adoptedRunId,
  onUse,
}: TranscribeBarProps) {
  const modelFieldId = useId();
  const { models, modelId, useGpu, running, result, error, damagedModelId, download } =
    transcription;

  const selected = models.find((model) => model.id === modelId) ?? null;
  // A model refused on its checksum lists as `ready` (the listing only reads lengths), so it is
  // Download rather than Transcribe that it offers. See BACKLOG.md M3.2.
  const damaged = selected !== null && selected.id === damagedModelId;
  const canDownload =
    selected !== null && (selected.state !== "ready" || damaged) && download === null;
  const canStart =
    mediaPath !== null && selected?.state === "ready" && !damaged && !running && download === null;
  const status = statusText(transcription);
  const progress = progressValue(transcription);
  // A finished run becomes the document on its own; this offers it again after the question that
  // guards unsaved work was answered with Cancel.
  const unused = !running && result !== null && result.runId !== adoptedRunId;

  return (
    <>
      <div className="asrbar">
        <label className="bar__label" htmlFor={modelFieldId}>
          {en.asr.modelLabel}
        </label>
        <select
          id={modelFieldId}
          className="asrbar__model"
          value={modelId}
          disabled={running}
          onChange={(event) => transcription.selectModel(event.target.value)}
        >
          {models.map((model) => (
            <option key={model.id} value={model.id}>
              {modelOptionLabel(model, model.id === damagedModelId)}
            </option>
          ))}
        </select>
        {canDownload && (
          <button
            className="asrbar__download"
            type="button"
            onClick={() => void transcription.downloadModel(modelId)}
          >
            {en.asr.download}
          </button>
        )}
        {download !== null && (
          <button
            className="asrbar__download-cancel"
            type="button"
            onClick={() => void transcription.cancelDownload(download.id)}
          >
            {en.asr.cancelDownload}
          </button>
        )}
        <label className="asrbar__gpu-label">
          <input
            className="asrbar__gpu"
            type="checkbox"
            checked={useGpu}
            disabled={running}
            onChange={(event) => transcription.setUseGpu(event.target.checked)}
          />
          {en.asr.gpuLabel}
        </label>
        <button
          className="asrbar__start"
          type="button"
          disabled={!canStart}
          onClick={() => mediaPath !== null && void transcription.start(mediaPath)}
        >
          {en.asr.start}
        </button>
        {running && (
          <button
            className="asrbar__cancel"
            type="button"
            onClick={() => void transcription.cancel()}
          >
            {en.asr.cancel}
          </button>
        )}
        {unused && (
          <button className="asrbar__use" type="button" onClick={() => onUse(result.runId)}>
            {en.asr.use}
          </button>
        )}
      </div>
      <p className="asrbar__status">
        <span className="asrbar__phase">{status}</span>
        {!running && result !== null && (
          <span className="asrbar__backend">{backendName(result.backend)}</span>
        )}
        {!running && result?.fellBackToCpu === true && <span>{en.asr.fellBackToCpu}</span>}
        {download !== null && (
          <span className="asrbar__download-status">
            {downloadLine(download.id, download.receivedBytes, download.totalBytes)}
          </span>
        )}
      </p>
      {(running || download !== null) && (
        <progress
          className={download === null ? "asrbar__progress" : "asrbar__download-progress"}
          max={100}
          value={progress}
        />
      )}
      {error !== null && (
        <p className="asrbar__error" role="alert">
          {asrErrorMessage(error)}
        </p>
      )}
      {result !== null && result.cues.length > 0 && (
        <ol className="asrbar__cues">
          {result.cues.map((cue, index) => (
            <li
              className="asrbar__cue"
              key={`${cue.startMs}-${index}`}
              data-start={cue.startMs}
              data-end={cue.endMs}
            >
              <span className="asrbar__cue-time">{timestamp(cue.startMs)}</span>
              <span className="asrbar__cue-text">{cue.lines.join(" ")}</span>
            </li>
          ))}
        </ol>
      )}
    </>
  );
}
