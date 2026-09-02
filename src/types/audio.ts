/**
 * The waveform IPC contract. Mirrors src-tauri/src/audio/mod.rs; the two change together.
 *
 * A job announces itself, then sends any number of chunks, then exactly one terminal event. Every
 * message carries the job id, because a chunk from a cancelled job can still be in flight when its
 * replacement has already started.
 */

export type AudioJobStarted = {
  jobId: number;
};

export type AudioPeaks = {
  jobId: number;
  /** The millisecond this chunk's first bucket starts at, and so how many came before it. */
  firstMs: number;
  /** One bucket per millisecond: the smallest and largest sample that fell in it. */
  min: number[];
  max: number[];
};

export type AudioDone = {
  jobId: number;
  /** Milliseconds peaked in total, which is the bucket count. */
  buckets: number;
};

/** Every variant of `AudioErrorCode` in src-tauri/src/audio/error.rs, in its order. */
export type AudioErrorCode =
  | "ffmpegMissing"
  | "mediaUnreadable"
  | "stalled"
  | "cancelled"
  | "internal"
  | "busy"
  | "notLoaded"
  | "noSuchTrack"
  | "playerUnavailable"
  | "commandFailed";

export type AudioFailed = {
  jobId: number;
  code: AudioErrorCode;
  detail: string;
};
