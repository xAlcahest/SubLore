import { useId, useState } from "react";

import {
  episodeLine,
  fileLine,
  projectDeletedLine,
  projectErrorMessage,
  projectStatusLine,
} from "../hooks/useProject";
import { en } from "../i18n/en";
import {
  type EpisodeView,
  type FileRole,
  type ProjectDeletedView,
  type ProjectError,
  type ProjectView,
} from "../types/project";

const ROLES: readonly FileRole[] = ["media", "source", "target"];

type ProjectPanelProps = {
  busy: boolean;
  project: ProjectView | null;
  deleted: ProjectDeletedView | null;
  error: ProjectError | null;
  onCreate: (folder: string) => void;
  onOpen: (folder: string) => void;
  onDelete: () => void;
  onAddEpisode: (title: string) => void;
  onAttachFile: (episodeId: number, role: FileRole, path: string) => void;
  onChoosePath: (kind: "project-folder" | "project-file") => Promise<string | null>;
};

/**
 * Create or open a project, see its episodes and their files, add an episode, attach a file.
 * M4.4 is "no editing beyond that": nothing here renames, reorders or detaches.
 */
export default function ProjectPanel({
  busy,
  project,
  deleted,
  error,
  onCreate,
  onOpen,
  onDelete,
  onAddEpisode,
  onAttachFile,
  onChoosePath,
}: ProjectPanelProps) {
  const folderId = useId();
  const episodeId = useId();
  const fileId = useId();
  const roleGroup = useId();
  const [folder, setFolder] = useState("");
  const [episodeTitle, setEpisodeTitle] = useState("");
  const [filePath, setFilePath] = useState("");
  const [role, setRole] = useState<FileRole>("source");
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const episodes = project?.episodes ?? [];
  // The last episode is the one just added, and it is also the sane target after a project change,
  // which is why a selection that no longer exists falls back to it rather than to nothing.
  const selected: EpisodeView | null =
    episodes.find((episode) => episode.id === selectedId) ?? episodes.at(-1) ?? null;

  const trimmedFolder = folder.trim();
  const trimmedEpisode = episodeTitle.trim();
  const trimmedFile = filePath.trim();

  async function choose(kind: "project-folder" | "project-file") {
    const picked = await onChoosePath(kind);
    if (picked === null) {
      return;
    }
    if (kind === "project-folder") {
      setFolder(picked);
    } else {
      setFilePath(picked);
    }
  }

  function addEpisode() {
    onAddEpisode(trimmedEpisode);
    setEpisodeTitle("");
    // Clearing the selection makes the episode that is about to arrive the selected one.
    setSelectedId(null);
  }

  function attachFile() {
    if (selected === null) {
      return;
    }
    onAttachFile(selected.id, role, trimmedFile);
    setFilePath("");
  }

  return (
    <>
      <div className="project">
        <label className="bar__label" htmlFor={folderId}>
          {en.project.pathLabel}
        </label>
        <input
          id={folderId}
          className="project__path"
          type="text"
          value={folder}
          placeholder={en.project.pathPlaceholder}
          onChange={(event) => setFolder(event.target.value)}
        />
        <button
          className="project__choose-folder"
          type="button"
          disabled={busy}
          onClick={() => void choose("project-folder")}
        >
          {en.project.choose}
        </button>
        <button
          className="project__create"
          type="button"
          disabled={trimmedFolder === "" || busy}
          onClick={() => onCreate(trimmedFolder)}
        >
          {en.project.create}
        </button>
        <button
          className="project__open"
          type="button"
          disabled={trimmedFolder === "" || busy}
          onClick={() => onOpen(trimmedFolder)}
        >
          {en.project.open}
        </button>
        {project !== null && (
          <button className="project__delete" type="button" disabled={busy} onClick={onDelete}>
            {en.project.delete}
          </button>
        )}
      </div>

      {project !== null && (
        <div className="project">
          <label className="bar__label" htmlFor={episodeId}>
            {en.project.episodeLabel}
          </label>
          <input
            id={episodeId}
            className="project__new-episode"
            type="text"
            value={episodeTitle}
            placeholder={en.project.episodePlaceholder}
            onChange={(event) => setEpisodeTitle(event.target.value)}
          />
          <button
            className="project__add-episode"
            type="button"
            disabled={trimmedEpisode === "" || busy}
            onClick={addEpisode}
          >
            {en.project.addEpisode}
          </button>

          <label className="bar__label" htmlFor={fileId}>
            {en.project.fileLabel}
          </label>
          <input
            id={fileId}
            className="project__file-path"
            type="text"
            value={filePath}
            placeholder={en.project.filePlaceholder}
            onChange={(event) => setFilePath(event.target.value)}
          />
          <button
            className="project__choose-file"
            type="button"
            disabled={busy}
            onClick={() => void choose("project-file")}
          >
            {en.project.choose}
          </button>
          {ROLES.map((option) => (
            <label className="project__role" key={option}>
              <input
                className={`project__role-${option}`}
                type="radio"
                name={roleGroup}
                value={option}
                checked={role === option}
                onChange={() => setRole(option)}
              />
              {en.project.roles[option]}
            </label>
          ))}
          <button
            className="project__attach"
            type="button"
            disabled={trimmedFile === "" || selected === null || busy}
            onClick={attachFile}
          >
            {en.project.attach}
          </button>
        </div>
      )}

      <p className="project__status">
        <span>{project === null ? en.project.noProject : projectStatusLine(project)}</span>
        {deleted !== null && <span>{projectDeletedLine(deleted)}</span>}
      </p>
      {error !== null && (
        <p className="project__error" role="alert">
          {projectErrorMessage(error)}
        </p>
      )}

      {project !== null && (
        <ul className="project__episodes">
          {episodes.length === 0 && <li className="project__none">{en.project.noEpisodes}</li>}
          {episodes.map((episode) => (
            <li
              key={episode.id}
              className={
                episode.id === selected?.id
                  ? "project__episode project__episode--selected"
                  : "project__episode"
              }
            >
              <button
                className="project__episode-title"
                type="button"
                aria-current={episode.id === selected?.id}
                onClick={() => setSelectedId(episode.id)}
              >
                {episodeLine(episode.ordinal, episode.title)}
              </button>
              {episode.files.length === 0 ? (
                <p className="project__none">{en.project.noFiles}</p>
              ) : (
                <ul className="project__files">
                  {episode.files.map((file) => (
                    <li className="project__file" key={file.id}>
                      {fileLine(file)}
                    </li>
                  ))}
                </ul>
              )}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
