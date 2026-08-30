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
      } catch (error) {
        // Only a backend failure reaches here: an empty command line resolves with two nulls rather
        // than rejecting. It goes to the log and no further; the user sees an app that opened nothing.
        console.error("startup files: the command line could not be read", error);
        return;
      }
      // Video first: the subtitle list is read against a document, not against the media, but a
      // person passing both expects to see the video behind the cues.
      if (files.video !== null) {
        await openOne(openVideo, files.video);
      }
      if (files.subtitle !== null) {
        await openOne(openSubtitle, files.subtitle);
      }
    })();
  }, [openVideo, openSubtitle]);
}

/**
 * Open one of the files named on the command line.
 *
 * Both callbacks wired in today put their own failure on screen and resolve, but this hook's type
 * does not promise that: without the guard a rejection from the video would be unhandled and would
 * take the subtitle after it down with it.
 *
 * A rejection that does reach here goes to the log and no further. Saying it to the user needs
 * somewhere to render it, and this hook has no surface of its own (gate 2b, `useStartupFiles.ts:63`).
 */
async function openOne(open: (path: string) => Promise<unknown>, path: string): Promise<void> {
  try {
    await open(path);
  } catch (error) {
    console.error(`startup files: opening ${path} failed`, error);
  }
}
