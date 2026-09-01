/**
 * N1c: the file picker must not start a second GTK thread (BACKLOG N1c).
 *
 * AC: "after choosing a project folder and a project file, /proc/self/task holds no thread running
 * gtk_main_iteration other than the main one, asserted by a check that fails when the plugin path
 * is restored."
 *
 * Deliberately not a WebDriver spec, for two reasons. Node has to be the app's parent to own its
 * pid, and WebKitWebDriver answers Element Click with "unsupported operation" against a wry webview,
 * so the buttons are reached through XTEST. `e2e/README.md` still forbids the WebDriver specs from
 * opening this picker: under Xvfb nobody answers a native dialog unless somebody is written to, and
 * that somebody is this file.
 *
 * How a GTK-iterating thread is found. `comm` cannot do it — rfd spawns with a bare
 * `std::thread::spawn`, so its thread answers to `sublore` like every other. `wchan` cannot either:
 * it reads `poll_schedule_timeout` while the dialog is up and `futex_do_wait` afterwards, both
 * shared with dozens of innocent threads. `/proc/PID/task/TID/stack` needs CAP_SYS_ADMIN and holds
 * the kernel stack anyway. What works is a userspace backtrace, `eu-stack -p PID -n 0`, and a
 * predicate on the GTK symbol. It must be the GTK symbol and not GLib's `g_main_context_iteration`:
 * the GLib worker and the GDBus thread both drive their own context from startup and would be two
 * false positives on every run.
 *
 * Every step is proved by what it caused, never by what failed to happen: the folder half by the
 * project file appearing in the folder the picker returned, the file half and the cancellation by
 * the lines `project::choose_path` writes. A dialog that closed proves nothing on its own — a
 * keystroke that missed closes it just as well as one that landed.
 */
import { spawn, spawnSync } from "node:child_process";
import console from "node:console";
import { copyFileSync, existsSync, mkdtempSync, readdirSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import { appLog, waitForLog } from "../lib/applog.js";
import { appEnv } from "../lib/env.js";
import { clickAt, focusWindow, pressKey, typeText } from "../lib/input.js";
import {
  repoRoot,
  requireAppBinary,
  requireDisplay,
  requireTool,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { allWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 9;
let checksRun = 0;

/** Points in the current shell, relative to the toplevel origin. M2.0 must revisit these. */
const CHOOSE_FOLDER = { x: 52, y: 64 };
const CREATE_PROJECT = { x: 136, y: 64 };
const CHOOSE_FILE = { x: 234, y: 234 };

/** The two chooser titles. Frozen contract with src-tauri/src/strings.rs. */
const FOLDER_TITLE = "Choose a project folder";
const FILE_TITLE = "Choose a video or subtitle file";

/**
 * A thread that iterates GTK. `gtk_main` is included though nothing emits it today: a worker
 * running a nested main loop is the same defect and would otherwise walk past this.
 */
const ITERATES_GTK = /^gtk_main(_iteration(_do)?)?\b/;

/** The first walk of a session, cold, took a minute against the 225 MB debug binary. */
const WALK_TIMEOUT = 180000;

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`picker thread check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/** A path typed by this script and then looked for in the app's log. */
function quoted(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Did the app say this within the deadline? A boolean rather than a throw, so what follows is a
 * real assertion carrying the log rather than a `check` that can only ever be true.
 */
async function said(dataHome, pattern, timeout = 15000) {
  try {
    await waitForLog(dataHome, pattern, { timeout });
    return true;
  } catch {
    return false;
  }
}

/**
 * Refuse to report a clean process when the tool was never allowed to look.
 *
 * Under `ptrace_scope=1` a tracer must be an ancestor of its tracee, and here eu-stack and the app
 * are siblings under Node. eu-stack then prints nothing at all, and a check that only counted hits
 * would call that zero.
 */
function requirePtrace() {
  let scope;
  try {
    scope = readFileSync("/proc/sys/kernel/yama/ptrace_scope", "utf8").trim();
  } catch {
    // No Yama in this kernel means nothing is restricting ptrace.
    return;
  }
  if (scope !== "0") {
    throw new Error(
      `E2E prerequisite missing: kernel.yama.ptrace_scope is ${scope}, so eu-stack cannot read the ` +
        `app's threads (it is a sibling of the app, not an ancestor). Run: ` +
        `sudo sysctl -w kernel.yama.ptrace_scope=0`,
    );
  }
}

/** The thread ids the kernel lists for a process right now. */
function threadIds(pid) {
  return readdirSync(`/proc/${pid}/task`)
    .map(Number)
    .filter((tid) => Number.isInteger(tid) && tid > 0);
}

/**
 * Every thread's userspace frames, or a failure saying why they could not be read.
 *
 * The completeness rules are the point: eu-stack exits 2 with empty output when it cannot attach,
 * and a grep over that output reads as "no GTK thread anywhere". So its own status is checked, its
 * per-thread errors are checked, and every thread that existed both before and after the walk has
 * to appear in it.
 * @returns {Map<number, string[]>}
 */
function walkThreads(pid) {
  const before = threadIds(pid);
  const walk = spawnSync("eu-stack", ["-p", String(pid), "-n", "0"], {
    encoding: "utf8",
    timeout: WALK_TIMEOUT,
    maxBuffer: 32 * 1024 * 1024,
  });
  const after = threadIds(pid);
  if (walk.error !== undefined && walk.error !== null) {
    throw new Error(`eu-stack could not run against ${pid}: ${walk.error.message}`);
  }
  const stderr = (walk.stderr ?? "").trim();
  // 0 is a complete walk and 1 is a walk with per-thread errors; 2 is "could not look at all".
  if (walk.status !== 0 && walk.status !== 1) {
    throw new Error(
      `eu-stack exited ${walk.status} against ${pid}, so nothing was read:\n${stderr}`,
    );
  }
  // A thread that exits mid-walk is a fact about the process, not a failure of the tool.
  const unexplained = stderr
    .split("\n")
    .filter((line) => line.trim() !== "" && !/No such process/.test(line));
  if (unexplained.length > 0) {
    throw new Error(`eu-stack could not read every thread of ${pid}:\n${unexplained.join("\n")}`);
  }

  const frames = new Map();
  let tid = null;
  for (const line of (walk.stdout ?? "").split("\n")) {
    const header = /^TID (\d+):/.exec(line);
    if (header !== null) {
      tid = Number(header[1]);
      frames.set(tid, []);
      continue;
    }
    const frame = /^#\d+\s+(.*)$/.exec(line);
    if (frame !== null && tid !== null) {
      const rest = frame[1].trim();
      const named = /^0x[0-9a-f]+\s+(.*)$/.exec(rest);
      frames.get(tid).push(named === null ? rest : named[1].trim());
    }
  }

  const missing = before
    .filter((id) => after.includes(id))
    .filter((id) => (frames.get(id) ?? []).length === 0);
  if (missing.length > 0) {
    throw new Error(
      `eu-stack walked ${frames.size} of the ${before.length} threads of ${pid} and skipped ` +
        `${missing.join(", ")}, which were alive throughout. A partial walk cannot say the ` +
        `process is clean.\n${stderr}`,
    );
  }
  return frames;
}

/**
 * Which threads are iterating GTK, sampled more than once and unioned: the answer has to be the
 * same however the sampling lands.
 * @returns {{main: boolean, others: Map<number, string[]>}}
 */
function gtkThreads(pid, samples = 3) {
  let main = false;
  const others = new Map();
  for (let sample = 0; sample < samples; sample += 1) {
    for (const [tid, frames] of walkThreads(pid)) {
      if (!frames.some((frame) => ITERATES_GTK.test(frame))) {
        continue;
      }
      if (tid === pid) {
        main = true;
      } else {
        others.set(tid, frames);
      }
    }
  }
  return { main, others };
}

/**
 * Everything the walk found, for the message when the detector saw nothing it recognises.
 *
 * Without it a control failure says only "nothing anywhere", which is the same sentence for a tool
 * that could not attach, a stripped library, and a main loop parked in a frame this does not know.
 */
function describeWalk(pid) {
  try {
    return [...walkThreads(pid)]
      .map(([tid, frames]) => `TID ${tid}: ${frames.slice(0, 6).join(" <- ") || "(no frames)"}`)
      .join("\n");
  } catch (error) {
    return `the walk itself failed: ${error.message}`;
  }
}

/** The offending threads, named with their frames: the caller below sits under the GTK symbol. */
function describeThreads(others) {
  return [...others]
    .map(([tid, frames]) => `TID ${tid}:\n${frames.map((frame) => `  ${frame}`).join("\n")}`)
    .join("\n");
}

function launch(dataHome) {
  const app = spawn(requireAppBinary(), [], {
    detached: true,
    stdio: ["ignore", "inherit", "inherit"],
    env: appEnv({ XDG_DATA_HOME: dataHome }),
  });
  const state = { app, pgid: app.pid, exit: null, spawnError: null };
  app.on("error", (error) => {
    state.spawnError = error;
  });
  app.on("exit", (code, signal) => {
    state.exit = { code, signal };
  });
  return state;
}

async function waitForWindow(state) {
  return waitFor(
    () => {
      if (state.spawnError !== null) {
        throw new Error(`the app failed to start: ${state.spawnError.message}`);
      }
      if (state.exit !== null) {
        throw new Error(`the app exited before its window appeared (code ${state.exit.code})`);
      }
      return findToplevel();
    },
    { timeout: 30000, message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel` },
  );
}

/**
 * The chooser that is on screen, or null. Only viewable ones count: the plugin's chooser is
 * unmapped rather than destroyed when it is answered, so under a plugin build the tree still holds
 * every chooser an earlier step opened. Two viewable at once is a real defect and fails here.
 */
function findChooser(title) {
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

async function waitForChooser(state, title, timeout = 20000) {
  return waitFor(
    () => {
      if (state.exit !== null) {
        throw new Error(`the app exited (code ${state.exit.code}) instead of raising a chooser`);
      }
      return findChooser(title);
    },
    { timeout, message: `a toplevel named "${title}"` },
  ).catch((error) => {
    throw new Error(`${error.message}\nwindows on the display were:\n${rootTree()}`);
  });
}

/**
 * Click until a chooser answers, because the window exists before the webview has painted it.
 *
 * A fixed wait before the first click is a number measured on one machine: 2500 ms was enough here
 * and would be a coin toss on a loaded runner, where it would fail late and read as the picker
 * being broken. Clicking again costs nothing when the button is already there.
 */
async function clickUntilChooser(state, toplevel, point, title, attempts = 8) {
  let last = null;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    focusWindow(toplevel.id);
    clickAt(point.x, point.y);
    try {
      return await waitForChooser(state, title, 4000);
    } catch (error) {
      last = error;
    }
  }
  throw new Error(`no chooser named "${title}" after ${attempts} clicks.\n${last?.message ?? ""}`);
}

/** An answered chooser is destroyed here and merely unmapped by the plugin, so both count. */
async function chooserClosed(chooser, timeout) {
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
async function answerChooser(chooser, chosen, what) {
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
    if (await chooserClosed(chooser, 5000)) {
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

async function reap(state) {
  return waitFor(
    () => {
      const alive = processGroupMembers(state.pgid);
      return alive.length === 0 ? [] : null;
    },
    { timeout: 10000, message: `process group ${state.pgid} to be empty` },
  ).catch(() => processGroupMembers(state.pgid));
}

function cleanup(state) {
  try {
    if (processGroupMembers(state.pgid).length > 0) {
      killGroup(state.pgid);
    }
  } catch {
    // Teardown must not mask the failure that got us here.
  }
}

async function main() {
  requireDisplay();
  requireAppBinary();
  requireTool("eu-stack", "read the app's thread backtraces (package: elfutils)");
  requireTool("xdotool", "click and type into the app and its choosers");
  requirePtrace();

  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-picker-"));
  const projectFolder = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-picker-project-"));
  const subtitle = path.join(dataHome, "episode-01.srt");
  copyFileSync(
    path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt"),
    subtitle,
  );

  const state = launch(dataHome);
  try {
    const toplevel = await waitForWindow(state);
    const at = (point) => ({ x: toplevel.absX + point.x, y: toplevel.absY + point.y });
    const pid = state.app.pid;

    // Before any picker: the detector finds the one thread that is supposed to be there, and finds
    // no other. Without this a broken detector reports a clean process and the run reads as green.
    // Waited for rather than sampled once: the window exists before the main loop settles into
    // gtk_main_iteration_do, and on a slower machine the first sample lands before it does.
    const before = await waitFor(() => (gtkThreads(pid).main ? gtkThreads(pid) : null), {
      timeout: 30000,
      interval: 1000,
      message: `a gtk_main_iteration frame on TID ${pid}`,
    }).catch(() => gtkThreads(pid));
    check(
      "eu-stack sees the main thread iterating GTK, so it can see a thread that does",
      before.main,
      `no gtk_main_iteration frame on TID ${pid} in 30s: the walk found nothing anywhere, which is ` +
        `what a detector that cannot look also reports. The walk saw:\n${describeWalk(pid)}`,
    );
    check(
      "no thread but the main one iterates GTK before a picker is opened",
      before.others.size === 0,
      describeThreads(before.others),
    );

    // The folder half. `clickUntilChooser` throws unless a chooser is on screen, so a `check` here
    // would only ever be true and would inflate the counter.
    const folderChooser = await clickUntilChooser(state, toplevel, at(CHOOSE_FOLDER), FOLDER_TITLE);
    await answerChooser(folderChooser, projectFolder, "folder");
    check(
      "the folder chooser handed back the folder it was given",
      await said(
        dataHome,
        new RegExp(`chooser: chose a project-folder: ${quoted(projectFolder)}$`, "m"),
      ),
      `${projectFolder} never came back. The app's log held:\n${appLog(dataHome)}`,
    );

    focusWindow(toplevel.id);
    clickAt(at(CREATE_PROJECT).x, at(CREATE_PROJECT).y);
    const projectFile = path.join(projectFolder, "project.sublore");
    const created = await waitFor(() => existsSync(projectFile), {
      timeout: 20000,
      message: `${projectFile} to be created`,
    })
      .then(() => true)
      .catch(() => false);
    check(
      "the app created the project in the folder the picker returned",
      created,
      `${projectFile} does not exist, so the chosen folder never reached the panel's field and ` +
        `Create had nothing to work with. The app's log held:\n${appLog(dataHome)}`,
    );

    // The file half. Its row exists only once a project is open (ProjectPanel.tsx).
    focusWindow(toplevel.id);
    const fileChooser = await clickUntilChooser(state, toplevel, at(CHOOSE_FILE), FILE_TITLE);
    await answerChooser(fileChooser, subtitle, "file");
    check(
      "the file chooser handed back the file it was given",
      await said(dataHome, new RegExp(`chooser: chose a project-file: ${quoted(subtitle)}$`, "m")),
      `${subtitle} never came back. The app's log held:\n${appLog(dataHome)}`,
    );

    // A cancelled choice is an outcome, not a failure, and the panel has to be told so. Counted
    // from before the Escape: matching the whole log would pass on a cancellation from earlier.
    const CANCELLED = /chooser: the project-file choice was cancelled/g;
    const before_escape = (appLog(dataHome).match(CANCELLED) ?? []).length;
    const cancelled = await clickUntilChooser(state, toplevel, at(CHOOSE_FILE), FILE_TITLE);
    focusWindow(cancelled.id);
    pressKey("Escape");
    check(
      "a chooser dismissed with Escape comes back as a cancellation",
      await waitFor(() => (appLog(dataHome).match(CANCELLED) ?? []).length > before_escape, {
        timeout: 15000,
        message: "the cancellation this Escape caused",
      }).then(
        () => true,
        () => false,
      ),
      `Escape produced no cancellation beyond the ${before_escape} already logged. ` +
        `The app's log held:\n${appLog(dataHome)}`,
    );

    const after = gtkThreads(pid);
    check(
      "the main thread is still the one iterating GTK after both pickers",
      after.main,
      `no gtk_main_iteration frame on TID ${pid} any more, so this scan proves nothing`,
    );
    // The acceptance criterion. rfd's thread is a OnceLock that nothing can stop once it exists,
    // so this asks whether one was ever created, not whether it went away.
    check(
      "no thread but the main one iterates GTK after a folder and a file have been chosen",
      after.others.size === 0,
      `${describeThreads(after.others)}\n` +
        `A thread under rfd::backend::gtk3::utils::GtkGlobalThread means project::choose_path is ` +
        `back on tauri-plugin-dialog. See BACKLOG N1c.`,
    );
    check(
      "the app survived both pickers",
      state.exit === null,
      `the app exited with ${JSON.stringify(state.exit)}`,
    );
  } finally {
    cleanup(state);
    await reap(state);
  }

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `picker thread guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`picker thread check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
