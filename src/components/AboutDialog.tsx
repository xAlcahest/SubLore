import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { en } from "../i18n/en";
import { fill } from "../i18n/format";

type AboutDialogProps = {
  onClose: () => void;
};

/** What Help > About opens: the name, the version and the licence, and nothing to answer. */
export default function AboutDialog({ onClose }: AboutDialogProps) {
  const [version, setVersion] = useState<string | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let showing = true;
    void getVersion().then(
      (found) => {
        if (showing) {
          setVersion(found);
        }
      },
      // A version the app cannot read is left off the panel rather than shown as a guess.
      () => setVersion(null),
    );
    return () => {
      showing = false;
    };
  }, []);

  useLayoutEffect(() => {
    panelRef.current?.focus();
  }, []);

  return (
    <div
      className="about"
      role="dialog"
      aria-modal="true"
      aria-label={en.about.title}
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        className="about__panel"
        tabIndex={-1}
        ref={panelRef}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
      >
        <h2 className="about__title">{en.about.title}</h2>
        <p className="about__tagline">{en.about.tagline}</p>
        {version !== null && (
          <p className="about__version">{fill(en.about.version, { version })}</p>
        )}
        <p className="about__licence">{en.about.licence}</p>
        <button className="about__close" type="button" onClick={onClose}>
          {en.about.close}
        </button>
      </div>
    </div>
  );
}
