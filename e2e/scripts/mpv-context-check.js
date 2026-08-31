/**
 * A gpu-context mpv refuses costs the request, not the application.
 *
 * Sublore pins `gpu-context=x11egl` whenever it hands mpv a window id, because with a Wayland
 * display in the environment mpv's own `auto` picks Wayland and draws past that window (BACKLOG
 * N2b). Twice in gate 2 that pin was made conditional and then unconditional again, and each shape
 * broke a different machine: narrowed, it stopped protecting plain X11 sessions; unconditional and
 * propagating, an mpv built without `x11egl` failed `Player::new`, which failed Tauri's setup hook,
 * which meant **no window at all** rather than no video.
 *
 * This check drives the case with the one lever that reproduces it without a second mpv build: the
 * `SUBLORE_MPV_GPU_CONTEXT` override, set to a name no mpv accepts. Before the fix the app exits
 * before its window exists; after it, the request is refused, the refusal is logged, mpv falls back
 * to the pin, and the video still attaches.
 */
import { execFileSync, spawn } from "node:child_process";
import console from "node:console";
import { mkdtempSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { appEnv } from "../lib/env.js";
import {
  closeWindowTool,
  requireAppBinary,
  requireCloseWindowTool,
  requireDisplay,
  requireVideoFixture,
  videoFixture,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { childWindows, findToplevel, rootTree } from "../lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 5;
let checksRun = 0;

/** No mpv accepts this, which is the point: the name has to be refused to test the refusal. */
const REFUSED = "definitely-not-a-gpu-context";

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`mpv context check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

function appLog(dataHome) {
  try {
    return readFileSync(path.join(dataHome, "com.sublore.app", "logs", "sublore.log"), "utf8");
  } catch {
    return "";
  }
}

async function main() {
  requireDisplay();
  requireAppBinary();
  requireCloseWindowTool();
  requireVideoFixture();

  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-mpvctx-"));
  const app = spawn(requireAppBinary(), [videoFixture], {
    detached: true,
    stdio: ["ignore", "ignore", "inherit"],
    env: appEnv({ XDG_DATA_HOME: dataHome, SUBLORE_MPV_GPU_CONTEXT: REFUSED }),
  });
  const pgid = app.pid;
  let exit = null;
  app.on("exit", (code, signal) => {
    exit = { code, signal };
  });

  try {
    // The assertion, not a precondition: before the fix the refusal propagated out of Player::new
    // and out of Tauri's setup hook, and the process ended here with no window and nothing on
    // screen. `waitFor` is given the exit so the failure names it instead of timing out blind.
    const toplevel = await waitFor(
      () => {
        if (exit !== null) {
          throw new Error(
            `the app exited (code ${exit.code}, signal ${exit.signal}) before its window ` +
              `appeared. A gpu-context mpv refuses must cost the request, not the launch.`,
          );
        }
        return findToplevel();
      },
      { timeout: 30000, message: 'the "Sublore" toplevel with a refused gpu-context' },
    );
    check("the app came up with a gpu-context mpv cannot give it", toplevel !== null);

    const surface = await waitFor(
      () => {
        const children = childWindows(toplevel.id).filter(
          (child) => child.width > 50 && child.height > 50,
        );
        return children.length > 0 ? children[0] : null;
      },
      { timeout: 30000, message: `the native video surface\n${rootTree()}` },
    );
    check("the video surface was created", surface !== null);

    // The fallback, not merely the survival: mpv attaching its own window inside ours is what the
    // pin buys, and a run that started but never attached would pass the first check while having
    // lost exactly what N2b fixed.
    const attached = await waitFor(
      () => (childWindows(surface.id).length > 0 ? childWindows(surface.id) : null),
      { timeout: 20000, message: `mpv's own window inside the surface\n${rootTree()}` },
    ).catch(() => null);
    check(
      "mpv still attached, so the refused name fell back to the pin",
      attached !== null,
      `the surface has no children: the fallback did not happen.\n${rootTree()}`,
    );

    execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "ignore", timeout: 15000 });
    await waitFor(() => exit !== null, { timeout: 20000, message: "the app to exit" });
    const survivors = await waitFor(() => (processGroupMembers(pgid).length === 0 ? [] : null), {
      timeout: 10000,
      message: `process group ${pgid} to be empty`,
    }).catch(() => processGroupMembers(pgid));
    check(
      "it closed with status 0 and left nothing running",
      exit.code === 0 && exit.signal === null && survivors.length === 0,
      `exit ${JSON.stringify(exit)}, survivors ${survivors.join(", ")}`,
    );

    // Read after the process is gone: a live run has no point at which the log is guaranteed to
    // have reached the disk.
    const log = appLog(dataHome);
    check(
      "the refusal is in the log, naming the value that was refused",
      log.includes(`gpu-context=${REFUSED}`) && log.includes("SUBLORE_MPV_GPU_CONTEXT"),
      `the log never named the refused context:\n${log}`,
    );
  } finally {
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
      `mpv context guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`mpv context check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
