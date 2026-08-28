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
  subtitle: {
    pathLabel: "Subtitle file",
    pathPlaceholder: "Full path to an SRT, VTT or ASS file",
    open: "Open",
    destinationLabel: "Save copy to",
    destinationPlaceholder: "Full path for the copy",
    save: "Save as",
    saveFile: "Save",
    undo: "Undo",
    redo: "Redo",
    discard: "Discard changes",
    /** Appended to the status line while the document differs from the file on disk. */
    dirty: "Unsaved changes",
    /** Shown once the undo bound has dropped its oldest entries. */
    truncated: "Undo history is full, so the oldest edits can no longer be undone.",
    noFile: "No subtitle file open.",
    /** Shown only when the file starts with a UTF-8 byte-order mark. */
    bom: "BOM",
    cues: {
      one: "{count} cue",
      other: "{count} cues",
    },
    newlines: {
      lf: "LF",
      crlf: "CRLF",
      mixed: "Mixed line endings",
      none: "No line endings",
    },
    saved: "Saved a copy to {path}.",
    savedWithBackup: "Saved a copy to {path}. The file that was there is kept at {backup}.",
    savedFile: "Saved {path}.",
    savedFileWithBackup: "Saved {path}. The file that was there is kept at {backup}.",
    lineDetail: "Line {line} — {reason}",
    errors: {
      invalidPath: "Type the full path to a subtitle file.",
      notAFile: "There is no file at that path.",
      tooLarge: "That file is bigger than Sublore opens as a subtitle (16 MB).",
      readFailed: "Sublore could not read that file.",
      unsupportedEncoding:
        "Sublore reads UTF-8 subtitles. Convert this file to UTF-8 and open it again.",
      unknownFormat: "Sublore opens SRT, VTT and ASS subtitles. That file is none of them.",
      parseFailed:
        "Sublore could not read this subtitle file, so it will not open it rather than risk changing it.",
      writeFailed: "Sublore could not write the copy. Check that the folder exists and has room.",
      backupFailed:
        "Sublore could not keep a backup of the existing file, so it did not overwrite it.",
      permissionDenied: "Sublore is not allowed to use that file.",
      noDocument: "Open a subtitle file first.",
      staleRevision:
        "Sublore and this list no longer agree about the file. Open it again before editing.",
      invalidCue: "That line is not in this file any more.",
      unwritableText:
        "This format cannot hold that text. Remove the blank line or the line break and try again.",
      editRefused: "Sublore did not make that change, so the file is exactly as it was.",
      unsavedChanges: "This file has changes that are not saved. Save them, or discard them.",
      commandFailed: "Sublore could not finish that action. Restart Sublore if it happens again.",
    },
    cueList: {
      label: "Cues",
      empty: "This file has no cues.",
      position: "#",
      number: "No.",
      start: "Start",
      end: "End",
      text: "Text",
      /** Marks an ASS Comment: event, which a player does not draw. */
      comment: "Comment",
    },
    reasons: {
      expectedTiming: "a timing line was expected here",
      badTimecode: "a timestamp is not valid",
      timecodeOutOfRange: "a timestamp is past the longest time Sublore can hold",
      missingVttHeader: "the file does not start with WEBVTT",
      missingFormatLine: "an event appears before its section's Format line",
      missingTimingFields: "the Format line declares no Start or End field",
      fieldCountMismatch: "this line has fewer fields than the Format line declares",
      badSectionHeader: "a section header has no closing bracket",
      unexpectedEndOfFile: "the file ends in the middle of a cue",
    },
  },
} as const;
