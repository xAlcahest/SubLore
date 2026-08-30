import { readFileSync } from "node:fs";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

/**
 * The app's own log, which is the only thing these scripts can observe about what the app believes.
 *
 * The checks that drive the shell have no DOM to wait on — `close-gate-check.js` says so in its own
 * header — so until now they waited a fixed number of milliseconds for the file to be parsed and
 * the cue list to paint. That number was calibrated on the owner's machine, and the first real CI
 * run failed on it: the runner is slower, the double-click landed before the list existed, the
 * document was never dirtied, and the gate had nothing to ask about (gate 2, run 33339776169).
 *
 * A wait on a line the app writes is machine-independent. It is also honest about what it proves:
 * the app says the document is open, which is what the next step needs, rather than "enough time
 * has probably passed".
 */
export function appLog(dataHome) {
  try {
    return readFileSync(path.join(dataHome, "com.sublore.app", "logs", "sublore.log"), "utf8");
  } catch {
    // Before the first line is written the file does not exist, which is not an error here.
    return "";
  }
}

/**
 * Wait until the app's log matches, and say what it did contain when it does not.
 *
 * @param {string} dataHome the `XDG_DATA_HOME` the app was launched with
 * @param {RegExp} pattern what the app has to have said
 * @param {{timeout?: number, what?: string}} options
 */
export async function waitForLog(dataHome, pattern, options = {}) {
  const timeout = options.timeout ?? 30000;
  const what = options.what ?? String(pattern);
  const deadline = Date.now() + timeout;
  for (;;) {
    const log = appLog(dataHome);
    if (pattern.test(log)) {
      return log;
    }
    if (Date.now() >= deadline) {
      throw new Error(
        `the app never logged ${what} within ${timeout}ms. Its log held:\n${log || "(nothing yet)"}`,
      );
    }
    await sleep(100);
  }
}

/** The line `subtitle::open_session` writes once a document is the one on screen. */
export const SUBTITLE_OPENED = /subtitle: opened .* — \d+ cues/;
