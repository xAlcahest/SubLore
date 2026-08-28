import SubtitleBar from "./components/SubtitleBar";
import TranscribeBar from "./components/TranscribeBar";
import VideoControls from "./components/VideoControls";
import VideoOpenBar from "./components/VideoOpenBar";
import VideoStage from "./components/VideoStage";
import { useSubtitleFile } from "./hooks/useSubtitleFile";
import { useTranscription } from "./hooks/useTranscription";
import { useVideoPlayer, videoErrorMessage } from "./hooks/useVideoPlayer";
import "./App.css";

export default function App() {
  const { state, position, errorCode, open, togglePlayback, seek, setRegion } = useVideoPlayer();
  const subtitle = useSubtitleFile();
  const transcription = useTranscription();
  const ready = state.status === "ready";

  return (
    <main className="app">
      <VideoOpenBar busy={state.status === "loading"} onOpen={(path) => void open(path)} />
      {errorCode !== null && (
        <p className="app__error" role="alert">
          {videoErrorMessage(errorCode)}
        </p>
      )}
      <SubtitleBar
        busy={subtitle.busy}
        summary={subtitle.summary}
        saved={subtitle.saved}
        error={subtitle.error}
        onOpen={(path) => void subtitle.open(path)}
        onSave={(destination) => void subtitle.saveAs(destination)}
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
    </main>
  );
}
