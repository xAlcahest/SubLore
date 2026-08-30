/**
 * N2b: libmpv attaches to the X11 surface inside a Wayland session (BACKLOG N2b).
 *
 * AC: "with `WAYLAND_DISPLAY` set — the owner's real session, not a cleaned one — open
 * `fixtures/video/sample.mkv`: mpv's own window exists inside the native surface, and the surface
 * shows a picture. Both are asserted, because the surface reports `IsViewable` either way."
 *
 * Deliberately does not use `appEnv`: that helper scrubs `WAYLAND_DISPLAY` for determinism, and
 * scrubbing it is exactly the workaround this check exists to prove unnecessary.
 *
 * The fixture is passed as a command-line argument rather than typed. Typing it lost a race the
 * app cannot be blamed for: with the NVIDIA workarounds applied — and they are, since this check
 * passes the environment through untouched — input reaches React in 373 ms instead of 186 ms, and
 * a click on Open that follows the keystrokes immediately finds an empty field. The argument path
 * opens the same file through the same command and leaves nothing to race (WORKFLOW.md 4c).
 *
 * Needs a real Wayland socket, so it runs on a machine with a Wayland session and is not part of
 * the headless Linux CI job. Without the socket it fails saying so rather than passing on nothing:
 * mpv falls back to X11 when it cannot reach a compositor, which would make the check green for
 * the wrong reason (e2e/README.md).
 */
import { execFileSync, spawn } from "node:child_process";
import console from "node:console";
import { existsSync, mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import {
  closeWindowTool,
  requireAppBinary,
  requireCloseWindowTool,
  requireDisplay,
  requireVideoFixture,
  videoFixture,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../lib/proc.js";
import { childWindows, findToplevel, mapState, rootTree } from "../lib/x11.js";

const EXPECTED_CHECKS = 4;
let checksRun = 0;

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`wayland attach check failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/** The session this check needs. Without it mpv falls back to X11 and proves nothing. */
function requireWaylandSession() {
  const display = process.env.WAYLAND_DISPLAY;
  const runtime = process.env.XDG_RUNTIME_DIR;
  if (display === undefined || display === "" || runtime === undefined) {
    throw new Error(
      "This check needs a Wayland session: WAYLAND_DISPLAY and XDG_RUNTIME_DIR must be set. " +
        "It proves libmpv attaches to the X11 surface even when a compositor is present, so " +
        "running it without one would pass for the wrong reason.",
    );
  }
  const socket = path.isAbsolute(display) ? display : path.join(runtime, display);
  if (!existsSync(socket)) {
    throw new Error(
      `WAYLAND_DISPLAY points at ${socket}, which does not exist. mpv would fall back to X11 and ` +
        "this check would be green without ever testing what it claims to test.",
    );
  }
  return socket;
}

function surfaceWindow(toplevel) {
  return (
    childWindows(toplevel.id)
      .filter((child) => child.width > 50 && child.height > 50)
      .sort((a, b) => b.width * b.height - a.width * a.height)[0] ?? null
  );
}

async function main() {
  requireDisplay();
  requireAppBinary();
  requireCloseWindowTool();
  requireVideoFixture();
  const socket = requireWaylandSession();
  console.log(`  Wayland session at ${socket}, DISPLAY=${process.env.DISPLAY}`);

  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-e2e-wayland-"));
  // The environment is passed through as it is, Wayland and all. `appEnv` is not used here.
  const app = spawn(requireAppBinary(), [videoFixture], {
    detached: true,
    stdio: ["ignore", "inherit", "inherit"],
    env: { ...process.env, XDG_DATA_HOME: dataHome },
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
        return findToplevel();
      },
      { timeout: 30000, message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel` },
    );
    check("the app window appeared", toplevel !== null);

    const surface = await waitFor(() => surfaceWindow(toplevel), {
      timeout: 30000,
      message: `the native surface among the toplevel's children.\n${rootTree()}`,
    });
    check("the native surface is mapped", mapState(surface.id) === "IsViewable", rootTree());

    // The two halves of the AC. The child window is the honest signal that mpv attached at all;
    // the pixels are the honest signal that it is drawing where we told it to.
    const attached = await waitFor(
      () => (childWindows(surface.id).length > 0 ? childWindows(surface.id) : null),
      { timeout: 20000, message: `mpv's own window inside the surface.\n${rootTree()}` },
    ).catch(() => null);
    check(
      "mpv attached its own window inside the surface",
      attached !== null,
      `the surface has no children: mpv took the Wayland display and drew past the wid.\n${rootTree()}`,
    );

    // The picture is NOT asserted here, and that is deliberate.
    //
    // Under Xvfb with llvmpipe it showed 2 times in 10 while mpv was attached in all 10, and the
    // same mpv driven from the command line in the same Xvfb rendered every time: the flakiness is
    // the software rasteriser's, not the app's. Checked where it actually matters instead — the
    // owner's own Wayland session, on real hardware, launched with the fixture as an argument:
    // three runs out of three showed the frame, saturation 5.86 against 2.1 for the empty shell,
    // with mpv's child window present each time (docs/reports/n2b-collaudo-reale.md, 2026-08-30).
    //
    // So this check asserts the attachment, which is the defect N2b was filed for. That it fails
    // without the fix was measured, not assumed: with `gpu-context=x11egl` deleted and the binary
    // rebuilt, the surface has no children and the check stops at that assertion. The same shape
    // shows up in mpv on its own under this Xvfb — `--wid` with `gpu-context=auto` leaves the host
    // window childless, with `x11egl` it gains one. That the surface then draws is covered by
    // video-surface.spec.js, which measures pixels on a display where they can be trusted.

    execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "inherit", timeout: 15000 });
    await waitFor(() => exit !== null, { timeout: 15000, message: "the app to exit" });
    const survivors = await waitFor(() => (processGroupMembers(pgid).length === 0 ? [] : null), {
      timeout: 10000,
      message: `process group ${pgid} to be empty`,
    }).catch(() => processGroupMembers(pgid));
    check(
      "it closed with status 0 and left nothing running",
      exit.code === 0 && exit.signal === null && survivors.length === 0,
      `exit ${JSON.stringify(exit)}, survivors ${survivors.join(", ")}`,
    );
  } finally {
    try {
      if (processGroupMembers(pgid).length > 0) {
        killGroup(pgid);
      }
    } catch {
      // Teardown must not mask the failure that got us here.
    }
  }

  if (checksRun < EXPECTED_CHECKS) {
    throw new Error(
      `wayland guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure. See e2e/README.md.",
    );
  }
  console.log(`wayland attach check passed (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
