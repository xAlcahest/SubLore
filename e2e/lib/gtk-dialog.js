/**
 * Click a native GTK close-gate dialog button by label. GtkButtonBox sizes buttons to the widest
 * label under the runner's theme, so these numbers are an estimate; each caller proves the click
 * landed by watching the dialog disappear (close-gate-check.js) or by recording the phase it
 * reached (n1b-load-probe.js). Shared so the two callers can't drift apart (gate2 register, L3).
 */
import { clickAt, focusWindow } from "./input.js";

/** `dialog::ask_close` adds save, discard, cancel in that order, so cancel sits rightmost. */
const SLOTS = { save: 2, discard: 1, cancel: 0 };
const BUTTON_WIDTH = 96;

export function clickDialogButton(dialog, which) {
  const slot = SLOTS[which];
  if (slot === undefined) {
    throw new Error(`unknown dialog button ${which}`);
  }
  const x = dialog.absX + dialog.width - 24 - BUTTON_WIDTH / 2 - slot * (BUTTON_WIDTH + 12);
  const y = dialog.absY + dialog.height - 34;
  focusWindow(dialog.id);
  clickAt(x, y);
}
