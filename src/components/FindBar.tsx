import { useId, useLayoutEffect, useRef } from "react";

import { en } from "../i18n/en";
import { fill } from "../i18n/format";
import { type Query } from "../search";

/** Find hides the replacement field and its two buttons; the two modes are one band (spec 9.2). */
export type FindMode = "find" | "replace";

type FindBarProps = {
  mode: FindMode;
  query: Query;
  replacement: string;
  /** What the last search said: it found something, it found nothing, or nothing has run yet. */
  outcome: "idle" | "found" | "missing";
  /** How many a replace all rewrote, drawn until the next search. Null before any has run. */
  replaced: number | null;
  onQueryChange: (query: Query) => void;
  onReplacementChange: (replacement: string) => void;
  onFindNext: () => void;
  onReplace: () => void;
  onReplaceAll: () => void;
  onClose: () => void;
};

/**
 * The find band, under the grid and inside the panel flow rather than over it.
 *
 * Deliberately not a layer: a layer hides the native video surface (decision 1, T8) and searching
 * while the video plays is the point of having it here. Non-modal for the same reason, so the grid
 * stays usable behind it. See docs/find-replace-tasks.md F2 and F3.
 */
export default function FindBar({
  mode,
  query,
  replacement,
  outcome,
  replaced,
  onQueryChange,
  onReplacementChange,
  onFindNext,
  onReplace,
  onReplaceAll,
  onClose,
}: FindBarProps) {
  const titleId = useId();
  const fieldId = useId();
  const replacementId = useId();
  const fieldRef = useRef<HTMLInputElement>(null);

  // Opened on purpose, so it takes the keyboard: a band the user has to click into first would be
  // slower than the menu it replaces. Refocused on a mode change, which is the same intent again.
  useLayoutEffect(() => {
    fieldRef.current?.focus();
  }, [mode]);

  const empty = query.needle === "";

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
        {mode === "replace" ? en.find.replaceTitle : en.find.title}
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
      {mode === "replace" && (
        <>
          <label className="bar__label" htmlFor={replacementId}>
            {en.find.replaceLabel}
          </label>
          <input
            id={replacementId}
            className="findbar__replacement"
            type="text"
            value={replacement}
            onChange={(event) => onReplacementChange(event.target.value)}
            onKeyDown={(event) => {
              // Enter in the replacement field is replace next, per interface-spec 9.2.
              if (event.key === "Enter") {
                event.preventDefault();
                onReplace();
              }
            }}
          />
        </>
      )}
      <label className="findbar__case-label">
        <input
          className="findbar__case"
          type="checkbox"
          checked={query.matchCase}
          onChange={(event) => onQueryChange({ ...query, matchCase: event.target.checked })}
        />
        {en.find.matchCase}
      </label>
      <button className="findbar__next" type="button" disabled={empty} onClick={onFindNext}>
        {en.find.findNext}
      </button>
      {mode === "replace" && (
        <>
          <button className="findbar__replace" type="button" disabled={empty} onClick={onReplace}>
            {en.find.replace}
          </button>
          <button
            className="findbar__replace-all"
            type="button"
            disabled={empty}
            onClick={onReplaceAll}
          >
            {en.find.replaceAll}
          </button>
        </>
      )}
      {/* Drawn only once a search has actually run: an empty band must not accuse the user of a
        pattern they have not searched for yet. */}
      {outcome === "missing" && <span className="findbar__missing">{en.find.noMatch}</span>}
      {replaced !== null && (
        <span className="findbar__replaced">
          {fill(replaced === 1 ? en.find.replaced.one : en.find.replaced.other, {
            count: replaced,
          })}
        </span>
      )}
      <button className="findbar__close" type="button" onClick={onClose}>
        {en.find.close}
      </button>
    </section>
  );
}
