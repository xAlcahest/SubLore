/** The project IPC contract. Mirrors src-tauri/src/project; changing either side means changing both. */

/** What an attached file is to the episode. Same spelling as the database column. */
export type FileRole = "media" | "source" | "target";

export type EpisodeFileView = {
  id: number;
  role: FileRole;
  path: string;
  /** Null when the size could not be read when the file was attached. */
  byteLength: number | null;
  /** There is no file at `path` any more. Read when the view is built (decision 24, D3). */
  missing: boolean;
};

export type EpisodeView = {
  id: number;
  /** Position in the series, 1-based. */
  ordinal: number;
  title: string;
  files: EpisodeFileView[];
};

export type ProjectView = {
  folder: string;
  title: string;
  schemaVersion: number;
  episodes: EpisodeView[];
};

/**
 * What was open when Sublore last ran (decision 24, D5). `recent` is what File > Recent projects
 * draws; the rail reads only the project to reopen and the episode to re-select.
 */
export type ProjectSession = {
  folder: string | null;
  episodeId: number | null;
  recent: string[];
};

export type ProjectDeletedView = {
  folder: string;
  /** Sublore's own files that were removed. */
  removed: string[];
  /** Sublore's own names that were there and were left alone. */
  kept: string[];
};

export type ProjectErrorCode =
  | "invalidPath"
  | "folderNotFound"
  | "notADirectory"
  | "alreadyAProject"
  | "noProjectHere"
  | "notASubloreProject"
  | "databaseCorrupt"
  | "schemaTooNew"
  | "migrationFailed"
  | "pathNotAbsolute"
  | "pathNotUtf8"
  | "fileNotFound"
  | "notAFile"
  | "duplicateFile"
  | "episodeNotFound"
  | "fileNotAttached"
  | "noProjectOpen"
  | "writeFailed"
  | "deleteFailed"
  | "permissionDenied"
  | "queryFailed"
  | "commandFailed";

export type ProjectError = {
  code: ProjectErrorCode;
  /** Both set exactly when the code is `schemaTooNew`. */
  found: number | null;
  supported: number | null;
  /** Technical, not user-facing, may be empty. */
  detail: string;
};

const ERROR_CODES: ReadonlySet<string> = new Set<ProjectErrorCode>([
  "invalidPath",
  "folderNotFound",
  "notADirectory",
  "alreadyAProject",
  "noProjectHere",
  "notASubloreProject",
  "databaseCorrupt",
  "schemaTooNew",
  "migrationFailed",
  "pathNotAbsolute",
  "pathNotUtf8",
  "fileNotFound",
  "notAFile",
  "duplicateFile",
  "episodeNotFound",
  "fileNotAttached",
  "noProjectOpen",
  "writeFailed",
  "deleteFailed",
  "permissionDenied",
  "queryFailed",
  "commandFailed",
]);

/** Commands reject with a ProjectError object, but a thrown value is never trusted on sight. */
export function isProjectError(value: unknown): value is ProjectError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { code?: unknown; found?: unknown; supported?: unknown };
  if (typeof candidate.code !== "string" || !ERROR_CODES.has(candidate.code)) {
    return false;
  }
  // The version sentence is built from these two, so a payload that disagrees is not one of ours.
  return (
    (candidate.found === null || typeof candidate.found === "number") &&
    (candidate.supported === null || typeof candidate.supported === "number")
  );
}
