/**
 * N2b real-session probe: does the video paint on the owner's real display?
 *
 * **This is a probe, not a check. It asserts nothing.** It prints one saturation reading for each
 * of two states — no video, and a fixture loaded via `startup_files` — so a human can judge whether
 * the picture actually painted on real hardware. `docs/reports/n2b-collaudo-reale.md` is the
 * durable record of what it found (2026-08-30): saturation ~5.9 loaded against ~2.1 empty, three
 * runs out of three. `wayland-attach-check.js` covers the same session's *attachment* as an
 * asserted check; this probe covers the *pixels*, which that check deliberately does not assert
 * (see its own comment on why).
 *
 * WORKFLOW.md 4c: on the real display only launching, passing argv, and capturing the app's own
 * window are allowed — this probe never types or clicks. The fixture goes in on argv through
 * `startup_files`, and the two states are two separate launches rather than one launch driven by
 * input. Capture is `import -window <id>`, 4c's stated method: it needs no raise and no focus,
 * unlike a full-desktop screenshot under this rootless-XWayland session (see the removed
 * `spectacle -f` approach in git history, and the incident it required work around: focus-following
 * `xdotool type` landed a fixture path in the owner's own window mid-run).
 *
 * Needs the owner's own Wayland session, so it does not run in CI and has no `pnpm` script entry;
 * invoke it by hand: `node e2e/scripts/real-session-check.mjs`.
 *
 * Saturation is measured inline rather than through `e2e/lib/pixels.js`'s `saturation()`: that
 * helper captures with `x11grab` on the X root, which reads black here (measured) because this
 * XWayland is rootless and holds no desktop. The two share the signalstats parse, not the capture.
 */
import { spawnSync, spawn } from "node:child_process";
import console from "node:console";
import { mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

import {
  requireAppBinary,
  requireDisplay,
  requireVideoFixture,
  videoFixture,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { allWindows } from "../lib/x11.js";

const SAT = /lavfi\.signalstats\.SATAVG=(-?\d+(?:\.\d+)?)/;

/**
 * The app window, found by name and not by size: on this compositor the window does not keep the
 * 1024x700 it asks for, so the harness's geometry-based finder cannot see it here.
 */
function appWindow() {
  const named = allWindows().filter((w) => w.name === "Sublore" && w.width > 200);
  return named.sort((a, b) => b.width * b.height - a.width * a.height)[0] ?? null;
}

/** Average saturation of one window, captured directly rather than cropped from the desktop. */
function windowSaturation(id, tag, out) {
  const png = path.join(out, `${tag}.png`);
  const capture = spawnSync("import", ["-silent", "-window", id, png], {
    encoding: "utf8",
    timeout: 15000,
  });
  if (capture.status !== 0) {
    throw new Error(`import -window ${id} failed (${capture.status}): ${capture.stderr ?? ""}`);
  }
  const stats = spawnSync(
    "ffmpeg",
    ["-hide_banner", "-i", png, "-vf", "signalstats,metadata=print", "-f", "null", "-"],
    { encoding: "utf8", timeout: 20000 },
  );
  const output = `${stats.stdout ?? ""}${stats.stderr ?? ""}`;
  if (stats.status !== 0) {
    throw new Error(
      `ffmpeg exited with status ${stats.status} measuring ${png}:\n` +
        `${output.trim().split("\n").slice(-15).join("\n")}`,
    );
  }
  const match = SAT.exec(output);
  if (match === null) {
    throw new Error(`no saturation from ${png}`);
  }
  rmSync(png, { force: true });
  return Number(match[1]);
}

/** One launch, one state, one saturation reading. `args` goes straight to argv, never typed. */
async function oneRun(tag, args) {
  const dataHome = mkdtempSync(path.join(os.tmpdir(), `sublore-e2e-real-${tag}-`));
  const out = mkdtempSync(path.join(os.tmpdir(), `sublore-e2e-real-shot-${tag}-`));
  const app = spawn(requireAppBinary(), args, {
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
    // The session's own environment, Wayland and all: nothing is scrubbed, that is the point. The
    // one addition is the WebKit workaround, without which nothing renders at all.
    env: { ...process.env, WEBKIT_DISABLE_DMABUF_RENDERER: "1", XDG_DATA_HOME: dataHome },
  });
  const pgid = app.pid;
  let log = "";
  app.stdout.on("data", (d) => (log += d));
  app.stderr.on("data", (d) => (log += d));

  try {
    const top = await waitFor(() => appWindow(), { timeout: 30000, message: "the Sublore window" });
    // It resizes itself after mapping, and a loaded fixture needs time to reach the first frame.
    await sleep(args.length > 0 ? 8000 : 3000);
    const saturation = windowSaturation(top.id, tag, out);
    return { tag, saturation, log: log.trim().split("\n").slice(-3).join(" | ") };
  } finally {
    try {
      if (processGroupMembers(pgid).length > 0) {
        killGroup(pgid);
      }
    } catch {
      // Teardown must not mask the failure that got us here.
    }
    rmSync(dataHome, { recursive: true, force: true });
    rmSync(out, { recursive: true, force: true });
  }
}

requireDisplay();
requireAppBinary();
requireVideoFixture();

const empty = await oneRun("empty", []);
console.log(`empty:  saturation=${empty.saturation.toFixed(3)}  log: ${empty.log}`);
const loaded = await oneRun("loaded", [videoFixture]);
console.log(`loaded: saturation=${loaded.saturation.toFixed(3)}  log: ${loaded.log}`);
