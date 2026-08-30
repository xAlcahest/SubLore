/**
 * The command line as an input surface (gate 2, `src-tauri/src/lib.rs` rows 75, 43 and 45).
 *
 * A Linux filename is a byte string, not text, and `std::env::args()` panics on one that is not
 * UTF-8. `startup_files` iterated it inside the builder chain, before any window existed and before
 * the crash dialog could be shown, so `sublore /media/série.srt` with a Latin-1 name exited 101 with
 * nothing on screen. Legacy subtitle collections are full of such names. The other two rows are
 * about silence: a real file whose name starts with `-` was dropped unread, and no argument dropped
 * for any reason was recorded anywhere.
 *
 * AC: launch the app with an argument that is not valid Unicode and a real subtitle beside it. The
 * window comes up, the subtitle is the one taken, the argument that could not be carried is named in
 * the log, and closing the window exits cleanly. Launch it again with a subtitle whose name starts
 * with a dash, a second real subtitle and a path that is not there: the dash-named file is taken,
 * and both the second subtitle and the missing path are named in the log.
 *
 * Two launches, because `startup_files` opens only the first subtitle it accepts: the subtitle
 * beside the bad argument and the dash-named subtitle cannot both be the one taken in a single run,
 * and asserting them from one log line would collapse two different claims into one. The one it does
 * not open is no longer dropped in silence — it is named in the log like any other argument the app
 * could not use (gate 2b, `src-tauri/src/lib.rs:76`), and the second launch asserts that line.
 *
 * Node cannot spawn a process with a non-UTF-8 argument directly: a JS string reaches execve as
 * UTF-8 and a lone surrogate becomes U+FFFD. The bytes are assembled by `sh` with `printf %b`, which
 * then `exec`s the binary, so the app really receives 0xE9 and the pid Node holds is the app's own.
 * That is why this launch goes through a shell and every other harness launch does not.
 *
 * This file is the merge of the wave-3 `startup-args-check.js` and `argv-startup-check.js`, which
 * two implementers wrote independently over the same rows. Every distinct assertion from both is
 * here; the one they shared (the unreadable argument is named in the log) is kept in its strongest
 * form, an exact match on the whole line including the name the app lossily rendered.
 */
import { Buffer } from "node:buffer";
import { execFileSync, spawn } from "node:child_process";
import console from "node:console";
import { copyFileSync, existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { appEnv } from "../lib/env.js";
import {
  closeWindowTool,
  repoRoot,
  requireAppBinary,
  requireCloseWindowTool,
  requireDisplay,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { findToplevel, rootTree } from "../lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 7;
let checksRun = 0;

/** The byte that is not valid UTF-8 on its own: Latin-1 "é", as the shell spells it for printf. */
const LATIN1_E_ACUTE = 0xe9;
const LATIN1_E_ACUTE_ESCAPE = "\\0351";
/** What `to_string_lossy` puts in its place, which is what the log line has to carry. */
const REPLACEMENT = "�";

/** A real subtitle whose name starts with a dash, which the old filter dropped before looking. */
const DASH_NAMED = "-export.srt";
/** A path that is not there: the typo case, which used to vanish without a trace. */
const MISSING = "epsiode.srt";
/** A second real subtitle: only the first is opened, and the other used to go unmentioned. */
const SECOND_SUBTITLE = "ep02.srt";

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`startup args check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/**
 * `sublore <not-unicode> <rest...>`, with the first argument assembled as raw bytes by the shell.
 * The substitution has to live in the script text: an argument handed to `sh -c` is never
 * re-evaluated, so a `printf` written there would arrive at the app verbatim. `exec` replaces the
 * shell, so the pid here is the app's own and the process group is the one it started.
 */
function launch(dataHome, cwd, badArgumentEscape, rest) {
  const script = 'first=$(printf %b "$1"); shift; exec "$0" "$first" "$@"';
  const app = spawn("sh", ["-c", script, requireAppBinary(), badArgumentEscape, ...rest], {
    cwd,
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

/** Returned instead of a window when the app is already gone, so the wait stops at once. */
const GONE = Symbol("the app exited before its window appeared");

/**
 * The window, or `null` and the sentence saying why there is none. It does not throw, because
 * "the window came up" is an assertion this check counts: a launch killed by its own command line
 * has to fail a `check`, not a wait, or removing that assertion would cost nothing (WORKFLOW §2.5b).
 */
async function windowOrReason(state) {
  try {
    const found = await waitFor(
      () => (state.spawnError !== null || state.exit !== null ? GONE : findToplevel()),
      { timeout: 30000, message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel` },
    );
    if (found !== GONE) {
      return { toplevel: found, reason: "" };
    }
    if (state.spawnError !== null) {
      return { toplevel: null, reason: `the app failed to start: ${state.spawnError.message}` };
    }
    return {
      toplevel: null,
      reason:
        `the app exited (code ${state.exit.code}, signal ${state.exit.signal}) before its window ` +
        "appeared. Status 101 is a panic: the argument that is not valid Unicode cost the whole " +
        "launch, which is gate 2 `src-tauri/src/lib.rs:75`.",
    };
  } catch (error) {
    return {
      toplevel: null,
      reason: `${error.message}\nwindows on the display were:\n${rootTree()}`,
    };
  }
}

/** The app's own log for this run. `XDG_DATA_HOME` is this run's, so it cannot be an older one. */
function logFile(dataHome) {
  return path.join(dataHome, "com.sublore.app", "logs", "sublore.log");
}

function appLog(dataHome) {
  const file = logFile(dataHome);
  return existsSync(file) ? readFileSync(file, "utf8") : "";
}

/** The command-line line is written in `setup`, so it is there while the app is still running. */
async function waitForCommandLine(state, dataHome) {
  await waitFor(
    () => {
      if (state.exit !== null) {
        throw new Error(
          `the app exited (code ${state.exit.code}) before it logged its command line. ` +
            "Status 101 is a panic on one of the arguments.",
        );
      }
      return appLog(dataHome).includes("command line:") ? true : null;
    },
    { timeout: 20000, message: `"command line:" to reach ${logFile(dataHome)}` },
  );
  return appLog(dataHome);
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

/**
 * A name that is not valid Unicode, beside a subtitle that is. The bad name is a real file on disk,
 * so the argument is refused for what it is and not for being missing.
 */
async function theArgumentThatCannotBeCarried() {
  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-argv-bad-"));

  const badName = Buffer.concat([
    Buffer.from(`${dataHome}/s`),
    Buffer.from([LATIN1_E_ACUTE]),
    Buffer.from("rie.srt"),
  ]);
  writeFileSync(badName, "1\n00:00:01,000 --> 00:00:02,000\nlatin-1\n\n");
  const badArgumentEscape = `${dataHome}/s${LATIN1_E_ACUTE_ESCAPE}rie.srt`;
  const badArgumentLossy = `${dataHome}/s${REPLACEMENT}rie.srt`;

  const source = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");
  const subtitle = path.join(dataHome, "beside-it.srt");
  copyFileSync(source, subtitle);

  const state = launch(dataHome, dataHome, badArgumentEscape, [subtitle]);
  try {
    const { toplevel, reason } = await windowOrReason(state);
    check(
      "the app came up with an argument that is not valid Unicode on its command line",
      toplevel !== null,
      reason,
    );

    execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "inherit", timeout: 15000 });
    const survivors = await reap(state, "the app to exit after the close");
    check(
      "closing it exits 0 with nothing left alive",
      state.exit.code === 0 && state.exit.signal === null && survivors.length === 0,
      `exit ${JSON.stringify(state.exit)}, survivors ${survivors.join(", ")}`,
    );

    // Read after the process is gone: the log file is written by the app, and a live run has no
    // point at which it is guaranteed to have reached the disk.
    const log = appLog(dataHome);
    check(
      "the argument it could not carry is named in the log",
      log.includes(`command line: ignored ${badArgumentLossy} (not valid Unicode)`),
      `the log never named the dropped argument:\n${log}`,
    );
    check(
      "the subtitle named beside it was still the one taken",
      log.includes(`command line: video=None, subtitle=Some("${subtitle}")`),
      `a bad argument took the good one with it:\n${log}`,
    );
  } finally {
    cleanup(state);
  }
}

/**
 * The two names the filter used to get wrong, in a run of their own because only the first subtitle
 * is kept. The unreadable argument rides along so both launches take the same path through `sh`.
 */
async function theNamesTheFilterUsedToDrop() {
  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-argv-names-"));
  const source = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");
  // The dash-named file is passed as a bare name from `cwd`, because a leading `-` only reads as a
  // switch when it is the first character of the argument.
  copyFileSync(source, path.join(dataHome, DASH_NAMED));
  copyFileSync(source, path.join(dataHome, SECOND_SUBTITLE));

  const state = launch(dataHome, dataHome, `s${LATIN1_E_ACUTE_ESCAPE}rie.srt`, [
    DASH_NAMED,
    SECOND_SUBTITLE,
    MISSING,
  ]);
  try {
    // Not a counted check: the same claim is asserted in the launch above, and asserting it twice
    // would inflate the counter that exists to catch a removed assertion.
    const { toplevel, reason } = await windowOrReason(state);
    if (toplevel === null) {
      throw new Error(reason);
    }
    const log = await waitForCommandLine(state, dataHome);
    check(
      "a subtitle whose name starts with a dash is taken, not dropped",
      log.includes(`command line: video=None, subtitle=Some("${DASH_NAMED}")`),
      log,
    );
    check(
      "a path that is not there is named in the log",
      log.includes(`command line: ignored ${MISSING} (not a file on disk)`),
      log,
    );
    check(
      "a second subtitle is named in the log rather than dropped in silence",
      log.includes(`command line: ignored ${SECOND_SUBTITLE} (a subtitle was already named)`),
      log,
    );
  } finally {
    cleanup(state);
  }
}

async function main() {
  requireDisplay();
  requireAppBinary();
  requireCloseWindowTool();

  // Sequential, and the first is fully reaped before the second starts: `findToplevel` throws when
  // two windows match, so two live instances would fail the run instead of poisoning it.
  await theArgumentThatCannotBeCarried();
  await theNamesTheFilterUsedToDrop();

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `startup args guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`startup args check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
