/**
 * The shipping configuration paints: the app launched with the NVIDIA WebKit workarounds **armed**
 * opens a window that is not blank (BACKLOG N2b, gate 2 register `e2e/lib/env.js:26`).
 *
 * Every other harness disarms the mitigation — `appEnv` sets `SUBLORE_WEBKIT_WORKAROUNDS=0`
 * (e2e/lib/env.js) — so the whole automated suite runs a configuration no user gets. This check is
 * the one that runs the configuration they do get: the session's own environment, the app's own
 * detection, and a measurement of the window's pixels rather than of any internal flag.
 *
 * It runs twice, armed and disarmed, and prints both. On a machine where the mitigation fires the
 * pair is the discrimination: armed paints, disarmed is the flat window of
 * docs/reports/n2b-collaudo-reale.md. Only the armed run is asserted, because a disarmed run that
 * paints is the correct outcome on hardware the workarounds are not for.
 *
 * Capture is `import -window <id>`, not a root grab: under rootless XWayland the X root holds no
 * desktop and `x11grab` reads black whatever the app draws (WORKFLOW.md 4c).
 *
 * The time it prints is spawn to the first capture that is not flat, at 500 ms polling granularity.
 * It is not asserted on and one run is not a measurement: four runs on the same Xvfb spread from
 * 698 to 2102 ms in both configurations. It is a starting point for CLAUDE.md §7's cold start, not
 * an answer to it.
 *
 * Not in the CI job: `ubuntu-latest` has no NVIDIA module, so the branch under test cannot be taken
 * there, and what WebKit's DMABUF renderer does under that runner's llvmpipe has never been
 * measured. Stating the gap rather than leaving it silent (gate 2 register, L7 F3).
 */
import { spawn, spawnSync } from "node:child_process";
import console from "node:console";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { requireAppBinary, requireDisplay, requireTool } from "../lib/paths.js";
import { requireFfmpeg } from "../lib/pixels.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { allWindows, mapState, rootTree } from "../lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 5;
let checksRun = 0;

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`webview paint check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/**
 * A window whose luma is flat is a window showing nothing. Measured on the owner's hardware: the
 * blank webview reads 46..46, the interface 16..235 (docs/reports/n2b-collaudo-reale.md). Any
 * boundary between 0 and 219 separates them; this one sits far from both.
 */
const PAINTED_RANGE = 32;
const NVIDIA_MODULE = "/sys/module/nvidia";
const DECISION =
  /^sublore: webview workarounds (applied|not applied) \(SUBLORE_WEBKIT_WORKAROUNDS=(.*), \/sys\/module\/nvidia (present|absent)\)$/m;

/**
 * The app toplevel by name, not by geometry: on a scaled or fractional display the window does not
 * keep the 1024x700 it asks for, and this check has to work on the owner's real display too.
 */
function appWindow() {
  const named = allWindows().filter((window) => window.name === "Sublore" && window.width > 200);
  return named.sort((a, b) => b.width * b.height - a.width * a.height)[0] ?? null;
}

/** Luma range of one window's own pixels: max minus min, 0 on a flat window. */
function windowLumaRange(id, file) {
  const shot = spawnSync("import", ["-window", id, `png:${file}`], {
    encoding: "utf8",
    timeout: 30000,
  });
  if (shot.status !== 0) {
    throw new Error(`import -window ${id} failed (${shot.status}): ${shot.stderr ?? ""}`);
  }
  const stats = spawnSync(
    "ffmpeg",
    ["-hide_banner", "-i", file, "-vf", "signalstats,metadata=print", "-f", "null", "-"],
    { encoding: "utf8", timeout: 30000 },
  );
  const output = `${stats.stdout ?? ""}${stats.stderr ?? ""}`;
  if (stats.status !== 0) {
    throw new Error(
      `ffmpeg exited with status ${stats.status} while measuring ${file}:\n` +
        `${output.trim().split("\n").slice(-15).join("\n")}`,
    );
  }
  const min = /lavfi\.signalstats\.YMIN=(-?\d+(?:\.\d+)?)/.exec(output);
  const max = /lavfi\.signalstats\.YMAX=(-?\d+(?:\.\d+)?)/.exec(output);
  if (min === null || max === null) {
    throw new Error(`ffmpeg printed no signalstats luma for ${file}`);
  }
  return { min: Number(min[1]), max: Number(max[1]), range: Number(max[1]) - Number(min[1]) };
}

/**
 * One launch. `armed` false sets the escape hatch; armed true adds nothing at all, so the app's own
 * detection is what decides and the run measures the code instead of standing in for it.
 */
async function launch(armed, outDir) {
  const dataHome = mkdtempSync(
    path.join(os.tmpdir(), `sublore-e2e-paint-${armed ? "on" : "off"}-`),
  );
  const env = { ...process.env, XDG_DATA_HOME: dataHome };
  delete env.SUBLORE_WEBKIT_WORKAROUNDS;
  if (!armed) {
    env.SUBLORE_WEBKIT_WORKAROUNDS = "0";
  }
  const started = Date.now();
  const app = spawn(requireAppBinary(), [], {
    detached: true,
    stdio: ["ignore", "ignore", "pipe"],
    env,
  });
  const pgid = app.pid;
  let stderr = "";
  app.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  let exit = null;
  app.on("exit", (code, signal) => {
    exit = { code, signal };
  });

  const result = { armed, stderr: "", pgid };
  try {
    const window = await waitFor(
      () => {
        if (exit !== null) {
          throw new Error(`the app exited before its window appeared (code ${exit.code})`);
        }
        return appWindow();
      },
      { timeout: 40000, message: `the "Sublore" toplevel (armed: ${armed})` },
    );
    await waitFor(() => mapState(window.id) === "IsViewable", {
      timeout: 20000,
      message: `the window to be mapped.\n${rootTree()}`,
    });

    // Poll rather than sleep: the first painted frame is what the cold-start number wants, and a
    // window that never paints has to cost the timeout, not a lucky early capture.
    const file = path.join(outDir, `${armed ? "armed" : "disarmed"}.png`);
    let luma = null;
    await waitFor(
      () => {
        luma = windowLumaRange(window.id, file);
        return luma.range >= PAINTED_RANGE;
      },
      { timeout: 25000, interval: 500, message: "the window to paint anything at all" },
    ).catch((error) => {
      // A capture that never succeeded is a broken instrument, not a blank window: it fails as
      // itself instead of being reported as the defect under test.
      if (luma === null) {
        throw error;
      }
    });
    result.painted = luma.range >= PAINTED_RANGE ? Date.now() - started : null;
    result.luma = luma;
    result.geometry = `${window.width}x${window.height}`;
  } finally {
    killGroup(pgid, "SIGTERM");
    await waitFor(() => processGroupMembers(pgid).length === 0, {
      timeout: 10000,
      message: `process group ${pgid} to be empty`,
    }).catch(() => killGroup(pgid));
    rmSync(dataHome, { recursive: true, force: true });
  }
  result.stderr = stderr;
  return result;
}

/** The run has to be able to tell the mitigation from an outside variable that does the same job. */
function requireUncontaminatedEnvironment() {
  if (process.env.WEBKIT_DISABLE_DMABUF_RENDERER !== undefined) {
    throw new Error(
      "WEBKIT_DISABLE_DMABUF_RENDERER is already set in this shell. The app would paint the same " +
        "whether its own mitigation fired or not, so this run could not discriminate. Unset it.",
    );
  }
}

async function main() {
  requireDisplay();
  requireAppBinary();
  requireFfmpeg();
  requireTool("import", "capture the app's own window, which a root grab cannot do under XWayland");
  requireUncontaminatedEnvironment();

  const moduleLoaded = existsSync(NVIDIA_MODULE);
  console.log(
    `  ${NVIDIA_MODULE} ${moduleLoaded ? "present" : "absent"}, DISPLAY=${process.env.DISPLAY}`,
  );
  const outDir = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-paint-"));

  try {
    const armed = await launch(true, outDir);
    const decision = DECISION.exec(armed.stderr);
    check(
      "the armed launch recorded the rendering path it chose",
      decision !== null,
      `nothing in the app's stderr matched the decision line:\n${armed.stderr.trim()}`,
    );
    check(
      "its decision agrees with this machine's driver state",
      decision[1] === (moduleLoaded ? "applied" : "not applied") &&
        decision[3] === (moduleLoaded ? "present" : "absent"),
      `the app said "${decision[0]}" while ${NVIDIA_MODULE} is ` +
        `${moduleLoaded ? "present" : "absent"}`,
    );
    check(
      "it left the escape hatch alone",
      decision[2] === "unset",
      `the app read SUBLORE_WEBKIT_WORKAROUNDS=${decision[2]}, so this run measured the hatch, ` +
        "not the detection",
    );
    console.log(
      `  armed:    luma ${armed.luma.min}..${armed.luma.max} (range ${armed.luma.range}), ` +
        `${armed.painted === null ? "never painted" : `${armed.painted} ms to first painted capture`}, ` +
        `window ${armed.geometry}`,
    );
    check(
      "the window painted in the configuration users get",
      armed.luma.range >= PAINTED_RANGE,
      `the window is flat at ${armed.luma.min}..${armed.luma.max}: the webview drew nothing. ` +
        `Capture in ${outDir}.`,
    );

    const disarmed = await launch(false, outDir);
    const off = DECISION.exec(disarmed.stderr);
    check(
      "the escape hatch turns the workarounds off",
      off !== null && off[1] === "not applied" && off[2] === "0",
      `stderr said:\n${disarmed.stderr.trim()}`,
    );
    console.log(
      `  disarmed: luma ${disarmed.luma.min}..${disarmed.luma.max} (range ${disarmed.luma.range}), ` +
        `${disarmed.painted === null ? "never painted" : `${disarmed.painted} ms to first painted capture`}, ` +
        `window ${disarmed.geometry}`,
    );

    // The pair, in words, because the numbers alone do not say which machine they describe.
    if (!moduleLoaded) {
      console.log(
        "  This machine has no NVIDIA module, so the mitigation could not fire in either run: " +
          "the paint assertion holds, the comparison proves nothing about the workaround.",
      );
    } else if (disarmed.luma.range < PAINTED_RANGE) {
      console.log(
        "  Disarmed, this machine's window is flat: the mitigation is what makes it paint.",
      );
    } else {
      console.log(
        "  Both configurations paint here, so the mitigation is not what makes this window " +
          "visible on this machine. Its cost is the difference in the two times above.",
      );
    }
  } finally {
    rmSync(outDir, { recursive: true, force: true });
  }

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `webview paint guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a failure. See e2e/README.md.",
    );
  }
  console.log(`webview paint check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
