import { Buffer } from "node:buffer";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, closeSync, copyFileSync, existsSync, mkdirSync, openSync } from "node:fs";
import { readdirSync, readFileSync, readSync, writeFileSync, writeSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { repoRoot } from "./paths.js";
import { requireLinuxBackend } from "./platform.js";

/**
 * The transcription spec's fixtures: a stand-in sidecar, the real model the app insists on, and the
 * process checks the cancellation criterion needs. See e2e/README.md and BACKLOG.md M3.4.
 *
 * Nothing here writes into the repository. Everything lands under SUBLORE_E2E_DATA_HOME, which
 * e2e/wdio.conf.js points at a fresh temp directory for every run.
 */

/** The app's identifier from src-tauri/tauri.conf.json: what app_data_dir() is named after. */
const APP_IDENTIFIER = "com.sublore.app";

/**
 * ggml-tiny.en.bin's row in crates/sublore-asr/src/model/catalog.rs. The app hashes a model file
 * before every run, so the harness cannot stand one in: it installs a copy of the real file and
 * checks it here, where a damaged cache fails with its own message rather than the app's.
 */
const TINY_EN = {
  file: "ggml-tiny.en.bin",
  sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
};

/** The byte the damaged-model spec flips. Deep in the tensor payload, and it never changes length. */
const DAMAGE_OFFSET = 40000000;

/**
 * Everything below is a function rather than a constant: e2e/wdio.conf.js creates the temp data
 * home in its own module body, which runs after this module has been imported.
 */
export function asrDir() {
  return path.join(dataHome(), "asr-stub");
}

/** Where SUBLORE_WHISPER_BIN points. A copy, so the run never needs to chmod a repository file. */
export function stubBinary() {
  return path.join(asrDir(), "whisper-stub.mjs");
}

function dataHome() {
  const dir = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dir !== "string" || dir === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  return dir;
}

/**
 * `app_data_dir()` as Tauri resolves it on Linux: $XDG_DATA_HOME/<identifier>. That equality is
 * what lets `e2e/wdio.conf.js` point a run at a throwaway data home, and it does not hold on
 * Windows, where the dir comes from a known folder id and no environment variable moves it. Seam
 * for MW.1b, which needs its own way to redirect the app's data home before it can assert on it.
 */
export function appDataDir() {
  requireLinuxBackend(
    "asr.js appDataDir",
    "resolve the app data dir a run redirected the app to, and redirect it in the first place",
  );
  return path.join(dataHome(), APP_IDENTIFIER);
}

/** The model the app runs against, inside the run's own data directory. */
function modelPath() {
  return path.join(appDataDir(), "models", TINY_EN.file);
}

/** Where scripts/fetch-model.sh leaves the model, which is also where the Rust suite caches it. */
function sourceModel() {
  const dir = process.env.SUBLORE_TEST_MODEL_DIR;
  if (typeof dir === "string" && dir !== "") {
    return path.join(dir, TINY_EN.file);
  }
  return path.join(cacheHome(), "sublore", "models", TINY_EN.file);
}

function cacheHome() {
  const xdg = process.env.XDG_CACHE_HOME;
  if (typeof xdg === "string" && xdg !== "") {
    return xdg;
  }
  const home = process.env.HOME;
  if (typeof home !== "string" || home === "") {
    throw new Error(
      "neither XDG_CACHE_HOME nor HOME is set, so the model cannot be found: point " +
        "SUBLORE_TEST_MODEL_DIR at the directory holding it",
    );
  }
  return path.join(home, ".cache");
}

/** A copy, never a link: the damaged-model spec writes to it and the developer's cache must not. */
function installModel() {
  const source = sourceModel();
  const fetch = "run `sh scripts/fetch-model.sh`, or point SUBLORE_TEST_MODEL_DIR at your own copy";
  if (!existsSync(source)) {
    throw new Error(`no ${TINY_EN.file} at ${source}: ${fetch}`);
  }
  const digest = createHash("sha256").update(readFileSync(source)).digest("hex");
  if (digest !== TINY_EN.sha256) {
    throw new Error(`${source} hashes to ${digest}, not ${TINY_EN.sha256}: ${fetch}`);
  }
  mkdirSync(path.join(appDataDir(), "models"), { recursive: true });
  copyFileSync(source, modelPath());
}

/**
 * Flip one bit in the model the app is about to run, and hand back the undo. The file keeps its
 * catalogued length, so only its checksum can tell: that is the case asr.spec.js drives.
 */
export function damageModel() {
  const original = Buffer.alloc(1);
  const file = openSync(modelPath(), "r+");
  try {
    if (readSync(file, original, 0, 1, DAMAGE_OFFSET) !== 1) {
      throw new Error(`${modelPath()} has no byte at ${DAMAGE_OFFSET}`);
    }
    writeSync(file, Buffer.from([original[0] ^ 0x01]), 0, 1, DAMAGE_OFFSET);
  } finally {
    closeSync(file);
  }
  return () => {
    const file = openSync(modelPath(), "r+");
    try {
      writeSync(file, original, 0, 1, DAMAGE_OFFSET);
    } finally {
      closeSync(file);
    }
  };
}

/**
 * Lay down the stand-in sidecar, the transcript it replays, and the model. Called from the launcher
 * before the first session, so the app finds all of it at startup.
 */
export function installStubSidecar() {
  // The app spawns SUBLORE_WHISPER_BIN directly, so the stub runs from its shebang and its mode
  // bit. Windows has neither and needs a shim it can execute. Seam for MW.1b.
  requireLinuxBackend(
    "asr.js installStubSidecar",
    "install a stand-in sidecar the app can spawn as SUBLORE_WHISPER_BIN, wrapping whisper-stub.mjs",
  );
  mkdirSync(asrDir(), { recursive: true });
  const stub = stubBinary();
  copyFileSync(path.join(repoRoot, "e2e", "tools", "whisper-stub.mjs"), stub);
  chmodSync(stub, 0o755);

  // A byte-exact capture of a real whisper run, committed as text. The stub copies it where
  // whisper would have written its own, so everything downstream parses genuine whisper output.
  copyFileSync(
    path.join(repoRoot, "fixtures", "asr", "whisper-tiny-en.json"),
    path.join(asrDir(), "transcript.json"),
  );
  setStubMode("fast");

  installModel();
}

/** "fast" finishes at once; "slow" keeps reporting progress until it is killed. */
export function setStubMode(mode) {
  writeFileSync(path.join(asrDir(), "mode"), `${mode}\n`);
}

/** Forget the last run's traces, so a stale file cannot be read as this run's evidence. */
export function forgetStubRun() {
  for (const name of ["pid", "argv"]) {
    writeFileSync(path.join(asrDir(), name), "");
  }
}

/** The process id the stub sidecar wrote when it started, or null before it has started. */
export function stubPid() {
  const text = readFileSync(path.join(asrDir(), "pid"), "utf8").trim();
  return text === "" ? null : Number(text);
}

/** The command line the app gave the sidecar, one argument per entry. */
export function stubArgv() {
  return readFileSync(path.join(asrDir(), "argv"), "utf8")
    .split("\n")
    .filter((line) => line !== "");
}

/**
 * `ps` output for one pid, or null when nothing is there. A killed child that was never waited for
 * is still a process here, in state `Z`, which is exactly the orphan the acceptance criterion is
 * about.
 */
export function processLine(pid) {
  // A zombie is a POSIX state and it is the state this check exists to catch. Seam for MW.1b.
  requireLinuxBackend(
    "asr.js processLine",
    "say whether one pid is still a process, and distinguish a reaped child from an unreaped one",
  );
  try {
    const out = execFileSync("ps", ["-o", "pid=,stat=,comm=", "-p", String(pid)], {
      encoding: "utf8",
      timeout: 10000,
    });
    return out.trim() === "" ? null : out.trim();
  } catch (error) {
    // ps exits 1 when the pid does not exist; anything else is a real failure.
    if (error.status === 1) {
      return null;
    }
    throw error;
  }
}

/** Run directories still sitting in the app's scratch space. Empty after every run, cancel or not. */
export function scratchRuns() {
  try {
    return readdirSync(path.join(appDataDir(), "scratch")).filter((name) =>
      name.startsWith("asr-"),
    );
  } catch {
    return [];
  }
}
