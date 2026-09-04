import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";

import { type CueSelection } from "../hooks/useCueSelection";
import { en } from "../i18n/en";
import { type CueRow } from "../types/subtitle";
import { CPS_LIMIT, readingRate, timecode } from "./cueView";

/**
 * Fixed row height in CSS pixels. The whole windowing calculation is this number, which is why it
 * is applied as an inline style and never restated in CSS. See BACKLOG.md M2.3.
 */
const ROW_HEIGHT = 28;
/** Rows kept rendered above and below the viewport, so a fast scroll does not show gaps. */
const OVERSCAN = 8;
/** The cursor is named by `aria-activedescendant`, which needs an id on the row it points at. */
function rowId(index: number): string {
  return `cuelist-row-${index}`;
}

type CueListProps = {
  cues: CueRow[];
  /** The cursor and the selection, held by the shell: the tools column reads the cursor too (T5). */
  selection: CueSelection;
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
};

/**
 * The cue list: index, the file's own number, start, end and text, with inline editing. Only the
 * rows in view are rendered, over a spacer as tall as the whole file, because a 2,000-row list
 * rendered whole spends the open budget on its own.
 */
export default function CueList({
  cues,
  selection,
  multiline,
  flushRef,
  onEditingChange,
  onCommit,
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
  const { active, selected, move, toggle, selectAll, collapse } = selection;

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

  // The shell owns every shortcut, and each of its commands flushes both editors before it acts,
  // so undo and save mean one thing wherever they were asked for. See docs/keyboard-tasks.md.
  useEffect(() => {
    flushRef.current = commit;
  });

  useLayoutEffect(() => {
    if (editing !== null) {
      editorRef.current?.focus();
    }
  }, [editing]);
  // The cursor is in view whoever moved it. Find and the playhead commands move it from outside the
  // grid, and a windowed list does not render a row it has scrolled past at all, so without this a
  // match in a long file leaves the grid looking like nothing happened. See find-replace-tasks F2.
  useLayoutEffect(() => {
    if (active !== null) {
      ensureVisible(active);
    }
  }, [active, ensureVisible]);

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
        <span className="cuelist__headcell cuelist__headcell--cps">{en.subtitle.cueList.cps}</span>
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
                    data-document-editor=""
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
