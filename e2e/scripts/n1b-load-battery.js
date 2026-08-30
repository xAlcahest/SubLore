/**
 * The battery N1b's closing criterion is written in terms of, so the criterion is reproducible.
 *
 * BACKLOG N1b asks for sixty save-branch runs in six concurrent streams with no SIGSEGV and no core
 * dump. Load is the condition under test: the crash does not reproduce sequentially at all — sixty
 * sequential runs produced nothing — and under six concurrent streams it reproduced at 2 in 30 on
 * the save branch and 0 in 30 on discard (`docs/reports/n1b-sessanta-corse.md`).
 *
 * That orchestration was a shell loop typed at a terminal and never committed, which the gate 2
 * closure audit found and was right to: a criterion nobody can re-run is a story about a
 * measurement, not a measurement. This is that loop, in the repository.
 *
 *   pnpm e2e:n1b-battery              # 60 save runs, six streams — the criterion
 *   pnpm e2e:n1b-battery discard 30 3 # any branch, count and streams, for a comparison
 *
 * Each stream gets its own display numbers and never reuses one: a second `xvfb-run -n` on a number
 * just used gets no server, and the app then exits before its window exists, which reads as a crash
 * and is not one (WORKFLOW.md 4c).
 *
 * Writes a CSV beside the summary so a run can be argued with afterwards rather than believed.
 */
import { spawn } from "node:child_process";
import console from "node:console";
import { writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { repoRoot, requireAppBinary, requireDisplay } from "../lib/paths.js";

const branch = process.argv[2] ?? "save";
const total = Number(process.argv[3] ?? 60);
const streams = Number(process.argv[4] ?? 6);

if (branch !== "save" && branch !== "discard") {
  console.error("usage: n1b-load-battery.js [save|discard] [runs] [streams]");
  process.exit(2);
}
if (!Number.isInteger(total) || total < 1 || !Number.isInteger(streams) || streams < 1) {
  console.error("runs and streams must be whole numbers greater than zero");
  process.exit(2);
}

const probe = path.join(repoRoot, "e2e", "scripts", "n1b-load-probe.js");
/** Distinct per stream and per run, so no two `xvfb-run` invocations ever share a number. */
const DISPLAY_BASE = 500;

/** One probe run under its own X server. Resolves to the probe's record, never rejects. */
function once(display) {
  return new Promise((resolve) => {
    const child = spawn(
      "xvfb-run",
      ["-n", String(display), "-s", "-screen 0 1024x700x24", "node", probe, branch],
      { cwd: repoRoot, stdio: ["ignore", "pipe", "ignore"] },
    );
    let out = "";
    child.stdout.on("data", (chunk) => {
      out += chunk;
    });
    child.on("error", (error) => {
      resolve({ answer: branch, phase: `spawn failed: ${error.message}`, display });
    });
    child.on("close", () => {
      const line = out.trim().split("\n").pop() ?? "";
      try {
        resolve({ ...JSON.parse(line), display });
      } catch {
        // A probe that printed nothing parseable is a failed run, not a missing one: recording it
        // as such is the difference between sixty runs and fifty-nine plus a shrug.
        resolve({ answer: branch, phase: "no record printed", display, raw: line });
      }
    });
  });
}

/** True when `coredumpctl` knows about this pid, which is how a crash is told from a bad exit. */
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
    const display = DISPLAY_BASE + index * 100 + i;
    const record = await once(display);
    record.core = await hadCore(record.pid);
    records.push(record);
    process.stdout.write(record.signal === null || record.signal === undefined ? "." : "X");
  }
  return records;
}

async function main() {
  requireDisplay();
  requireAppBinary();

  const per = Math.ceil(total / streams);
  console.log(
    `${total} ${branch} runs in ${streams} concurrent streams, ${per} each, one display per run.`,
  );

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

  const crashed = records.filter((r) => r.signal !== null && r.signal !== undefined);
  const cored = records.filter((r) => r.core);
  const incomplete = records.filter((r) => r.phase !== "done");
  const killed = records.filter((r) => r.killedRunning);

  const csv = [
    "display,branch,phase,exit,signal,killedRunning,pid,core",
    ...records.map((r) =>
      [r.display, r.answer, r.phase, r.exit, r.signal, r.killedRunning, r.pid, r.core].join(","),
    ),
  ].join("\n");
  const out = path.join(os.tmpdir(), `n1b-battery-${branch}-${records.length}.csv`);
  writeFileSync(out, `${csv}\n`);

  console.log(`  runs        ${records.length} in ${minutes.toFixed(1)} min`);
  console.log(`  SIGSEGV     ${crashed.length}`);
  console.log(`  core dumps  ${cored.length}`);
  console.log(
    `  not "done"  ${incomplete.length}${incomplete.length ? ` (${[...new Set(incomplete.map((r) => r.phase))].join("; ")})` : ""}`,
  );
  console.log(`  cut short   ${killed.length} (teardown had to kill a live process group)`);
  console.log(`  records     ${out}`);

  // The criterion, stated where it is checked. A run that never reached the close path is not a
  // clean run: it is a run that proved nothing, and counting it as a pass is how a battery lies.
  const met = crashed.length === 0 && cored.length === 0 && incomplete.length === 0;
  console.log(
    met
      ? `\nBACKLOG N1b's criterion is met by this battery: ${records.length} runs, no crash, no core, every run reached the end.`
      : `\nBACKLOG N1b's criterion is NOT met by this battery.`,
  );
  process.exit(met ? 0 : 1);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
