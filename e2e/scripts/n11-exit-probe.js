/**
 * One launch with a video, one close, one line of output. Reproduces BACKLOG N11.
 *
 * **This is a probe, not a check. It asserts nothing.** It records how the app left, so batteries
 * of runs can put a rate on a crash the checks can only meet by accident.
 *
 * Usage, one run:
 *   xvfb-run -n 600 -s "-screen 0 1024x700x24" node e2e/scripts/n11-exit-probe.js
 *
 * The crash wants a video that mpv has actually drawn into: twenty-five launches with no video on
 * the command line produced no core at all. So this waits for a settled native surface before
 * closing, the way `scaled-surface-check.js` does, rather than closing as soon as the window is up.
 *
 * Prints one JSON object: { phase, exit, signal, pid, killedRunning }. The caller asks
 * `coredumpctl` about the pid, for the same reason the N1b probe does not.
 */
import { execFileSync, spawn } from "node:child_process";
import console from "node:console";
import { mkdtempSync } from "node:fs";
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
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { childWindows, findToplevel } from "../lib/x11.js";

/** The surface is the biggest child of the toplevel; mpv's own window lives inside it. */
function surfaceOf(toplevel) {
  return (
    childWindows(toplevel.id)
      .filter((child) => child.width > 50 && child.height > 50)
      .sort((a, b) => b.width * b.height - a.width * a.height)[0] ?? null
  );
}

requireDisplay();
requireCloseWindowTool();
const fixture = requireVideoFixture();

const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-n11-"));
const app = spawn(requireAppBinary(), [fixture], {
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

  // Two identical reads, not the first one: the surface is sized from the page's rectangle and the
  // reading between layout and settle is an intermediate.
  phase = "surface";
  let previous = null;
  await waitFor(
    () => {
      if (exit !== null) {
        return null;
      }
      const now = surfaceOf(toplevel);
      const settled =
        now !== null &&
        previous !== null &&
        now.width === previous.width &&
        now.height === previous.height;
      previous = now;
      return settled ? now : null;
    },
    { timeout: 30000, message: "a settled native surface" },
  );

  phase = "close";
  execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "ignore", timeout: 15000 });

  phase = "exit";
  await waitFor(() => exit !== null, { timeout: 20000, message: "the app to exit" });
  phase = "done";
} catch {
  // The probe records; it does not judge. The phase it stopped in is the finding.
} finally {
  try {
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
    phase,
    exit: exit === null ? null : exit.code,
    signal: exit === null ? null : exit.signal,
    killedRunning,
    pid: pgid,
  }),
);
process.exit(0);
