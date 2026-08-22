import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { en } from "../i18n/en";
import {
  isVideoError,
  type VideoError,
  type VideoErrorCode,
  type VideoOpened,
  type VideoPlayerState,
  type VideoPositionEvent,
  type VideoRegion,
} from "../types/video";

/** Typed so that adding a VideoErrorCode without a string is a compile error. */
const errorMessages: Record<VideoErrorCode, string> = en.video.errors;

export function videoErrorMessage(code: VideoErrorCode): string {
  return errorMessages[code];
}

const IDLE_STATE: VideoPlayerState = {
  status: "idle",
  path: null,
  duration: null,
  paused: true,
};

/** A rejection from the backend carries a VideoError; anything else is a broken command. */
function toErrorCode(error: unknown): VideoErrorCode {
  return isVideoError(error) ? error.code : "commandFailed";
}

export type VideoPlayer = {
  state: VideoPlayerState;
  position: number;
  errorCode: VideoErrorCode | null;
  open: (path: string) => Promise<void>;
  togglePlayback: () => Promise<void>;
  seek: (position: number) => Promise<void>;
  setRegion: (region: VideoRegion) => void;
};

export function useVideoPlayer(): VideoPlayer {
  const [state, setState] = useState<VideoPlayerState>(IDLE_STATE);
  const [position, setPosition] = useState(0);
  const [errorCode, setErrorCode] = useState<VideoErrorCode | null>(null);

  useEffect(() => {
    // The backend never assumes the frontend is listening: the idle state above is the default.
    const listeners = Promise.all([
      listen<VideoPlayerState>("video://state", (event) => {
        setState(event.payload);
      }),
      listen<VideoPositionEvent>("video://position", (event) => {
        setPosition(event.payload.position);
      }),
      listen<VideoError>("video://error", (event) => {
        setErrorCode(event.payload.code);
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

  const open = useCallback(async (path: string) => {
    setErrorCode(null);
    try {
      const opened = await invoke<VideoOpened>("video_open", { path });
      setPosition(0);
      setState({
        status: "ready",
        path: opened.path,
        duration: opened.duration,
        paused: true,
      });
    } catch (error) {
      setErrorCode(toErrorCode(error));
    }
  }, []);

  const togglePlayback = useCallback(async () => {
    // Store the value we asked for, never a flip: a video://state event may land first and a
    // relative toggle would then undo it.
    const paused = !state.paused;
    setErrorCode(null);
    try {
      await invoke(paused ? "video_pause" : "video_play");
      setState((current) => ({ ...current, paused }));
    } catch (error) {
      setErrorCode(toErrorCode(error));
    }
  }, [state.paused]);

  const seek = useCallback(async (target: number) => {
    setErrorCode(null);
    setPosition(target);
    try {
      await invoke("video_seek", { position: target });
    } catch (error) {
      setErrorCode(toErrorCode(error));
    }
  }, []);

  const setRegion = useCallback((region: VideoRegion) => {
    // Fire and forget: a region update the backend rejects must not block layout.
    void invoke("video_set_region", { region }).catch((error: unknown) => {
      setErrorCode(toErrorCode(error));
    });
  }, []);

  return { state, position, errorCode, open, togglePlayback, seek, setRegion };
}
