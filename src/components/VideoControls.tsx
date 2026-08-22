import { useId, useState, type ChangeEvent } from "react";

import { en } from "../i18n/en";

/** m:ss. The separator is punctuation, not translatable copy. */
function formatTime(seconds: number): string {
  const safe = Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  const minutes = Math.floor(safe / 60);
  const rest = safe % 60;
  return `${minutes}:${rest.toString().padStart(2, "0")}`;
}

type VideoControlsProps = {
  enabled: boolean;
  paused: boolean;
  duration: number;
  position: number;
  onToggle: () => void;
  onSeek: (position: number) => void;
};

export default function VideoControls({
  enabled,
  paused,
  duration,
  position,
  onToggle,
  onSeek,
}: VideoControlsProps) {
  const sliderId = useId();
  // While the user drags, the slider shows the dragged value instead of the event stream.
  const [dragged, setDragged] = useState<number | null>(null);
  const value = dragged ?? position;

  function change(event: ChangeEvent<HTMLInputElement>) {
    const next = Number(event.target.value);
    if (dragged === null) {
      onSeek(next);
      return;
    }
    setDragged(next);
  }

  function commit() {
    if (dragged !== null) {
      onSeek(dragged);
      setDragged(null);
    }
  }

  return (
    <div className="controls">
      <button className="controls__button" type="button" disabled={!enabled} onClick={onToggle}>
        {paused ? en.video.play : en.video.pause}
      </button>
      <label className="controls__label" htmlFor={sliderId}>
        {en.video.position}
      </label>
      <input
        id={sliderId}
        className="controls__slider"
        type="range"
        min={0}
        max={duration}
        step={0.01}
        value={Math.min(value, duration)}
        disabled={!enabled}
        onPointerDown={() => setDragged(position)}
        onPointerUp={commit}
        onPointerCancel={commit}
        onChange={change}
      />
      <span className="controls__time">
        {formatTime(value)} / {formatTime(duration)}
      </span>
    </div>
  );
}
