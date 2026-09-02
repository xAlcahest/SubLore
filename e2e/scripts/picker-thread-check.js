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
 *
 * N7 lives here too, in a second run of the app over the same data home, because this is the only
 * thing that drives a chooser without WebDriver. AC: "choosing a folder, closing the app and
 * choosing again opens the chooser at the folder chosen last, proved by a check that fails when the
 * stored path is ignored." Where a chooser opened is read the only way it can be read from outside:
 * it is accepted with nothing chosen in it, and a `SelectFolder` chooser then hands back the folder
 * it is showing. The other two criteria are here too: a remembered folder that has been deleted is
 * dropped and its chooser still answers, and a chooser that was browsed elsewhere and then
 * cancelled leaves the memory alone — which is what the run after it opens at.
 */
import { spawn, spawnSync } from "node:child_process";
import console from "node:console";
import { copyFileSync, existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import { appLog, waitForLog } from "../lib/applog.js";
import {
  acceptChooser,
  answerChooser,
  cancelChooser,
  findChooser,
  waitForChooser,
} from "../lib/chooser.js";
import { appEnv } from "../lib/env.js";
import { clickAt, focusWindow, pressKey } from "../lib/input.js";
import {
  repoRoot,
  requireAppBinary,
  requireDisplay,
  requireTool,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 14;
let checksRun = 0;

/**
 * The one point this script clicks, relative to the toplevel origin, measured rather than guessed.
 *
 * It is the transcription status line, which is a paragraph nothing can focus, spanning the whole
 * window inside the chrome. Measured from a screenshot of this check's own window at 1024x700: the
 * band runs y 154 to 183, so this is its middle. When this check reports that no chooser opened,
 * this is what to re-measure, from a screenshot of this check's own window.
 */
const CHROME_TEXT = { x: 900, y: 168 };

/**
 * Tab stops from `CHROME_TEXT` to each rail control, in ProjectPanel.tsx's own order: Choose,
 * Create, Open, Delete project, the episode field, Add episode, Choose again.
 *
 * The rail is reached by the keyboard rather than by pixel because it scrolls and its viewport is
 * 13.5rem tall whatever the window is (src/styles/shell.css) — so no window size brings the file
 * row above its fold, and the row's pixel is a dozen font-dependent line heights the harness does
 * not choose. A tab stop is the app's own DOM order. Clicking a paragraph outside the rail puts the
 * focus navigation starting point there, so these counts hold wherever the rail happens to sit.
 *
 * A disabled control is not a tab stop: Add episode is skipped throughout because this check never
 * types an episode title, and Create and Open are skipped until a folder has been chosen — which is
 * why Choose is one stop in from the chrome whether or not a project is open.
 */
const CHOOSE_FOLDER_TABS = 1;
const CREATE_PROJECT_TABS = 2;
const CHOOSE_FILE_TABS = 6;

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

/**
 * Put the keyboard focus on the rail control this many tab stops past the chrome's status line.
 * `at` turns a point in the window into the point on the screen it is drawn at.
 */
async function focusRailControl(toplevel, at, tabs) {
  focusWindow(toplevel.id);
  clickAt(at(CHROME_TEXT).x, at(CHROME_TEXT).y);
  await sleep(300);
  for (let step = 0; step < tabs; step += 1) {
    pressKey("Tab");
    await sleep(80);
  }
}

/**
 * Press a rail control until a chooser answers, because a button exists before the webview has
 * wired it up. The whole walk is repeated on each attempt rather than the last keystroke: a Tab
 * that missed would otherwise leave every later press on a control nobody chose.
 */
async function pressUntilChooser(toplevel, at, tabs, title, { attempts = 8, alive } = {}) {
  let last = null;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    // A chooser the last attempt opened just too late to be seen is the one being asked for, and it
    // covers the point the walk below clicks: retrying over it would type into the dialog.
    const late = attempt === 0 ? null : findChooser(title);
    if (late !== null) {
      return late;
    }
    await focusRailControl(toplevel, at, tabs);
    pressKey("space");
    try {
      return await waitForChooser(title, { timeout: 4000, alive });
    } catch (error) {
      last = error;
    }
  }
  throw new Error(
    `no chooser named "${title}" after ${attempts} presses of the control ${tabs} tab stops past ` +
      `${JSON.stringify(CHROME_TEXT)}. Either that point is no longer the chrome's status line, or ` +
      `the rail's focus order changed.\n${last?.message ?? ""}`,
  );
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
    // The script owns the child, so it can tell a dead app from a slow chooser. A spec cannot.
    const alive = () => state.exit === null;

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

    // The folder half. `pressUntilChooser` throws unless a chooser is on screen, so a `check` here
    // would only ever be true and would inflate the counter.
    const folderChooser = await pressUntilChooser(toplevel, at, CHOOSE_FOLDER_TABS, FOLDER_TITLE, {
      alive,
    });
    await answerChooser(folderChooser, projectFolder, "folder");
    check(
      "the folder chooser handed back the folder it was given",
      await said(
        dataHome,
        new RegExp(`chooser: chose a project-folder: ${quoted(projectFolder)}$`, "m"),
      ),
      `${projectFolder} never came back. The app's log held:\n${appLog(dataHome)}`,
    );

    await focusRailControl(toplevel, at, CREATE_PROJECT_TABS);
    pressKey("space");
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

    // The file half. Its row exists only once a project is open (ProjectPanel.tsx), and since T2 it
    // sits below the rail's fold, which is why it is reached by focus rather than by pixel.
    const fileChooser = await pressUntilChooser(toplevel, at, CHOOSE_FILE_TABS, FILE_TITLE, {
      alive,
    });
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
    const cancelled = await pressUntilChooser(toplevel, at, CHOOSE_FILE_TABS, FILE_TITLE, {
      alive,
    });
    await cancelChooser(cancelled, "file");
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

    // N7, first half: a chooser browsed away from its opening folder and then dismissed. What the
    // second run below proves depends on this one having really been cancelled, so it is asserted
    // here rather than assumed: an Escape that missed would leave the same log as a memory the
    // cancellation never touched.
    const FOLDER_CANCELLED = /chooser: the project-folder choice was cancelled/g;
    const cancelledBefore = (appLog(dataHome).match(FOLDER_CANCELLED) ?? []).length;
    const browsed = await pressUntilChooser(toplevel, at, CHOOSE_FOLDER_TABS, FOLDER_TITLE, {
      alive,
    });
    // Alt+Home is GTK's own "go to the home folder", so this cancellation happens somewhere other
    // than the folder that was chosen. A memory written from where the chooser was browsing would
    // be the home folder, and the second run would say so.
    focusWindow(browsed.id);
    pressKey("alt+Home");
    await cancelChooser(browsed, "folder");
    check(
      "a folder chooser browsed elsewhere and then dismissed comes back as a cancellation",
      await waitFor(
        () => (appLog(dataHome).match(FOLDER_CANCELLED) ?? []).length > cancelledBefore,
        { timeout: 15000, message: "the cancellation this Escape caused" },
      ).then(
        () => true,
        () => false,
      ),
      `Escape produced no cancellation beyond the ${cancelledBefore} already logged. ` +
        `The app's log held:\n${appLog(dataHome)}`,
    );
  } finally {
    cleanup(state);
    await reap(state);
  }

  // N7, second half: the same data home, a new process. Everything above is over, and what the app
  // knows about the first run is now only what it wrote down.
  const CHOSE_FOLDER = new RegExp(
    `chooser: chose a project-folder: ${quoted(projectFolder)}$`,
    "gm",
  );
  const chosenBefore = (appLog(dataHome).match(CHOSE_FOLDER) ?? []).length;
  const second = launch(dataHome);
  try {
    const toplevel = await waitForWindow(second);
    const at = (point) => ({ x: toplevel.absX + point.x, y: toplevel.absY + point.y });
    const alive = () => second.exit === null;

    // Accepted with nothing chosen in it, so the folder it hands back is the folder it opened at.
    const reopened = await pressUntilChooser(toplevel, at, CHOOSE_FOLDER_TABS, FOLDER_TITLE, {
      alive,
    });
    await acceptChooser(reopened, "folder");
    check(
      "the folder chooser opened at the folder chosen before the app was closed",
      await waitFor(() => (appLog(dataHome).match(CHOSE_FOLDER) ?? []).length > chosenBefore, {
        timeout: 15000,
        message: `a second choice of ${projectFolder}`,
      }).then(
        () => true,
        () => false,
      ),
      `the chooser handed back something other than ${projectFolder}, so it did not open there. ` +
        `The app's log held:\n${appLog(dataHome)}`,
    );

    // A remembered folder that is gone is a folder the user moved or deleted between sessions.
    rmSync(projectFolder, { recursive: true, force: true });
    const defaulted = await pressUntilChooser(toplevel, at, CHOOSE_FOLDER_TABS, FOLDER_TITLE, {
      alive,
    });
    check(
      "a remembered folder that is no longer there is dropped instead of handed to the chooser",
      await said(
        dataHome,
        new RegExp(
          `chooser: the project-folder chooser's remembered folder is gone.*${quoted(projectFolder)}$`,
          "m",
        ),
      ),
      `nothing said ${projectFolder} was gone, so the chooser was opened at a folder that is not ` +
        `there. The app's log held:\n${appLog(dataHome)}`,
    );
    await answerChooser(defaulted, dataHome, "folder");
    check(
      "and that chooser still opened, and still answers",
      await said(
        dataHome,
        new RegExp(`chooser: chose a project-folder: ${quoted(dataHome)}$`, "m"),
      ),
      `${dataHome} never came back, so the chooser that opened over a deleted folder could not be ` +
        `answered. The app's log held:\n${appLog(dataHome)}`,
    );
    check(
      "the app survived the second run",
      second.exit === null,
      `the app exited with ${JSON.stringify(second.exit)}`,
    );
  } finally {
    cleanup(second);
    await reap(second);
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
