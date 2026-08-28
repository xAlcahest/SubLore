import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { en } from "../i18n/en";
import { fill } from "../i18n/format";
import {
  isSubtitleError,
  type CuePatch,
  type CueRow,
  type SubtitleError,
  type SubtitleErrorCode,
  type SubtitleNewline,
  type SubtitleOpened,
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

export function subtitleSavedLine(saved: SubtitleSaved, inPlace: boolean): string {
  if (inPlace) {
    return saved.backupPath === null
      ? fill(en.subtitle.savedFile, { path: saved.path })
      : fill(en.subtitle.savedFileWithBackup, { path: saved.path, backup: saved.backupPath });
  }
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
  summary: SubtitleSummary | null;
  cues: CueRow[];
  canUndo: boolean;
  canRedo: boolean;
  dirty: boolean;
  truncated: boolean;
  saved: SubtitleSaved | null;
  savedInPlace: boolean;
  error: SubtitleError | null;
  /** Set when an open was refused because the open file has unsaved edits. */
  blockedPath: string | null;
  /** Counts successful opens. The list is keyed on it, so a new file starts at the top. */
  openId: number;
  open: (path: string) => Promise<void>;
  discardAndOpen: () => Promise<void>;
  setText: (cue: number, text: string) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  save: () => Promise<void>;
  saveAs: (destination: string) => Promise<void>;
};

export function useSubtitleFile(): SubtitleFile {
  const [summary, setSummary] = useState<SubtitleSummary | null>(null);
  const [cues, setCues] = useState<CueRow[]>([]);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [truncated, setTruncated] = useState(false);
  const [saved, setSaved] = useState<SubtitleSaved | null>(null);
  const [savedInPlace, setSavedInPlace] = useState(false);
  const [error, setError] = useState<SubtitleError | null>(null);
  const [blockedPath, setBlockedPath] = useState<string | null>(null);
  const [openId, setOpenId] = useState(0);

  /** The revision the backend is at. A ref, not state: every call needs the value it has now. */
  const revision = useRef(0);
  /**
   * Commands run one at a time, in the order they were asked for. A queue rather than a busy flag:
   * the click that saves a file arrives while the blur it caused is still committing the editor,
   * so a control disabled for the duration of that commit would swallow the save.
   */
  const queue = useRef<Promise<void>>(Promise.resolve());

  const serialize = useCallback((work: () => Promise<void>): Promise<void> => {
    const next = queue.current.then(work);
    // Every unit below catches its own failure, so the chain cannot reject; this is the belt.
    queue.current = next.catch(() => undefined);
    return next;
  }, []);

  const applyPatch = useCallback((patch: CuePatch) => {
    revision.current = patch.revision;
    setCues((current) => [
      ...current.slice(0, patch.from),
      ...patch.cues,
      ...current.slice(patch.from + patch.removed),
    ]);
    setSummary((current) => (current === null ? null : { ...current, cueCount: patch.cueCount }));
    setCanUndo(patch.canUndo);
    setCanRedo(patch.canRedo);
    setDirty(patch.dirty);
    setTruncated(patch.truncated);
  }, []);

  const openFile = useCallback(async (path: string) => {
    setError(null);
    setSaved(null);
    try {
      const opened = await invoke<SubtitleOpened>("subtitle_open", { path });
      revision.current = opened.revision;
      setSummary(opened.summary);
      setCues(opened.cues);
      setCanUndo(opened.canUndo);
      setCanRedo(opened.canRedo);
      setDirty(opened.dirty);
      setTruncated(opened.truncated);
      setBlockedPath(null);
      setOpenId((current) => current + 1);
    } catch (failure) {
      const rejected = toSubtitleError(failure);
      // Unsaved work is the one refusal that leaves the current file open: keep it on screen and
      // let the user choose. Anything else means the file on screen did not open.
      if (rejected.code === "unsavedChanges") {
        setBlockedPath(path);
      } else {
        setSummary(null);
        setCues([]);
        setCanUndo(false);
        setCanRedo(false);
        setDirty(false);
        setTruncated(false);
        setBlockedPath(null);
      }
      setError(rejected);
    }
  }, []);

  const open = useCallback(
    (path: string) => serialize(() => openFile(path)),
    [openFile, serialize],
  );

  const discardAndOpen = useCallback(
    () =>
      serialize(async () => {
        if (blockedPath === null) {
          return;
        }
        setError(null);
        try {
          await invoke<void>("subtitle_close", { discard: true });
          setBlockedPath(null);
        } catch (failure) {
          setError(toSubtitleError(failure));
          return;
        }
        await openFile(blockedPath);
      }),
    [blockedPath, openFile, serialize],
  );

  /** Every mutating command has the same shape: send the revision, take back a patch. */
  const command = useCallback(
    (name: string, args: Record<string, unknown>) =>
      serialize(async () => {
        setError(null);
        try {
          applyPatch(await invoke<CuePatch>(name, { revision: revision.current, ...args }));
        } catch (failure) {
          setError(toSubtitleError(failure));
        }
      }),
    [applyPatch, serialize],
  );

  const setText = useCallback(
    (cue: number, text: string) => command("subtitle_set_text", { cue, text }),
    [command],
  );

  const undo = useCallback(() => command("subtitle_undo", {}), [command]);

  const redo = useCallback(() => command("subtitle_redo", {}), [command]);

  const save = useCallback(
    () =>
      serialize(async () => {
        if (summary === null) {
          return;
        }
        setError(null);
        try {
          setSaved(await invoke<SubtitleSaved>("subtitle_save", { revision: revision.current }));
          setSavedInPlace(true);
          // The bytes in hand are the bytes on disk, which is what the backend just recorded.
          setDirty(false);
        } catch (failure) {
          setSaved(null);
          setError(toSubtitleError(failure));
        }
      }),
    [serialize, summary],
  );

  const saveAs = useCallback(
    (destination: string) =>
      serialize(async () => {
        if (summary === null) {
          return;
        }
        setError(null);
        try {
          setSaved(
            await invoke<SubtitleSaved>("subtitle_save_as", {
              revision: revision.current,
              destination,
            }),
          );
          // A copy elsewhere is not this file being saved, so unsaved edits stay unsaved.
          setSavedInPlace(false);
        } catch (failure) {
          setSaved(null);
          setError(toSubtitleError(failure));
        }
      }),
    [serialize, summary],
  );

  return {
    summary,
    cues,
    canUndo,
    canRedo,
    dirty,
    truncated,
    saved,
    savedInPlace,
    error,
    blockedPath,
    openId,
    open,
    discardAndOpen,
    setText,
    undo,
    redo,
    save,
    saveAs,
  };
}
