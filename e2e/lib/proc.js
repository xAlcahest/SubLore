import { execFileSync } from "node:child_process";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import { requireLinuxBackend } from "./platform.js";

/**
 * Poll until `probe` returns something truthy. Never a fixed sleep: every wait has a deadline and
 * a message saying what was expected (design section 10).
 * @template T
 * @param {() => T | Promise<T>} probe
 * @param {{timeout?: number, interval?: number, message: string}} options
 * @returns {Promise<T>}
 */
export async function waitFor(probe, { timeout = 30000, interval = 250, message }) {
  const deadline = Date.now() + timeout;
  let lastError = null;
  for (;;) {
    try {
      const value = await probe();
      if (value) {
        return value;
      }
      lastError = null;
    } catch (error) {
      lastError = error;
    }
    if (Date.now() >= deadline) {
      const cause = lastError === null ? "" : `\nlast error: ${lastError.message}`;
      throw new Error(`timed out after ${timeout}ms waiting for ${message}${cause}`);
    }
    await sleep(interval);
  }
}

/**
 * Process ids still alive in a process group. The group is the exact set a run created, which is
 * why the shutdown check uses it instead of a name scan: another agent may be running their own
 * copy of the app on this machine.
 * @param {number} pgid
 * @returns {number[]}
 */
export function processGroupMembers(pgid) {
  // A process group is POSIX; on Windows a job object is the equivalent unit. Seam for MW.1b.
  requireLinuxBackend(
    "proc.js processGroupMembers",
    "list the processes one run spawned, as the exact set it created rather than a name scan",
  );
  try {
    const out = execFileSync("pgrep", ["-g", String(pgid)], { encoding: "utf8", timeout: 10000 });
    return out
      .split("\n")
      .map((line) => Number(line.trim()))
      .filter((pid) => Number.isInteger(pid) && pid > 0);
  } catch (error) {
    // pgrep exits 1 with no output when nothing matches; anything else is a real failure.
    if (error.status === 1 && String(error.stdout ?? "").trim() === "") {
      return [];
    }
    throw error;
  }
}

/**
 * Best-effort teardown of a whole process group. Never throws for a group that is already gone:
 * that is the outcome asked for, and this runs on failure paths. It does throw off Linux, where
 * the catch below would swallow the negative signal and leave the app running.
 */
export function killGroup(pgid, signal = "SIGKILL") {
  requireLinuxBackend(
    "proc.js killGroup",
    "tear down every process one run spawned, including the sidecar and the driver's children",
  );
  try {
    process.kill(-pgid, signal);
  } catch {
    // Already gone, which is the outcome we wanted anyway.
  }
}
