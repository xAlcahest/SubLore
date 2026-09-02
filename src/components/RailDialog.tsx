import { useEffect, useId, useRef, useState } from "react";

import { useLayer } from "../hooks/useLayers";
import { en } from "../i18n/en";

type RailDialogProps = {
  title: string;
  /** The question, for a command that only has to be confirmed. */
  message?: string;
  /** The label over the field, for a command that needs a name. Absent means no field. */
  fieldLabel?: string;
  initial?: string;
  confirmLabel: string;
  onConfirm: (value: string) => void;
  onCancel: () => void;
};

/**
 * The one question the rail asks before it changes anything: rename and add carry a field, close,
 * delete and detach carry a sentence. Asked once, and nothing on disk moves either way
 * (decision 24, D2).
 */
export default function RailDialog({
  title,
  message,
  fieldLabel,
  initial = "",
  confirmLabel,
  onConfirm,
  onCancel,
}: RailDialogProps) {
  const fieldId = useId();
  const titleId = useId();
  const [value, setValue] = useState(initial);
  const field = useRef<HTMLInputElement>(null);
  const confirm = useRef<HTMLButtonElement>(null);

  // Mounted only while the question is up (decision 1, T8).
  useLayer(true);

  useEffect(() => {
    // The field when there is one, so the name can be typed straight away; otherwise the button
    // that does the thing, so Escape and Enter both land somewhere sensible.
    (field.current ?? confirm.current)?.focus();
  }, []);

  const ready = fieldLabel === undefined || value.trim() !== "";

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (ready) {
      onConfirm(value.trim());
    }
  }

  return (
    <div className="raildialog__backdrop" onMouseDown={onCancel}>
      <form
        className="raildialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={submit}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
        }}
      >
        <h2 className="raildialog__title" id={titleId}>
          {title}
        </h2>
        {message !== undefined && <p className="raildialog__message">{message}</p>}
        {fieldLabel !== undefined && (
          <>
            <label className="raildialog__label" htmlFor={fieldId}>
              {fieldLabel}
            </label>
            <input
              ref={field}
              id={fieldId}
              className="raildialog__field"
              type="text"
              value={value}
              onChange={(event) => setValue(event.target.value)}
            />
          </>
        )}
        <div className="raildialog__buttons">
          <button className="raildialog__cancel" type="button" onClick={onCancel}>
            {en.project.ask.cancel}
          </button>
          <button ref={confirm} className="raildialog__confirm" type="submit" disabled={!ready}>
            {confirmLabel}
          </button>
        </div>
      </form>
    </div>
  );
}
