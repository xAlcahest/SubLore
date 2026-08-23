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
