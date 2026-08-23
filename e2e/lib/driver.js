import { spawn } from "node:child_process";
import net from "node:net";
import process from "node:process";

import { waitFor } from "./proc.js";

/** tauri-driver is a dev tool installed with `cargo install tauri-driver --locked`, not a repo dep. */
const DRIVER = process.env.TAURI_DRIVER_PATH ?? "tauri-driver";

export const driverPort = Number(process.env.E2E_PORT ?? 4444);
const nativePort = driverPort + 1;

let child = null;
let exited = null;
let handlersInstalled = false;

function portFree(port) {
  return new Promise((resolve) => {
    const socket = net.connect({ host: "127.0.0.1", port });
    const done = (free) => {
      socket.destroy();
      resolve(free);
    };
    socket.once("connect", () => done(false));
    socket.once("error", () => done(true));
  });
}

function portListening(port) {
  return portFree(port).then((free) => !free);
}

/** WebKitWebDriver is tauri-driver's child; killing only the parent orphans it. */
export function stopDriver() {
  if (child === null) {
    return;
  }
  const pid = child.pid;
  child = null;
  try {
    process.kill(-pid, "SIGTERM");
  } catch {
    // Already dead.
  }
}

function installHandlers() {
  if (handlersInstalled) {
    return;
  }
  handlersInstalled = true;
  process.on("exit", stopDriver);
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.on(signal, () => {
      stopDriver();
      process.exit(1);
    });
  }
}

export async function startDriver() {
  installHandlers();
  await waitFor(() => portFree(driverPort), {
    timeout: 30000,
    message: `port ${driverPort} to be free for tauri-driver`,
  });

  exited = null;
  // Own process group so the WebKitWebDriver grandchild dies with it.
  child = spawn(
    DRIVER,
    [
      "--port",
      String(driverPort),
      "--native-port",
      String(nativePort),
      ...(process.env.WEBKIT_WEBDRIVER_PATH
        ? ["--native-driver", process.env.WEBKIT_WEBDRIVER_PATH]
        : []),
    ],
    { detached: true, stdio: ["ignore", "inherit", "inherit"] },
  );
  child.on("error", (error) => {
    exited = `tauri-driver failed to start: ${error.message}. Install it with \`cargo install tauri-driver --locked\`.`;
  });
  child.on("exit", (code, signal) => {
    exited = `tauri-driver exited early (code ${code}, signal ${signal})`;
  });

  await waitFor(
    () => {
      // A driver that died on its own is a hard failure, never something to keep polling through.
      if (exited !== null) {
        throw new Error(exited);
      }
      return portListening(driverPort);
    },
    { timeout: 30000, message: `tauri-driver to listen on 127.0.0.1:${driverPort}` },
  );
}
