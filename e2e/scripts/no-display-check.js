/**
 * A launch with no display is a message, not a panic (BACKLOG N4).
 *
 * AC: with no display reachable, the app exits non-zero having printed one plain line naming the
 * cause and what to do, and no panic trace. Before the guard in `main.rs`, `sublore` over SSH or in
 * a container died at `tao-0.35.3/.../event_loop.rs:217` with `Failed to initialize gtk backend!`,
 * a `RUST_BACKTRACE` note and status 101, and wrote a crash report on the way out.
 *
 * The one check that must not have a display, so it cannot go through `.github/scripts/e2e-check.sh`:
 * that wrapper hands every check an X server through `xvfb-run`. It has its own CI step, and it
 * builds the child's environment itself, so it proves the same thing whether or not the shell
 * running it has a display of its own.
 *
 * `TMPDIR` points at this run's own directory because the panic path writes its crash report to
 * `std::env::temp_dir()` when it fires this early — `crash::attach` has not run, so there is no log
 * directory yet. Pointing it here is what lets the check assert that nothing was written at all.
 */
import { spawn } from "node:child_process";
import console from "node:console";
import { mkdtempSync, readdirSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { appEnv } from "../lib/env.js";
import { requireAppBinary } from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 5;
let checksRun = 0;

/** The status the panic runtime uses, and the one this guard exists to stop the user seeing. */
const EXIT_PANIC = 101;

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`no display check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/**
 * The app with nowhere to draw. `appEnv` already drops `WAYLAND_DISPLAY`; `DISPLAY` is dropped
 * here, which is the whole point of the run.
 */
function launch(home) {
  const env = appEnv({ TMPDIR: home, XDG_DATA_HOME: home });
  delete env.DISPLAY;
  const app = spawn(requireAppBinary(), [], {
    detached: true,
    stdio: ["ignore", "ignore", "pipe"],
    env,
  });
  const state = { pgid: app.pid, exit: null, stderr: "", spawnError: null };
  app.stderr.on("data", (chunk) => {
    state.stderr += chunk;
  });
  app.on("error", (error) => {
    state.spawnError = error;
  });
  // `close`, not `exit`: it carries the same status and fires once stderr has been drained, so the
  // line the check reads cannot be a partial one.
  app.on("close", (code, signal) => {
    state.exit = { code, signal };
  });
  return state;
}

async function main() {
  requireAppBinary();
  const home = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-no-display-"));

  const state = launch(home);
  try {
    // A hang is a failure of this run, not a timeout somebody has to interpret: the guard's whole
    // claim is that the app gives up and says so.
    await waitFor(
      () => {
        if (state.spawnError !== null) {
          throw new Error(`the app failed to start: ${state.spawnError.message}`);
        }
        return state.exit !== null;
      },
      { timeout: 30000, message: "the app to exit with no display to open a window on" },
    );
    // Read after the exit, so nothing written on the way out is missed.
    const stderr = state.stderr;
    const lines = stderr.split("\n").filter((line) => line.trim() !== "");

    check(
      "it exits non-zero, and not with the panic status",
      state.exit.code !== null && state.exit.code !== 0 && state.exit.code !== EXIT_PANIC,
      `exit ${JSON.stringify(state.exit)}. ${EXIT_PANIC} is the panic status: the raw library ` +
        `panic is back.\nstderr was:\n${stderr}`,
    );
    check(
      "it never panics",
      !stderr.includes("panicked") && !stderr.includes("RUST_BACKTRACE"),
      `stderr carried a panic trace:\n${stderr}`,
    );
    check(
      "it says so in one line and nothing else",
      lines.length === 1,
      `stderr had ${lines.length} lines instead of one:\n${stderr}`,
    );
    check(
      "that line names the missing display and what to do about it",
      lines.length === 1 && lines[0].includes("DISPLAY") && lines[0].includes("xvfb-run"),
      `the line was:\n${lines[0] ?? "<nothing on stderr>"}`,
    );
    // The panic path writes one; a refusal has nothing to report.
    const left = readdirSync(home);
    check(
      "it leaves no crash report behind",
      left.length === 0,
      `${home} holds ${left.join(", ")} after a launch that never crashed`,
    );
  } finally {
    try {
      if (processGroupMembers(state.pgid).length > 0) {
        killGroup(state.pgid);
      }
    } catch {
      // Teardown must not mask the failure that got us here.
    }
  }

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `no display guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`no display check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
