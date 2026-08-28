/** The subtitle IPC contract. Mirrors src-tauri/src/subtitle; changing either side means changing both. */

export type SubtitleFormatName = "srt" | "vtt" | "ass";

export type SubtitleNewline = "lf" | "crlf" | "mixed" | "none";

export type SubtitleSummary = {
  path: string;
  format: SubtitleFormatName;
  /** Cues a player would draw; ASS `Comment:` events are not among them. */
  cueCount: number;
  hasBom: boolean;
  newline: SubtitleNewline;
  byteLength: number;
};

/** One row of the cue list. Its index is its position in the array, never a field of its own. */
export type CueRow = {
  startMs: number;
  endMs: number;
  /** Line breaks are always "\n" here, whatever the file uses. */
  text: string;
  /** An ASS `Comment:` event: listed and editable, but not a line a player draws. */
  comment: boolean;
  /** The cue's own number, when the file wrote one. Never renumbered. */
  number: number | null;
};

export type SubtitleOpened = {
  summary: SubtitleSummary;
  revision: number;
  /** Every cue, ASS comments included, unlike `summary.cueCount`. */
  cues: CueRow[];
  canUndo: boolean;
  canRedo: boolean;
  dirty: boolean;
  truncated: boolean;
};

/** One contiguous run of rows replaced by another, and the state that changed with it. */
export type CuePatch = {
  revision: number;
  from: number;
  removed: number;
  cues: CueRow[];
  /** For the status line: ASS `Comment:` events excluded. */
  cueCount: number;
  canUndo: boolean;
  canRedo: boolean;
  dirty: boolean;
  truncated: boolean;
};

export type SubtitleSaved = {
  path: string;
  bytesWritten: number;
  /** Null when the destination did not exist before. */
  backupPath: string | null;
};

export type SubtitleErrorCode =
  | "invalidPath"
  | "notAFile"
  | "tooLarge"
  | "readFailed"
  | "unsupportedEncoding"
  | "unknownFormat"
  | "parseFailed"
  | "writeFailed"
  | "backupFailed"
  | "permissionDenied"
  | "noDocument"
  | "staleRevision"
  | "invalidCue"
  | "unwritableText"
  | "editRefused"
  | "unsavedChanges"
  | "commandFailed";

/** Why a parse stopped. Sent only with `parseFailed`, always together with a line number. */
export type SubtitleReason =
  | "expectedTiming"
  | "badTimecode"
  | "timecodeOutOfRange"
  | "missingVttHeader"
  | "missingFormatLine"
  | "missingTimingFields"
  | "fieldCountMismatch"
  | "badSectionHeader"
  | "unexpectedEndOfFile";

export type SubtitleError = {
  code: SubtitleErrorCode;
  /** 1-based, null unless `reason` is set too. */
  line: number | null;
  reason: SubtitleReason | null;
  /** Technical, not user-facing, may be empty. */
  detail: string;
};

const ERROR_CODES: ReadonlySet<string> = new Set<SubtitleErrorCode>([
  "invalidPath",
  "notAFile",
  "tooLarge",
  "readFailed",
  "unsupportedEncoding",
  "unknownFormat",
  "parseFailed",
  "writeFailed",
  "backupFailed",
  "permissionDenied",
  "noDocument",
  "staleRevision",
  "invalidCue",
  "unwritableText",
  "editRefused",
  "unsavedChanges",
  "commandFailed",
]);

const REASONS: ReadonlySet<string> = new Set<SubtitleReason>([
  "expectedTiming",
  "badTimecode",
  "timecodeOutOfRange",
  "missingVttHeader",
  "missingFormatLine",
  "missingTimingFields",
  "fieldCountMismatch",
  "badSectionHeader",
  "unexpectedEndOfFile",
]);

/** Commands reject with a SubtitleError object, but a thrown value is never trusted on sight. */
export function isSubtitleError(value: unknown): value is SubtitleError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { code?: unknown; line?: unknown; reason?: unknown };
  if (typeof candidate.code !== "string" || !ERROR_CODES.has(candidate.code)) {
    return false;
  }
  // The error line is rendered from these two, so a payload that disagrees is not one of ours.
  return (
    (candidate.line === null || typeof candidate.line === "number") &&
    (candidate.reason === null ||
      (typeof candidate.reason === "string" && REASONS.has(candidate.reason)))
  );
}
