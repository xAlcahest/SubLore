/**
 * The two waveform numbers CONTRIBUTING.md section 7 claims, measured rather than asserted from
 * outside (M2.4 W10).
 *
 * Both are read from the app's own log, which says when a job first had peaks to show and how long
 * it took altogether. Nothing here reads the canvas: the number is about when the peaks reached the
 * page, `waveform.spec.js` already proves the panel draws them when they arrive, and a check that
 * needed a DOM could not run outside WebdriverIO.
 *
 *   pnpm e2e:waveform-budget              # the 60 s fixture, and the 2 s guard. This is CI's.
 *   pnpm e2e:waveform-budget --with-24min # also the 24 minute one, which is the owner's machine
 *
 * The 24-minute number is not asserted in CI on purpose: a runner is not the machine the budget
 * names, and a 24-minute fixture is not a thing to generate on every push.
 *
 * Each fixture is opened twice against one data home: the first run reads the media, the second
 * reads what the first left in the cache, and the difference is printed because that difference is
 * the cache doing its job.
 */
import { spawn } from "node:child_process";
import console from "node:console";
import { mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { appLog } from "../lib/applog.js";
import { appEnv } from "../lib/env.js";
import {
  requireAppBinary,
  requireDisplay,
  requireLongFixture,
  requireWaveformFixture,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";

/** CONTRIBUTING.md section 7: something to look at within this long of the media opening. */
const FIRST_PEAKS_MS = 2000;

/** And a 24 minute episode read all the way inside this, on the machine the budget names. */
const FULL_PEAK_MS = 20000;

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
let checksRun = 0;

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`waveform budget check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

function millisecondsIn(log, pattern, what) {
  const found = log.match(pattern);
  if (found === null) {
    throw new Error(`the app never said ${what}. Its log held:\n${log}`);
  }
  return Number(found[1]);
}

/**
 * The log this launch wrote, and nothing the ones before it did.
 *
 * The file is appended to across launches, so a pattern that matches anywhere is satisfied by
 * history: the second run's wait returned on the first run's line, and the numbers read back were
 * the first run's numbers. Everything here is read from `after` onwards.
 */
async function logAfter(dataHome, after, pattern, what) {
  return waitFor(
    () => {
      const fresh = appLog(dataHome).slice(after);
      return pattern.test(fresh) ? fresh : null;
    },
    { timeout: 120000, message: what },
  );
}

/** One launch against `dataHome`, closed as soon as the job it started has finished. */
async function open(dataHome, media) {
  const before = appLog(dataHome).length;
  const app = spawn(requireAppBinary(), [media], {
    detached: true,
    stdio: ["ignore", "ignore", "ignore"],
    env: appEnv({ XDG_DATA_HOME: dataHome }),
  });
  const pgid = app.pid;
  let exit = null;
  app.on("exit", () => {
    exit = true;
  });
  try {
    const log = await logAfter(
      dataHome,
      before,
      /waveform: job \d+ finished in \d+ ms/,
      "the peak job this launch started to finish",
    );
    return {
      firstPeaks: millisecondsIn(
        log,
        /waveform: job \d+ had its first peaks after (\d+) ms/,
        "when it first had peaks",
      ),
      finished: millisecondsIn(log, /waveform: job \d+ finished in (\d+) ms/, "when it finished"),
      fromCache: /read from the cache: true/.test(log),
    };
  } finally {
    try {
      if (exit === null && processGroupMembers(pgid).length > 0) {
        killGroup(pgid);
      }
      await waitFor(() => (processGroupMembers(pgid).length === 0 ? true : null), {
        timeout: 15000,
        message: "the app to go",
      });
    } catch {
      // Teardown must not rewrite the result.
    }
  }
}

/** Both runs of one fixture, and what they cost. */
async function measure(name, media) {
  const dataHome = mkdtempSync(path.join(os.tmpdir(), `sublore-budget-${name}-`));
  const cold = await open(dataHome, media);
  const warm = await open(dataHome, media);
  console.log(
    `  ${name}: first peaks ${cold.firstPeaks} ms cold and ${warm.firstPeaks} ms warm, ` +
      `read in ${cold.finished} ms cold and ${warm.finished} ms warm`,
  );
  return { cold, warm };
}

async function main() {
  requireDisplay();
  requireAppBinary();
  const withLong = process.argv.includes("--with-24min");

  const short = await measure("60s", requireWaveformFixture());
  check(
    `the 60 s fixture has peaks on screen inside ${FIRST_PEAKS_MS} ms`,
    short.cold.firstPeaks < FIRST_PEAKS_MS,
    `it took ${short.cold.firstPeaks} ms`,
  );
  // The regression this guard exists for: peaks delivered in one block at the end would make the
  // first of them arrive when the last one does.
  check(
    "the first peaks arrive before the job is over, which is what streaming means",
    short.cold.firstPeaks < short.cold.finished,
    `first peaks at ${short.cold.firstPeaks} ms, finished at ${short.cold.finished} ms`,
  );
  check(
    "the second run reads what the first left in the cache",
    short.warm.fromCache,
    "the app did not say it read from the cache",
  );
  check(
    "and it is not slower for it",
    short.warm.finished <= short.cold.finished,
    `cold ${short.cold.finished} ms, warm ${short.warm.finished} ms`,
  );

  let expected = 4;
  if (withLong) {
    const long = await measure("24min", requireLongFixture());
    check(
      `the 24 minute fixture is read inside ${FULL_PEAK_MS} ms`,
      long.cold.finished < FULL_PEAK_MS,
      `it took ${long.cold.finished} ms`,
    );
    check(
      `and it too has peaks on screen inside ${FIRST_PEAKS_MS} ms`,
      long.cold.firstPeaks < FIRST_PEAKS_MS,
      `it took ${long.cold.firstPeaks} ms`,
    );
    expected += 2;
  } else {
    console.log("  (the 24 minute number belongs to the owner's machine: --with-24min)");
  }

  // Not a `check` of its own: one that counts the others cannot count itself without arithmetic
  // nobody should have to follow.
  if (checksRun !== expected) {
    throw new Error(
      `waveform budget check failed: ${checksRun} of ${expected} checks ran, so one was skipped`,
    );
  }
  console.log(`waveform budget check passed (${checksRun}/${expected} checks)`);
}

await main();
