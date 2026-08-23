import { createHash } from "node:crypto";
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

import { driverPort } from "./driver.js";
import { repoRoot } from "./paths.js";

/**
 * WebdriverIO's `results.passed` in `onComplete` counts spec *files*, not tests, so it cannot see
 * an `it.skip` inside a file that otherwise passes. The launcher and the workers are separate
 * processes, so passed tests are tallied through a file both derive the same way.
 */
const tallyFile = path.join(
  os.tmpdir(),
  `sublore-e2e-tally-${createHash("sha1").update(repoRoot).digest("hex").slice(0, 12)}-${driverPort}`,
);

export function resetTally() {
  writeFileSync(tallyFile, "");
}

export function recordPassedTest(title) {
  appendFileSync(tallyFile, `${title.replace(/\n/g, " ")}\n`);
}

export function passedTests() {
  try {
    return readFileSync(tallyFile, "utf8")
      .split("\n")
      .filter((line) => line !== "");
  } catch {
    return [];
  }
}
