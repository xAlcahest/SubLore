#!/usr/bin/env node
/**
 * A stand-in for whisper-cli, so the transcription spec can drive the real app end to end without
 * a whisper build, a 77 MB model or a network. See e2e/README.md and BACKLOG.md M3.4.
 *
 * Everything around it is real: the app spawns it exactly as it spawns whisper, ffmpeg really
 * extracts the audio from the fixture, and the JSON this writes is a byte-exact capture of a real
 * whisper run (fixtures/asr/whisper-tiny-en.json). Only the inference is skipped, which is the one
 * part that needs a model.
 *
 * It spawns nothing itself: whisper-cli has no children either, so the orphan check the spec runs
 * after a cancel is asking the same question of the same shape of process tree.
 */
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

/** Where the harness keeps the control file and where this writes what it did. */
const dir = process.env.SUBLORE_E2E_ASR_DIR;
if (dir === undefined || dir === "") {
  process.stderr.write("whisper-stub: SUBLORE_E2E_ASR_DIR is not set\n");
  process.exit(9);
}

/** How long a slow run keeps going before giving up, so a failed cancel cannot hang the suite. */
const SLOW_LIMIT_MS = 60000;
const STEP_MS = 200;

const argv = process.argv.slice(2);

function flag(name) {
  const index = argv.indexOf(name);
  return index < 0 ? null : (argv[index + 1] ?? null);
}

function progress(percent) {
  // The exact literal whisper's progress callback prints; crates/sublore-asr/src/progress.rs
  // parses this and nothing else.
  process.stderr.write(
    `whisper_print_progress_callback: progress = ${String(percent).padStart(3)}%\n`,
  );
}

// What the app asked for, kept so the spec can assert on the real command line: that the user's
// media is never handed to whisper, and that a CPU run really passes -ng.
writeFileSync(path.join(dir, "argv"), `${argv.join("\n")}\n`);
// Written before any work, so the spec can find this process and prove it is gone after a cancel.
writeFileSync(path.join(dir, "pid"), `${process.pid}\n`);

const stem = flag("-of");
if (stem === null) {
  process.stderr.write("whisper-stub: no -of on the command line\n");
  process.exit(9);
}
const mode = readFileSync(path.join(dir, "mode"), "utf8").trim();

if (mode === "fast") {
  for (const percent of [20, 40, 60, 80, 100]) {
    progress(percent);
    await sleep(20);
  }
  copyFileSync(path.join(dir, "transcript.json"), `${stem}.json`);
  process.exit(0);
}

if (mode === "slow") {
  const deadline = Date.now() + SLOW_LIMIT_MS;
  let percent = 0;
  while (Date.now() < deadline) {
    // Past 90 the line repeats, which keeps the sidecar's stall timer fed without the bar ever
    // reaching a value that would look like a finished run.
    percent = Math.min(percent + 5, 90);
    progress(percent);
    await sleep(STEP_MS);
  }
  // Only reachable when the cancel under test did not work; the spec has already failed by then.
  process.stderr.write("whisper-stub: never cancelled\n");
  process.exit(1);
}

process.stderr.write(`whisper-stub: unknown mode ${JSON.stringify(mode)}\n`);
process.exit(9);
