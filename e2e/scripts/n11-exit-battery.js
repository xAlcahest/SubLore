/**
 * The battery N11's closing criterion is written in terms of, so the criterion is reproducible.
 *
 * BACKLOG N11 is a SIGSEGV on the way out that CI had seen three times and no check could put a
 * number on. It does not reproduce on an idle machine at all; under concurrent load it reproduces
 * on roughly one launch in three, which is enough to measure a change against.
 *
 *   pnpm e2e:n11-battery        # 25 runs, five streams
 *   pnpm e2e:n11-battery 50 10  # count and streams, for a comparison
 *
 * Each stream gets its own display numbers and never reuses one: a second `xvfb-run -n` on a number
 * just used gets no server, and the app then exits before its window exists, which reads as a crash
 * and is not one.
 *
 * Writes a CSV beside the summary so a run can be argued with afterwards rather than believed.
 */
import { spawn } from "node:child_process";
import console from "node:console";
import { writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import {
  repoRoot,
  requireAppBinary,
  requireDisplay,
  requireTool,
  requireVideoFixture,
} from "../lib/paths.js";

const total = Number(process.argv[2] ?? 25);
const streams = Number(process.argv[3] ?? 5);

if (!Number.isInteger(total) || total < 1 || !Number.isInteger(streams) || streams < 1) {
  console.error("usage: n11-exit-battery.js [runs] [streams]");
  process.exit(2);
}

const probe = path.join(repoRoot, "e2e", "scripts", "n11-exit-probe.js");
/** Distinct per stream and per run, so no two `xvfb-run` invocations ever share a number. */
const DISPLAY_BASE = 600;

/** One probe run under its own X server. Resolves to the probe's record, never rejects. */
function once(display) {
  return new Promise((resolve) => {
    const child = spawn(
      "xvfb-run",
      ["-n", String(display), "-s", "-screen 0 1024x700x24", "node", probe],
      { cwd: repoRoot, stdio: ["ignore", "pipe", "ignore"] },
    );
    let out = "";
    child.stdout.on("data", (chunk) => {
      out += chunk;
    });
    child.on("error", (error) => {
      resolve({ phase: `spawn failed: ${error.message}`, display });
    });
    child.on("close", () => {
      const line = out.trim().split("\n").pop() ?? "";
      try {
        resolve({ ...JSON.parse(line), display });
      } catch {
        resolve({ phase: "no record printed", display });
      }
    });
  });
}

/**
 * True when `coredumpctl` knows about this pid. Corroboration only, and it errs both ways:
 * systemd-coredump records a crash after the fact, so a run asked about too soon answers false for
 * a crash that did happen, and pids are reused, so a clean run can inherit a number an earlier
 * crash left behind. The number N11 is counted by is the signal below, which the exit carries.
 */
function hadCore(pid) {
  return new Promise((resolve) => {
    if (pid === undefined || pid === null) {
      resolve(false);
      return;
    }
    const child = spawn("coredumpctl", ["--no-pager", "info", String(pid)], { stdio: "ignore" });
    child.on("error", () => resolve(false));
    child.on("close", (code) => resolve(code === 0));
  });
}

async function stream(index, runs) {
  const records = [];
  for (let i = 0; i < runs; i += 1) {
    const record = await once(DISPLAY_BASE + index * 100 + i);
    record.core = await hadCore(record.pid);
    records.push(record);
    process.stdout.write(record.signal === null || record.signal === undefined ? "." : "X");
  }
  return records;
}

async function main() {
  requireDisplay();
  requireAppBinary();
  requireVideoFixture();
  requireTool("coredumpctl", "tell a crash from a bad exit status, which is the whole of N11");

  const per = Math.ceil(total / streams);
  console.log(`${total} runs in ${streams} concurrent streams, ${per} each, one display per run.`);

  const started = process.hrtime.bigint();
  const records = (
    await Promise.all(
      Array.from({ length: streams }, (_, index) =>
        stream(index, index === streams - 1 ? total - per * (streams - 1) : per),
      ),
    )
  ).flat();
  process.stdout.write("\n");
  const minutes = Number(process.hrtime.bigint() - started) / 60e9;

  // A run this script had to SIGKILL was cut off rather than observed, so it is not a crash and
  // not a clean exit; only a run that reached "done" and still carried a signal is N11.
  const killed = records.filter((r) => r.killedRunning);
  const crashed = records.filter(
    (r) => r.phase === "done" && r.signal !== null && r.signal !== undefined,
  );
  const cored = records.filter((r) => r.core);
  const incomplete = records.filter((r) => r.phase !== "done");

  const csv = [
    "display,phase,exit,signal,killedRunning,pid,core",
    ...records.map((r) =>
      [r.display, r.phase, r.exit, r.signal, r.killedRunning, r.pid, r.core].join(","),
    ),
  ].join("\n");
  const out = path.join(os.tmpdir(), `n11-battery-${records.length}.csv`);
  writeFileSync(out, `${csv}\n`);

  console.log(
    `${records.length} runs in ${minutes.toFixed(1)} min: ${crashed.length} closed on a signal ` +
      `after reaching "done", which is N11. ${cored.length} were still known to coredumpctl when ` +
      `asked, ${killed.length} were still running at teardown, ${incomplete.length} did not reach ` +
      `"done".`,
  );
  console.log(`per-run records: ${out}`);
}

await main();
