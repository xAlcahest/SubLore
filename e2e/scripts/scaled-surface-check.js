/**
 * An integer display scale is applied to the video surface exactly once.
 *
 * **This does not prove N2c, and saying so is the point.** N2c is a fractional-scale defect: there
 * GTK's own factor is 1 and the 1.5 arrives as page zoom, so the page's rectangle has to be
 * resolved to native pixels before it crosses the IPC boundary. Under `GDK_SCALE` the ratio comes
 * from GTK's factor instead, GDK re-applies it on the way to X, and the old code and the new one
 * produce the same geometry. A fractional ratio cannot be produced here at all: `Xft.dpi` through
 * `xrdb` does not reach WebKitGTK without an XSETTINGS manager, and neither does a `gtk-xft-dpi`
 * settings file — both measured, both leaving `devicePixelRatio` at 1. N2c's own criterion is met
 * on the owner's 1.5 display, and nowhere else.
 *
 * What this guards is the regression the N2c work nearly shipped: resolving in the page without
 * dividing GDK's factor back out made the surface land at four times its rectangle instead of two.
 * The assertion is relative and needs no knowledge of the layout — the same app twice, at ratio 1
 * and ratio 2, and the surface has to double, not quadruple and not stand still.
 */
import { execFileSync, spawn } from "node:child_process";
import console from "node:console";
import { mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { appEnv } from "../lib/env.js";
import {
  closeWindowTool,
  requireAppBinary,
  requireCloseWindowTool,
  requireDisplay,
  requireVideoFixture,
  videoFixture,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { allWindows, childWindows, rootTree } from "../lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 5;
let checksRun = 0;

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`scaled surface check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/**
 * By name, not by size: `findToplevel` looks for the configured 1024x700 and at ratio 2 the window
 * is twice that, which is the very thing under test.
 */
function toplevelByName() {
  const named = allWindows().filter((window) => window.name === "Sublore" && window.width > 200);
  if (named.length > 1) {
    throw new Error(`expected one "Sublore" toplevel, found ${named.length}\n${rootTree()}`);
  }
  return named.length === 1 ? named[0] : null;
}

/** The surface is the biggest child of the toplevel; mpv's own window lives inside it. */
function surfaceOf(toplevel) {
  return (
    childWindows(toplevel.id)
      .filter((child) => child.width > 50 && child.height > 50)
      .sort((a, b) => b.width * b.height - a.width * a.height)[0] ?? null
  );
}

/** Launch at one ratio, read the geometry, close. Returns the window and surface rectangles. */
async function measureAt(scale) {
  const dataHome = mkdtempSync(path.join(os.tmpdir(), `sublore-e2e-scale${scale}-`));
  const app = spawn(requireAppBinary(), [videoFixture], {
    detached: true,
    stdio: ["ignore", "ignore", "inherit"],
    env: appEnv({ XDG_DATA_HOME: dataHome, GDK_SCALE: String(scale) }),
  });
  const pgid = app.pid;
  let exit = null;
  app.on("exit", (code, signal) => {
    exit = { code, signal };
  });

  try {
    const toplevel = await waitFor(
      () => {
        if (exit !== null) {
          throw new Error(`the app exited before its window appeared (code ${exit.code})`);
        }
        return toplevelByName();
      },
      { timeout: 30000, message: `the "Sublore" toplevel at GDK_SCALE=${scale}` },
    );
    const surface = await waitFor(() => surfaceOf(toplevel), {
      timeout: 30000,
      message: `the native surface at GDK_SCALE=${scale}\n${rootTree()}`,
    });

    execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "ignore", timeout: 15000 });
    await waitFor(() => exit !== null, { timeout: 20000, message: "the app to exit" });
    const survivors = await waitFor(() => (processGroupMembers(pgid).length === 0 ? [] : null), {
      timeout: 10000,
      message: `process group ${pgid} to be empty`,
    }).catch(() => processGroupMembers(pgid));

    return { toplevel, surface, exit, survivors };
  } finally {
    try {
      if (processGroupMembers(pgid).length > 0) {
        killGroup(pgid);
      }
    } catch {
      // Teardown must not mask the failure that got us here.
    }
  }
}

/**
 * Three pixels of slack, derived rather than chosen. The page rounds each edge once per ratio, and
 * at ratio 2 the Linux backend divides the result by two and rounds again: `round(b*2) - round(t*2)`
 * is within one of `2h`, halving leaves half a pixel, and rounding that leaves one, which the
 * doubling turns into two — three once the ratio-1 rounding is counted too. The defect this check
 * exists to catch is off by a factor, not by three pixels: a surface that ignored the ratio would
 * miss by hundreds.
 */
function doubles(one, two) {
  return Math.abs(two - one * 2) <= 3;
}

async function main() {
  requireDisplay();
  requireAppBinary();
  requireCloseWindowTool();
  requireVideoFixture();

  const single = await measureAt(1);
  const double = await measureAt(2);

  console.log(
    `  ratio 1: toplevel ${single.toplevel.width}x${single.toplevel.height}, ` +
      `surface ${single.surface.width}x${single.surface.height}+${single.surface.relX}+${single.surface.relY}`,
  );
  console.log(
    `  ratio 2: toplevel ${double.toplevel.width}x${double.toplevel.height}, ` +
      `surface ${double.surface.width}x${double.surface.height}+${double.surface.relX}+${double.surface.relY}`,
  );

  check("the app came up at both ratios", single.surface !== null && double.surface !== null);

  // Without this the whole check could pass by comparing two identical runs, if GDK_SCALE were
  // ignored: the assertion below would then compare a rectangle with twice itself and fail, but it
  // would fail for a reason nobody could read.
  check(
    "GDK_SCALE reached the window: the toplevel doubled",
    doubles(single.toplevel.width, double.toplevel.width) &&
      doubles(single.toplevel.height, double.toplevel.height),
    `toplevel was ${single.toplevel.width}x${single.toplevel.height} at ratio 1 and ` +
      `${double.toplevel.width}x${double.toplevel.height} at ratio 2. If those are the same, the ` +
      `ratio never changed and this check proves nothing.`,
  );

  check(
    "the surface doubled in size with the ratio",
    doubles(single.surface.width, double.surface.width) &&
      doubles(single.surface.height, double.surface.height),
    `surface was ${single.surface.width}x${single.surface.height} then ` +
      `${double.surface.width}x${double.surface.height}. Unchanged means the page's rectangle ` +
      `reached X without being resolved to native pixels, which is BACKLOG N2c.`,
  );

  check(
    "the surface doubled in position with the ratio",
    doubles(single.surface.relX, double.surface.relX) &&
      doubles(single.surface.relY, double.surface.relY),
    `surface sat at ${single.surface.relX},${single.surface.relY} then ` +
      `${double.surface.relX},${double.surface.relY}`,
  );

  check(
    "both runs closed with status 0 and left nothing running",
    single.exit.code === 0 &&
      double.exit.code === 0 &&
      single.survivors.length === 0 &&
      double.survivors.length === 0,
    `exits ${JSON.stringify([single.exit, double.exit])}, survivors ` +
      `${JSON.stringify([single.survivors, double.survivors])}`,
  );

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `scaled surface guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`scaled surface check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
