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
  /** The transcription run whose cues are the open document, or null. See BACKLOG.md M3.5. */
  adoptedRunId: number | null;
  open: (path: string) => Promise<void>;
  discardAndOpen: () => Promise<void>;
  adoptTranscription: (runId: number) => Promise<void>;
  setText: (cue: number, text: string) => Promise<void>;
  /** Many cues in one call, and so one undo step whatever the count. See find-replace-tasks F1. */
  setTexts: (edits: { cue: number; text: string }[]) => Promise<void>;
  setTimes: (cue: number, startMs: number, endMs: number) => Promise<void>;
  /** `before === cues.length` appends; the four below carry the backend's own argument names. */
  insertCue: (before: number, startMs: number, endMs: number, text: string) => Promise<void>;
  deleteCue: (cue: number) => Promise<void>;
  /** `textOffset` counts UTF-8 bytes into the cue's text, which is what the backend splits on. */
  splitCue: (cue: number, textOffset: number, atMs: number) => Promise<void>;
  /** Joins `cue` with the one after it, so the last row has nothing to merge with. */
  mergeCue: (cue: number) => Promise<void>;
  /**
   * A contributed item was activated, in the module that contributed it.
   *
   * Here rather than beside the other module calls because a module may edit: the revision it is
   * given and the patches it sends back are this hook's, and one owner for both is what keeps them
   * from disagreeing. See docs/module-abi.md section 4.5.
   */
  invokeModule: (module: number, item: number, cue: number | null) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  save: () => Promise<void>;
  saveAs: (destination: string) => Promise<void>;
};

/** Told after every patch that changed the row count, so the cursor and the selection follow. */
export type RowsMoved = (at: number, removed: number, inserted: number) => void;

/**
 * What one activation of a contributed item did. Mirrors `modules::InvokeOutcome`.
 *
 * The two halves are independent: a module that edited and then refused did both, and the code is
 * the module's own rather than one the core translates.
 */
type ModuleOutcome = { code: number; patches: CuePatch[] };

export function useSubtitleFile(onRowsMoved: RowsMoved): SubtitleFile {
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
  const [adoptedRunId, setAdoptedRunId] = useState<number | null>(null);

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

  const applyPatch = useCallback(
    (patch: CuePatch) => {
      revision.current = patch.revision;
      // Before the rows change, so whoever indexes by row is told once per patch and cannot be
      // forgotten by a caller: every command that changes the count comes through here.
      onRowsMoved(patch.from, patch.removed, patch.cues.length);
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
    },
    [onRowsMoved],
  );

  /** Take a document the backend has just made the open one, whichever route opened it. */
  const applyOpened = useCallback((opened: SubtitleOpened) => {
    revision.current = opened.revision;
    setSummary(opened.summary);
    setCues(opened.cues);
    setCanUndo(opened.canUndo);
    setCanRedo(opened.canRedo);
    setDirty(opened.dirty);
    setTruncated(opened.truncated);
    setBlockedPath(null);
    setOpenId((current) => current + 1);
  }, []);

  const openFile = useCallback(
    async (path: string) => {
      setError(null);
      setSaved(null);
      try {
        applyOpened(await invoke<SubtitleOpened>("subtitle_open", { path }));
        setAdoptedRunId(null);
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
          setAdoptedRunId(null);
        }
        setError(rejected);
      }
    },
    [applyOpened],
  );

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

  /**
   * Make a finished transcription's cues the open document. The backend asks about unsaved work
   * itself, in the native dialog the close gate uses, and answers with the document it opened or
   * with nothing when the user cancelled. See BACKLOG.md M3.5.
   */
  const adoptTranscription = useCallback(
    (runId: number) =>
      serialize(async () => {
        setError(null);
        setSaved(null);
        try {
          const opened = await invoke<SubtitleOpened | null>("subtitle_adopt_transcription", {
            runId,
          });
          // Cancelled: the document on screen and the result in the bar both stay as they were.
          if (opened === null) {
            return;
          }
          applyOpened(opened);
          setAdoptedRunId(runId);
        } catch (failure) {
          setError(toSubtitleError(failure));
        }
      }),
    [applyOpened, serialize],
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

  /**
   * Carry an activation into a module, and take back whatever it changed.
   *
   * Not `command`: that one applies exactly one patch and treats anything else as a failure, and a
   * module may change nothing, one cue, or several before it answers.
   */
  const invokeModule = useCallback(
    (module: number, item: number, cue: number | null) =>
      serialize(async () => {
        setError(null);
        try {
          const outcome = await invoke<ModuleOutcome>("module_invoke", {
            module,
            item,
            at: {
              revision: revision.current,
              cue,
              // Both are a panel's, and panels are not built. Section 4.1 says `row` is only
              // meaningful when `panelId` is not zero, so zero here says there is no row.
              row: 0,
              panelId: 0,
              // Nothing to carry yet: a module keys its own storage on this, storage is not built,
              // and `ProjectView` has no id to give. See docs/module-host-tasks.md H6.
              projectKey: 0,
            },
          });
          // Applied before the code is looked at. A module that changed rows and then refused
          // changed them, and the grid has to draw the document the session holds.
          for (const patch of outcome.patches) {
            applyPatch(patch);
          }
          if (outcome.code !== 0) {
            // The core does not know what the module was doing, so it has no sentence for this.
            // The Rust side names the module, the item and the code in the log.
            console.warn("a module refused its own activation", item, outcome.code);
          }
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

  const setTexts = useCallback(
    (edits: { cue: number; text: string }[]) => command("subtitle_set_texts", { edits }),
    [command],
  );

  const setTimes = useCallback(
    (cue: number, startMs: number, endMs: number) =>
      command("subtitle_set_times", { cue, startMs, endMs }),
    [command],
  );

  const insertCue = useCallback(
    (before: number, startMs: number, endMs: number, text: string) =>
      command("subtitle_insert", { before, startMs, endMs, text }),
    [command],
  );

  const deleteCue = useCallback((cue: number) => command("subtitle_delete", { cue }), [command]);

  const splitCue = useCallback(
    (cue: number, textOffset: number, atMs: number) =>
      command("subtitle_split", { cue, textOffset, atMs }),
    [command],
  );

  const mergeCue = useCallback((cue: number) => command("subtitle_merge", { cue }), [command]);

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
          const written = await invoke<SubtitleSaved>("subtitle_save", {
            revision: revision.current,
          });
          setSaved(written);
          setSavedInPlace(true);
          // What the write left behind, as the backend recorded it.
          setDirty(written.dirty);
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
        // A document that has never had a file is not writing a copy: this write is its first save,
        // so it adopts the path and points there from now on (decision 24, B2).
        const adopting = summary.path === null;
        setError(null);
        try {
          const written = await invoke<SubtitleSaved>("subtitle_save_as", {
            revision: revision.current,
            destination,
          });
          setSaved(written);
          setSavedInPlace(adopting);
          if (adopting) {
            setSummary((current) => (current === null ? null : { ...current, path: written.path }));
          }
          // A copy elsewhere leaves a file of its own unsaved, and a document that had none saved.
          setDirty(written.dirty);
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
    adoptedRunId,
    open,
    discardAndOpen,
    adoptTranscription,
    setText,
    setTexts,
    setTimes,
    insertCue,
    deleteCue,
    splitCue,
    mergeCue,
    undo,
    redo,
    save,
    saveAs,
    invokeModule,
  };
}
