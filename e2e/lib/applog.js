import { readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
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

/**
 * The `XDG_DATA_HOME` this run was launched with.
 *
 * One place rather than a copy in every spec that reads the log: the value is the harness's own and
 * `wdio.conf.js` is the only thing that sets it.
 */
export function dataHome() {
  const home = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof home !== "string" || home === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  return home;
}

/** The first line each launch writes, and the only mark that separates one app's log from the next. */
const STARTED = /Sublore \S+ starting on \S+/g;

/**
 * The log from the last launch onward.
 *
 * Every spec in a run shares one log file, so a line matched across the whole of it may belong to
 * an app that has already exited. `chooser.spec.js` counts its own matches for the same reason;
 * this is the other way of scoping, and the one to use when the question is about now.
 */
export function appLogSinceStart(home) {
  const log = appLog(home);
  let last = -1;
  for (const match of log.matchAll(STARTED)) {
    last = match.index;
  }
  return last === -1 ? log : log.slice(last);
}

/** The line `subtitle::open_session` writes once a document is the one on screen. */
export const SUBTITLE_OPENED = /subtitle: opened .* — \d+ cues/;

/** The line `subtitle::apply_edit` writes once a finished edit is in the document. */
export const EDIT_COMMITTED = /subtitle: edit committed, revision \d+, dirty/;

/**
 * Wait until an edit has actually changed the first cue's length.
 *
 * "An edit was committed" is not "the text changed": an inline editor that takes Enter before the
 * keystrokes reach it commits the field unchanged, which bumps the revision and marks the session
 * dirty while leaving the document identical. That is exactly what happened on CI, and the harness
 * believed it (gate 2, run 33363671401). The length is what the app logs, and the length is enough.
 */
export async function waitForEditedLength(dataHome, unchangedLength, options = {}) {
  const timeout = options.timeout ?? 4000;
  const deadline = Date.now() + timeout;
  for (;;) {
    const seen = [...appLog(dataHome).matchAll(/edit committed[^\n]*now (\d+) chars/g)].map((m) =>
      Number(m[1]),
    );
    if (seen.some((length) => length !== unchangedLength)) {
      return true;
    }
    if (Date.now() >= deadline) {
      return false;
    }
    await sleep(100);
  }
}
