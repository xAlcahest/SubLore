import { mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { driverPort, startDriver, stopDriver } from "./lib/driver.js";
import { requireAppBinary, requireDisplay, requireVideoFixture } from "./lib/paths.js";
import { passedTests, recordPassedTest, resetTally } from "./lib/tally.js";

/**
 * Every spec that exists must run. WebdriverIO does not reliably fail a run that executed nothing,
 * so the count is asserted here. Bump it when you add a test; see e2e/README.md.
 */
const EXPECTED_TESTS = 22;

// Keeps a run out of the real data dir. Created once in the launcher; workers inherit the value.
process.env.SUBLORE_E2E_DATA_HOME ??= mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-"));
process.env.XDG_DATA_HOME = process.env.SUBLORE_E2E_DATA_HOME;

export const config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: driverPort,
  specs: ["./specs/*.spec.js"],
  maxInstances: 1,
  capabilities: [{ "tauri:options": { application: requireAppBinary() } }],
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 60000 },
  reporters: ["spec"],
  logLevel: "warn",
  waitforTimeout: 20000,

  onPrepare: () => {
    // Fail before the first session rather than mid-assertion with a confusing message.
    requireDisplay();
    requireAppBinary();
    requireVideoFixture();
    resetTally();
  },

  afterTest: (test, context, result) => {
    if (result.passed) {
      recordPassedTest(`${test.parent} ${test.title}`);
    }
  },

  beforeSession: async () => {
    await startDriver();
  },

  afterSession: () => {
    stopDriver();
  },

  onComplete: (exitCode, capabilities, config_, results) => {
    if (results.failed > 0) {
      return;
    }
    const passed = passedTests();
    if (passed.length < EXPECTED_TESTS) {
      throw new Error(
        `E2E guard: expected at least ${EXPECTED_TESTS} passing tests, got ${passed.length}` +
          `${passed.length === 0 ? "" : ` (${passed.join("; ")})`}. ` +
          "Deleting, skipping or filtering out a spec is a CI failure, not a green run. " +
          "See e2e/README.md.",
      );
    }
  },
};
