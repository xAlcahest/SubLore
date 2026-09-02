import { projectDeletedLine, projectErrorMessage } from "../hooks/useProject";
import {
  subtitleErrorDetail,
  subtitleErrorMessage,
  subtitleSavedLine,
  subtitleStatusLine,
} from "../hooks/useSubtitleFile";
import { videoErrorMessage } from "../hooks/useVideoPlayer";
import { en } from "../i18n/en";
import { type ProjectDeletedView, type ProjectError } from "../types/project";
import { type SubtitleError, type SubtitleSaved, type SubtitleSummary } from "../types/subtitle";
import { type VideoErrorCode } from "../types/video";

type StatusBarProps = {
  summary: SubtitleSummary | null;
  dirty: boolean;
  truncated: boolean;
  saved: SubtitleSaved | null;
  savedInPlace: boolean;
  subtitleError: SubtitleError | null;
  videoErrorCode: VideoErrorCode | null;
  projectDeleted: ProjectDeletedView | null;
  projectError: ProjectError | null;
  /** What a command from the menu or the toolbar could not do. See T3. */
  chromeError: string | null;
};

/**
 * The one line across the bottom of the window: what is open and whether it is saved on the left,
 * what just happened and what went wrong on the right. Outside the five regions and never a layer,
 * so it never hides the video surface. See decision 24, A1.
 */
export default function StatusBar({
  summary,
  dirty,
  truncated,
  saved,
  savedInPlace,
  subtitleError,
  videoErrorCode,
  projectDeleted,
  projectError,
  chromeError,
}: StatusBarProps) {
  const detail = subtitleError === null ? null : subtitleErrorDetail(subtitleError);

  return (
    <footer className="statusbar">
      <p className="statusbar__document">
        <span>{summary === null ? en.subtitle.noFile : subtitleStatusLine(summary)}</span>
        {dirty && <span className="statusbar__dirty">{en.subtitle.dirty}</span>}
      </p>
      <div className="statusbar__messages">
        {truncated && <span className="statusbar__truncated">{en.subtitle.truncated}</span>}
        {projectDeleted !== null && (
          <span className="statusbar__project-message">{projectDeletedLine(projectDeleted)}</span>
        )}
        {saved !== null && (
          <span className="statusbar__message">{subtitleSavedLine(saved, savedInPlace)}</span>
        )}
        {projectError !== null && (
          <p className="statusbar__project-error" role="alert">
            {projectErrorMessage(projectError)}
          </p>
        )}
        {videoErrorCode !== null && (
          <p className="statusbar__video-error" role="alert">
            {videoErrorMessage(videoErrorCode)}
          </p>
        )}
        {chromeError !== null && (
          <p className="statusbar__chrome-error" role="alert">
            {chromeError}
          </p>
        )}
        {subtitleError !== null && (
          <p className="statusbar__error" role="alert">
            <span>{subtitleErrorMessage(subtitleError)}</span>
            {detail !== null && <span>{detail}</span>}
          </p>
        )}
      </div>
    </footer>
  );
}
