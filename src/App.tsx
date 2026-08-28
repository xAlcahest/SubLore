import ProjectPanel from "./components/ProjectPanel";
import SubtitleBar from "./components/SubtitleBar";
import VideoControls from "./components/VideoControls";
import VideoOpenBar from "./components/VideoOpenBar";
import VideoStage from "./components/VideoStage";
import { useProject } from "./hooks/useProject";
import { useSubtitleFile } from "./hooks/useSubtitleFile";
import { useVideoPlayer, videoErrorMessage } from "./hooks/useVideoPlayer";
import "./App.css";

export default function App() {
  const { state, position, errorCode, open, togglePlayback, seek, setRegion } = useVideoPlayer();
  const subtitle = useSubtitleFile();
  const project = useProject();
  const ready = state.status === "ready";

  return (
    <main className="app">
      <VideoOpenBar busy={state.status === "loading"} onOpen={(path) => void open(path)} />
      {errorCode !== null && (
        <p className="app__error" role="alert">
          {videoErrorMessage(errorCode)}
        </p>
      )}
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
      <SubtitleBar
        busy={subtitle.busy}
        summary={subtitle.summary}
        saved={subtitle.saved}
        error={subtitle.error}
        onOpen={(path) => void subtitle.open(path)}
        onSave={(destination) => void subtitle.saveAs(destination)}
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
    </main>
  );
}
