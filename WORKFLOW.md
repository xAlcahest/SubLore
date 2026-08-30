# WORKFLOW.md — How Sublore builds itself

This file defines how agent sessions run. CLAUDE.md defines what may be built; BACKLOG.md defines what to build next; this file defines how. Together they let the owner launch work and return only to verify.

## 1. Roles

- **Orchestrator (Claude Fable):** reads BACKLOG.md, picks the next task, decomposes it, spawns implementers, reviews their output, merges or rejects, updates BACKLOG.md, writes the session report. The orchestrator never writes feature code.
- **Implementer (Claude Opus):** executes exactly one task at a time on its own branch, following CLAUDE.md in full. Writes code and tests, runs the suite, fixes findings from `/review`.
- **Owner (human):** launches milestones, runs the milestone acceptance checklist in the app, answers BLOCKED reports. Nothing else is his job.

## 2. The task loop (every task, no exceptions)

1. Read CLAUDE.md and the task's entry in BACKLOG.md. If the task has no acceptance criteria, STOP — the task is not ready; report it instead of guessing.
2. Write the behavioral tests from the acceptance criteria first. They must fail before implementation.
3. Implement the minimum that makes them pass. One task = one branch = one delivery.
4. Run the full test suite, not just the new tests.
5. **Self-check the diff** (owner ruling 2026-08-30, replacing the per-delivery review). Read your own diff line by line against CLAUDE.md §3 and §6: data-loss paths, errors that reach the user instead of a log, no `unwrap` outside tests, comments that carry measurements rather than intentions. Then the test-side pass: every assertion can fail for a cause the test builds, no assertion on a constant, no threshold or number in a comment that was not actually measured. A delegated lens is not run per task any more; it runs at the gate.
5b. **No assertion on a constant.** `expect(x).toBe(true)` where `x` can only be `true`, and any check whose condition the test itself guarantees, is banned: it inflates the count that exists to catch removed assertions. Every check must be able to fail for a cause the test constructs. Three separate reviews in this repo have found the same defect.
6. Self-check against CLAUDE.md §6 checklist and §3 data-safety rules.
7. Delivery description: what changed, why, and human verification steps written for a non-coder ("open file X, click Y, expect Z").
8. Mark the task done in BACKLOG.md with its verification status: `verified-by-tests` or `needs-human-e2e`.

## 3. Autonomy contract

Proceed without asking whenever the work stays inside the task's written scope. STOP and write a BLOCKED report instead of proceeding when any of these appears:

- The task needs something outside its scope, outside milestone scope, or outside CLAUDE.md §1.
- A new dependency, a database schema change, or a change to a public interface (IPC, module-loading API, file formats).
- Anything that touches CLAUDE.md §3 (data safety) in a way the rules don't explicitly permit.
- A performance budget (§7) would regress.
- Two serious implementation attempts have failed. Do not try a third silently.

A BLOCKED report states: the task, what was attempted, why it stopped, and 1–3 options with a recommendation. Blocking early is success, not failure: a wrong guess pursued for hours costs more than any pause.

## 4. Drift control

- Max task size: one delivery reviewable in one sitting. If a task grows past that, split it in BACKLOG.md before continuing.
- Never "improve" things outside the task while passing by. File a new BACKLOG entry instead.
- The orchestrator re-reads acceptance criteria before accepting any delivery: tests passing is necessary, criteria met is the standard.
- Any test weakened, skipped, or deleted must be named in the delivery description with the reason. Silent test changes are grounds for rejection.

## 4a. Gates (owner ruling 2026-08-30)

Reviews no longer run per delivery. They run in batches, at gates, and a gate is the only thing that stops new code.

**Between gates:** each task closes with its own behavioural tests, a green full battery, and the self-check of §2.5. Then it merges into main and the next task starts. Speed comes from here.

**At a gate:** new code stops. One large multi-lens review workflow runs over every delivery since the previous gate, diff by diff, in the shape of the Aegisub scan — many lenses in parallel, each with its own hunt list, each writing its report to a file and then terminating. The standing lenses are: what the corrections themselves broke, data-loss paths, assertions that cannot fail, and platform claims that were only checked on one machine. The orchestrator adds a lens for every point it declares suspect, and saying "nothing looks suspect" is not an available answer.

**Every finding is fixed before the gate opens.** Not triaged, not deferred with a note: fixed, or explicitly ruled on by the owner. The gate opening is what lets new code start again.

**The gates:**

1. Now: N2, and everything merged since N1.
2. After N2b and decision 1, immediately before the owner's manual checklist. **Register, owner ruling 2026-08-30:** every delivery merged under the gate regime from N2b onwards went in without a dedicated review, by choice of regime, so this gate's lenses cover all of them — the NVIDIA webview mitigation, the native GTK dialogs, the close path fix, and N2c when it lands. One lens is named in advance: **the close path and the single-use `CLOSING` flag**, which is code adjacent to data safety and deserves eyes that are not its author's.
3. The end of every M2.x milestone.
4. Before any merge that touches saving, subtitle formats, or the open-core boundary — whatever the regime, whatever the schedule. These three stay watched because a defect there costs the user's work, their file, or the licence line.

**The pipeline never fully stops.** A gate freezes merges of new code and nothing else: documentation, the M2.0 preparation, task decomposition and planning all keep running through it.

## 4b. Delegates (owner ruling 2026-08-29)

The repo is local and private. There is no collaboration platform and no pull requests: a task produces a **delivery** — a branch plus a description saying what changed, why, and how to verify it by using the app in steps a non-coder can follow — and integration is a **local merge** into main, allowed only once the delegated review has passed and the full battery is green.

**Every delegated agent writes its report to a file, then stops.** The brief says to terminate once the file is written; a delegate that has written its file and keeps talking is stopped and treated as finished, and nothing after the file is read. One review agent repeated its whole summary four times instead of ending.

**Every delegated agent writes its report to a file, and the caller reads that file.** The brief must name a path under `docs/reports/` and require the report to be written there before the agent finishes. The caller never treats the closing message as the report. An agent whose report file is missing or empty has failed, whatever its closing message says.

This rule is paid for. Two delegations in one session returned "Concluso." as their entire result: two research agents lost their findings outright, and a review carrying three blockers was nearly recorded as "the agent produced nothing" while its report sat alive in a transcript nobody had opened.

**A review's own fixes get reviewed.** Corrections written under review pressure are new code, and the next pass hunts explicitly for what they broke. The second N1 review found a blocker created by a fix from the first one.

**Gate reviews are always delegated, and start from `docs/reviews/review-prompt.md`.** The implementer reading their own diff is the per-task self-check of §2.5, and it never satisfies the gate: the gate exists because the author's blind spots survive their own rereading. The template is there because the review it came from found, in code its author had just declared clean, a save path that could never succeed, a behavioural test whose assertion counter guarded three assertions that asserted nothing, and an acceptance criterion no automation ran.

## 4c. Driving the app (owner ruling 2026-08-30)

- **Synthetic input belongs in an isolated server.** `xdotool` typing, clicks and key presses are allowed only inside an X server the harness owns and started itself (Xvfb). They are never used on the owner's real display: on a live compositor keystrokes go to whichever window holds the focus, and during the N2b check they landed in the owner's own window and typed a fixture path into it.
- **On the real display, three things are allowed:** launching the app, passing it files as command-line arguments, and capturing its own window. Nothing else. `startup_files` in `src-tauri/src/lib.rs` exists so that a real-session check can reach a loaded document without touching the keyboard.
- **Capture the window, not the screen.** Under rootless XWayland `x11grab` on the root window reads black whatever the app draws; `import -window <id>` reads the window directly and needs no raise and no focus.
- **A discrimination experiment proves the rebuild happened before it measures.** When a test is meant to fail without a fix, removing the fix is only half the experiment: check the build's exit status explicitly and say so, never chain it behind a silent `&&`. On 2026-08-30 a failed build inside `pnpm e2e:build >/dev/null 2>&1 && echo ok` printed nothing, the check ran against the previous binary, and the experiment reported the exact opposite of the truth. An experiment that never ran is the worst defect available, because it arrives dressed as certainty.
- **The E2E binary is built last.** `cargo test` and `cargo clippy --all-targets` rebuild `src-tauri/target/debug/sublore` as a plain cargo debug binary, which looks for the Vite dev server instead of the embedded assets, and `pnpm build` regenerates `dist` under a binary already built. Everything that compiles Rust or the frontend runs before `pnpm e2e:build`, never after. Twice now this ordering has made a green suite look red and cost a debugging session.

## 5. Parallelism

Independent tasks may run as parallel implementers (agent teams per the parallel-build skill). Rules:

- Tasks running in parallel must not touch the same files. The orchestrator checks file ownership before spawning.
- Shared interfaces are frozen before parallel work starts; changing them mid-flight requires stopping the affected implementers.
- Merge order is decided by the orchestrator; conflicts are resolved by re-running the loser's task loop, never by hand-patching the merge.

## 6. Milestone checkpoints (the owner's only job)

- Work proceeds autonomously within a milestone. A milestone is DONE only when the owner has run its acceptance checklist in the built app on Windows or Linux and said "pass".
- No release tag, version bump, or public announcement without an owner pass.
- If the owner fails a checklist item, that failure becomes a task with the failing step as its acceptance criterion, and the milestone reopens.

## 7. Session report

Every session ends with a report in plain language: tasks completed with verification status, tasks blocked and why, what the owner needs to do next (usually: nothing, or one checklist). Never report unverified work as done — CLAUDE.md §9 applies to reports above all.
