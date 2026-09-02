/**
 * Driving the app's native file chooser from a check or a spec.
 *
 * These helpers grew inside `picker-thread-check.js`, which was the only thing that opened a
 * chooser. M2.0's T1 removes every field for typing a path, so the specs that used to type one
 * reach the app through the chooser instead and need the same four operations. One copy, because
 * two copies of a keystroke sequence this fiddly drift apart and only one of them gets fixed.
 *
 * The chooser is a separate X toplevel: the webview cannot see it and WebDriver cannot touch it, so
 * everything here works at the X level and belongs inside a server the harness owns.
 */
import { setTimeout as sleep } from "node:timers/promises";

import { focusWindow, pressKey, typeText } from "./input.js";
import { waitFor } from "./proc.js";
import { allWindows, mapState, rootTree } from "./x11.js";

/**
 * The chooser that is on screen, or null. Only viewable ones count: the plugin's chooser is
 * unmapped rather than destroyed when it is answered, so under a plugin build the tree still holds
 * every chooser an earlier step opened. Two viewable at once is a real defect and fails here.
 */
export function findChooser(title) {
  const onScreen = allWindows()
    .filter((window) => window.name === title)
    .filter((window) => mapState(window.id) === "IsViewable");
  if (onScreen.length > 1) {
    throw new Error(
      `expected at most one "${title}" chooser on screen, found ${onScreen.length} ` +
        `(${onScreen.map((w) => w.id).join(", ")}).\n${rootTree()}`,
    );
  }
  return onScreen.length === 1 ? onScreen[0] : null;
}

/**
 * Wait for a chooser with this title.
 *
 * `alive` is optional and is how a caller that owns the process says it died: a script holds the
 * child and can tell, a spec runs against a driver-managed app and cannot. Without it the wait can
 * only time out, which is the honest outcome there.
 */
export async function waitForChooser(title, { timeout = 20000, alive } = {}) {
  return waitFor(
    () => {
      if (alive !== undefined && !alive()) {
        throw new Error(`the app exited instead of raising the "${title}" chooser`);
      }
      return findChooser(title);
    },
    { timeout, message: `a toplevel named "${title}"` },
  ).catch((error) => {
    throw new Error(`${error.message}\nwindows on the display were:\n${rootTree()}`);
  });
}

/** An answered chooser is destroyed by the app's own one and merely unmapped by the plugin's. */
export async function chooserClosed(chooser, timeout = 5000) {
  try {
    await waitFor(() => mapState(chooser.id) !== "IsViewable", {
      timeout,
      interval: 200,
      message: `the chooser ${chooser.id} to go away`,
    });
    return true;
  } catch {
    return false;
  }
}

/**
 * Answer a chooser with a path, from the keyboard.
 *
 * Alt+Home first: GTK opens on its Recent list, where the accept button is insensitive, and an
 * insensitive accept button swallows the location entry's Return (measured — the dialog just sits
 * there). Ctrl+L is the location entry, and Delete drops the suffix inline completion appended and
 * selected, or nothing when there was none.
 */
export async function answerChooser(chooser, chosen, what) {
  for (let attempt = 1; ; attempt += 1) {
    focusWindow(chooser.id);
    pressKey("alt+Home");
    await sleep(400);
    pressKey("ctrl+l");
    await sleep(400);
    // The entry keeps what a previous attempt typed into it, so each attempt starts from empty.
    pressKey("ctrl+a");
    typeText(chosen);
    await sleep(400);
    pressKey("Delete");
    pressKey("Return");
    if (await chooserClosed(chooser)) {
      return;
    }
    if (attempt >= 4) {
      throw new Error(
        `the ${what} chooser did not take "${chosen}" in ${attempt} attempts. It is still on ` +
          `screen, so the keystrokes reached nothing that acted on them.\n${rootTree()}`,
      );
    }
    // Escape leaves the location entry; a half-open one would eat the next attempt's Ctrl+L.
    pressKey("Escape");
    await sleep(400);
  }
}

/**
 * Accept a folder chooser with whatever it is already showing, through its accept button.
 *
 * This is how a check outside the app asks where the chooser opened: a `SelectFolder` chooser hands
 * back its current folder, and one that opened on GTK's Recent list cannot be accepted at all —
 * there the accept button is insensitive (measured, see `answerChooser`). So a chooser that opened
 * where it was told answers with that folder, and one that ignored it stays on screen and fails
 * here. Alt+O is the mnemonic `chooser.rs` gives the button.
 */
export async function acceptChooser(chooser, what) {
  focusWindow(chooser.id);
  pressKey("alt+o");
  if (!(await chooserClosed(chooser))) {
    throw new Error(
      `the ${what} chooser would not accept what it was showing, so it did not open at a folder ` +
        `it could hand back — GTK's Recent list is where that happens.\n${rootTree()}`,
    );
  }
}

/** Dismiss a chooser without choosing. The app reports this as a cancellation, not a failure. */
export async function cancelChooser(chooser, what) {
  focusWindow(chooser.id);
  pressKey("Escape");
  if (!(await chooserClosed(chooser))) {
    throw new Error(`the ${what} chooser did not close on Escape.\n${rootTree()}`);
  }
}
