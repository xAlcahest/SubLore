import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";

import { useCueSelection } from "../hooks/useCueSelection";
import { en } from "../i18n/en";
import { type CueRow } from "../types/subtitle";

/**
 * Fixed row height in CSS pixels. The whole windowing calculation is this number, which is why it
 * is applied as an inline style and never restated in CSS. See BACKLOG.md M2.3.
 */
const ROW_HEIGHT = 28;
/** Rows kept rendered above and below the viewport, so a fast scroll does not show gaps. */
const OVERSCAN = 8;
/** Reading rate a line is flagged above, fixed and not configurable in v1. Decision 24 A8. */
const CPS_LIMIT = 21;
/** The markup A8 does not count: ASS override blocks and HTML-style tags. */
const MARKUP = /\{[^}]*\}|<[^>]*>/g;
/** Line breaks in both spellings a cue holds: a real one, and the `\N` of an ASS field. */
const LINE_BREAKS = /\r\n|[\r\n]|\\[Nn]/g;
/**
 * Owed to src/i18n/en.ts as `subtitle.cueList.cps`; T3 owns that file this wave. See m2-0-tasks T6.
 */
const CPS_HEADER = "CPS";

/** Input types that hold typed text, and so keep their own undo. A range slider holds none. */
const TEXT_INPUT_TYPES = ["text", "search", "url", "email", "tel", "password", "number"];

/**
 * A field that owns its own keyboard: a path box, anything typed into but the cue editor. The
 * document shortcuts below stay out of those, so Ctrl+Z there means what it means everywhere else.
 */
function ownsTheKeyboard(target: EventTarget | null, editor: HTMLTextAreaElement | null): boolean {
  if (!(target instanceof HTMLElement) || target === editor) {
    return false;
  }
  if (target instanceof HTMLInputElement) {
    return TEXT_INPUT_TYPES.includes(target.type);
  }
  return target instanceof HTMLTextAreaElement || target.isContentEditable;
}

/** The cursor is named by `aria-activedescendant`, which needs an id on the row it points at. */
function rowId(index: number): string {
  return `cuelist-row-${index}`;
}

/**
 * Characters per second: spaces counted, line breaks not, over text with its markup stripped.
 * Null when the cue has no duration to divide by. Decision 24 A8.
 */
function readingRate(cue: CueRow): number | null {
  const seconds = (cue.endMs - cue.startMs) / 1000;
  if (!(seconds > 0)) {
    return null;
  }
  return cue.text.replace(MARKUP, "").replace(LINE_BREAKS, "").length / seconds;
}

/** hh:mm:ss.mmm. Separators are punctuation, not translatable copy. */
function timecode(milliseconds: number): string {
  const safe = Number.isFinite(milliseconds) && milliseconds > 0 ? Math.floor(milliseconds) : 0;
  const millis = safe % 1000;
  const seconds = Math.floor(safe / 1000) % 60;
  const minutes = Math.floor(safe / 60_000) % 60;
  const hours = Math.floor(safe / 3_600_000);
  const pad = (value: number, width: number) => value.toString().padStart(width, "0");
  return `${pad(hours, 2)}:${pad(minutes, 2)}:${pad(seconds, 2)}.${pad(millis, 3)}`;
}

type CueListProps = {
  cues: CueRow[];
  /** ASS writes line breaks as `\N` inside one field, so a real one cannot be committed there. */
  multiline: boolean;
  /**
   * Filled with a function that sends whatever the open editor holds. A save must write what the
   * user typed, whichever way the save was asked for. See BACKLOG.md M2.3.
   */
  flushRef: { current: () => Promise<void> };
  /** Told whenever an editor opens or closes: text in a field is unsaved work too (design 5.3). */
  onEditingChange: (open: boolean) => void;
  onCommit: (cue: number, text: string) => Promise<void>;
  onUndo: () => Promise<void>;
  onRedo: () => Promise<void>;
  onSave: () => Promise<void>;
};

/**
 * The cue list: index, the file's own number, start, end and text, with inline editing. Only the
 * rows in view are rendered, over a spacer as tall as the whole file, because a 2,000-row list
 * rendered whole spends the open budget on its own.
 */
export default function CueList({
  cues,
  multiline,
  flushRef,
  onEditingChange,
  onCommit,
  onUndo,
  onRedo,
  onSave,
}: CueListProps) {
  const listRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(0);
  const [editing, setEditing] = useState<number | null>(null);
  const [draft, setDraft] = useState("");
  const [committing, setCommitting] = useState(false);
  /** Cleared the moment an edit ends, so a blur that arrives after Escape cannot commit it. */
  const editingRef = useRef<number | null>(null);

  const count = cues.length;
  const { active, selected, move, toggle, selectAll, collapse } = useCueSelection(count);

  useEffect(() => {
    const list = listRef.current;
    if (list === null) {
      return;
    }
    const measure = () => setViewport(list.clientHeight);
    const observer = new ResizeObserver(measure);
    observer.observe(list);
    measure();
    return () => observer.disconnect();
  }, []);

  const ensureVisible = useCallback((index: number) => {
    const list = listRef.current;
    if (list === null) {
      return;
    }
    const top = index * ROW_HEIGHT;
    if (top < list.scrollTop) {
      list.scrollTop = top;
    } else if (top + ROW_HEIGHT > list.scrollTop + list.clientHeight) {
      list.scrollTop = top + ROW_HEIGHT - list.clientHeight;
    }
  }, []);

  const beginEdit = useCallback(
    (index: number) => {
      const cue = cues[index];
      if (cue === undefined) {
        return;
      }
      // Editing is about the cursor, never about the selection: a bulk operation issued after an
      // edit still means what it meant before (decision 5).
      editingRef.current = index;
      setEditing(index);
      setDraft(cue.text);
    },
    [cues],
  );

  const cancelEdit = useCallback(() => {
    editingRef.current = null;
    setEditing(null);
  }, []);

  /** Send the open editor's text, if it is still open and the text actually changed. */
  const commit = useCallback(async () => {
    const index = editingRef.current;
    if (index === null) {
      return;
    }
    editingRef.current = null;
    setEditing(null);
    const cue = cues[index];
    if (cue === undefined || cue.text === draft) {
      return;
    }
    // Still counted as an open editor until the text lands: the click that saves a file arrives
    // one event after the blur that started this, and a control disabled in between eats it.
    setCommitting(true);
    try {
      await onCommit(index, draft);
    } finally {
      setCommitting(false);
    }
  }, [cues, draft, onCommit]);

  // The window-level shortcuts and the toolbar both flush a pending editor first, so "undo" and
  // "save" each mean one thing wherever they were asked for.
  const latest = useRef({ commit, onUndo, onRedo, onSave });
  useEffect(() => {
    latest.current = { commit, onUndo, onRedo, onSave };
    flushRef.current = commit;
  });

  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey || event.metaKey) {
        return;
      }
      const pressed = event.key.toLowerCase();
      if (pressed !== "z" && pressed !== "y" && pressed !== "s") {
        return;
      }
      if (ownsTheKeyboard(event.target, editorRef.current)) {
        return;
      }
      // Intercepted inside the cue editor: the webview's own text undo must never diverge from
      // the document's.
      event.preventDefault();
      const shift = event.shiftKey;
      const actions = latest.current;
      void (async () => {
        await actions.commit();
        if (pressed === "s") {
          await actions.onSave();
        } else if (pressed === "y" || shift) {
          await actions.onRedo();
        } else {
          await actions.onUndo();
        }
      })();
    };
    window.addEventListener("keydown", handle, true);
    return () => window.removeEventListener("keydown", handle, true);
  }, []);

  useLayoutEffect(() => {
    if (editing !== null) {
      editorRef.current?.focus();
    }
  }, [editing]);

  useEffect(() => {
    onEditingChange(editing !== null || committing);
  }, [committing, editing, onEditingChange]);

  // A list replaced by a new file carries no open editor with it.
  useEffect(() => () => onEditingChange(false), [onEditingChange]);

  function onListKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
    if (editingRef.current !== null || count === 0 || active === null) {
      return;
    }
    // Alt is excluded from both: AltGr arrives as ctrl+alt, and it is typing, not a shortcut.
    if (event.ctrlKey && !event.altKey && event.key.toLowerCase() === "a") {
      event.preventDefault();
      selectAll();
      return;
    }
    // Ctrl+Space is how the keyboard scatters a selection: it flips the row the cursor walked to.
    if (event.ctrlKey && !event.altKey && event.key === " ") {
      event.preventDefault();
      toggle(active);
      return;
    }
    if (event.key === "Escape") {
      if (selected.size > 1) {
        event.preventDefault();
        collapse();
      }
      return;
    }
    const last = count - 1;
    const page = Math.max(1, Math.floor(viewport / ROW_HEIGHT));
    let next = active;
    switch (event.key) {
      case "ArrowDown":
        next = Math.min(last, active + 1);
        break;
      case "ArrowUp":
        next = Math.max(0, active - 1);
        break;
      case "PageDown":
        next = Math.min(last, active + page);
        break;
      case "PageUp":
        next = Math.max(0, active - page);
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = last;
        break;
      case "Enter":
        event.preventDefault();
        beginEdit(active);
        return;
      default:
        return;
    }
    event.preventDefault();
    move(next, event.shiftKey ? "extend" : event.ctrlKey ? "cursorOnly" : "plain");
    ensureVisible(next);
  }

  /** Every mouse gesture on a row mirrors a key: ctrl toggles, shift extends, a plain one moves. */
  function onRowMouseDown(event: ReactMouseEvent<HTMLDivElement>, index: number) {
    if (event.ctrlKey) {
      toggle(index);
      return;
    }
    move(index, event.shiftKey ? "extend" : "plain");
  }

  function onEditorKeyDown(event: ReactKeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      cancelEdit();
      listRef.current?.focus();
      return;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      // Tab collapses the selection onto the row it lands on: tabbing down a file must not grow a
      // range behind it (decision 5).
      const next = Math.min(count - 1, (editingRef.current ?? active ?? 0) + 1);
      void commit().then(() => {
        move(next, "plain");
        ensureVisible(next);
        listRef.current?.focus();
      });
      return;
    }
    if (event.key === "Enter") {
      if (event.shiftKey && multiline) {
        return;
      }
      event.preventDefault();
      void commit().then(() => listRef.current?.focus());
    }
  }

  const visible = Math.max(1, Math.ceil(viewport / ROW_HEIGHT));
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(count, first + visible + 2 * OVERSCAN);
  const indices: number[] = [];
  for (let index = first; index < last; index += 1) {
    indices.push(index);
  }
  // The row being edited stays in the DOM wherever it is, or the textarea would lose focus.
  if (editing !== null && (editing < first || editing >= last)) {
    indices.push(editing);
  }
  // aria-activedescendant has to name an element that exists, and the cursor's row is dropped from
  // the DOM like any other when it scrolls out of view.
  const activeId = active !== null && indices.includes(active) ? rowId(active) : undefined;

  return (
    <div className="cuelist__panel">
      <div className="cuelist__head">
        <span className="cuelist__headcell cuelist__headcell--pos">
          {en.subtitle.cueList.position}
        </span>
        <span className="cuelist__headcell cuelist__headcell--num">
          {en.subtitle.cueList.number}
        </span>
        <span className="cuelist__headcell cuelist__headcell--time">
          {en.subtitle.cueList.start}
        </span>
        <span className="cuelist__headcell cuelist__headcell--time">{en.subtitle.cueList.end}</span>
        <span className="cuelist__headcell cuelist__headcell--cps">{CPS_HEADER}</span>
        <span className="cuelist__headcell">{en.subtitle.cueList.text}</span>
      </div>
      <div
        className="cuelist"
        role="listbox"
        aria-label={en.subtitle.cueList.label}
        aria-multiselectable={true}
        aria-activedescendant={activeId}
        tabIndex={0}
        ref={listRef}
        onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
        onKeyDown={onListKeyDown}
      >
        <div className="cuelist__sizer" style={{ height: count * ROW_HEIGHT }}>
          {indices.map((index) => {
            const cue = cues[index];
            if (cue === undefined) {
              return null;
            }
            const classes = ["cuelist__row"];
            if (selected.has(index)) {
              classes.push("cuelist__row--selected");
            }
            if (index === active) {
              classes.push("cuelist__row--active");
            }
            if (cue.comment) {
              classes.push("cuelist__row--comment");
            }
            const rate = readingRate(cue);
            const cpsClasses = ["cuelist__cps"];
            if (rate !== null && rate > CPS_LIMIT) {
              cpsClasses.push("cuelist__cps--over");
            }
            return (
              <div
                key={index}
                id={rowId(index)}
                className={classes.join(" ")}
                role="option"
                aria-selected={selected.has(index)}
                title={cue.comment ? en.subtitle.cueList.comment : undefined}
                style={{ top: index * ROW_HEIGHT, height: ROW_HEIGHT }}
                onMouseDown={(event) => onRowMouseDown(event, index)}
              >
                <span className="cuelist__pos">{index + 1}</span>
                <span className="cuelist__number">{cue.number ?? ""}</span>
                <span className="cuelist__start">{timecode(cue.startMs)}</span>
                <span className="cuelist__end">{timecode(cue.endMs)}</span>
                <span className={cpsClasses.join(" ")}>
                  {rate === null ? "" : Math.round(rate)}
                </span>
                {editing === index ? (
                  <textarea
                    className="cuelist__editor"
                    ref={editorRef}
                    value={draft}
                    spellCheck={false}
                    onChange={(event) => setDraft(event.target.value)}
                    onKeyDown={onEditorKeyDown}
                    onBlur={() => void commit()}
                  />
                ) : (
                  <span
                    className="cuelist__text"
                    // A plain click opens the editor; the modified ones are selecting, not editing.
                    onClick={(event) => {
                      if (!event.ctrlKey && !event.shiftKey) {
                        beginEdit(index);
                      }
                    }}
                  >
                    {cue.text}
                  </span>
                )}
              </div>
            );
          })}
        </div>
        {count === 0 && <p className="cuelist__empty">{en.subtitle.cueList.empty}</p>}
      </div>
    </div>
  );
}
