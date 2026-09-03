import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";

/** One of the open media's audio tracks, as `src-tauri/src/video/player.rs` reports it. */
export type AudioTrack = {
  /** mpv's own `aid`, which is what switching sets. */
  id: number;
  ffIndex: number;
  lang: string | null;
  title: string | null;
  playing: boolean;
};

/**
 * The open media's audio tracks, and the one being drawn.
 *
 * Which track counts as current follows the same rule the peak job uses: the one mpv has marked, or
 * the first when it has marked none. mpv does not always have it marked on a loaded machine by the
 * time this is read (BACKLOG N14), and a menu that marks nothing while the panel draws stream one
 * would be telling two different stories about the same file.
 */
export function useAudioTracks(
  path: string | null,
  ready: boolean,
): {
  tracks: AudioTrack[];
  currentId: number | null;
  switchTo: (id: number) => void;
} {
  const [tracks, setTracks] = useState<AudioTrack[]>([]);

  // Ready, not merely open: `video_open` sets the path and says Loading before mpv has the file,
  // and the backend refuses a track list at that point for the reason `loaded_path` gives. Asking
  // then answered nothing, and the path did not change again when the load finished, so the menu
  // stayed empty for the whole session.
  useEffect(() => {
    if (path === null || !ready) {
      setTracks([]);
      return;
    }
    let alive = true;
    void invoke<AudioTrack[]>("audio_tracks")
      .then((listed) => {
        if (alive) {
          setTracks(listed);
        }
      })
      .catch(() => {
        // No tracks to offer is the same shape as a media with none, and the panel's own line
        // already says that. Nothing here is worth a second message.
        if (alive) {
          setTracks([]);
        }
      });
    return () => {
      alive = false;
    };
  }, [path, ready]);

  const switchTo = useCallback((id: number) => {
    void invoke<AudioTrack[]>("audio_switch_track", { id })
      .then(setTracks)
      .catch(() => {
        // The waveform's own failure line covers a switch that could not be peaked; a menu that
        // silently keeps its old mark would be the worse answer, so the list is asked again.
        void invoke<AudioTrack[]>("audio_tracks")
          .then(setTracks)
          .catch(() => undefined);
      });
  }, []);

  const current = tracks.find((track) => track.playing) ?? tracks[0];
  return { tracks, currentId: current?.id ?? null, switchTo };
}
