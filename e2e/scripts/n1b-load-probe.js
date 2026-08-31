/**
 * One close-gate run, one branch, one line of output. Reproduces BACKLOG N1b.
 *
 * **This is a probe, not a check. It asserts nothing.** It records what happened so that batteries
 * of runs can say something the checks cannot: N1b's exit crash does not reproduce sequentially at
 * all, and under concurrent load it reproduces on the save branch and not on discard
 * (`docs/reports/n1b-sessanta-corse.md`). N1b's closing criterion is written in terms of this
 * script, which is why it lives here rather than in a scratch directory.
 *
 * Usage, one run:
 *   xvfb-run -n 500 -s "-screen 0 1024x700x24" node e2e/scripts/n1b-load-probe.js save
 *
 * The criterion needs sixty save-branch runs in six concurrent streams; the caller drives that,
 * because load is the condition under test and this script must not decide it.
 *
 * Prints one JSON object: { answer, phase, exit, signal, killedRunning, pid }. The caller asks
 * `coredumpctl` about the pid afterwards; a probe that queried it itself would slow the run it is
 * timing. `killedRunning` is true when teardown had to SIGKILL a still-alive process group — such a
 * run was cut off, not observed to completion, and a battery should not count it as "done".
 */
import { execFileSync, spawn } from "node:child_process";
import console from "node:console";
import { copyFileSync, mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import { appEnv } from "../lib/env.js";
import { doubleClickAt, focusWindow, pressKey, typeText } from "../lib/input.js";
import { answerDialog } from "../lib/gtk-dialog.js";
import { closeWindowTool, repoRoot, requireAppBinary } from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { allWindows, findToplevel } from "../lib/x11.js";

/** Point in the current shell, relative to the toplevel origin. M2.0 must revisit this. */
const FIRST_CUE_TEXT = { x: 750, y: 540 };
/** Frozen contract with src-tauri/src/strings.rs, same as the close gate check's. */
const DIALOG_TITLE = "Unsaved changes";
const EDIT_MARK = "SUBLORE_N1B";

const answer = process.argv[2];
if (answer !== "save" && answer !== "discard") {
  console.error("usage: n1b-load-probe.js save|discard");
  process.exit(2);
}

const dataHome = mkdtempSync(path.join(os.tmpdir(), `sublore-n1b-${answer}-`));
const workFile = path.join(dataHome, "probe.srt");
copyFileSync(
  path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt"),
  workFile,
);

// The subtitle arrives as a command-line argument rather than typed, per WORKFLOW.md 4c.
const app = spawn(requireAppBinary(), [workFile], {
  detached: true,
  stdio: ["ignore", "ignore", "ignore"],
  env: appEnv({ XDG_DATA_HOME: dataHome }),
});
const pgid = app.pid;
let exit = null;
app.on("exit", (code, signal) => {
  exit = { code, signal };
});

let phase = "start";
let killedRunning = false;
try {
  phase = "window";
  const toplevel = await waitFor(() => (exit === null ? findToplevel() : null), {
    timeout: 30000,
    message: "the toplevel",
  });

  phase = "dirty";
  // Fixed waits, for the same reason close-gate-check.js has them: without a DOM there is nothing
  // observable to wait on here. A wait that turns out too short shows up as a phase, not as a pass.
  await sleep(3500);
  focusWindow(toplevel.id);
  doubleClickAt(toplevel.absX + FIRST_CUE_TEXT.x, toplevel.absY + FIRST_CUE_TEXT.y);
  await sleep(600);
  typeText(EDIT_MARK);
  pressKey("Return");
  await sleep(2500);

  phase = "close";
  execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "ignore", timeout: 15000 });

  phase = "dialog";
  const dialog = await waitFor(
    () => {
      if (exit !== null) {
        return null;
      }
      const found = allWindows().filter((window) => window.name === DIALOG_TITLE);
      return found.length === 1 ? found[0] : null;
    },
    { timeout: 15000, message: "the dialog" },
  );

  phase = "answer";
  answerDialog(dialog, answer);

  phase = "exit";
  await waitFor(() => exit !== null, { timeout: 20000, message: "the app to exit" });
  phase = "done";
} catch {
  // The probe records; it does not judge. The phase it stopped in is the finding.
} finally {
  try {
    // Recorded before the kill: a run still alive here was cut off before it could crash or exit
    // on its own, which a battery must be able to tell apart from a genuine no-op (see gate2
    // register, L10 — this teardown used to erase that distinction).
    killedRunning = exit === null && processGroupMembers(pgid).length > 0;
    if (killedRunning) {
      killGroup(pgid);
    }
  } catch {
    // Teardown must not rewrite the result.
  }
}

console.log(
  JSON.stringify({
    answer,
    phase,
    exit: exit === null ? null : exit.code,
    signal: exit === null ? null : exit.signal,
    killedRunning,
    pid: pgid,
  }),
);
process.exit(0);
