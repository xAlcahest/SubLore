import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { en } from "../i18n/en";
import { fill } from "../i18n/format";
import {
  isSubtitleError,
  type SubtitleError,
  type SubtitleErrorCode,
  type SubtitleNewline,
  type SubtitleReason,
  type SubtitleSaved,
  type SubtitleSummary,
} from "../types/subtitle";

/** Typed so that adding a code, a reason or a line ending without a string is a compile error. */
const errorMessages: Record<SubtitleErrorCode, string> = en.subtitle.errors;
const reasonMessages: Record<SubtitleReason, string> = en.subtitle.reasons;
const newlineNames: Record<SubtitleNewline, string> = en.subtitle.newlines;

/** Between the parts of the status line. Punctuation, not translatable copy. */
const SEPARATOR = " · ";

export function subtitleErrorMessage(error: SubtitleError): string {
  return errorMessages[error.code];
}

/** Where the parser stopped, when it stopped somewhere the user can go and look. */
export function subtitleErrorDetail(error: SubtitleError): string | null {
  if (error.line === null || error.reason === null) {
    return null;
  }
  return fill(en.subtitle.lineDetail, {
    line: error.line,
    reason: reasonMessages[error.reason],
  });
}

/** What the file is, in the order a translator reads it: format, size, shape. */
export function subtitleStatusLine(summary: SubtitleSummary): string {
  const cues = fill(summary.cueCount === 1 ? en.subtitle.cues.one : en.subtitle.cues.other, {
    count: summary.cueCount,
  });
  const parts = [summary.format.toUpperCase(), cues, newlineNames[summary.newline]];
  if (summary.hasBom) {
    parts.push(en.subtitle.bom);
  }
  return parts.join(SEPARATOR);
}

export function subtitleSavedLine(saved: SubtitleSaved): string {
  return saved.backupPath === null
    ? fill(en.subtitle.saved, { path: saved.path })
    : fill(en.subtitle.savedWithBackup, { path: saved.path, backup: saved.backupPath });
}

/** A rejection from the backend carries a SubtitleError; anything else is a broken command. */
function toSubtitleError(failure: unknown): SubtitleError {
  return isSubtitleError(failure)
    ? failure
    : { code: "commandFailed", line: null, reason: null, detail: String(failure) };
}

export type SubtitleFile = {
  busy: boolean;
  summary: SubtitleSummary | null;
  saved: SubtitleSaved | null;
  error: SubtitleError | null;
  open: (path: string) => Promise<void>;
  saveAs: (destination: string) => Promise<void>;
};

export function useSubtitleFile(): SubtitleFile {
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<SubtitleSummary | null>(null);
  const [saved, setSaved] = useState<SubtitleSaved | null>(null);
  const [error, setError] = useState<SubtitleError | null>(null);

  const open = useCallback(async (path: string) => {
    setBusy(true);
    setError(null);
    setSaved(null);
    try {
      setSummary(await invoke<SubtitleSummary>("subtitle_open", { path }));
    } catch (failure) {
      // A file that did not open is not the file on screen, so the old summary goes with it.
      setSummary(null);
      setError(toSubtitleError(failure));
    } finally {
      setBusy(false);
    }
  }, []);

  const saveAs = useCallback(
    async (destination: string) => {
      // Nothing open means nothing to write: the button is disabled, and this is the same rule.
      const source = summary?.path;
      if (source === undefined) {
        return;
      }
      setBusy(true);
      setError(null);
      try {
        setSaved(await invoke<SubtitleSaved>("subtitle_save_as", { source, destination }));
      } catch (failure) {
        setSaved(null);
        setError(toSubtitleError(failure));
      } finally {
        setBusy(false);
      }
    },
    [summary],
  );

  return { busy, summary, saved, error, open, saveAs };
}
