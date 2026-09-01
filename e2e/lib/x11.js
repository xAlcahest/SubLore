import { execFileSync } from "node:child_process";

import { windowHeight, windowTitle, windowWidth } from "./paths.js";

/**
 * One window line of `xwininfo -tree` output, e.g.
 *   0x200004 "Sublore": ("sublore" "Sublore")  1024x700+0+0  +0+0
 *   0x200023 (has no name): ()  1024x602+0+49  +0+49
 * Offsets are printed as `+-1`, hence the optional minus inside each group.
 */
const WINDOW_LINE =
  /^\s*(0x[0-9a-f]+)\s+(?:"([^"]*)"|\(has no name\)):\s*\([^)]*\)\s+(\d+)x(\d+)\+(-?\d+)\+(-?\d+)\s+\+(-?\d+)\+(-?\d+)\s*$/i;

function xwininfo(args) {
  // stderr captured, not inherited: `mapState` classifies a destroyed window by reading it, and the
  // default leaves it undefined while printing "X Error: 9: BadDrawable" over every run's output.
  return execFileSync("xwininfo", args, {
    encoding: "utf8",
    timeout: 15000,
    stdio: ["ignore", "pipe", "pipe"],
  });
}

/**
 * @param {string} text
 * @returns {{id: string, name: string|null, width: number, height: number,
 *            relX: number, relY: number, absX: number, absY: number}[]}
 */
function parseWindowLines(text) {
  const windows = [];
  for (const line of text.split("\n")) {
    const match = WINDOW_LINE.exec(line);
    if (match === null) {
      continue;
    }
    windows.push({
      id: match[1],
      name: match[2] === undefined ? null : match[2],
      width: Number(match[3]),
      height: Number(match[4]),
      relX: Number(match[5]),
      relY: Number(match[6]),
      absX: Number(match[7]),
      absY: Number(match[8]),
    });
  }
  return windows;
}

/** The whole tree, for assertion messages that have to be readable at 3am in CI. */
export function rootTree() {
  return xwininfo(["-root", "-tree"]);
}

/** Every window on the display, toplevels and descendants alike. */
export function allWindows() {
  return parseWindowLines(rootTree());
}

/** Direct children only. mpv's own window lives inside our surface and must never match. */
export function childWindows(id) {
  return parseWindowLines(xwininfo(["-id", id, "-children"])).filter((child) => child.id !== id);
}

/** `IsViewable`, `IsUnMapped` or `IsUnviewable`. An unmapped surface is not on screen. */
export function mapState(id) {
  let described;
  try {
    described = xwininfo(["-id", id]);
  } catch (error) {
    // A window can be destroyed between being listed and being asked about, and X answers that with
    // `BadDrawable` on a request whose id no longer exists. That is a fact about the window, not a
    // failure of the harness, and reporting it as one cost a CI run its diagnosis (gate 2, run
    // 33366855143). Anything else is a real error and still throws.
    if (/No such window|BadDrawable/i.test(`${error.message}${error.stderr ?? ""}`)) {
      return "IsGone";
    }
    throw error;
  }
  const match = /Map State:\s*(\S+)/.exec(described);
  if (match === null) {
    throw new Error(`xwininfo -id ${id} printed no Map State line`);
  }
  return match[1];
}

/**
 * The app toplevel, selected by geometry and exact name. Never by name alone: GTK also creates a
 * 10x10 group-leader window that answers to the same name (BACKLOG M0.5 harness note).
 * @returns {{id: string, name: string|null, width: number, height: number,
 *            relX: number, relY: number, absX: number, absY: number}|null}
 */
export function findToplevel() {
  const tree = rootTree();
  const matches = parseWindowLines(tree).filter(
    (window) =>
      window.name === windowTitle && window.width === windowWidth && window.height === windowHeight,
  );
  if (matches.length > 1) {
    throw new Error(
      `expected exactly one ${windowWidth}x${windowHeight} "${windowTitle}" toplevel, found ` +
        `${matches.length} (${matches.map((w) => w.id).join(", ")}). ` +
        `A leftover app instance from an earlier run poisons every assertion.\n${tree}`,
    );
  }
  return matches.length === 1 ? matches[0] : null;
}

/**
 * Every toplevel whose geometry matches the app window, regardless of its name. Used by the title
 * assertion so a wrong title fails on the name, not on "no window found".
 */
export function findWindowsWithAppGeometry() {
  return allWindows().filter(
    (window) => window.width === windowWidth && window.height === windowHeight,
  );
}
