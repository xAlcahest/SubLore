import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));

export const repoRoot = path.resolve(here, "..", "..");

/** Honour CARGO_TARGET_DIR the way cargo does, so the harness finds the build cargo actually made. */
const targetDir = process.env.CARGO_TARGET_DIR
  ? path.resolve(repoRoot, process.env.CARGO_TARGET_DIR)
  : path.join(repoRoot, "target");

export const appBinary = path.join(targetDir, "debug", "sublore");
export const videoFixture = path.join(repoRoot, "fixtures", "video", "sample.mkv");
export const closeWindowTool = path.join(repoRoot, "e2e", "tools", "close-window.py");

/** The toplevel the app configures in src-tauri/tauri.conf.json. Frozen contract, see e2e/README.md. */
export const windowTitle = "Sublore";
export const windowWidth = 1024;
export const windowHeight = 700;

/**
 * Where the first cue's text sits inside the window the close-gate scripts launch.
 *
 * Those scripts click a cue without a WebDriver session, so the point has to be a number rather
 * than a lookup, and T1 moved it: the path boxes left both bars and every row came up with them.
 *
 * It belongs to those scripts and to no one else. They start the app on an empty data home, so the
 * transcribe bar carries a Download button for the model that is not there and stands taller than
 * it does under WebDriver, which runs with a model on disk: measured, 23 px of difference in where
 * the rows land. Re-measure it the way it was measured — a screenshot of the running check — and
 * not from a spec, whose window is a different one.
 */
export const firstCueText = { x: 840, y: 523 };

/**
 * Missing prerequisites are failures with an actionable message, never skips (design section 10).
 * @param {string} file
 * @param {string} how
 */
function requireFile(file, how) {
  if (!existsSync(file)) {
    throw new Error(`E2E prerequisite missing: ${file} does not exist. Run: ${how}`);
  }
  return file;
}

export function requireAppBinary() {
  return requireFile(
    appBinary,
    "pnpm e2e:build  (plain `cargo build` produces a binary that loads the Vite dev URL and is unusable here)",
  );
}

export function requireVideoFixture() {
  return requireFile(videoFixture, "sh fixtures/video/make-sample.sh");
}

/** A command the harness drives the app with. Missing means one sentence, never a cryptic error. */
export function requireTool(name, what) {
  try {
    execFileSync("sh", ["-c", `command -v ${name}`], { stdio: "ignore", timeout: 10000 });
  } catch {
    throw new Error(
      `E2E prerequisite missing: ${name} is not on PATH. The harness uses it to ${what}.`,
    );
  }
}

export function requireCloseWindowTool() {
  return requireFile(closeWindowTool, "restore e2e/tools/close-window.py from git");
}

/** The harness drives a real X server; without one every assertion below is meaningless. */
export function requireDisplay() {
  const display = process.env.DISPLAY;
  if (display === undefined || display === "") {
    throw new Error(
      "E2E prerequisite missing: DISPLAY is not set. Run under Xvfb, e.g. `xvfb-run -a pnpm e2e`.",
    );
  }
  return display;
}
