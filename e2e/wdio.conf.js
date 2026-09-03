import { mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { asrDir, cacheHome, installStubSidecar, stubBinary } from "./lib/asr.js";
import { appEnv } from "./lib/env.js";
import { driverPort, startDriver, stopDriver } from "./lib/driver.js";
import { requireAppBinary, requireDisplay, requireTool, requireVideoFixture } from "./lib/paths.js";
import { passedTests, recordPassedTest, resetTally } from "./lib/tally.js";

/**
 * Every spec that exists must run. WebdriverIO does not reliably fail a run that executed nothing,
 * so the count is asserted here. Bump it when you add a test; see e2e/README.md.
 */
const EXPECTED_TESTS = 126;

// Keeps a run out of the real data dir. Created once in the launcher; workers inherit the value.
process.env.SUBLORE_E2E_DATA_HOME ??= mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-"));
process.env.XDG_DATA_HOME = process.env.SUBLORE_E2E_DATA_HOME;
// Pinned before the line below points XDG_CACHE_HOME at this run's own tree: a real model lives in
// the developer's cache, and `sourceModel` falls back to whatever XDG_CACHE_HOME says.
process.env.SUBLORE_TEST_MODEL_DIR ??= path.join(cacheHome(), "sublore", "models");
// One rule, one place: `appEnv` owns it and this copies the result onto the environment the
// driver chain inherits.
Object.assign(process.env, appEnv());
delete process.env.WAYLAND_DISPLAY;

// The transcription spec always runs against the stand-in sidecar, never a real whisper build: it
// needs a run it can cancel mid-flight, and CI has no model. Set unconditionally, so an inherited
// SUBLORE_WHISPER_BIN cannot quietly change what asr.spec.js is asserting. See e2e/README.md.
process.env.SUBLORE_E2E_ASR_DIR = asrDir();
process.env.SUBLORE_WHISPER_BIN = stubBinary();
// For the app, not the harness: no spec measures pixels, asr.spec.js runs a real extraction. At
// load rather than in `onPrepare`, where a throw is logged and every spec runs regardless.
requireTool("ffmpeg", "extract the audio the transcription spec transcribes");

export const config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: driverPort,
  specs: ["./specs/*.spec.js"],
  maxInstances: 1,
  capabilities: [{ "tauri:options": { application: requireAppBinary() } }],
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 60000 },
  // The spec reporter prints a file's whole tick list only when that file ends, so a long spec is
  // minutes of silence. Realtime sends one line per test to the launcher as each test finishes.
  reporters: [["spec", { realtimeReporting: true }]],
  logLevel: "warn",
  waitforTimeout: 20000,

  onPrepare: () => {
    // Fail before the first session rather than mid-assertion with a confusing message.
    requireDisplay();
    requireTool("xdotool", "click and type into the app");
    requireTool("xwininfo", "read the window tree");
    requireAppBinary();
    requireVideoFixture();
    installStubSidecar();
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
