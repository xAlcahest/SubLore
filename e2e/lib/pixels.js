import { execFileSync, spawnSync } from "node:child_process";
import process from "node:process";

import { requireLinuxBackend } from "./platform.js";

/**
 * Is the native video surface showing a picture?
 *
 * The only honest answer is the pixels: the surface reports `IsViewable` while showing nothing at
 * all when mpv has not attached. Measured with ffmpeg rather than
 * ImageMagick: ffmpeg is installed by the e2e CI job (.github/workflows/ci.yml) and is a build
 * dependency here, while `magick` exists only in ImageMagick 7 and the runner ships 6.
 *
 * The measure is average saturation, not brightness. The empty stage is grey chrome that already
 * spans black to white, so a luma range cannot tell it from a picture: measured, both read about
 * 200 out of 255. Saturation separates them cleanly, because the chrome has none and the fixture is
 * colour bars.
 */

const SAT = /lavfi\.signalstats\.SATAVG=(-?\d+(?:\.\d+)?)/;

/** Missing tools fail with their own name, never as a cryptic error read as a regression. */
export function requireFfmpeg() {
  try {
    execFileSync("ffmpeg", ["-version"], { stdio: "ignore", timeout: 15000 });
  } catch {
    throw new Error(
      "E2E prerequisite missing: ffmpeg is not on PATH. It measures whether the video surface is " +
        "showing a picture. Install it (apt: ffmpeg, dnf: ffmpeg) and re-run.",
    );
  }
}

/**
 * The ffmpeg input that grabs `rect` off the screen, which is the only platform-shaped part of this
 * file: the saturation measure below and what it means are the same anywhere. Seam for MW.1b,
 * whose input device is `gdigrab` with an offset rather than a display name.
 * @param {{absX: number, absY: number, width: number, height: number}} rect
 */
function screenGrabArgs(rect) {
  requireLinuxBackend(
    "pixels.js screenGrabArgs",
    "hand ffmpeg one frame of a screen rectangle given in root coordinates",
  );
  const display = process.env.DISPLAY;
  if (display === undefined || display === "") {
    throw new Error("DISPLAY is not set; there is no screen to measure.");
  }
  return [
    "-f",
    "x11grab",
    // Explicit: x11grab's defaults for these two have changed between releases, and a different
    // frame rate or draw-mouse setting changes the pixels this measures.
    "-framerate",
    "1",
    "-draw_mouse",
    "0",
    "-video_size",
    `${rect.width}x${rect.height}`,
    "-i",
    `${display}+${rect.absX},${rect.absY}`,
  ];
}

/**
 * Average saturation inside `rect` of the screen.
 * @param {{absX: number, absY: number, width: number, height: number}} rect
 */
export function saturation(rect) {
  // ffmpeg writes signalstats to stderr, so both streams are read: reading stdout alone returns
  // nothing at all and the parse below fails with a null.
  const run = spawnSync(
    "ffmpeg",
    [
      "-hide_banner",
      ...screenGrabArgs(rect),
      "-frames:v",
      "1",
      "-vf",
      "signalstats,metadata=print",
      "-f",
      "null",
      "-",
    ],
    { encoding: "utf8", timeout: 20000 },
  );
  const output = `${run.stdout ?? ""}${run.stderr ?? ""}`;
  if (run.error !== undefined && run.error !== null) {
    throw new Error(`ffmpeg could not run: ${run.error.message}`);
  }
  // A non-zero exit with a parseable line would otherwise be read as a measurement. On a platform
  // this has never run on, the diagnosis has to be ffmpeg's own words, not a silent number.
  if (run.status !== 0) {
    throw new Error(
      `ffmpeg exited with status ${run.status} while measuring ${JSON.stringify(rect)}:\n` +
        `${output.trim().split("\n").slice(-15).join("\n")}`,
    );
  }

  const match = SAT.exec(output);
  if (match === null) {
    throw new Error(`ffmpeg printed no signalstats saturation for ${JSON.stringify(rect)}`);
  }
  return Number(match[1]);
}
