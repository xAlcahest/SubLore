/**
 * N1 close gate (BACKLOG NOW block, owner decision 9).
 *
 * AC: "open a subtitle fixture, edit a cue, close the window: a dialog appears offering save,
 * discard and cancel. Cancel leaves the app open with the edit still there and the file on disk
 * untouched. Discard closes and leaves the file untouched. Save writes the edit and then closes."
 *
 * Deliberately not a WebDriver spec, for the same reason as shutdown-check.js: two of the three
 * answers end the process, and the W3C protocol exposes neither the exit status nor the survivor
 * list. Here Node is the parent, so both are exact.
 *
 * Every answer is proved by what it caused, never by what failed to happen (owner rule,
 * 2026-08-29): a click that misses a button leaves the app alive and the file intact just as a
 * working Cancel does, so each branch first proves the dialog was up and then proves it went away.
 */
import { execFileSync, spawn } from "node:child_process";
import console from "node:console";
import { copyFileSync, existsSync, mkdtempSync, readdirSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import { doubleClickAt, focusWindow, pressKey, typeText } from "../lib/input.js";
import {
  closeWindowTool,
  firstCueText,
  repoRoot,
  requireAppBinary,
  requireCloseWindowTool,
  requireDisplay,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { SUBTITLE_OPENED, waitForEditedLength, waitForLog } from "../lib/applog.js";
import { appEnv } from "../lib/env.js";
import { answerDialog } from "../lib/gtk-dialog.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { allWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 12;
let checksRun = 0;

/** The close dialog's window name. Frozen contract with src-tauri/src/strings.rs. */
const DIALOG_TITLE = "Unsaved changes";
/** The fixture both phases open. Its first cue is what the edit below changes. */
const SOURCE_FIXTURE = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");

/**
 * The first cue's length as the fixture carries it, read rather than pinned: a pinned number and an
 * edited fixture disagreeing puts the guard below back to accepting an unchanged commit, which is
 * the failure it was rebuilt to reject. See BACKLOG.md N9, S15.
 */
function firstCueChars(bytes) {
  const text = bytes.toString("utf8").split("\n\n")[0].split("\n").slice(2).join("\n");
  if (text.length === 0) {
    throw new Error(`no first cue text in ${SOURCE_FIXTURE}: the fixture changed shape`);
  }
  return text.length;
}

const UNEDITED_FIRST_CUE_CHARS = firstCueChars(readFileSync(SOURCE_FIXTURE));

/** The text committed into cue 1, which the save branch then looks for in the file. */
const EDIT_MARK = "SUBLORE_N1";

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`close gate check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/**
 * The dialog, or null. Guarded against a second one the way `findToplevel` is: two gates on one
 * document is a defect the re-entrancy guard exists to prevent, and picking one at random would
 * hide it (e2e/lib/x11.js).
 */
function findDialog() {
  const matches = allWindows().filter((window) => window.name === DIALOG_TITLE);
  if (matches.length > 1) {
    throw new Error(
      `expected at most one "${DIALOG_TITLE}" dialog, found ${matches.length} ` +
        `(${matches.map((w) => w.id).join(", ")}). A second gate over the first means one answer ` +
        `destroys the window the other is still asking about.\n${rootTree()}`,
    );
  }
  return matches.length === 1 ? matches[0] : null;
}

/**
 * Wait for the dialog, but tell the two failures apart: a gate that never opened and a setup that
 * never dirtied the document look identical from here, and only the first is a defect in the app.
 */
async function waitForDialog(state) {
  return waitFor(
    () => {
      if (state.exit !== null) {
        throw new Error(
          `the app exited (code ${state.exit.code}) instead of asking. Two causes look identical ` +
            "from here and both are failures: the gate let a dirty document close, which is the " +
            "regression this script exists to catch, or the setup never dirtied the document, " +
            "which means the run proved nothing. Check the file on disk and the app log to tell " +
            "them apart.",
        );
      }
      return findDialog();
    },
    { timeout: 15000, message: `a toplevel named "${DIALOG_TITLE}"` },
  ).catch((error) => {
    throw new Error(`${error.message}\nwindows on the display were:\n${rootTree()}`);
  });
}

/** Positive proof that an answer landed: the dialog it was given to is gone. */
async function waitForDialogGone(what) {
  return waitFor(() => (findDialog() === null ? true : null), {
    timeout: 10000,
    message: `the dialog to close after ${what}`,
  }).catch((error) => {
    throw new Error(
      `${error.message}\nThe answer did not reach a button, so whatever follows would pass for ` +
        `the wrong reason.\n${rootTree()}`,
    );
  });
}

/**
 * Cancel, driven by Escape rather than by a click. GTK answers Escape with the delete response and
 * the app's catch-all reads that as Cancel: deterministic, and free of the button geometry the
 * other two branches have to estimate.
 */
function dismissDialog(dialog) {
  focusWindow(dialog.id);
  pressKey("Escape");
}

/** The subtitle is passed as an argument, never typed: see `startup_files`. */
function launch(dataHome, file) {
  const app = spawn(requireAppBinary(), [file], {
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

/** Commit a marked edit into the first cue of the file the app opened, leaving it dirty. */
async function openAndDirty(toplevel, dataHome) {
  const at = (point) => ({ x: toplevel.absX + point.x, y: toplevel.absY + point.y });
  // The app says when the document is open, so this waits for that rather than for a number of
  // milliseconds someone measured on their own machine. The number was 3500 and the first real CI
  // run was slower than it (gate 2, run 33339776169).
  await waitForLog(dataHome, SUBTITLE_OPENED, { what: "the subtitle to be open" });

  // The backend having parsed the file is not the cue list being on screen, and nothing in the log
  // says when a row paints. So the edit is attempted rather than assumed: the app writes a line
  // when one lands, and this retries until it does. Before this, a click that arrived early left
  // the document clean and the failure surfaced four assertions later as "nothing changed on disk"
  // (gate 2, CI run 33341052061).
  const cue = at(firstCueText);
  for (let attempt = 1; ; attempt += 1) {
    focusWindow(toplevel.id);
    doubleClickAt(cue.x, cue.y);
    await sleep(600);
    typeText(EDIT_MARK);
    // Enter commits the inline edit into the document. Without it only the frontend knows about
    // the change and the backend session is still clean, which is a different case.
    pressKey("Return");
    // Not "an edit happened" but "the text changed": a field committed unchanged bumps the
    // revision and dirties the session while leaving the document identical, which is what CI kept
    // producing and what the earlier version of this wait accepted.
    if (await waitForEditedLength(dataHome, UNEDITED_FIRST_CUE_CHARS)) {
      return;
    }
    {
      if (attempt >= 6) {
        throw new Error(
          `the edit never changed the first cue in ${attempt} attempts. The app's log is the ` +
            `evidence: look for "edit committed ... now N chars" with N still ` +
            `${UNEDITED_FIRST_CUE_CHARS}.`,
        );
      }
      // Escape first: a half-open inline editor would take the next attempt's keystrokes and the
      // mark would end up in the document twice.
      pressKey("Escape");
      await sleep(500);
    }
  }
}

function requestClose(toplevel) {
  execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "inherit", timeout: 15000 });
}

async function reap(state, label) {
  await waitFor(() => state.exit !== null, { timeout: 15000, message: label });
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

/** Every file under the run's backup directory, which is where an overwrite archives the old copy. */
function backupsUnder(dataHome) {
  // XDG_DATA_HOME/<identifier>/backups: the identifier is src-tauri/tauri.conf.json.
  const root = path.join(dataHome, "com.sublore.app", "backups");
  if (!existsSync(root)) {
    return [];
  }
  const found = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else {
        found.push(full);
      }
    }
  };
  walk(root);
  return found;
}

async function main() {
  requireDisplay();
  requireAppBinary();
  requireCloseWindowTool();

  const source = SOURCE_FIXTURE;
  const original = readFileSync(source);

  // Phase one: cancel, then discard, on the same instance.
  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-closegate-"));
  const workFile = path.join(dataHome, "cancel-then-discard.srt");
  copyFileSync(source, workFile);

  let state = launch(dataHome, workFile);
  try {
    const toplevel = await waitForWindow(state);
    await openAndDirty(toplevel, dataHome);
    requestClose(toplevel);

    const dialog = await waitForDialog(state);
    // waitForDialog throws rather than resolving falsy, so `dialog !== null` here is guaranteed;
    // map state is the first fact about this window that is not (see gate2 register, L3).
    check(
      "the dialog is mapped, not just present in the tree",
      mapState(dialog.id) === "IsViewable",
    );

    dismissDialog(dialog);
    // The proof that the answer landed is `waitForDialogGone` throwing if it did not; a `check`
    // here would only inflate the counter.
    await waitForDialogGone("cancel");
    // The dialog is gone, so the answer landed; give the app the same grace the exit paths get
    // before calling it alive, or a late exit would pass for a survivor.
    await sleep(1000);
    check(
      "cancel left the app running",
      state.exit === null,
      `the app exited with ${JSON.stringify(state.exit)} after cancel`,
    );
    check(
      "cancel left the file on disk untouched",
      readFileSync(workFile).equals(original),
      "the file changed even though nothing was saved",
    );

    requestClose(toplevel);
    const again = await waitForDialog(state);
    check(
      "the second dialog is mapped, not just present in the tree",
      mapState(again.id) === "IsViewable",
    );

    answerDialog(again, "discard");
    // The proof that the answer landed is `waitForDialogGone` throwing if it did not; a `check`
    // here would only inflate the counter.
    await waitForDialogGone("discard");

    const survivors = await reap(state, "the app to exit after discard");
    check(
      "discard exited the app with status 0",
      state.exit.code === 0 && state.exit.signal === null,
      `exit was ${JSON.stringify(state.exit)}`,
    );
    check(
      "discard left the file on disk untouched",
      readFileSync(workFile).equals(original),
      "discard wrote to the file it was told to abandon",
    );
    check(
      "no process survived discard",
      survivors.length === 0,
      survivors.length === 0 ? "" : `survivors: ${survivors.join(", ")}`,
    );
  } finally {
    cleanup(state);
  }

  // Phase two: save, on a fresh instance and a fresh copy.
  const saveHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-closegate-save-"));
  const saveFile = path.join(saveHome, "save-on-close.srt");
  copyFileSync(source, saveFile);

  state = launch(saveHome, saveFile);
  try {
    const toplevel = await waitForWindow(state);
    await openAndDirty(toplevel, saveHome);
    requestClose(toplevel);

    const dialog = await waitForDialog(state);
    check(
      "the save branch's dialog is mapped, not just present in the tree",
      mapState(dialog.id) === "IsViewable",
    );

    answerDialog(dialog, "save");
    // The proof that the answer landed is `waitForDialogGone` throwing if it did not; a `check`
    // here would only inflate the counter.
    await waitForDialogGone("save");

    const saveSurvivors = await reap(state, "the app to exit after save");
    check(
      "no process survived save",
      saveSurvivors.length === 0,
      saveSurvivors.length === 0 ? "" : `survivors: ${saveSurvivors.join(", ")}`,
    );
    check(
      "save exited the app with status 0",
      state.exit.code === 0 && state.exit.signal === null,
      `exit was ${JSON.stringify(state.exit)}`,
    );

    // Not "the bytes changed": a truncated or corrupted file passes that. The saved file has to be
    // the original with the edit in it and nothing else moved (CONTRIBUTING.md §3).
    const saved = readFileSync(saveFile).toString("utf8");
    const before = original.toString("utf8");
    const savedBlocks = saved.split("\n\n");
    const beforeBlocks = before.split("\n\n");
    const differing = beforeBlocks
      .map((block, index) => (block === savedBlocks[index] ? null : index))
      .filter((index) => index !== null);
    check(
      "save wrote the edit and moved nothing else",
      savedBlocks.length === beforeBlocks.length &&
        differing.length === 1 &&
        savedBlocks[differing[0]].includes(EDIT_MARK),
      `blocks before ${beforeBlocks.length}, after ${savedBlocks.length}, differing ${JSON.stringify(differing)}`,
    );
    check(
      "save kept a timestamped backup of what it overwrote",
      backupsUnder(saveHome).length > 0,
      `no backup under ${path.join(saveHome, "com.sublore.app", "backups")}`,
    );
  } finally {
    cleanup(state);
  }

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `close gate guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`close gate check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
