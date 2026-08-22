import VideoControls from "./components/VideoControls";
import VideoOpenBar from "./components/VideoOpenBar";
import VideoStage from "./components/VideoStage";
import { useVideoPlayer, videoErrorMessage } from "./hooks/useVideoPlayer";
import "./App.css";

export default function App() {
  const { state, position, errorCode, open, togglePlayback, seek, setRegion } = useVideoPlayer();
  const ready = state.status === "ready";

  return (
    <main className="app">
      <VideoOpenBar busy={state.status === "loading"} onOpen={(path) => void open(path)} />
      {errorCode !== null && (
        <p className="app__error" role="alert">
          {videoErrorMessage(errorCode)}
        </p>
      )}
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
