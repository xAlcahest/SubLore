/**
 * The close gate over the interval its own dialog no longer covers (gate 2, `src-tauri/src/lib.rs`
 * rows 138 and 192).
 *
 * The dialog destroys itself the instant it is answered, so between "Save" and the close that
 * answer asks for the window is on screen, focused and editable. An edit committed inside that
 * interval was never asked about, and before this check the close carried it away in silence:
 * no dialog, no warning, no log line, and the work gone (CONTRIBUTING.md §3).
 *
 * AC, in the owner's terms: open a subtitle, edit a cue, close the window, click Save, and while
 * the save is still in flight edit again. The app must ask a second time instead of closing, and
 * the second edit must reach the disk.
 *
 * The interval is a race in production and would be unobservable from here, so the app holds it
 * open on request: `SUBLORE_CLOSE_ANSWER_DELAY_MS` (debug builds only, like `SUBLORE_FORCE_PANIC`).
 * That the hold actually happened is check 2 — without it the run would prove nothing while
 * looking green, which is the failure mode WORKFLOW §4c names as the worst one available.
 *
 * Not a WebDriver spec, for close-gate-check.js's reason: the run ends with the process exiting and
 * the W3C protocol reports neither the exit status nor the survivors.
 */
import { execFileSync, spawn } from "node:child_process";
import console from "node:console";
import { copyFileSync, existsSync, mkdtempSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import { answerDialog } from "../lib/gtk-dialog.js";
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
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { allWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 8;
let checksRun = 0;

/** The close dialog's window name. Frozen contract with src-tauri/src/strings.rs. */
const DIALOG_TITLE = "Unsaved changes";

/** The edit made before the gate goes up, which the first Save writes. */
const EDIT_EARLY = "SUBLORE_EARLY";
/** The edit committed after that Save was answered. This is the one the defect threw away. */
const EDIT_LATE = "SUBLORE_LATE";

// The wait below reads the cue's length as its witness, so two marks of one length would leave the
// second edit unwitnessed. See BACKLOG.md N9, S16.
if (EDIT_EARLY.length === EDIT_LATE.length) {
  throw new Error("EDIT_EARLY and EDIT_LATE must differ in length");
}

/** The fixture both phases open, and its first cue's length as committed. See N9, S15. */
const SOURCE_FIXTURE = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");
const UNEDITED_FIRST_CUE_CHARS = readFileSync(SOURCE_FIXTURE)
  .toString("utf8")
  .split("\n\n")[0]
  .split("\n")
  .slice(2)
  .join("\n").length;

/**
 * How long the app holds the answer before the close it asks for. It has to outlast the editing
 * below with room to spare: an edit that lands after the hold ends is a close that legitimately
 * finds nothing new, and would fail this check for the wrong reason.
 */
const ANSWER_HOLD_MS = 12000;

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`late-edit gate check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/** The gate dialog, or null. A second one over the first is a defect, so two is an error. */
function findDialog() {
  const matches = allWindows().filter((window) => window.name === DIALOG_TITLE);
  if (matches.length > 1) {
    throw new Error(
      `expected at most one "${DIALOG_TITLE}" dialog, found ${matches.length}.\n${rootTree()}`,
    );
  }
  return matches.length === 1 ? matches[0] : null;
}

async function waitForDialog(state, what) {
  return waitFor(
    () => {
      if (state.exit !== null) {
        throw new Error(
          `the app exited (code ${state.exit.code}) instead of ${what}. Either the gate let a ` +
            "dirty document close, which is the defect this script exists to catch, or the setup " +
            "never dirtied the document, which means the run proved nothing. The app log and the " +
            "file on disk tell them apart.",
        );
      }
      return findDialog();
    },
    { timeout: 20000, message: `a toplevel named "${DIALOG_TITLE}" (${what})` },
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
 * What the app did with the close it was already deciding: raised a second gate, or went away.
 * Both are returned rather than thrown, because which one happened is the finding.
 */
async function secondGateOrExit(state) {
  return waitFor(
    () => {
      const dialog = findDialog();
      if (dialog !== null) {
        return { dialog, exited: false };
      }
      return state.exit === null ? null : { dialog: null, exited: true };
    },
    { timeout: 25000, message: "the app to ask again or to exit" },
  );
}

/** The subtitle is passed as an argument, never typed: see `startup_files`. */
function launch(dataHome, file) {
  const app = spawn(requireAppBinary(), [file], {
    detached: true,
    stdio: ["ignore", "inherit", "inherit"],
    env: appEnv({
      XDG_DATA_HOME: dataHome,
      SUBLORE_CLOSE_ANSWER_DELAY_MS: String(ANSWER_HOLD_MS),
    }),
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
 * Commit a marked edit into the first cue. The sleeps are fixed for close-gate-check.js's reason:
 * without a DOM there is nothing here to wait on, and the checks below say so when the setup
 * misses rather than passing anyway.
 */
async function editFirstCue(toplevel, mark, settleMs, dataHome) {
  const cue = { x: toplevel.absX + firstCueText.x, y: toplevel.absY + firstCueText.y };
  // Attempted rather than assumed: the cue list paints after the backend has parsed the file, and
  // a click that lands early leaves the document clean while every later assertion still runs. The
  // app writes a line when an edit is committed, so this retries until it does (CI run 33341052061).
  // Not "an edit was committed" but "the text changed": a field committed unchanged bumps the
  // revision while leaving the document identical, which counting commits accepts. This is the
  // guard its two sibling scripts already carry. See BACKLOG.md N9, S16.
  const unchanged = lastEditedLength(dataHome);
  for (let attempt = 1; ; attempt += 1) {
    focusWindow(toplevel.id);
    doubleClickAt(cue.x, cue.y);
    await sleep(600);
    typeText(mark);
    // Enter commits the inline edit into the backend session. Without it only the frontend knows.
    pressKey("Return");
    const landed = await waitForEditedLength(dataHome, unchanged, { timeout: 4000 });
    if (landed) {
      break;
    }
    if (attempt >= 6) {
      throw new Error(`the edit "${mark}" never landed in ${attempt} attempts`);
    }
    // A half-open inline editor would take the next attempt's keystrokes as well.
    pressKey("Escape");
    await sleep(500);
  }
  await sleep(settleMs);
}

/** The length the first cue last logged, or the fixture's own before any edit landed. */
function lastEditedLength(dataHome) {
  const seen = [...appLog(dataHome).matchAll(/edit committed[^\n]*now (\d+) chars/g)].map((match) =>
    Number(match[1]),
  );
  return seen.length === 0 ? UNEDITED_FIRST_CUE_CHARS : seen[seen.length - 1];
}

function requestClose(toplevel) {
  execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "inherit", timeout: 15000 });
}

/** The app's own log for this run. `XDG_DATA_HOME` is this run's, so it cannot be an older one. */
function appLog(dataHome) {
  const file = path.join(dataHome, "com.sublore.app", "logs", "sublore.log");
  return existsSync(file) ? readFileSync(file, "utf8") : "";
}

async function reap(state, label) {
  await waitFor(() => state.exit !== null, { timeout: 20000, message: label });
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
  requireCloseWindowTool();

  const source = SOURCE_FIXTURE;
  const original = readFileSync(source);

  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-lateedit-"));
  const workFile = path.join(dataHome, "late-edit.srt");
  copyFileSync(source, workFile);

  const state = launch(dataHome, workFile);
  try {
    const toplevel = await waitForWindow(state);
    // The app says when the document is open; waiting for that instead of for a fixed number of
    // milliseconds is what makes this run on a slower machine than the one it was written on.
    await waitForLog(dataHome, SUBTITLE_OPENED, { what: "the subtitle to be open" });
    await editFirstCue(toplevel, EDIT_EARLY, 2500, dataHome);

    requestClose(toplevel);
    const first = await waitForDialog(state, "asking about the first edit");
    check("the gate is mapped, not just present in the tree", mapState(first.id) === "IsViewable");

    answerDialog(first, "save");
    await waitForDialogGone("save");

    // The interval under test: the dialog is gone, the save is in flight, the window is live.
    await editFirstCue(toplevel, EDIT_LATE, 1500, dataHome);
    check(
      "the app was still running when the second edit was committed",
      state.exit === null,
      `the app exited with ${JSON.stringify(state.exit)} before the edit could be made, so the ` +
        "interval this check is about was never entered",
    );
    check(
      "the app really held its answer open across that edit",
      appLog(dataHome).includes("SUBLORE_CLOSE_ANSWER_DELAY_MS"),
      "the hold never armed, so the edit above did not land inside the interval and the rest of " +
        "this run proves nothing. A release build carries no hook: use `pnpm e2e:build`.",
    );

    const outcome = await secondGateOrExit(state);
    check(
      "the edit committed after the answer was asked about instead of closed away",
      outcome.dialog !== null,
      "the app closed on the answer it was given before that edit existed, and the edit is gone " +
        "with it. This is gate 2 `src-tauri/src/lib.rs:138`.",
    );
    check(
      "the second gate is mapped, not just present in the tree",
      mapState(outcome.dialog.id) === "IsViewable",
    );

    answerDialog(outcome.dialog, "save");
    await waitForDialogGone("the second save");

    const survivors = await reap(state, "the app to exit after the second save");
    check(
      "the second save exited the app with status 0",
      state.exit.code === 0 && state.exit.signal === null,
      `exit was ${JSON.stringify(state.exit)}`,
    );
    check(
      "no process survived",
      survivors.length === 0,
      survivors.length === 0 ? "" : `survivors: ${survivors.join(", ")}`,
    );

    // Not "the bytes changed": the point is that the late edit reached the disk and nothing else
    // moved (CONTRIBUTING.md §3).
    const savedBlocks = readFileSync(workFile).toString("utf8").split("\n\n");
    const beforeBlocks = original.toString("utf8").split("\n\n");
    const differing = beforeBlocks
      .map((block, index) => (block === savedBlocks[index] ? null : index))
      .filter((index) => index !== null);
    check(
      "the file on disk carries the edit that was committed after the answer, and nothing else moved",
      savedBlocks.length === beforeBlocks.length &&
        differing.length === 1 &&
        savedBlocks[differing[0]].includes(EDIT_LATE),
      `blocks before ${beforeBlocks.length}, after ${savedBlocks.length}, differing ` +
        `${JSON.stringify(differing)}\n${readFileSync(workFile).toString("utf8")}`,
    );
  } finally {
    cleanup(state);
  }

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `late-edit gate guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`late-edit gate check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
