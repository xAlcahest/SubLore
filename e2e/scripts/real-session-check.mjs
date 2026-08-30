/**
 * N2b real-session check: does the video reach the screen in the owner's own Wayland session?
 *
 * Xvfb could not answer this. The attachment fix is reliable there — mpv's child window appears in
 * every run — but the picture showed only twice in ten, while the same mpv driven from the command
 * line in the same Xvfb rendered perfectly (docs/reports/n2b-stato.md).
 *
 * What this run has to work around, all of it measured rather than assumed:
 * - XWayland here is `-rootless`, so the X root holds no desktop and `x11grab` reads black whatever
 *   the app does. Capture goes through the compositor with `spectacle -f`.
 * - The composited image is 8960x2880 while X reports 6720x2160: exactly 4/3 on both axes, which is
 *   how a window's X geometry maps onto the screenshot.
 * - The window opens behind whatever is already there, and a covered window is not in the capture.
 *   It is raised and activated before every measurement.
 * - WebKit cannot allocate its GBM buffer on this machine and paints nothing at all without
 *   `WEBKIT_DISABLE_DMABUF_RENDERER=1`. That is a defect of its own, filed separately; without the
 *   variable there is no UI to test the video against.
 */
import { execFileSync, spawnSync, spawn } from "node:child_process";
import console from "node:console";
import { mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

const REPO = "/home/alcahest/git/SubLore";
const { clickAt, focusWindow, typeText } = await import(path.join(REPO, "e2e/lib/input.js"));
const { allWindows, childWindows, mapState } = await import(path.join(REPO, "e2e/lib/x11.js"));

/**
 * The app window, found by name and not by size. On this display the window does not keep the
 * 1024x700 it asks for — measured, it settles at 2151x1236 seconds after mapping — so the harness's
 * geometry-based finder, written for a 1:1 Xvfb screen, cannot see it at all here.
 */
function appWindow() {
  const named = allWindows().filter((w) => w.name === "Sublore" && w.width > 200);
  return named.sort((a, b) => b.width * b.height - a.width * a.height)[0] ?? null;
}
const { waitFor } = await import(path.join(REPO, "e2e/lib/proc.js"));

const OUT = mkdtempSync(path.join(os.tmpdir(), "n2b-real-"));
const FIXTURE = path.join(REPO, "fixtures/video/sample.mkv");
/** Composited pixels per X coordinate on this desktop, measured: 8960/6720 and 2880/2160. */
const K = 4 / 3;

/**
 * Click points as fractions of the window, taken from the 1024x700 layout the harness knows. Fixed
 * pixels do not survive here: this window settles at 2151x1236, so 1024-based coordinates land
 * outside the controls and the run silently measures a video that was never opened.
 */
const POINTS = {
  videoField: { fx: 683 / 1024, fy: 24 / 700 },
  videoOpen: { fx: 978 / 1024, fy: 24 / 700 },
  transport: { fx: 329 / 1024, fy: 480 / 700 },
};

function clickIn(win, point) {
  clickAt(
    Math.round(win.absX + win.width * point.fx),
    Math.round(win.absY + win.height * point.fy),
  );
}

function raise(id) {
  execFileSync("xdotool", ["windowraise", id], { timeout: 15000 });
  execFileSync("xdotool", ["windowactivate", "--sync", id], { timeout: 15000 });
}

/** Saturation of the app window, cropped out of a compositor screenshot. */
function windowSaturation(rect, tag) {
  const full = path.join(OUT, `${tag}-full.png`);
  const run = spawnSync("spectacle", ["-f", "-b", "-n", "-o", full, "-d", "200"], {
    encoding: "utf8",
    timeout: 30000,
  });
  if (run.status !== 0) {
    throw new Error(`spectacle failed (${run.status}): ${run.stderr ?? ""}`);
  }
  const crop =
    `${Math.round(rect.width * K)}:${Math.round(rect.height * K)}:` +
    `${Math.round(rect.absX * K)}:${Math.round(rect.absY * K)}`;
  const win = path.join(OUT, `${tag}.png`);
  spawnSync("ffmpeg", ["-hide_banner", "-y", "-i", full, "-vf", `crop=${crop}`, win], {
    timeout: 20000,
  });
  const stats = spawnSync(
    "ffmpeg",
    ["-hide_banner", "-i", win, "-vf", "signalstats,metadata=print", "-f", "null", "-"],
    { encoding: "utf8", timeout: 20000 },
  );
  const output = `${stats.stdout ?? ""}${stats.stderr ?? ""}`;
  const match = /lavfi\.signalstats\.SATAVG=(-?\d+(?:\.\d+)?)/.exec(output);
  if (match === null) {
    throw new Error(`no saturation from ${win}`);
  }
  rmSync(full, { force: true });
  return Number(match[1]);
}

async function oneRun(index) {
  const dataHome = mkdtempSync(path.join(os.tmpdir(), `n2b-real-data-${index}-`));
  const app = spawn(path.join(REPO, "target/debug/sublore"), [], {
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
    // The session's own environment, Wayland and all: nothing is scrubbed, that is the point. The
    // one addition is the WebKit workaround, without which nothing renders at all.
    env: { ...process.env, WEBKIT_DISABLE_DMABUF_RENDERER: "1", XDG_DATA_HOME: dataHome },
  });
  let log = "";
  app.stdout.on("data", (d) => (log += d));
  app.stderr.on("data", (d) => (log += d));
  const row = { run: index };

  try {
    const top = await waitFor(() => appWindow(), { timeout: 30000, message: "the Sublore window" });
    await sleep(2500); // it resizes itself after mapping; measure the size it settles on
    raise(top.id);
    await sleep(3000);
    row.empty = windowSaturation(appWindow(), `run${index}-0-empty`);

    const live = appWindow();
    focusWindow(live.id);
    clickIn(live, POINTS.videoField);
    typeText(FIXTURE);
    clickIn(live, POINTS.videoOpen);
    await sleep(7000);

    const surface = childWindows(live.id)
      .filter((c) => c.width > 50 && c.height > 50)
      .sort((a, b) => b.width * b.height - a.width * a.height)[0];
    row.mpvChildren = surface === undefined ? -1 : childWindows(surface.id).length;
    row.surfaceMap = surface === undefined ? "none" : mapState(surface.id);

    raise(live.id);
    await sleep(600);
    row.paused = windowSaturation(appWindow(), `run${index}-1-paused`);

    clickIn(appWindow(), POINTS.transport);
    await sleep(2500);
    raise(live.id);
    await sleep(600);
    row.playing = windowSaturation(appWindow(), `run${index}-2-playing`);
    row.log = log.trim().split("\n").slice(-3).join(" | ");
  } finally {
    try {
      process.kill(-app.pid, "SIGTERM");
    } catch {
      /* gone */
    }
    await sleep(1500);
    try {
      process.kill(-app.pid, "SIGKILL");
    } catch {
      /* gone */
    }
  }
  return row;
}

const rows = [];
for (let i = 1; i <= 3; i += 1) {
  const row = await oneRun(i);
  rows.push(row);
  console.log(
    `run ${row.run}: empty=${row.empty?.toFixed(3)} paused=${row.paused?.toFixed(3)} ` +
      `playing=${row.playing?.toFixed(3)} mpvChildren=${row.mpvChildren} surface=${row.surfaceMap}`,
  );
}
console.log(`\nartifacts in ${OUT}`);
