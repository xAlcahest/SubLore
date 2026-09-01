import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { en } from "../i18n/en";
import { fill } from "../i18n/format";
import {
  isAsrError,
  type AsrCompute,
  type AsrDone,
  type AsrErrorCode,
  type AsrModelProgress,
  type AsrModelState,
  type AsrModelStatus,
  type AsrPhase,
  type AsrProgress,
  type AsrRunFailed,
  type AsrRunStarted,
} from "../types/asr";

/** Typed so that adding a code, a model state or a backend without a string is a compile error. */
const errorMessages: Record<AsrErrorCode, string> = en.asr.errors;
const modelStateNames: Record<AsrModelState, string> = en.asr.modelStates;
const backendNames: Record<AsrCompute, string> = en.asr.backends;

/**
 * Outcomes that are not faults: the user asked for one, and the other is what silence sounds like.
 * They go on the status line, never in an error banner.
 */
const NOTICES: ReadonlySet<AsrErrorCode> = new Set<AsrErrorCode>(["cancelled", "emptyTranscript"]);

const MEGABYTE = 1024 * 1024;

export function asrErrorMessage(code: AsrErrorCode): string {
  return errorMessages[code];
}

/**
 * `tiny.en · 74 MB · ready`. `damaged` is for a model a run refused on its checksum: the listing
 * only reads lengths, so without it the row would keep saying `ready` under the error banner.
 */
export function modelOptionLabel(model: AsrModelStatus, damaged = false): string {
  return fill(en.asr.modelOption, {
    id: model.id,
    size: Math.round(model.bytes / MEGABYTE),
    state: modelStateNames[damaged ? "corrupt" : model.state],
  });
}

export function backendName(backend: AsrCompute): string {
  return backendNames[backend];
}

/** How far a run has got, in the words the user reads. */
export function runStatusLine(phase: AsrPhase | null, percent: number): string {
  if (phase === "extracting") {
    return en.asr.extracting;
  }
  return fill(en.asr.transcribing, { percent });
}

export function cueCountLine(count: number): string {
  return fill(count === 1 ? en.asr.cues.one : en.asr.cues.other, { count });
}

export function downloadLine(id: string, receivedBytes: number, totalBytes: number): string {
  const percent = totalBytes === 0 ? 0 : Math.floor((receivedBytes / totalBytes) * 100);
  return fill(en.asr.downloading, { id, percent });
}

function toErrorCode(failure: unknown): AsrErrorCode {
  return isAsrError(failure) ? failure.code : "commandFailed";
}

export type ModelDownload = { id: string; receivedBytes: number; totalBytes: number };

export type Transcription = {
  models: AsrModelStatus[];
  modelId: string;
  useGpu: boolean;
  running: boolean;
  phase: AsrPhase | null;
  percent: number;
  result: AsrDone | null;
  /** An outcome the user asked for, or silence. Not a fault. */
  notice: AsrErrorCode | null;
  error: AsrErrorCode | null;
  /** The model a run refused because its bytes do not hash to the catalog, or null. */
  damagedModelId: string | null;
  download: ModelDownload | null;
  selectModel: (id: string) => void;
  setUseGpu: (useGpu: boolean) => void;
  start: (media: string) => Promise<void>;
  cancel: () => Promise<void>;
  downloadModel: (id: string) => Promise<void>;
  cancelDownload: (id: string) => Promise<void>;
};

/**
 * `adopt` is called with the run id the moment a run finishes: a finished transcription becomes the
 * open document, and the document is the subtitle session's to hold, not this hook's. See
 * BACKLOG.md M3.5.
 */
export function useTranscription(adopt: (runId: number) => void): Transcription {
  const [models, setModels] = useState<AsrModelStatus[]>([]);
  const [modelId, setModelId] = useState("");
  const [useGpu, setUseGpu] = useState(true);
  const [running, setRunning] = useState(false);
  const [runId, setRunId] = useState<number | null>(null);
  const [phase, setPhase] = useState<AsrPhase | null>(null);
  const [percent, setPercent] = useState(0);
  const [result, setResult] = useState<AsrDone | null>(null);
  const [notice, setNotice] = useState<AsrErrorCode | null>(null);
  const [error, setError] = useState<AsrErrorCode | null>(null);
  const [damagedModelId, setDamagedModelId] = useState<string | null>(null);
  const [download, setDownload] = useState<ModelDownload | null>(null);

  /** The run we know we started, and the last one that reached an end. See `mine` below. */
  const startedRef = useRef<number | null>(null);
  const finishedRef = useRef<number | null>(null);
  /** Read through a ref: the listeners below are installed once, and reinstalling them because the
   * caller handed over a new function would drop the events that arrive in between. */
  const adoptRef = useRef(adopt);
  useEffect(() => {
    adoptRef.current = adopt;
  });

  const refreshModels = useCallback(async () => {
    try {
      const listed = await invoke<AsrModelStatus[]>("asr_models");
      setModels(listed);
      // Preselect what is already on disk, and never overrule a choice the user has made.
      setModelId((current) => {
        if (current !== "") {
          return current;
        }
        return (listed.find((model) => model.state === "ready") ?? listed[0])?.id ?? "";
      });
    } catch (failure) {
      setError(toErrorCode(failure));
    }
  }, []);

  useEffect(() => {
    void refreshModels();
  }, [refreshModels]);

  useEffect(() => {
    /**
     * A run can end before the promise that carries its id has resolved, so an event that arrives
     * while no id is known is ours: only one run exists at a time, and `start` clears the id.
     */
    const mine = (id: number) => startedRef.current === null || startedRef.current === id;

    const listeners = Promise.all([
      listen<AsrProgress>("asr://progress", (event) => {
        if (!mine(event.payload.runId)) {
          return;
        }
        setPhase(event.payload.phase);
        setPercent(event.payload.percent);
      }),
      listen<AsrDone>("asr://done", (event) => {
        if (!mine(event.payload.runId)) {
          return;
        }
        finishedRef.current = event.payload.runId;
        setRunning(false);
        setPercent(100);
        setResult(event.payload);
        adoptRef.current(event.payload.runId);
      }),
      listen<AsrRunFailed>("asr://error", (event) => {
        if (!mine(event.payload.runId)) {
          return;
        }
        finishedRef.current = event.payload.runId;
        setRunning(false);
        const code = event.payload.code;
        if (NOTICES.has(code)) {
          setNotice(code);
        } else {
          setError(code);
        }
      }),
      listen<AsrModelProgress>("asr://model-progress", (event) => {
        setDownload({
          id: event.payload.id,
          receivedBytes: event.payload.receivedBytes,
          totalBytes: event.payload.totalBytes,
        });
      }),
    ]);

    return () => {
      void listeners.then((unlisteners) => {
        for (const unlisten of unlisteners) {
          unlisten();
        }
      });
    };
  }, []);

  const start = useCallback(
    async (media: string) => {
      startedRef.current = null;
      finishedRef.current = null;
      setRunId(null);
      setResult(null);
      setNotice(null);
      setError(null);
      setDamagedModelId(null);
      setPhase(null);
      setPercent(0);
      setRunning(true);
      try {
        const started = await invoke<AsrRunStarted>("asr_transcribe_start", {
          media,
          modelId,
          compute: useGpu ? "gpu" : "cpu",
        });
        startedRef.current = started.runId;
        // It may already be over: the outcome event does not wait for this promise.
        if (finishedRef.current !== started.runId) {
          setRunId(started.runId);
        }
      } catch (failure) {
        setRunning(false);
        const code = toErrorCode(failure);
        // The model failed its checksum in the preflight, so this one is not startable until it is
        // downloaded again. See BACKLOG.md M3.2.
        if (code === "checksumMismatch") {
          setDamagedModelId(modelId);
        }
        if (NOTICES.has(code)) {
          setNotice(code);
        } else {
          setError(code);
        }
      }
    },
    [modelId, useGpu],
  );

  const cancel = useCallback(async () => {
    if (runId === null) {
      return;
    }
    try {
      await invoke("asr_transcribe_cancel", { runId });
    } catch (failure) {
      setError(toErrorCode(failure));
    }
  }, [runId]);

  const downloadModel = useCallback(
    async (id: string) => {
      setError(null);
      setDownload({ id, receivedBytes: 0, totalBytes: 0 });
      try {
        await invoke("asr_model_download", { id });
        // Only a download that finished replaces a damaged file, so only that clears the flag.
        setDamagedModelId((current) => (current === id ? null : current));
      } catch (failure) {
        const code = toErrorCode(failure);
        if (code !== "cancelled") {
          setError(code);
        }
      } finally {
        setDownload(null);
        await refreshModels();
      }
    },
    [refreshModels],
  );

  const cancelDownload = useCallback(async (id: string) => {
    try {
      await invoke("asr_model_download_cancel", { id });
    } catch (failure) {
      setError(toErrorCode(failure));
    }
  }, []);

  return {
    models,
    modelId,
    useGpu,
    running,
    phase,
    percent,
    result,
    notice,
    error,
    damagedModelId,
    download,
    selectModel: setModelId,
    setUseGpu,
    start,
    cancel,
    downloadModel,
    cancelDownload,
  };
}
