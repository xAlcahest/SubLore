import { useState } from "react";

import { choosePath } from "../chooser";
import { en } from "../i18n/en";

type VideoOpenBarProps = {
  busy: boolean;
  onOpen: (path: string) => void;
};

export default function VideoOpenBar({ busy, onOpen }: VideoOpenBarProps) {
  // The chooser is modal and answers on its own thread, so a second click while it is up would
  // raise a second one behind the first.
  const [choosing, setChoosing] = useState(false);

  async function pick() {
    setChoosing(true);
    try {
      const path = await choosePath("video");
      // Cancelled is an outcome, not a failure: nothing opens and nothing is said.
      if (path !== null) {
        onOpen(path);
      }
    } finally {
      setChoosing(false);
    }
  }

  return (
    <div className="bar">
      <span className="bar__brand">{en.appName}</span>
      <button
        className="bar__button"
        type="button"
        disabled={busy || choosing}
        onClick={() => void pick()}
      >
        {en.video.open}
      </button>
    </div>
  );
}
