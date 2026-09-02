import { useEffect, useRef, useState } from "react";

import { choosePath, type ChooseKind } from "./chooser";
import AboutDialog from "./components/AboutDialog";
import CueList from "./components/CueList";
import MenuBar from "./components/MenuBar";
import ProjectRail from "./components/ProjectRail";
import StatusBar from "./components/StatusBar";
import Toolbar from "./components/Toolbar";
import TranscribeBar from "./components/TranscribeBar";
import VideoControls from "./components/VideoControls";
import VideoStage from "./components/VideoStage";
import { useProject } from "./hooks/useProject";
import { useStartupFiles } from "./hooks/useStartupFiles";
import { useSubtitleFile } from "./hooks/useSubtitleFile";
import { useTranscription } from "./hooks/useTranscription";
import { useVideoPlayer } from "./hooks/useVideoPlayer";
import { en } from "./i18n/en";
import { requestQuit } from "./quit";
import { type Command } from "./types/chrome";
import { type EpisodeFileView } from "./types/project";
import "./App.css";

/** The command an accelerator asks for, or null when the chrome does not own that key. */
function acceleratorFor(
  key: string,
  shift: boolean,
  commands: Record<string, Command>,
): Command | null {
  if (key === "o") {
    return shift ? commands.openVideo : commands.openSubtitle;
  }
  if (key === "s" && shift) {
    return commands.saveCopy;
  }
  if (key === "q" && !shift) {
    return commands.quit;
  }
  return null;
}

export default function App() {
  const { state, position, errorCode, open, togglePlayback, seek, setRegion } = useVideoPlayer();
  const subtitle = useSubtitleFile();
  const project = useProject();
  // A finished transcription becomes the open document, and the backend asks about unsaved work on
  // the way there. See BACKLOG.md M3.5.
  const transcription = useTranscription((runId) => void adoptTranscription(runId));
  const ready = state.status === "ready";
  useStartupFiles(open, subtitle.open);
  // Saving writes the document, so it has to include the text sitting in an open editor, and an
  // open editor is unsaved work whether or not it has reached the document yet.
  const flushEditor = useRef<() => Promise<void>>(() => Promise.resolve());
  const [editorOpen, setEditorOpen] = useState(false);
  // The chooser is modal and answers on its own thread, so a second one asked for while it is up
  // would sit behind the first. Every chooser the chrome raises is raised here, so one flag covers
  // them all.
  const [choosing, setChoosing] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [quitError, setQuitError] = useState<string | null>(null);

  async function pick(
    kind: ChooseKind,
    suggested: string | undefined,
    act: (path: string) => void,
  ) {
    if (choosing) {
      return;
    }
    setChoosing(true);
    try {
      const path = await choosePath(kind, suggested);
      // Cancelled is an outcome, not a failure: nothing opens, nothing is written, nothing is said.
      if (path !== null) {
        act(path);
      }
    } finally {
      setChoosing(false);
    }
  }

  /**
   * Save the document where it belongs. One that has never had a file is asked where that is, and
   * points at the path it is given from then on, so the next save writes there (decision 24, B2).
   */
  async function saveDocument() {
    await flushEditor.current();
    if (subtitle.summary === null) {
      return;
    }
    if (subtitle.summary.path === null) {
      await pick("subtitle-first-save", undefined, (path) => void subtitle.saveAs(path));
      return;
    }
    await subtitle.save();
  }

  /** A copy elsewhere, which leaves a document with a file of its own unsaved. */
  async function saveCopy() {
    await flushEditor.current();
    // The chooser opens on the open file's own name, which is what a copy is usually called; a
    // document that has never had a file has no name to offer.
    await pick("subtitle-save", subtitle.summary?.path ?? undefined, (path) => {
      void subtitle.saveAs(path);
    });
  }

  /**
   * Activating a file in the rail opens it, through the same commands the chooser route uses: a
   * video in the player, a subtitle as the document. See BACKLOG.md M4.5.
   */
  function openAttachedFile(file: EpisodeFileView) {
    if (file.role === "media") {
      void open(file.path);
      return;
    }
    void subtitle.open(file.path);
  }

  async function adoptTranscription(runId: number) {
    // Text sitting in an open editor is unsaved work too, so it reaches the document before
    // anything asks whether the document may be replaced.
    await flushEditor.current();
    await subtitle.adoptTranscription(runId);
  }

  /** Quit through the one route the close gate guards, with the open editor flushed into the
   * document first so the gate is asked about it. See BACKLOG.md N6. */
  async function quit() {
    setQuitError(null);
    await flushEditor.current();
    try {
      await requestQuit();
    } catch {
      setQuitError(en.menu.errors.quitFailed);
    }
  }

  const dirty = subtitle.dirty || editorOpen;
  const blocked = subtitle.blockedPath !== null;

  const commands = {
    openSubtitle: {
      id: "open-subtitle",
      label: en.menu.file.openSubtitle,
      accelerator: en.menu.keys.openSubtitle,
      enabled: !choosing,
      run: () => void pick("subtitle", undefined, (path) => void subtitle.open(path)),
    },
    openVideo: {
      id: "open-video",
      label: en.menu.file.openVideo,
      accelerator: en.menu.keys.openVideo,
      enabled: !choosing && state.status !== "loading",
      run: () => void pick("video", undefined, (path) => void open(path)),
    },
    save: {
      id: "save",
      label: en.menu.file.save,
      accelerator: en.menu.keys.save,
      enabled: subtitle.summary !== null && dirty && !choosing,
      run: () => void saveDocument(),
    },
    saveCopy: {
      id: "save-copy",
      label: en.menu.file.saveCopy,
      accelerator: en.menu.keys.saveCopy,
      enabled: subtitle.summary !== null && !choosing,
      run: () => void saveCopy(),
    },
    discard: {
      id: "discard",
      label: en.menu.file.discard,
      enabled: true,
      run: () => void subtitle.discardAndOpen(),
    },
    quit: {
      id: "quit",
      label: en.menu.file.quit,
      accelerator: en.menu.keys.quit,
      enabled: true,
      run: () => void quit(),
    },
    undo: {
      id: "undo",
      label: en.menu.edit.undo,
      accelerator: en.menu.keys.undo,
      enabled: subtitle.canUndo,
      run: () => void subtitle.undo(),
    },
    redo: {
      id: "redo",
      label: en.menu.edit.redo,
      accelerator: en.menu.keys.redo,
      enabled: subtitle.canRedo,
      run: () => void subtitle.redo(),
    },
    about: {
      id: "about",
      label: en.menu.help.about,
      enabled: true,
      run: () => setAboutOpen(true),
    },
  };

  // Only while an open was refused for unsaved edits, in both routes at once: there is nothing to
  // discard the rest of the time.
  const discardable = blocked ? [commands.discard] : [];
  const menus = [
    {
      id: "file",
      title: en.menu.file.title,
      items: [
        commands.openSubtitle,
        commands.openVideo,
        commands.save,
        commands.saveCopy,
        ...discardable,
        commands.quit,
      ],
    },
    { id: "edit", title: en.menu.edit.title, items: [commands.undo, commands.redo] },
    { id: "help", title: en.menu.help.title, items: [commands.about] },
  ];
  const toolbar = [
    [commands.openSubtitle, commands.openVideo, commands.save, commands.saveCopy, ...discardable],
    [commands.undo, commands.redo],
  ];

  // Read by the accelerator listener, which is registered once and outlives every render.
  const latest = useRef(commands);
  useEffect(() => {
    latest.current = commands;
  });

  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey || event.metaKey) {
        return;
      }
      // Ctrl+S, Ctrl+Z and Ctrl+Y are the cue list's: it flushes an open editor before it acts.
      const command = acceleratorFor(event.key.toLowerCase(), event.shiftKey, latest.current);
      if (command === null) {
        return;
      }
      event.preventDefault();
      if (command.enabled) {
        command.run();
      }
    };
    window.addEventListener("keydown", handle, true);
    return () => window.removeEventListener("keydown", handle, true);
  }, []);

  return (
    <div className="shell">
      <header className="shell__chrome">
        <MenuBar menus={menus} />
        <Toolbar groups={toolbar} />
        {/* T4 takes this off the screen and opens it from the menu. */}
        <TranscribeBar
          mediaPath={state.path}
          transcription={transcription}
          adoptedRunId={subtitle.adoptedRunId}
          onUse={(runId) => void adoptTranscription(runId)}
        />
      </header>
      <div className="shell__body">
        <aside className="shell__rail">
          <ProjectRail project={project} onOpenFile={openAttachedFile} />
        </aside>
        <div className="shell__top">
          <section className="shell__video">
            <VideoStage hasVideo={ready} onRegionChange={setRegion} />
            <VideoControls
              enabled={ready}
              paused={state.paused}
              duration={state.duration ?? 0}
              position={position}
              onToggle={() => void togglePlayback()}
              onSeek={(target) => void seek(target)}
            />
          </section>
          {/* Empty until the waveform (M2.4) and the current line (T5) land beside the video. */}
          <section className="shell__tools" />
        </div>
      </div>
      {/* Full width, crossing under the rail: the layout drawing, the M2.0 criterion and T2 all
          say the grid takes everything below. Owner ruling 2026-09-02. */}
      <section className="shell__grid">
        <CueList
          key={subtitle.openId}
          cues={subtitle.cues}
          multiline={subtitle.summary?.format !== "ass"}
          flushRef={flushEditor}
          onEditingChange={setEditorOpen}
          onCommit={subtitle.setText}
          onUndo={subtitle.undo}
          onRedo={subtitle.redo}
          onSave={saveDocument}
        />
      </section>
      <StatusBar
        summary={subtitle.summary}
        dirty={dirty}
        truncated={subtitle.truncated}
        saved={subtitle.saved}
        savedInPlace={subtitle.savedInPlace}
        subtitleError={subtitle.error}
        videoErrorCode={errorCode}
        projectDeleted={project.deleted}
        projectError={project.error}
        chromeError={quitError}
      />
      {aboutOpen && <AboutDialog onClose={() => setAboutOpen(false)} />}
    </div>
  );
}
