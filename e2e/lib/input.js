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

export function clickAt(x, y) {
  xdotool(["mousemove", "--sync", String(Math.round(x)), String(Math.round(y))]);
  xdotool(["click", "1"]);
}

export function typeText(text) {
  xdotool(["type", "--delay", "5", text]);
}
