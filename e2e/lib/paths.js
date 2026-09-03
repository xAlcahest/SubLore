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
 * than a lookup, and every layout change moves it: T1 took the path boxes out of both bars, T2
 * put the grid under a fixed-height top block instead of under a stack of them, and T3 replaced
 * two command bars with a menu bar and a toolbar, which are 35 px shorter together.
 *
 * T4 took the transcription band off the screen, which moved the row up 87 px: measured off two
 * screenshots of the launch below, where the highlighted first row sat at y 393..420 before and at
 * 306..333 after.
 *
 * It belongs to those scripts and to the N1b probe beside them, which launches the app the same
 * way and had been carrying a copy of its own two layouts out of date. They start the app on an
 * empty data home and the window is not the one a spec drives. Re-measure it the way it was
 * measured — a screenshot of a launch made the way those scripts make it — and never from a spec.
 */
export const firstCueText = { x: 840, y: 320 };

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

/** The M2.4 fixture whose audio is known: six ten-second blocks, tone first. Generated, not committed. */
export const waveformFixture = path.join(repoRoot, "fixtures", "video", "waveform-60s.mkv");

/** The M2.4 fixture for the 24 minute number. Written only by `--with-24min`, never in CI. */
export const longFixture = path.join(repoRoot, "fixtures", "video", "waveform-24min.mkv");

export function requireLongFixture() {
  return requireFile(longFixture, "sh fixtures/video/make-waveform-fixtures.sh --with-24min");
}

export function requireWaveformFixture() {
  return requireFile(waveformFixture, "sh fixtures/video/make-waveform-fixtures.sh");
}

/** The M2.4 fixture with a picture and no audio at all (decision 24 E3). Generated, not committed. */
export const silentFixture = path.join(repoRoot, "fixtures", "video", "waveform-silent.mkv");

export function requireSilentFixture() {
  return requireFile(silentFixture, "sh fixtures/video/make-waveform-fixtures.sh");
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
