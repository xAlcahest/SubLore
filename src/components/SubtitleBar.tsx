import { en } from "../i18n/en";
import { type SubtitleSummary } from "../types/subtitle";

type SubtitleBarProps = {
  summary: SubtitleSummary | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  /** Set when an open was refused because the file on screen has unsaved edits. */
  blocked: boolean;
  /** Set while a chooser is up. It is modal, so nothing here may raise a second one behind it. */
  choosing: boolean;
  onOpen: () => void;
  onDiscard: () => void;
  onSave: () => void;
  onSaveCopy: () => void;
  onUndo: () => void;
  onRedo: () => void;
};

/** Open a subtitle file and save it. What state it is in is the status bar's line (decision 24 A1). */
export default function SubtitleBar({
  summary,
  dirty,
  canUndo,
  canRedo,
  blocked,
  choosing,
  onOpen,
  onDiscard,
  onSave,
  onSaveCopy,
  onUndo,
  onRedo,
}: SubtitleBarProps) {
  return (
    <div className="subbar">
      <button className="subbar__open" type="button" disabled={choosing} onClick={onOpen}>
        {en.subtitle.open}
      </button>
      <button
        className="subbar__save"
        type="button"
        disabled={summary === null || !dirty || choosing}
        onClick={onSave}
      >
        {en.subtitle.saveFile}
      </button>
      <button className="subbar__undo" type="button" disabled={!canUndo} onClick={onUndo}>
        {en.subtitle.undo}
      </button>
      <button className="subbar__redo" type="button" disabled={!canRedo} onClick={onRedo}>
        {en.subtitle.redo}
      </button>
      {blocked && (
        <button className="subbar__discard" type="button" onClick={onDiscard}>
          {en.subtitle.discard}
        </button>
      )}
      <button
        className="subbar__save-copy"
        type="button"
        disabled={summary === null || choosing}
        onClick={onSaveCopy}
      >
        {en.subtitle.save}
      </button>
    </div>
  );
}
