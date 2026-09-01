import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { en } from "../i18n/en";
import { fill } from "../i18n/format";
import {
  isProjectError,
  type EpisodeFileView,
  type FileRole,
  type ProjectDeletedView,
  type ProjectError,
  type ProjectErrorCode,
  type ProjectView,
} from "../types/project";

/** Typed so that adding a code or a role without a string is a compile error. */
const errorMessages: Record<ProjectErrorCode, string> = en.project.errors;
const roleNames: Record<FileRole, string> = en.project.roles;

export function projectErrorMessage(error: ProjectError): string {
  const message = errorMessages[error.code];
  // Only the version failure carries numbers, and its sentence is the only one that asks for them.
  if (error.found === null || error.supported === null) {
    return message;
  }
  return fill(message, { found: error.found, supported: error.supported });
}

export function projectStatusLine(project: ProjectView): string {
  return fill(en.project.status, { title: project.title, folder: project.folder });
}

export function projectDeletedLine(deleted: ProjectDeletedView): string {
  return fill(en.project.deleted, { folder: deleted.folder });
}

export function episodeLine(ordinal: number, title: string): string {
  return fill(en.project.episode, { ordinal, title });
}

export function fileLine(file: EpisodeFileView): string {
  return fill(en.project.file, { role: roleNames[file.role], path: file.path });
}

/** A rejection from the backend carries a ProjectError; anything else is a broken command. */
function toProjectError(failure: unknown): ProjectError {
  return isProjectError(failure)
    ? failure
    : { code: "commandFailed", found: null, supported: null, detail: String(failure) };
}

export type Project = {
  busy: boolean;
  project: ProjectView | null;
  deleted: ProjectDeletedView | null;
  error: ProjectError | null;
  create: (folder: string) => Promise<void>;
  open: (folder: string) => Promise<void>;
  addEpisode: (title: string) => Promise<void>;
  attachFile: (episodeId: number, role: FileRole, path: string) => Promise<void>;
  remove: () => Promise<void>;
  choosePath: (kind: "project-folder" | "project-file") => Promise<string | null>;
};

export function useProject(): Project {
  const [busy, setBusy] = useState(false);
  const [project, setProject] = useState<ProjectView | null>(null);
  const [deleted, setDeleted] = useState<ProjectDeletedView | null>(null);
  const [error, setError] = useState<ProjectError | null>(null);

  /**
   * Every command that returns a project ends the same way, so the handling lives in one place.
   * `replacesProject` says what the backend does on failure: an open that fails leaves nothing
   * open, while an edit that fails changed nothing, so the view on screen is still the truth.
   */
  const run = useCallback(
    async (command: string, args: Record<string, unknown>, replacesProject: boolean) => {
      setBusy(true);
      setError(null);
      setDeleted(null);
      try {
        setProject(await invoke<ProjectView>(command, args));
      } catch (failure) {
        if (replacesProject) {
          setProject(null);
        }
        setError(toProjectError(failure));
      } finally {
        setBusy(false);
      }
    },
    [],
  );

  const create = useCallback(
    async (folder: string) => run("project_create", { folder }, true),
    [run],
  );

  const open = useCallback(async (folder: string) => run("project_open", { folder }, true), [run]);

  const addEpisode = useCallback(
    async (title: string) => run("project_add_episode", { title }, false),
    [run],
  );

  const attachFile = useCallback(
    async (episodeId: number, role: FileRole, path: string) =>
      run("project_attach_file", { episodeId, role, path }, false),
    [run],
  );

  const remove = useCallback(async () => {
    setBusy(true);
    setError(null);
    // The backend closes and clears the project before it removes a single file, so nothing is
    // open afterwards whether the removal worked or not.
    setProject(null);
    try {
      setDeleted(await invoke<ProjectDeletedView>("project_delete"));
    } catch (failure) {
      setError(toProjectError(failure));
    } finally {
      setBusy(false);
    }
  }, []);

  const choosePath = useCallback(async (kind: "project-folder" | "project-file") => {
    setBusy(true);
    setError(null);
    try {
      // Null means the user cancelled the dialog, which is not a failure.
      return await invoke<string | null>("choose_path", { kind });
    } catch (failure) {
      setError(toProjectError(failure));
      return null;
    } finally {
      setBusy(false);
    }
  }, []);

  return {
    busy,
    project,
    deleted,
    error,
    create,
    open,
    addEpisode,
    attachFile,
    remove,
    choosePath,
  };
}
