import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";

/** One of the open media's audio tracks, as `src-tauri/src/video/player.rs` reports it. */
export type AudioTrack = {
  /** mpv's own `aid`, which is what switching sets. */
  id: number;
  ffIndex: number;
  lang: string | null;
  title: string | null;
  /** mpv's own `selected` flag. Not where the mark comes from: see `currentId` below. */
  playing: boolean;
};

/** What both audio track commands answer: the list, and which track the panel is drawing. */
export type AudioTrackList = {
  tracks: AudioTrack[];
  currentId: number | null;
};

/** No media open, and a media with no audio: neither has a track to mark. */
const NO_TRACKS: AudioTrackList = { tracks: [], currentId: null };

/**
 * The open media's audio tracks, and the one being drawn.
 *
 * Which track that is comes from the backend, which is the side that knows: it is the stream the
 * peak job was started on. Working it out here from mpv's `selected` flag meant the panel followed
 * the track that was asked for while the menu followed a flag mpv had not set, so a switch drew one
 * track and ticked another. See BACKLOG N14.
 */
export function useAudioTracks(
  path: string | null,
  ready: boolean,
): {
  tracks: AudioTrack[];
  currentId: number | null;
  switchTo: (id: number) => void;
} {
  const [list, setList] = useState<AudioTrackList>(NO_TRACKS);

  // Ready, not merely open: `video_open` sets the path and says Loading before mpv has the file,
  // and the backend refuses a track list at that point for the reason `loaded_path` gives. Asking
  // then answered nothing, and the path did not change again when the load finished, so the menu
  // stayed empty for the whole session.
  useEffect(() => {
    if (path === null || !ready) {
      setList(NO_TRACKS);
      return;
    }
    let alive = true;
    void invoke<AudioTrackList>("audio_tracks")
      .then((listed) => {
        if (alive) {
          setList(listed);
        }
      })
      .catch(() => {
        // No tracks to offer is the same shape as a media with none, and the panel's own line
        // already says that. Nothing here is worth a second message.
        if (alive) {
          setList(NO_TRACKS);
        }
      });
    return () => {
      alive = false;
    };
  }, [path, ready]);

  const switchTo = useCallback((id: number) => {
    void invoke<AudioTrackList>("audio_switch_track", { id })
      .then(setList)
      .catch(() => {
        // The waveform's own failure line covers a switch that could not be peaked; a menu that
        // silently keeps its old mark would be the worse answer, so the list is asked again.
        void invoke<AudioTrackList>("audio_tracks")
          .then(setList)
          .catch(() => undefined);
      });
  }, []);

  return { tracks: list.tracks, currentId: list.currentId, switchTo };
}
