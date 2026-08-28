/** The transcription IPC contract. Mirrors src-tauri/src/asr; changing either side means both. */

export type AsrModelState = "missing" | "partial" | "ready" | "corrupt";

export type AsrModelStatus = {
  /** What a start or a download sends back, e.g. `tiny.en`. */
  id: string;
  /** The whole file's length, from the catalog. */
  bytes: number;
  state: AsrModelState;
  downloadedBytes: number;
};

/** Which binary to run. "gpu" still lands on the processor when there is no Vulkan build. */
export type AsrCompute = "gpu" | "cpu";

export type AsrPhase = "extracting" | "transcribing";

export type AsrRunStarted = { runId: number };

export type AsrProgress = {
  runId: number;
  phase: AsrPhase;
  percent: number;
};

/** One generated cue. `lines` never holds a line terminator, and is never empty. */
export type AsrCue = {
  startMs: number;
  endMs: number;
  lines: string[];
};

export type AsrDone = {
  runId: number;
  backend: AsrCompute;
  /** The GPU was asked for and not used. The UI says so: a silent fallback would be a lie. */
  fellBackToCpu: boolean;
  audioDurationMs: number;
  cues: AsrCue[];
};

export type AsrModelProgress = {
  id: string;
  receivedBytes: number;
  totalBytes: number;
};

export type AsrErrorCode =
  | "binaryMissing"
  | "binaryUnrunnable"
  | "ffmpegMissing"
  | "mediaUnreadable"
  | "modelMissing"
  | "modelCorrupt"
  | "modelRejected"
  | "noInput"
  | "badArguments"
  | "noOutput"
  | "emptyTranscript"
  | "stalled"
  | "cancelled"
  | "scratchFailed"
  | "internal"
  | "networkFailed"
  | "downloadWriteFailed"
  | "sizeMismatch"
  | "checksumMismatch"
  | "busy"
  | "commandFailed";

export type AsrError = {
  code: AsrErrorCode;
  /** Technical, not user-facing, may be empty. */
  detail: string;
};

/** A failure that belongs to one run, so an event from an older one can be told apart. */
export type AsrRunFailed = AsrError & { runId: number };

const ERROR_CODES: ReadonlySet<string> = new Set<AsrErrorCode>([
  "binaryMissing",
  "binaryUnrunnable",
  "ffmpegMissing",
  "mediaUnreadable",
  "modelMissing",
  "modelCorrupt",
  "modelRejected",
  "noInput",
  "badArguments",
  "noOutput",
  "emptyTranscript",
  "stalled",
  "cancelled",
  "scratchFailed",
  "internal",
  "networkFailed",
  "downloadWriteFailed",
  "sizeMismatch",
  "checksumMismatch",
  "busy",
  "commandFailed",
]);

/** Commands reject with an AsrError object, but a thrown value is never trusted on sight. */
export function isAsrError(value: unknown): value is AsrError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { code?: unknown };
  return typeof candidate.code === "string" && ERROR_CODES.has(candidate.code);
}
