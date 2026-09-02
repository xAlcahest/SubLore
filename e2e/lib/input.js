import { execFileSync } from "node:child_process";
import process from "node:process";

import { mapState, windowSize } from "./x11.js";

/**
 * Real X11 input. WebKitWebDriver answers Element Click, Element Send Keys and the Actions
 * endpoint with "unsupported operation" against a wry webview, so the harness types and clicks
 * through XTEST instead. These are genuine key and button events, not synthesized DOM events.
 */
function xdotool(args) {
  requireOwnedDisplay();
  return execFileSync("xdotool", args, { encoding: "utf8", timeout: 15000 });
}

/** Answered once per process: asking X the same question for every keystroke buys nothing. */
let ownsDisplay;

/**
 * Refuse to drive XTEST on a display that belongs to somebody.
 *
 * XTEST types into whatever holds focus on the whole server, not into a window this code chose. The
 * checks allowed on a real display only launch the app, read its arguments and take screenshots;
 * anything that types or clicks belongs on a bare X server the run owns. Running `pnpm e2e:close-gate`
 * without `xvfb-run` sent keystrokes into the owner's session on 2026-08-31, and nothing stopped it.
 *
 * A window manager is the difference that can be asked about: Xvfb here runs without one, and every
 * real session has one.
 */
function requireOwnedDisplay() {
  if (ownsDisplay === undefined) {
    let root;
    try {
      root = execFileSync("xprop", ["-root", "_NET_SUPPORTING_WM_CHECK"], {
        encoding: "utf8",
        timeout: 10000,
      });
    } catch (error) {
      // Not knowing whose display this is is not permission to type on it.
      throw new Error(
        `cannot tell whose display ${process.env.DISPLAY} is: xprop failed (${error.message}). ` +
          `Install x11-utils, or run this check under Xvfb.`,
      );
    }
    ownsDisplay = !/window id #/.test(root);
  }
  if (!ownsDisplay) {
    throw new Error(
      `refusing to send synthetic input to ${process.env.DISPLAY}: a window manager is running ` +
        `there, so it is a real session and XTEST would type into it. Run the check under its own ` +
        `X server: xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:<check>.`,
    );
  }
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
    if (state === "IsGone") {
      throw new Error(
        `window ${id} was destroyed before it could be focused. Whatever asked for this focus is ` +
          `holding an id that outlived its window.`,
      );
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

/**
 * Resize a toplevel and return only once X reports the new size.
 *
 * `xdotool windowsize` sends the request and returns, so a caller that measures straight afterwards
 * measures the old geometry. The layout a resize is asked for is never the point of the resize.
 */
export function resizeWindow(id, width, height, timeoutMs = 5000) {
  xdotool(["windowsize", id, String(width), String(height)]);
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const size = windowSize(id);
    if (size === null) {
      throw new Error(
        `window ${id} was destroyed while it was being resized to ${width}x${height}.`,
      );
    }
    if (size.width === width && size.height === height) {
      return;
    }
    if (Date.now() >= deadline) {
      throw new Error(
        `window ${id} is still ${size.width}x${size.height} ${timeoutMs}ms after being asked for ` +
          `${width}x${height}. Either the screen is too small to hold it, or something is resizing ` +
          `it back.\n${describeWindow(id)}`,
      );
    }
    sleepBriefly();
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

/**
 * Press at one point, travel to another and release: the gesture a click cannot stand in for.
 *
 * A range input reads the motion between the two ends, so the pointer is walked there in steps
 * rather than teleported: one jump from press to release is a path the control never sees. Each
 * step skips the move when the pointer is already there, for the `--sync` reason `clickAt` gives.
 */
export function dragAt(fromX, fromY, toX, toY, steps = 8) {
  const start = { x: Math.round(fromX), y: Math.round(fromY) };
  const end = { x: Math.round(toX), y: Math.round(toY) };
  moveTo(start);
  xdotool(["mousedown", "1"]);
  try {
    for (let step = 1; step <= steps; step += 1) {
      moveTo({
        x: Math.round(start.x + ((end.x - start.x) * step) / steps),
        y: Math.round(start.y + ((end.y - start.y) * step) / steps),
      });
    }
  } finally {
    // A button left down lands on whatever the next check clicks, in a run nobody is watching.
    xdotool(["mouseup", "1"]);
  }
}

/** Move the pointer, unless it is already there. See the `--sync` note in `clickAt`. */
function moveTo(target) {
  const now = pointerLocation();
  if (now.x !== target.x || now.y !== target.y) {
    xdotool(["mousemove", "--sync", String(target.x), String(target.y)]);
  }
}

export function typeText(text) {
  xdotool(["type", "--delay", "5", text]);
}

/** A named key, e.g. Return or Escape. */
export function pressKey(key) {
  xdotool(["key", key]);
}
