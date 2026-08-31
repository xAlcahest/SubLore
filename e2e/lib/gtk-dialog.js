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
 */
import { focusWindow, pressKey } from "./input.js";

/** Frozen contract with `src-tauri/src/strings.rs`: the letter after the underscore in each label. */
const KEYS = { save: "alt+s", discard: "alt+d", cancel: "Escape" };

export function answerDialog(dialog, which) {
  const key = KEYS[which];
  if (key === undefined) {
    throw new Error(`unknown dialog answer ${which}`);
  }
  focusWindow(dialog.id);
  pressKey(key);
}
