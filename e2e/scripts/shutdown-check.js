/**
 * AC: "a close that leaves a non-zero exit status or a surviving process fails the test."
 *
 * Deliberately not a WebDriver spec. Under wdio the app is a grandchild (node -> tauri-driver ->
 * WebKitWebDriver -> app) and the W3C protocol exposes no process status. Here Node is the parent,
 * so the exit status and the survivor list are exact.
 */
import { execFileSync, spawn } from "node:child_process";
import console from "node:console";
import { mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { clearInterval, setInterval } from "node:timers";

import {
  closeWindowTool,
  requireAppBinary,
  requireCloseWindowTool,
  requireDisplay,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { allWindows, findToplevel, rootTree } from "../lib/x11.js";

/** Frozen contract with src-tauri/src/strings.rs, mirrored in close-gate-check.js. */
const UNSAVED_DIALOG_TITLE = "Unsaved changes";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 5;
let checksRun = 0;

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`shutdown check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

async function main() {
  requireDisplay();
  const binary = requireAppBinary();
  requireCloseWindowTool();

  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-shutdown-"));
  // Own process group: the survivor scan is then the exact set of processes this run created,
  // never another agent's copy of the app running on the same machine.
  const app = spawn(binary, [], {
    detached: true,
    stdio: ["ignore", "inherit", "inherit"],
    env: { ...process.env, XDG_DATA_HOME: dataHome },
  });
  const pgid = app.pid;

  let exit = null;
  let spawnError = null;
  let dialogSeen = false;
  // Declared out here so the teardown below can stop it whatever went wrong inside.
  let dialogWatch = null;
  app.on("error", (error) => {
    spawnError = error;
  });
  app.on("exit", (code, signal) => {
    exit = { code, signal };
  });

  try {
    const toplevel = await waitFor(
      () => {
        if (spawnError !== null) {
          throw new Error(`the app failed to start: ${spawnError.message}`);
        }
        if (exit !== null) {
          throw new Error(`the app exited before its window appeared (code ${exit.code})`);
        }
        return findToplevel();
      },
      { timeout: 30000, message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel` },
    );
    check("the app window appeared", toplevel !== null);

    // Sampled while the app closes, because a dialog that appeared and went would leave no trace
    // to look for afterwards.
    dialogWatch = setInterval(() => {
      if (allWindows().some((window) => window.name === UNSAVED_DIALOG_TITLE)) {
        dialogSeen = true;
      }
    }, 100);

    execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "inherit", timeout: 15000 });

    await waitFor(() => exit !== null, {
      timeout: 15000,
      message: "the app to exit after WM_DELETE_WINDOW",
    });

    // Nothing was edited, so the close gate must stay out of the way. Without this, a regression
    // that raised the gate on a clean document would still pass every check below (BACKLOG N1).
    check(
      "no unsaved-changes dialog appeared on a clean close",
      !dialogSeen,
      "the gate asked about a document that had never been edited",
    );

    check("the app exited with status 0", exit.code === 0, `exit code was ${exit.code}`);
    check("the app exited without a signal", exit.signal === null, `terminated by ${exit.signal}`);

    // The app's own children (mpv threads aside, anything it forked) share its process group.
    const survivors = await waitFor(
      () => {
        const alive = processGroupMembers(pgid);
        return alive.length === 0 ? [] : null;
      },
      { timeout: 10000, message: `process group ${pgid} to be empty` },
    ).catch(() => processGroupMembers(pgid));
    check(
      "no process survived in the app's process group",
      survivors.length === 0,
      survivors.length === 0 ? "" : `survivors: ${survivors.join(", ")}\n${rootTree()}`,
    );
  } finally {
    if (dialogWatch !== null) {
      clearInterval(dialogWatch);
    }
    // Never leak processes on a failure path, but never signal a pgid the kernel may already have
    // recycled: a group that still has members cannot have had its id reused.
    try {
      if (processGroupMembers(pgid).length > 0) {
        killGroup(pgid);
      }
    } catch {
      // Teardown must not mask the failure that got us here.
    }
  }

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `shutdown guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`shutdown check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
