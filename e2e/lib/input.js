import { execFileSync } from "node:child_process";

/**
 * Real X11 input. WebKitWebDriver answers Element Click, Element Send Keys and the Actions
 * endpoint with "unsupported operation" against a wry webview, so the harness types and clicks
 * through XTEST instead. These are genuine key and button events, not synthesized DOM events.
 */
function xdotool(args) {
  return execFileSync("xdotool", args, { encoding: "utf8", timeout: 15000 });
}

/** XSetInputFocus, which works without a window manager. Typed keys follow the input focus. */
export function focusWindow(id) {
  xdotool(["windowfocus", "--sync", id]);
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
