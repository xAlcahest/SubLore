/**
 * M0.3: an artifact somebody installed starts, and puts the app's own window on screen.
 *
 * The binary is an argument rather than `paths.js`'s `appBinary`, because that one is the build
 * tree's and this check is about what came out of the bundler: /usr/bin/sublore from the .deb, or
 * the AppImage. Everything else is the E2E harness's — the same launch environment, the same
 * window inspection, the same process-group teardown — so MW.1b's Windows backend reaches this
 * check through the same seams as the rest of the suite.
 *
 * What it does not cover: M0.2's playback, and a clean close. Those are the owner checklist's
 * (M0.3 status note) and the e2e job's respectively.
 */
import { spawn } from "node:child_process";
import console from "node:console";
import { accessSync, constants, mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { appEnv } from "../../e2e/lib/env.js";
import { requireDisplay, windowHeight, windowTitle, windowWidth } from "../../e2e/lib/paths.js";
import { killGroup, processGroupMembers, waitFor } from "../../e2e/lib/proc.js";
import { findToplevel, findWindowsWithAppGeometry, mapState, rootTree } from "../../e2e/lib/x11.js";

/** Gutting an assertion has to be as red as failing one, so the checks count themselves. */
const EXPECTED_CHECKS = 3;
let checksRun = 0;

function check(label, ok, detail = "") {
  checksRun += 1;
  if (!ok) {
    throw new Error(`packaging smoke failed: ${label}${detail === "" ? "" : `\n${detail}`}`);
  }
  console.log(`  ok  ${label}`);
}

/** Every window the app's size, whatever it is called: a wrong title fails on the name. */
function sameGeometryWindows() {
  return findWindowsWithAppGeometry()
    .map((window) => `${window.id} ${JSON.stringify(window.name)}`)
    .join(", ");
}

async function main() {
  const binary = process.argv[2];
  if (binary === undefined || binary === "") {
    throw new Error("usage: package-smoke.mjs <path to an installed sublore binary>");
  }
  requireDisplay();

  // The one runnable file the artifact exists to deliver. A package that stops installing it, or
  // installs it without the execute bit, says so here instead of through a spawn error.
  let installed = true;
  try {
    accessSync(binary, constants.X_OK);
  } catch {
    // Reported by the check below, which names the path.
    installed = false;
  }
  check(`${binary} is installed and executable`, installed);

  // Own process group and its own data home: the teardown is then the exact set of processes this
  // run created, and the app starts from nothing rather than from an earlier run's state.
  const dataHome = mkdtempSync(path.join(os.tmpdir(), "sublore-package-smoke-"));
  const app = spawn(binary, [], {
    detached: true,
    stdio: ["ignore", "inherit", "inherit"],
    env: appEnv({ XDG_DATA_HOME: dataHome }),
  });
  const pgid = app.pid;

  let exit = null;
  let spawnError = null;
  app.on("error", (error) => {
    spawnError = error;
  });
  app.on("exit", (code, signal) => {
    exit = { code, signal };
  });

  try {
    const toplevel = await waitFor(
      () => {
        if (spawnError !== null) {
          throw new Error(`the installed app failed to start: ${spawnError.message}`);
        }
        if (exit !== null) {
          throw new Error(
            `the installed app exited before its window appeared (code ${exit.code}, ` +
              `signal ${exit.signal}). A dependency the package does not declare looks like this.`,
          );
        }
        return findToplevel();
      },
      { timeout: 60000, message: `the ${windowWidth}x${windowHeight} "${windowTitle}" toplevel` },
    ).catch((error) => {
      throw new Error(
        `${error.message}\nwindows of the app's size on this display: ${sameGeometryWindows()}\n` +
          rootTree(),
      );
    });
    check(`the installed app opened its "${windowTitle}" window`, toplevel !== null);

    // Created is not shown: an unmapped toplevel is a window nobody can see.
    const state = mapState(toplevel.id);
    check("that window is mapped on screen", state === "IsViewable", `map state was ${state}`);
  } finally {
    // Never leak processes on a failure path, but never signal a pgid the kernel may already have
    // recycled: a group that still has members cannot have had its id reused.
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
      `packaging smoke guard: expected ${EXPECTED_CHECKS} checks, only ${checksRun} ran. ` +
        "Removing an assertion here is a CI failure.",
    );
  }
  console.log(`packaging smoke passed for ${binary} (${checksRun}/${EXPECTED_CHECKS} checks)`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
