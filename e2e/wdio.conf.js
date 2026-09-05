import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
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
const EXPECTED_TESTS = 213;

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

/**
 * The one spec that needs a module file beside the executable, and the fixture it needs there.
 *
 * Cargo writes `libsublore_module_wrong_major.so`; a module ships as `sublore_module_*.so`, which
 * is the shape the loader matches, so the copy is also a rename.
 */
const MODULE_SPEC = "modules.spec.js";
/** One that loads and contributes, one that is refused: the spec asserts both in one launch. */
const MODULE_FIXTURES = ["sublore_module_fixture", "sublore_module_wrong_major"];

function wantsModuleFixture(specs) {
  return Array.isArray(specs) && specs.some((spec) => spec.endsWith(MODULE_SPEC));
}

function moduleFixturePaths(name) {
  const beside = path.dirname(requireAppBinary());
  return {
    source: path.join(beside, "examples", `lib${name}.so`),
    target: path.join(beside, `${name}.so`),
  };
}

function installModuleFixture(specs) {
  if (!wantsModuleFixture(specs)) {
    return;
  }
  for (const name of MODULE_FIXTURES) {
    const { source, target } = moduleFixturePaths(name);
    if (!existsSync(source)) {
      throw new Error(
        `${source} does not exist. The module fixtures are example targets of ` +
          "crates/sublore-module-fixture; `cargo test --workspace` builds them, and so does " +
          "`cargo build -p sublore-module-fixture --examples`.",
      );
    }
    copyFileSync(source, target);
  }
}

function removeModuleFixture(specs) {
  if (!wantsModuleFixture(specs)) {
    return;
  }
  for (const name of MODULE_FIXTURES) {
    rmSync(moduleFixturePaths(name).target, { force: true });
  }
}

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
    // First, before anything that can throw. A throw in here is logged and every spec runs anyway,
    // so a tally left from the last run then accumulates and the count guard below can be satisfied
    // by running subsets until the file is long enough. Measured 2026-09-03: four tests reported as
    // 4, 6, 9, 13 then 17 over five consecutive runs, because the model copy above was failing on a
    // full disk and, when it ran first, taking `resetTally` down with it.
    resetTally();
    // Fail before the first session rather than mid-assertion with a confusing message.
    requireDisplay();
    requireTool("xdotool", "click and type into the app");
    requireTool("xwininfo", "read the window tree");
    requireAppBinary();
    requireVideoFixture();
    // Stays here, not at module load: this file is read by the launcher and again by every worker,
    // and this copies a 75 MB model. At load it ran once per spec and they fought over the file.
    installStubSidecar();
  },

  afterTest: (test, context, result) => {
    if (result.passed) {
      recordPassedTest(`${test.parent} ${test.title}`);
    }
  },

  /**
   * The app is launched by the session, so anything that has to be on disk before it starts has to
   * be put there here. `modules.spec.js` needs a module file beside the executable, and no `before`
   * hook inside a spec runs early enough for the app to see one.
   */
  beforeSession: async (config_, capabilities, specs) => {
    installModuleFixture(specs);
    await startDriver();
  },

  afterSession: (config_, capabilities, specs) => {
    stopDriver();
    // Unconditional: a module file left beside the executable would change what every later spec
    // starts with, and a failed run is exactly when it would be left behind.
    removeModuleFixture(specs);
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
