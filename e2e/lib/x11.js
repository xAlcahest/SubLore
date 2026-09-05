import { execFileSync } from "node:child_process";

import { windowHeight, windowTitle, windowWidth } from "./paths.js";
import { requireLinuxBackend } from "./platform.js";

/**
 * One window line of `xwininfo -tree` output, e.g.
 *   0x200004 "Sublore": ("sublore" "Sublore")  1024x700+0+0  +0+0
 *   0x200023 (has no name): ()  1024x602+0+49  +0+49
 * Offsets are printed as `+-1`, hence the optional minus inside each group.
 */
const WINDOW_LINE =
  /^\s*(0x[0-9a-f]+)\s+(?:"([^"]*)"|\(has no name\)):\s*\([^)]*\)\s+(\d+)x(\d+)\+(-?\d+)\+(-?\d+)\s+\+(-?\d+)\+(-?\d+)\s*$/i;

function xwininfo(args) {
  // Every reader in this file goes through here, so this is the seam MW.1b replaces.
  requireLinuxBackend(
    "x11.js window inspection",
    "list the app's toplevels with their name, geometry, map state and children",
  );
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
  // Read again when a window goes away underneath the walk. `xwininfo -root -tree` lists the
  // children and then asks about each, so a window destroyed between the two makes X answer
  // `BadWindow` and the whole walk ends in "Can't query window tree". That is the same fact
  // `describeOrNull` already forgives one window at a time, and the check it cost a CI run on
  // 2026-09-05 is `shutdown`, whose whole subject is windows going away. Bounded: a tree that will
  // not settle in three reads is a display nobody can describe, and that still throws.
  for (let attempt = 1; ; attempt += 1) {
    try {
      return xwininfo(["-root", "-tree"]);
    } catch (error) {
      const said = `${error.stderr ?? ""}`;
      const raced = said.includes("Can't query window tree") || said.includes("BadWindow");
      if (!raced || attempt === 3) {
        throw error;
      }
    }
  }
}

/** Every window on the display, toplevels and descendants alike. */
export function allWindows() {
  return parseWindowLines(rootTree());
}

/** Direct children only. mpv's own window lives inside our surface and must never match. */
export function childWindows(id) {
  return parseWindowLines(xwininfo(["-id", id, "-children"])).filter((child) => child.id !== id);
}

/** What `xwininfo` says about one window, or null once it is gone. */
function describeOrNull(id) {
  try {
    return xwininfo(["-id", id]);
  } catch (error) {
    // A window can be destroyed between being listed and being asked about, and X answers that with
    // `BadDrawable` on a request whose id no longer exists. That is a fact about the window, not a
    // failure of the harness, and reporting it as one cost a CI run its diagnosis (gate 2, run
    // 33366855143). Anything else is a real error and still throws.
    if (/No such window|BadDrawable/i.test(`${error.message}${error.stderr ?? ""}`)) {
      return null;
    }
    throw error;
  }
}

/** `IsViewable`, `IsUnMapped` or `IsUnviewable`. An unmapped surface is not on screen. */
export function mapState(id) {
  const described = describeOrNull(id);
  if (described === null) {
    return "IsGone";
  }
  const match = /Map State:\s*(\S+)/.exec(described);
  if (match === null) {
    throw new Error(`xwininfo -id ${id} printed no Map State line`);
  }
  return match[1];
}

/** GTK's group-leader window, which is never the app. */
const groupLeaderWidth = 10;
const groupLeaderHeight = 10;

/**
 * The size X reports for one window, or null once it is gone.
 * @param {string} id
 * @returns {{width: number, height: number}|null}
 */
export function windowSize(id) {
  const described = describeOrNull(id);
  if (described === null) {
    return null;
  }
  const width = /^\s*Width:\s*(\d+)\s*$/m.exec(described);
  const height = /^\s*Height:\s*(\d+)\s*$/m.exec(described);
  if (width === null || height === null) {
    throw new Error(`xwininfo -id ${id} printed no Width and Height lines:\n${described}`);
  }
  return { width: Number(width[1]), height: Number(height[1]) };
}

/**
 * The app toplevel at the size the caller states, defaulting to the size the app starts at.
 * Selected by geometry and exact name, never by name alone: GTK also creates a 10x10 group-leader
 * window that answers to the same name (BACKLOG M0.5 harness note).
 * @param {{width?: number, height?: number}} size
 * @returns {{id: string, name: string|null, width: number, height: number,
 *            relX: number, relY: number, absX: number, absY: number}|null}
 */
export function findToplevel({ width = windowWidth, height = windowHeight } = {}) {
  // A size that cannot match anything would otherwise be a null and a caller's timeout, which reads
  // as the app never opening its window.
  if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0) {
    throw new Error(
      `findToplevel was asked for a ${JSON.stringify(width)}x${JSON.stringify(height)} window, ` +
        `which is not a size.`,
    );
  }
  // Asking for the group leader's geometry would hand back the one window this function exists to
  // refuse, so the guard survives the size becoming the caller's to choose.
  if (width === groupLeaderWidth && height === groupLeaderHeight) {
    throw new Error(
      `refusing to look for a ${width}x${height} "${windowTitle}" toplevel: that size is GTK's ` +
        `group-leader window, which is never the app.`,
    );
  }
  const tree = rootTree();
  const matches = parseWindowLines(tree).filter(
    (window) => window.name === windowTitle && window.width === width && window.height === height,
  );
  if (matches.length > 1) {
    throw new Error(
      `expected exactly one ${width}x${height} "${windowTitle}" toplevel, found ` +
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

/**
 * The smallest width the window declares to whatever places it, read from `WM_NORMAL_HINTS`.
 *
 * This is what actually keeps a person's window above the shell's floor: a window manager honours
 * the hint. Under the bare X server this battery runs on there is no window manager and nothing
 * enforces it, so the hint is what can be asserted here and the shell pulling a window back up is
 * not. See N32.
 *
 * @returns the declared minimum width, or null when the window declares none.
 */
export function minimumWidthHint(id) {
  const printed = execFileSync("xprop", ["-id", String(id), "WM_NORMAL_HINTS"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  const found = printed.replace(/\s+/g, " ").match(/minimum size: (\d+) by (\d+)/);
  return found === null ? null : Number(found[1]);
}
