import { useRef, useState } from "react";

import { choosePath, type ChooseKind } from "./chooser";
import CueList from "./components/CueList";
import ProjectPanel from "./components/ProjectPanel";
import StatusBar from "./components/StatusBar";
import SubtitleBar from "./components/SubtitleBar";
import TranscribeBar from "./components/TranscribeBar";
import VideoControls from "./components/VideoControls";
import VideoOpenBar from "./components/VideoOpenBar";
import VideoStage from "./components/VideoStage";
import { useProject } from "./hooks/useProject";
import { useStartupFiles } from "./hooks/useStartupFiles";
import { useSubtitleFile } from "./hooks/useSubtitleFile";
import { useTranscription } from "./hooks/useTranscription";
import { useVideoPlayer } from "./hooks/useVideoPlayer";
import "./App.css";

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
  // would sit behind the first. Every subtitle chooser is raised here, so one flag covers them all.
  const [choosing, setChoosing] = useState(false);

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

  async function adoptTranscription(runId: number) {
    // Text sitting in an open editor is unsaved work too, so it reaches the document before
    // anything asks whether the document may be replaced.
    await flushEditor.current();
    await subtitle.adoptTranscription(runId);
  }

  const dirty = subtitle.dirty || editorOpen;

  return (
    <div className="shell">
      {/* The command bars M0 to M4 left behind. T3 replaces them with a menu bar and a toolbar. */}
      <header className="shell__chrome">
        <VideoOpenBar busy={state.status === "loading"} onOpen={(path) => void open(path)} />
        <SubtitleBar
          summary={subtitle.summary}
          dirty={dirty}
          canUndo={subtitle.canUndo}
          canRedo={subtitle.canRedo}
          blocked={subtitle.blockedPath !== null}
          choosing={choosing}
          onOpen={() => void pick("subtitle", undefined, (path) => void subtitle.open(path))}
          onDiscard={() => void subtitle.discardAndOpen()}
          onSave={() => void saveDocument()}
          onSaveCopy={() => void saveCopy()}
          onUndo={() => void subtitle.undo()}
          onRedo={() => void subtitle.redo()}
        />
        <TranscribeBar
          mediaPath={state.path}
          transcription={transcription}
          adoptedRunId={subtitle.adoptedRunId}
          onUse={(runId) => void adoptTranscription(runId)}
        />
      </header>
      <div className="shell__body">
        <aside className="shell__rail">
          <ProjectPanel
            busy={project.busy}
            project={project.project}
            deleted={project.deleted}
            error={project.error}
            onCreate={(folder) => void project.create(folder)}
            onOpen={(folder) => void project.open(folder)}
            onDelete={() => void project.remove()}
            onAddEpisode={(title) => void project.addEpisode(title)}
            onAttachFile={(episodeId, role, path) => void project.attachFile(episodeId, role, path)}
            onChoosePath={project.choosePath}
          />
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
      />
    </div>
  );
}
