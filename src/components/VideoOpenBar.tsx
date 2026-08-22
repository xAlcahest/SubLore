import { useId, useState, type SubmitEvent } from "react";

import { en } from "../i18n/en";

type VideoOpenBarProps = {
  busy: boolean;
  onOpen: (path: string) => void;
};

export default function VideoOpenBar({ busy, onOpen }: VideoOpenBarProps) {
  const inputId = useId();
  const [path, setPath] = useState("");
  const trimmed = path.trim();

  function submit(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (trimmed === "" || busy) {
      return;
    }
    onOpen(trimmed);
  }

  return (
    <form className="bar" onSubmit={submit}>
      <span className="bar__brand">{en.appName}</span>
      <label className="bar__label" htmlFor={inputId}>
        {en.video.pathLabel}
      </label>
      <input
        id={inputId}
        className="bar__input"
        type="text"
        value={path}
        placeholder={en.video.pathPlaceholder}
        onChange={(event) => setPath(event.target.value)}
      />
      <button className="bar__button" type="submit" disabled={trimmed === "" || busy}>
        {en.video.open}
      </button>
    </form>
  );
}
