/** The video IPC contract. Mirrors src-tauri/src/video; changing either side means changing both. */

export type VideoRegion = {
  /** CSS px, relative to the webview viewport. */
  x: number;
  y: number;
  /** CSS px. Zero in either dimension hides the surface. */
  width: number;
  height: number;
};

export type VideoOpened = {
  path: string;
  /** Seconds, always greater than zero. */
  duration: number;
};

export type VideoErrorCode =
  | "playerUnavailable"
  | "invalidPath"
  | "openFailed"
  | "openTimeout"
  | "notLoaded"
  | "commandFailed"
  | "playbackStopped";

export type VideoError = {
  code: VideoErrorCode;
  /** Technical, not user-facing, may be empty. */
  detail: string;
};

export type VideoPlayerStatus = "idle" | "loading" | "ready";

export type VideoPlayerState = {
  status: VideoPlayerStatus;
  path: string | null;
  duration: number | null;
  paused: boolean;
};

export type VideoPositionEvent = {
  position: number;
};

const ERROR_CODES: ReadonlySet<string> = new Set<VideoErrorCode>([
  "playerUnavailable",
  "invalidPath",
  "openFailed",
  "openTimeout",
  "notLoaded",
  "commandFailed",
  "playbackStopped",
]);

/** Commands reject with a VideoError object, but a thrown value is never trusted on sight. */
export function isVideoError(value: unknown): value is VideoError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { code?: unknown; detail?: unknown };
  return typeof candidate.code === "string" && ERROR_CODES.has(candidate.code);
}
