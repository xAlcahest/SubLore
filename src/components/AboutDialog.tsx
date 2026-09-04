import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { useLayer } from "../hooks/useLayers";
import { refusalLine, useModules } from "../hooks/useModules";
import { en } from "../i18n/en";
import { fill } from "../i18n/format";

type AboutDialogProps = {
  onClose: () => void;
};

/** What Help > About opens: the name, the version and the licence, and nothing to answer. */
export default function AboutDialog({ onClose }: AboutDialogProps) {
  const [version, setVersion] = useState<string | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  // Permanent, unlike the status bar's line, which is transient: About is where a fault the user
  // did not see at startup can still be read (module-abi.md 3.5).
  const modules = useModules();
  // Mounted only while the panel is open (decision 1, T8).
  useLayer(true);

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
        {/* Absent entirely when nothing was found: a user who never bought a module must not be
          shown a heading about one. Present the moment a file was found, loaded or not. */}
        {(modules.loaded.length > 0 || modules.refused.length > 0 || modules.skipped) && (
          <section className="about__modules">
            <h3 className="about__modules-heading">{en.modules.heading}</h3>
            {modules.skipped && <p className="about__module">{en.modules.skipped}</p>}
            {modules.loaded.map((file) => (
              <p className="about__module" key={file}>
                {fill(en.modules.loaded, { file })}
              </p>
            ))}
            {modules.refused.map((refused) => (
              <p className="about__module about__module--refused" key={refused.file}>
                {refusalLine(refused, en.modules)}
              </p>
            ))}
          </section>
        )}
        <button className="about__close" type="button" onClick={onClose}>
          {en.about.close}
        </button>
      </div>
    </div>
  );
}
