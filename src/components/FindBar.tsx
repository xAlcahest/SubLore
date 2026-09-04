import { useId, useLayoutEffect, useRef } from "react";

import { en } from "../i18n/en";
import { type Query } from "../search";

type FindBarProps = {
  query: Query;
  /** What the last search said: the cue it landed in, or that the pattern is in no cue at all. */
  outcome: "idle" | "found" | "missing";
  onQueryChange: (query: Query) => void;
  onFindNext: () => void;
  onClose: () => void;
};

/**
 * The find band, under the grid and inside the panel flow rather than over it.
 *
 * Deliberately not a layer: a layer hides the native video surface (decision 1, T8) and searching
 * while the video plays is the point of having it here. Non-modal for the same reason, so the grid
 * stays usable behind it. See docs/find-replace-tasks.md F2.
 */
export default function FindBar({
  query,
  outcome,
  onQueryChange,
  onFindNext,
  onClose,
}: FindBarProps) {
  const titleId = useId();
  const fieldId = useId();
  const fieldRef = useRef<HTMLInputElement>(null);

  // Opened on purpose, so it takes the keyboard: a band the user has to click into first would be
  // slower than the menu it replaces.
  useLayoutEffect(() => {
    fieldRef.current?.focus();
  }, []);

  return (
    <section
      className="findbar"
      aria-labelledby={titleId}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onClose();
        }
      }}
    >
      <h2 className="findbar__title" id={titleId}>
        {en.find.title}
      </h2>
      <label className="bar__label" htmlFor={fieldId}>
        {en.find.needleLabel}
      </label>
      <input
        id={fieldId}
        ref={fieldRef}
        className="findbar__needle"
        type="text"
        value={query.needle}
        onChange={(event) => onQueryChange({ ...query, needle: event.target.value })}
        onKeyDown={(event) => {
          // Enter in the field is find next, per interface-spec 9.2.
          if (event.key === "Enter") {
            event.preventDefault();
            onFindNext();
          }
        }}
      />
      <label className="findbar__case-label">
        <input
          className="findbar__case"
          type="checkbox"
          checked={query.matchCase}
          onChange={(event) => onQueryChange({ ...query, matchCase: event.target.checked })}
        />
        {en.find.matchCase}
      </label>
      <button
        className="findbar__next"
        type="button"
        disabled={query.needle === ""}
        onClick={onFindNext}
      >
        {en.find.findNext}
      </button>
      {/* Drawn only once a search has actually run: an empty band must not accuse the user of a
        pattern they have not searched for yet. */}
      {outcome === "missing" && <span className="findbar__missing">{en.find.noMatch}</span>}
      <button className="findbar__close" type="button" onClick={onClose}>
        {en.find.close}
      </button>
    </section>
  );
}
