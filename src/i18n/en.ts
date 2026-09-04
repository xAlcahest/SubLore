/** English source strings. All user-facing copy lives here, never inline in components. */
export const en = {
  appName: "Sublore",
  /**
   * The menu bar and the toolbar. Every title here is always drawn, greyed when nothing behind it
   * can be used; Timing and Terms are absent because no command of theirs is registered yet, and
   * they arrive with the milestone that writes them (decision 24 A4).
   */
  menu: {
    file: {
      title: "File",
      openSubtitle: "Open subtitle…",
      openVideo: "Open video…",
      save: "Save",
      saveCopy: "Save a copy…",
      discard: "Discard changes",
      quit: "Quit",
    },
    edit: {
      title: "Edit",
      undo: "Undo",
      redo: "Redo",
      find: "Find…",
      replace: "Replace…",
      /** Here until an Audio title of its own arrives with the milestone that registers it. */
      transcribe: "Transcribe…",
    },
    timing: {
      title: "Timing",
      startToPlayhead: "Set start to playhead",
      endToPlayhead: "Set end to playhead",
      toCueStart: "Jump to cue start",
      toCueEnd: "Jump to cue end",
      selectAtPlayhead: "Select cue at playhead",
      /** The 500 ms is in the label on purpose: the key says what it will do before you press it. */
      playLine: "Play line",
      playBefore: "Play 500 ms before line",
      playAfter: "Play 500 ms after line",
      playToEnd: "Play from line start to the end",
      startEarlier: "Start 10 ms earlier",
      startLater: "Start 10 ms later",
      endEarlier: "End 10 ms earlier",
      endLater: "End 10 ms later",
    },
    view: {
      title: "View",
      waveform: "Waveform",
      subtitles: "Subtitles on video",
      /** One of the five interface size radio items (S1). `{percent}` is a whole number. */
      scale: "{percent}%",
    },
    /** The four cue structure edits, interface-spec section 3 order (M2.7 E2, T3 C2). */
    subtitles: {
      title: "Subtitles",
      insert: "Insert cue",
      delete: "Delete cue",
      split: "Split cue",
      merge: "Merge with next",
    },
    audio: {
      title: "Audio",
      /** For a track the file gives neither a title nor a language, numbered as the file lists them. */
      track: "Track",
    },
    help: {
      title: "Help",
      about: "About Sublore",
    },
    /** Drawn beside a menu item. Each key is handled by whichever component owns that command. */
    keys: {
      openSubtitle: "Ctrl+O",
      openVideo: "Ctrl+Shift+O",
      save: "Ctrl+S",
      saveCopy: "Ctrl+Shift+S",
      undo: "Ctrl+Z",
      redo: "Ctrl+Y",
      quit: "Ctrl+Q",
      videoToCueStart: "Ctrl+1",
      videoToCueEnd: "Ctrl+2",
      startToPlayhead: "Ctrl+3",
      endToPlayhead: "Ctrl+4",
      find: "Ctrl+F",
      replace: "Ctrl+H",
    },
    errors: {
      quitFailed: "Sublore could not quit. Close the window instead.",
    },
  },
  /** The find band, in both its modes: replace adds a second field and two buttons to the same row. */
  find: {
    title: "Find",
    replaceTitle: "Find and replace",
    needleLabel: "Find",
    replaceLabel: "Replace with",
    matchCase: "Match case",
    regex: "Regular expression",
    /** The selection, whatever its size: one selected cue restricts too (F4b). */
    inSelection: "Selected cues only",
    findNext: "Find next",
    replace: "Replace",
    replaceAll: "Replace all",
    noMatch: "No match",
    badPattern: "That expression is not one this can read. Nothing was changed.",
    /** A pattern that backtracks for ever. The document is untouched and the window kept answering. */
    tooSlow: "That expression takes too long to run. Nothing was changed.",
    /** `{count}` is a whole number. Drawn after a replace all, so the count is never a guess. */
    replaced: {
      one: "1 replaced",
      other: "{count} replaced",
    },
    close: "Close",
  },
  about: {
    title: "About Sublore",
    tagline: "Translation memory for subtitles.",
    version: "Version {version}",
    licence: "GNU General Public License, version 3 or later.",
    close: "Close",
  },
  /** The draggable edges between the panels (D1). Read aloud where a separator is announced. */
  shell: {
    videoSash: "Video panel width",
    gridSash: "Top block height",
  },

  video: {
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
  project: {
    /** Over the tree, so the rail says what it is listing before anything is open. */
    cap: "Project",
    noProject: "No project open.",
    noEpisodes: "No episodes yet.",
    episodePlaceholder: "Episode name",
    episode: "{ordinal}. {title}",
    noFiles: "No files attached.",
    /** A rail row is narrow, so the row carries the file's name and its tooltip the rest. */
    file: "{role} · {path}",
    missing: "missing",
    roles: {
      media: "Video",
      source: "Source",
      target: "Target",
    },
    /** What right-clicking the rail opens, for anyone reaching it without seeing it. */
    menuLabel: "Project actions",
    menu: {
      createProject: "Create project…",
      openProject: "Open project…",
      closeProject: "Close project",
      deleteProject: "Delete project…",
      addEpisode: "Add episode…",
      attach: "Attach {role}…",
      renameEpisode: "Rename episode…",
      deleteEpisode: "Delete episode…",
      openFile: "Open",
      locateFile: "Locate…",
      detachFile: "Detach",
    },
    /** Every one of these is asked once and answered before anything changes (decision 24, D2). */
    ask: {
      cancel: "Cancel",
      addEpisodeTitle: "Add episode",
      addEpisodeConfirm: "Add",
      renameEpisodeTitle: "Rename episode",
      renameEpisodeConfirm: "Rename",
      closeProjectTitle: "Close project",
      closeProjectMessage: "Close {title}? Nothing on disk is touched.",
      closeProjectConfirm: "Close",
      deleteProjectTitle: "Delete project",
      deleteProjectMessage:
        "Delete the project in {folder}? Sublore removes its own project file there and leaves your video and subtitle files exactly where they are.",
      deleteProjectConfirm: "Delete project",
      deleteEpisodeTitle: "Delete episode",
      deleteEpisodeMessage: "Delete {episode}? The files attached to it stay on disk.",
      deleteEpisodeConfirm: "Delete episode",
      detachFileTitle: "Detach file",
      detachFileMessage: "Detach {name} from {episode}? The file stays on disk.",
      detachFileConfirm: "Detach",
    },
    deleted: "Deleted the project in {folder}. Your own video and subtitle files were not touched.",
    errors: {
      invalidPath: "That is not a folder Sublore can use.",
      folderNotFound: "There is no folder at that path.",
      notADirectory: "That path is not a folder.",
      alreadyAProject: "There is already a Sublore project in that folder. Open it instead.",
      noProjectHere: "There is no Sublore project in that folder.",
      notASubloreProject: "That folder holds a project.sublore file Sublore did not write.",
      databaseCorrupt: "That project file is damaged. Sublore left it exactly as it is.",
      schemaTooNew:
        "That project was made by a newer Sublore, which writes version {found}; this one reads version {supported}. Update Sublore to open it.",
      migrationFailed:
        "Sublore could not bring that project file up to date, so it left it at the version it was.",
      pathNotAbsolute: "That path does not start from the top of the drive.",
      pathNotUtf8: "Sublore cannot store that path. Move the file somewhere with a plainer name.",
      fileNotFound: "There is no file at that path.",
      notAFile: "That path is not a file.",
      duplicateFile: "That file is already attached to this episode.",
      episodeNotFound: "That episode is not in the project any more.",
      fileNotAttached: "That file is not attached to this episode any more.",
      noProjectOpen: "Open a project first.",
      writeFailed: "Sublore could not write to the project file. Check that the disk has room.",
      deleteFailed:
        "Sublore could not remove the project file. Check that the folder is not read-only.",
      permissionDenied: "Sublore is not allowed to use that folder.",
      queryFailed: "Sublore could not read that project file.",
      commandFailed: "Sublore could not finish that action. Restart Sublore if it happens again.",
    },
  },
  waveform: {
    sash: "Waveform height",
    canvas: "Waveform, arrows to scroll, plus and minus to zoom",
    noAudio: "This video has no audio, so there is no waveform to draw.",
    label: "Waveform",
    /** Shown in the status bar when a peak job fails. The detail is technical and stays in the log. */
    failed: "The waveform could not be read for this file. The video is unaffected.",
  },

  preview: {
    /**
     * Shown in the status bar when the open document could not be put on the video frame. It says
     * what is safe as well as what failed: a preview never writes the user's file, so nothing of
     * theirs is at stake here.
     */
    failed:
      "The subtitles could not be shown on the video. Your subtitle file and video are unchanged.",
  },

  subtitle: {
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
      invalidPath: "That is not a subtitle file Sublore can read.",
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
      noPath: "This document has never been saved, so Sublore does not know where to write it.",
      transcriptionGone:
        "Those cues are gone: another transcription has started since. Run it again.",
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
      /** Characters per second, the reading rate of decision 24 A8. */
      cps: "CPS",
      /** Marks an ASS Comment: event, which a player does not draw. */
      comment: "Comment",
    },
    /** The box in the tools column that edits whichever line the cursor is on (T5). */
    currentLine: {
      label: "Current line",
      none: "No line to edit.",
      start: "Start",
      end: "End",
      /** In seconds, which is the scale a line's length is judged against. */
      duration: "Duration",
      cps: "CPS",
      text: "Text",
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
  asr: {
    /** Over the panel the menu opens, which is absent until it is asked for (T4). */
    panelTitle: "Transcription",
    close: "Close",
    modelLabel: "Model",
    /** `{size}` is whole megabytes; the separator is punctuation, not copy. */
    modelOption: "{id} · {size} MB · {state}",
    modelStates: {
      missing: "not downloaded",
      partial: "partly downloaded",
      ready: "ready",
      corrupt: "damaged",
    },
    download: "Download",
    cancelDownload: "Stop",
    downloading: "Downloading {id}… {percent}%",
    gpuLabel: "Use GPU when available",
    start: "Transcribe",
    cancel: "Cancel",
    /** Offered while a finished run's cues are not the open document: how a replacement the user
     * cancelled is asked for again. */
    use: "Use these cues",
    idle: "No transcription yet.",
    extracting: "Extracting audio…",
    transcribing: "Transcribing… {percent}%",
    cues: {
      one: "{count} cue",
      other: "{count} cues",
    },
    backends: {
      gpu: "GPU",
      cpu: "CPU",
    },
    /** Shown when the user asked for the GPU and the run happened on the processor anyway. */
    fellBackToCpu: "No graphics acceleration was available, so the processor did the work.",
    errors: {
      binaryMissing:
        "Sublore cannot find the transcription engine. Run scripts/build-whisper.sh, then restart Sublore.",
      binaryUnrunnable:
        "Sublore found the transcription engine but could not start it. Build it again with scripts/build-whisper.sh.",
      ffmpegMissing:
        "Sublore needs ffmpeg to read audio from a video. Install ffmpeg and try again.",
      mediaUnreadable: "Sublore could not read any audio from that file.",
      modelMissing: "That model is not on this computer yet. Download it first.",
      modelCorrupt: "That model file is damaged. Download it again.",
      modelRejected: "The transcription engine could not load that model. Download it again.",
      noInput: "The transcription engine could not open the audio Sublore extracted.",
      badArguments: "The transcription engine rejected how Sublore called it.",
      noOutput:
        "The transcription engine produced nothing Sublore could read. Try another model, or check the log.",
      emptyTranscript: "No speech was found in this audio.",
      stalled: "The transcription stopped responding, so Sublore ended it.",
      cancelled: "Transcription cancelled.",
      scratchFailed:
        "Sublore could not make room for the audio it extracts. Check the free space on this disk.",
      internal: "Sublore could not finish the transcription. Restart Sublore if it happens again.",
      networkFailed:
        "The download stopped. What arrived is kept, so downloading again carries on from there.",
      downloadWriteFailed:
        "Sublore could not write the model to disk. Check the free space and the permissions.",
      sizeMismatch: "The download was not the size Sublore expects, so it was refused.",
      checksumMismatch:
        "That model file failed its checksum, so Sublore refused it. Download it again.",
      busy: "Sublore is already working on that. Wait for it to finish, or stop it first.",
      commandFailed: "Sublore could not finish that action. Restart Sublore if it happens again.",
    },
  },
} as const;
