import { useCallback, useEffect, useRef, useState } from "react";
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
  /** Play a stretch and stop at its end, both in seconds. See docs/play-range-tasks.md. */
  playRange: (from: number, to: number) => Promise<void>;
  setRegion: (region: VideoRegion) => void;
};

/**
 * @param covered whether an HTML layer is open over the page. The surface hides while it is, and
 * the backend derives that from the flag: the frontend never shows or hides it (decision 1, T8).
 */
export function useVideoPlayer(covered: boolean): VideoPlayer {
  const [state, setState] = useState<VideoPlayerState>(IDLE_STATE);
  const [position, setPosition] = useState(0);
  const [errorCode, setErrorCode] = useState<VideoErrorCode | null>(null);
  // The rectangle keeps being measured while a layer is open; it stops being sent, so no `raise`
  // can restack the surface over the layer. It goes back with the uncover. See T8.
  const held = useRef<VideoRegion | null>(null);
  const covering = useRef(covered);
  const transitions = useRef<Promise<void>>(Promise.resolve());

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

  const playRange = useCallback(async (from: number, to: number) => {
    setErrorCode(null);
    // The position is not set here the way `seek` sets it: playback is about to move it anyway,
    // and drawing the start for one frame before the first event would fight the player.
    try {
      await invoke("video_play_range", { from, to });
    } catch (error) {
      setErrorCode(toErrorCode(error));
    }
  }, []);

  const setRegion = useCallback((region: VideoRegion) => {
    held.current = region;
    // Held while a layer is open, and sent again when the last one closes (T8).
    if (covering.current) {
      return;
    }
    // Fire and forget: a region update the backend rejects must not block layout.
    void invoke("video_set_region", { region }).catch((error: unknown) => {
      setErrorCode(toErrorCode(error));
    });
  }, []);

  useEffect(() => {
    covering.current = covered;
    // One at a time and in this order: the held rectangle first, so the frame is placed before it
    // may be shown, and no stale answer can leave the picture hidden with no layer open (T8).
    transitions.current = transitions.current
      .then(async () => {
        const region = held.current;
        if (!covered && region !== null) {
          await invoke("video_set_region", { region });
        }
        await invoke("video_set_layers", { open: covered });
      })
      .catch((error: unknown) => {
        setErrorCode(toErrorCode(error));
      });
  }, [covered]);

  return { state, position, errorCode, open, togglePlayback, seek, playRange, setRegion };
}
