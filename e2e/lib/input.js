import { execFileSync } from "node:child_process";

import { mapState } from "./x11.js";

/**
 * Real X11 input. WebKitWebDriver answers Element Click, Element Send Keys and the Actions
 * endpoint with "unsupported operation" against a wry webview, so the harness types and clicks
 * through XTEST instead. These are genuine key and button events, not synthesized DOM events.
 */
function xdotool(args) {
  return execFileSync("xdotool", args, { encoding: "utf8", timeout: 15000 });
}

/** XSetInputFocus, which works without a window manager. Typed keys follow the input focus. */
/**
 * Focus a window, waiting until X can actually give it focus.
 *
 * `XSetInputFocus` answers `BadMatch` for a window that is not viewable, and on a bare X server with
 * no window manager a toplevel can be mapped without being viewable for a while — or, if nothing
 * ever maps it, forever. The two cases look identical from the error and are not the same defect,
 * so this waits for `IsViewable` and then says which one it met.
 */
export function focusWindow(id, timeoutMs = 5000) {
  const deadline = Date.now() + timeoutMs;
  let state = "unknown";
  for (;;) {
    state = mapState(id);
    if (state === "IsViewable") {
      break;
    }
    if (Date.now() >= deadline) {
      throw new Error(
        `window ${id} never became viewable in ${timeoutMs}ms (it is ${state}), so X would answer ` +
          `BadMatch to a focus request. Either nothing mapped it, or this machine is slower than ` +
          `the wait.\n${describeWindow(id)}`,
      );
    }
    sleepBriefly();
  }
  try {
    xdotool(["windowfocus", "--sync", id]);
  } catch (error) {
    // Viewable and still refused: not a race. The usual cause is that nothing on this display can
    // hold focus, which is what a bare X server with no window manager looks like.
    throw new Error(
      `window ${id} is ${state} and X still refused focus: ${error.message}\n${describeWindow(id)}`,
    );
  }
}

/** `xwininfo` for one window, for an error message that can be read without a second run. */
function describeWindow(id) {
  try {
    return execFileSync("xwininfo", ["-id", id], { encoding: "utf8", timeout: 10000 });
  } catch (error) {
    return `xwininfo could not describe ${id}: ${error.message}`;
  }
}

/** Busy-wait in small steps: this file is synchronous and its callers are not all async. */
function sleepBriefly() {
  const until = Date.now() + 50;
  while (Date.now() < until) {
    // Spin. Fifty milliseconds at a time, bounded by the caller's timeout.
  }
}

/** Where the pointer is, in root coordinates. */
function pointerLocation() {
  const shell = xdotool(["getmouselocation", "--shell"]);
  const x = /^X=(-?\d+)$/m.exec(shell);
  const y = /^Y=(-?\d+)$/m.exec(shell);
  if (x === null || y === null) {
    throw new Error(`xdotool getmouselocation printed no X and Y lines:\n${shell}`);
  }
  return { x: Number(x[1]), y: Number(y[1]) };
}

export function clickAt(x, y) {
  const target = { x: Math.round(x), y: Math.round(y) };
  // `mousemove --sync` waits for the pointer to leave where it was, so asking it for the position
  // the pointer already holds never returns while a window sits under it. Clicking the same
  // element twice in a row is exactly that case. See e2e/README.md.
  const now = pointerLocation();
  if (now.x !== target.x || now.y !== target.y) {
    xdotool(["mousemove", "--sync", String(target.x), String(target.y)]);
  }
  xdotool(["click", "1"]);
}

/** Two clicks inside the double-click interval, which is what opens the cue list's inline editor. */
export function doubleClickAt(x, y) {
  const target = { x: Math.round(x), y: Math.round(y) };
  const now = pointerLocation();
  if (now.x !== target.x || now.y !== target.y) {
    xdotool(["mousemove", "--sync", String(target.x), String(target.y)]);
  }
  xdotool(["click", "--repeat", "2", "--delay", "40", "1"]);
}

export function typeText(text) {
  xdotool(["type", "--delay", "5", text]);
}

/** A named key, e.g. Return or Escape. */
export function pressKey(key) {
  xdotool(["key", key]);
}
