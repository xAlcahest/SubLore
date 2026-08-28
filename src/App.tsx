import { useRef, useState } from "react";

import CueList from "./components/CueList";
import SubtitleBar from "./components/SubtitleBar";
import VideoControls from "./components/VideoControls";
import VideoOpenBar from "./components/VideoOpenBar";
import VideoStage from "./components/VideoStage";
import { useSubtitleFile } from "./hooks/useSubtitleFile";
import { useVideoPlayer, videoErrorMessage } from "./hooks/useVideoPlayer";
import "./App.css";

export default function App() {
  const { state, position, errorCode, open, togglePlayback, seek, setRegion } = useVideoPlayer();
  const subtitle = useSubtitleFile();
  const ready = state.status === "ready";
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
    </main>
  );
}
