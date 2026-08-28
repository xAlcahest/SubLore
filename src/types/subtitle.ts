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
