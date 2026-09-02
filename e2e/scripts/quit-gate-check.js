/**
 * The exit that is not a window close (BACKLOG N6).
 *
 * The close gate hangs off `CloseRequested`, so it only ever saw the window's X button. A quit
 * asked for programmatically — `AppHandle::exit`, which is what a menu's Quit item and Ctrl+Q call
 * — reached `ExitRequested`, where nothing prevented the exit, and the unsaved work left with the
 * process in silence (CONTRIBUTING.md §3).
 *
 * AC: "quitting through every route the app offers asks the same question the window's X button
 * asks, proved by a check that drives the non-X route." So this script never touches the X button:
 * `close-gate-check.js` owns that route, and this one drives the quit.
 *
 * T3 built the caller this check was written for, so the debug-only hook it used to drive
 * (`SUBLORE_QUIT_ON_FILE`) is gone and the routes here are the app's own: the File menu's Quit item,
 * driven from the keyboard the way shell-layout.md says a menu is driven, and Ctrl+Q in the save
 * branch. That the quit really went that way is check 1 — the app writes one line from the `quit`
 * command and nowhere else — because without it the run could prove nothing while looking green,
 * which WORKFLOW §4c names as the worst available outcome.
 *
 * Not a WebDriver spec, for close-gate-check.js's reason: two of the three answers end the process,
 * and the W3C protocol reports neither the exit status nor the survivors. Here Node is the parent.
 */
import { spawn } from "node:child_process";
import console from "node:console";
import { copyFileSync, mkdtempSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import { SUBTITLE_OPENED, appLog, waitForEditedLength, waitForLog } from "../lib/applog.js";
import { appEnv } from "../lib/env.js";
import { answerDialog, findUnsavedDialog, waitForUnsavedDialogGone } from "../lib/gtk-dialog.js";
import { doubleClickAt, focusWindow, pressKey, typeText } from "../lib/input.js";
import {
  firstCueText,
  repoRoot,
  requireAppBinary,
  requireDisplay,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { findToplevel, rootTree } from "../lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 17;
let checksRun = 0;

/** The line the `quit` command writes, and nothing else does. Contract with src-tauri/src/lib.rs. */
const QUIT_TAKEN = "quitting through AppHandle::exit";

/** How long the menu is given to open before the keys that walk it are sent. */
const MENU_MS = 600;

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
const EDIT_MARK = "SUBLORE_N6";

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`quit gate check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/**
 * Wait for the gate, and tell the two failures apart: an app that exited on the quit is the defect
 * this script exists to catch, and an app that is still up with no dialog is a setup that never
 * dirtied the document. Only the first is a defect in the product.
 */
async function waitForDialog(state, what) {
  return waitFor(
    () => {
      if (state.exit !== null) {
        throw new Error(
          `the app exited (code ${state.exit.code}) on the quit instead of ${what}. The quit route ` +
            "carried the unsaved edit away with the process, which is BACKLOG N6 itself. If the " +
            "app log holds no committed edit, the setup never dirtied the document and the run " +
            "proved nothing instead.",
        );
      }
      return findUnsavedDialog();
    },
    { timeout: 20000, message: `the unsaved-changes dialog (${what})` },
  ).catch((error) => {
    throw new Error(`${error.message}\nwindows on the display were:\n${rootTree()}`);
  });
}

/**
 * Launch the app on `file`. The subtitle is passed as an argument, never typed: see
 * `startup_files`.
 */
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
  // milliseconds someone measured on their own machine (gate 2, run 33339776169).
  await waitForLog(dataHome, SUBTITLE_OPENED, { what: "the subtitle to be open" });

  // Attempted rather than assumed: the cue list paints after the backend has parsed the file, and
  // a click that lands early leaves the document clean while every later assertion still runs.
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
    // revision and dirties the session while leaving the document identical (gate 2, run
    // 33363671401).
    if (await waitForEditedLength(dataHome, UNEDITED_FIRST_CUE_CHARS)) {
      return;
    }
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

/**
 * Quit through the File menu, from the keyboard: Alt opens the first dropdown, Up puts the cursor
 * on its last enabled item, which is Quit, and Enter activates it (shell-layout.md's key table).
 * Up rather than a count of Downs, because Save is enabled in one phase here and disabled in
 * another and the item above Quit is not the same one in both.
 *
 * There is no DOM to wait on from here, so the menu is given a moment and the proof that the route
 * was driven is the app's own line, which `waitForQuitTaken` reads.
 */
async function quitFromTheMenu(toplevel) {
  focusWindow(toplevel.id);
  pressKey("alt");
  await sleep(MENU_MS);
  pressKey("Up");
  await sleep(200);
  pressKey("Return");
}

/** The same command through its accelerator, which is the other route a person has to it. */
function quitWithCtrlQ(toplevel) {
  focusWindow(toplevel.id);
  pressKey("ctrl+q");
}

/** How many times a line the app writes has appeared in its log. */
function occurrences(dataHome, line) {
  return appLog(dataHome).split(line).length - 1;
}

/** How many quits the app has taken through `AppHandle::exit` so far, from its own log. */
function quitsTaken(dataHome) {
  return occurrences(dataHome, QUIT_TAKEN);
}

/** Positive proof the non-X route was the one driven, and not something else closing the window. */
async function waitForQuitTaken(dataHome, wanted) {
  return waitFor(() => (quitsTaken(dataHome) >= wanted ? true : null), {
    timeout: 20000,
    message: `the app to log "${QUIT_TAKEN}" ${wanted} time(s)`,
  }).catch((error) => {
    throw new Error(
      `${error.message}\nThe app never reached its quit command, so nothing drove the route this ` +
        "check is about: either the File menu did not open on Alt, or its last enabled item is no " +
        "longer Quit.\n" +
        `the app's log held:\n${appLog(dataHome) || "(nothing yet)"}`,
    );
  });
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

async function main() {
  requireDisplay();
  requireAppBinary();

  const source = SOURCE_FIXTURE;
  const original = readFileSync(source);

  // Phase one: cancel, then discard, on the same instance. The second quit is the point of doing
  // both here: an answered gate must not wave a later quit through.
  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-quitgate-"));
  const workFile = path.join(dataHome, "cancel-then-discard.srt");
  copyFileSync(source, workFile);

  let state = launch(dataHome, workFile);
  try {
    const toplevel = await waitForWindow(state);
    await openAndDirty(toplevel, dataHome);

    await quitFromTheMenu(toplevel);
    await waitForQuitTaken(dataHome, 1);
    check(
      "the menu's Quit item went through AppHandle::exit and not through the window",
      quitsTaken(dataHome) === 1,
      "the route this check exists for was never driven",
    );

    const dialog = await waitForDialog(state, "asking about the unsaved edit");
    check(
      "the quit raised the same unsaved-changes dialog the X button raises",
      dialog !== null,
      "the quit did not ask, and the edit would have gone with the process",
    );

    // GTK answers Escape with the delete response and the app reads that as Cancel: deterministic,
    // and free of the button geometry the other answers reach through mnemonics.
    answerDialog(dialog, "cancel");
    await waitForUnsavedDialogGone("cancel");
    // The dialog is gone, so the answer landed; give the app the same grace the exit paths get
    // before calling it alive, or a late exit would pass for a survivor.
    await sleep(1000);
    check(
      "cancel kept the app the quit asked to end",
      state.exit === null,
      `the app exited with ${JSON.stringify(state.exit)} after cancel`,
    );
    check(
      "cancel left the file on disk untouched",
      readFileSync(workFile).equals(original),
      "the file changed even though nothing was saved",
    );

    await quitFromTheMenu(toplevel);
    await waitForQuitTaken(dataHome, 2);
    const again = await waitForDialog(state, "asking about the second quit");
    check(
      "a second quit asks again instead of closing on the answer that was cancelled",
      again !== null,
      "the gate waved the second quit through, and the edit is gone with it",
    );

    answerDialog(again, "discard");
    await waitForUnsavedDialogGone("discard");

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
  const saveHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-quitgate-save-"));
  const saveFile = path.join(saveHome, "save-on-quit.srt");
  copyFileSync(source, saveFile);

  state = launch(saveHome, saveFile);
  try {
    const toplevel = await waitForWindow(state);
    await openAndDirty(toplevel, saveHome);

    quitWithCtrlQ(toplevel);
    await waitForQuitTaken(saveHome, 1);
    check(
      "the save branch's Ctrl+Q went through AppHandle::exit as well",
      quitsTaken(saveHome) === 1,
      "the route this check exists for was never driven",
    );

    const dialog = await waitForDialog(state, "asking before the save");
    check(
      "the quit raised the dialog on a second instance too",
      dialog !== null,
      "the quit did not ask, and the edit would have gone with the process",
    );

    answerDialog(dialog, "save");
    await waitForUnsavedDialogGone("save");

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
    const savedBlocks = readFileSync(saveFile).toString("utf8").split("\n\n");
    const beforeBlocks = original.toString("utf8").split("\n\n");
    const differing = beforeBlocks
      .map((block, index) => (block === savedBlocks[index] ? null : index))
      .filter((index) => index !== null);
    check(
      "the save the quit asked for wrote the edit and moved nothing else",
      savedBlocks.length === beforeBlocks.length &&
        differing.length === 1 &&
        savedBlocks[differing[0]].includes(EDIT_MARK),
      `blocks before ${beforeBlocks.length}, after ${savedBlocks.length}, differing ${JSON.stringify(differing)}`,
    );
  } finally {
    cleanup(state);
  }

  // Phase three: a quit with nothing unsaved. The route now holds every quit long enough to route
  // it through a window close, so a quit that should just go has to be proved to still just go.
  const cleanHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-quitgate-clean-"));
  const cleanFile = path.join(cleanHome, "nothing-to-ask-about.srt");
  copyFileSync(source, cleanFile);

  state = launch(cleanHome, cleanFile);
  try {
    const toplevel = await waitForWindow(state);
    // Opened and left alone: a document on screen with nothing unsaved in it.
    await waitForLog(cleanHome, SUBTITLE_OPENED, { what: "the subtitle to be open" });

    await quitFromTheMenu(toplevel);
    await waitForQuitTaken(cleanHome, 1);
    check(
      "the clean branch's menu quit went through AppHandle::exit as well",
      quitsTaken(cleanHome) === 1,
      "the route this check exists for was never driven",
    );

    const cleanSurvivors = await reap(state, "the app to exit on a quit with nothing unsaved");
    check(
      "a quit with nothing unsaved exited the app with status 0",
      state.exit.code === 0 && state.exit.signal === null,
      `exit was ${JSON.stringify(state.exit)}. The quit is held while the window is asked to ` +
        "close, and a quit with nothing to ask about must come out the other side of that.",
    );
    check(
      "no process survived the clean quit",
      cleanSurvivors.length === 0,
      cleanSurvivors.length === 0 ? "" : `survivors: ${cleanSurvivors.join(", ")}`,
    );
    check(
      "the clean quit left the file on disk untouched",
      readFileSync(cleanFile).equals(original),
      "a quit nobody was asked about wrote to the file",
    );
  } finally {
    cleanup(state);
  }

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `quit gate guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`quit gate check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
