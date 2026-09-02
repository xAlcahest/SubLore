/**
 * Answer the native close-gate dialog from the keyboard.
 *
 * This used to aim a pointer at a button whose position it computed from a fixed 96-pixel width,
 * because GtkButtonBox sizes buttons to the widest label under whatever theme is installed. That
 * arithmetic is right on the machine it was measured on and wrong elsewhere: on GitHub's runner the
 * click landed beside the Save button, the dialog stayed open, no save ran, and the failure
 * surfaced as "the file on disk did not change" — a save defect that did not exist (gate 2, run
 * 33366855143).
 *
 * `dialog::ask_close` now gives its buttons mnemonics, so each answer has a keystroke that does not
 * depend on a theme, a font, a screen size or a window manager. Escape is GTK's own answer for
 * cancel and needs no mnemonic.
 *
 * The shape here is GTK's: a separate toplevel found by name, answered by an Alt mnemonic. Windows
 * shows `dialog.rs`'s non-Linux arm instead, which is a different dialog with different keys, so
 * MW.1b owns the counterpart. It needs no guard of its own: every call reaches `x11.js` or
 * `input.js` first, and those refuse by name.
 */
import { focusWindow, pressKey } from "./input.js";
import { waitFor } from "./proc.js";
import { allWindows, mapState, rootTree } from "./x11.js";

/** Frozen contract with `src-tauri/src/strings.rs`: the letter after the underscore in each label. */
const KEYS = { save: "alt+s", discard: "alt+d", cancel: "Escape" };

/** The dialog's window name. Frozen contract with `src-tauri/src/strings.rs`. */
export const UNSAVED_TITLE = "Unsaved changes";

/**
 * The unsaved-changes dialog, or null. Two at once is a real defect — one answer would decide what
 * the other is still asking about — so it fails here rather than picking one.
 */
export function findUnsavedDialog() {
  const onScreen = allWindows()
    .filter((window) => window.name === UNSAVED_TITLE)
    .filter((window) => mapState(window.id) === "IsViewable");
  if (onScreen.length > 1) {
    throw new Error(
      `expected at most one "${UNSAVED_TITLE}" dialog on screen, found ${onScreen.length} ` +
        `(${onScreen.map((w) => w.id).join(", ")}).\n${rootTree()}`,
    );
  }
  return onScreen.length === 1 ? onScreen[0] : null;
}

export async function waitForUnsavedDialog(timeout = 20000) {
  return waitFor(findUnsavedDialog, {
    timeout,
    message: `a toplevel named "${UNSAVED_TITLE}"`,
  }).catch((error) => {
    throw new Error(`${error.message}\nwindows on the display were:\n${rootTree()}`);
  });
}

/** Positive proof that an answer landed: the dialog it was given to is gone. */
export async function waitForUnsavedDialogGone(what, timeout = 10000) {
  return waitFor(() => (findUnsavedDialog() === null ? true : null), {
    timeout,
    message: `the dialog to close after ${what}`,
  }).catch((error) => {
    throw new Error(
      `${error.message}\nThe answer did not reach a button, so whatever follows would pass for ` +
        `the wrong reason.\n${rootTree()}`,
    );
  });
}

export function answerDialog(dialog, which) {
  const key = KEYS[which];
  if (key === undefined) {
    throw new Error(`unknown dialog answer ${which}`);
  }
  focusWindow(dialog.id);
  pressKey(key);
}
