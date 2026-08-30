import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

type StartupFiles = {
  video: string | null;
  subtitle: string | null;
};

/**
 * Opens whatever was named on the command line, once, when the app comes up.
 *
 * `sublore episode.mkv episode.srt` opens both, in any order. Beyond convenience, this is how
 * automation reaches the app on a real desktop: synthetic keystrokes go to whichever window holds
 * the X focus, and under a compositor that is not reliably ours (WORKFLOW.md).
 */
export function useStartupFiles(
  openVideo: (path: string) => Promise<unknown>,
  openSubtitle: (path: string) => Promise<unknown>,
): void {
  // Strict mode mounts effects twice in development; opening the same file twice would race the
  // session against itself.
  const started = useRef(false);

  useEffect(() => {
    if (started.current) {
      return;
    }
    started.current = true;

    void (async () => {
      let files: StartupFiles;
      try {
        files = await invoke<StartupFiles>("startup_files_command");
      } catch {
        // Nothing was asked for, or the command is unavailable: the app is perfectly usable
        // without it, so this stays silent rather than showing an error nobody caused.
        return;
      }
      // Video first: the subtitle list is read against a document, not against the media, but a
      // person passing both expects to see the video behind the cues.
      if (files.video !== null) {
        await openVideo(files.video);
      }
      if (files.subtitle !== null) {
        await openSubtitle(files.subtitle);
      }
    })();
  }, [openVideo, openSubtitle]);
}
