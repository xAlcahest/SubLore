/** English source strings. All user-facing copy lives here, never inline in components. */
export const en = {
  appName: "Sublore",
  video: {
    pathLabel: "Video file",
    pathPlaceholder: "Full path to a video file",
    open: "Open",
    play: "Play",
    pause: "Pause",
    position: "Position",
    noFile: "No video open.",
    errors: {
      playerUnavailable: "The video player is not running. Restart Sublore.",
      invalidPath: "That path is empty or is not a file Sublore can read.",
      openFailed:
        "Sublore could not open this video. The file may be unreadable or in a format libmpv does not support.",
      openTimeout: "Opening this video took too long, so Sublore stopped waiting.",
      notLoaded: "Open a video first.",
      commandFailed: "The video player rejected that action.",
      playbackStopped: "Playback stopped before the end of the file.",
    },
  },
} as const;
