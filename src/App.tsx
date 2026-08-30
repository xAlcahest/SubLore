import { useRef, useState } from "react";

import CueList from "./components/CueList";
import ProjectPanel from "./components/ProjectPanel";
import SubtitleBar from "./components/SubtitleBar";
import TranscribeBar from "./components/TranscribeBar";
import VideoControls from "./components/VideoControls";
import VideoOpenBar from "./components/VideoOpenBar";
import VideoStage from "./components/VideoStage";
import { useProject } from "./hooks/useProject";
import { useStartupFiles } from "./hooks/useStartupFiles";
import { useSubtitleFile } from "./hooks/useSubtitleFile";
import { useTranscription } from "./hooks/useTranscription";
import { useVideoPlayer, videoErrorMessage } from "./hooks/useVideoPlayer";
import "./App.css";

export default function App() {
  const { state, position, errorCode, open, togglePlayback, seek, setRegion } = useVideoPlayer();
  const subtitle = useSubtitleFile();
  const project = useProject();
  const transcription = useTranscription();
  const ready = state.status === "ready";
  useStartupFiles(open, subtitle.open);
  // Saving writes the document, so it has to include the text sitting in an open editor, and an
  // open editor is unsaved work whether or not it has reached the document yet.
  const flushEditor = useRef<() => Promise<void>>(() => Promise.resolve());
  const [editorOpen, setEditorOpen] = useState(false);

  async function saveWithPendingEdit(destination: string | null) {
    await flushEditor.current();
    await (destination === null ? subtitle.save() : subtitle.saveAs(destination));
  }

  return (
    <main className="app">
      <aside className="sidebar">
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
      <div className="workspace">
        <VideoOpenBar busy={state.status === "loading"} onOpen={(path) => void open(path)} />
        {errorCode !== null && (
          <p className="app__error" role="alert">
            {videoErrorMessage(errorCode)}
          </p>
        )}
        <SubtitleBar
          summary={subtitle.summary}
          saved={subtitle.saved}
          savedInPlace={subtitle.savedInPlace}
          error={subtitle.error}
          dirty={subtitle.dirty || editorOpen}
          truncated={subtitle.truncated}
          canUndo={subtitle.canUndo}
          canRedo={subtitle.canRedo}
          blocked={subtitle.blockedPath !== null}
          onOpen={(path) => void subtitle.open(path)}
          onDiscard={() => void subtitle.discardAndOpen()}
          onSave={() => void saveWithPendingEdit(null)}
          onSaveAs={(destination) => void saveWithPendingEdit(destination)}
          onUndo={() => void subtitle.undo()}
          onRedo={() => void subtitle.redo()}
        />
        <TranscribeBar mediaPath={state.path} transcription={transcription} />
        <VideoStage hasVideo={ready} onRegionChange={setRegion} />
        <VideoControls
          enabled={ready}
          paused={state.paused}
          duration={state.duration ?? 0}
          position={position}
          onToggle={() => void togglePlayback()}
          onSeek={(target) => void seek(target)}
        />
        <CueList
          key={subtitle.openId}
          cues={subtitle.cues}
          multiline={subtitle.summary?.format !== "ass"}
          flushRef={flushEditor}
          onEditingChange={setEditorOpen}
          onCommit={subtitle.setText}
          onUndo={subtitle.undo}
          onRedo={subtitle.redo}
          onSave={subtitle.save}
        />
      </div>
    </main>
  );
}
