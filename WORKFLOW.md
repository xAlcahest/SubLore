# WORKFLOW.md — How Sublore builds itself

This file defines how agent sessions run. CLAUDE.md defines what may be built; BACKLOG.md defines what to build next; this file defines how. Together they let the owner launch work and return only to verify.

## 1. Roles

- **Orchestrator (Claude Fable):** reads BACKLOG.md, picks the next task, decomposes it, spawns implementers, reviews their output, merges or rejects, updates BACKLOG.md, writes the session report. The orchestrator never writes feature code.
- **Implementer (Claude Opus):** executes exactly one task at a time on its own branch, following CLAUDE.md in full. Writes code and tests, runs the suite, fixes findings from `/review`.
- **Owner (human):** launches milestones, runs the milestone acceptance checklist in the app, answers BLOCKED reports. Nothing else is his job.

## 2. The task loop (every task, no exceptions)

1. Read CLAUDE.md and the task's entry in BACKLOG.md. If the task has no acceptance criteria, STOP — the task is not ready; report it instead of guessing.
2. Write the behavioral tests from the acceptance criteria first. They must fail before implementation.
3. Implement the minimum that makes them pass. One task = one branch = one PR.
4. Run the full test suite, not just the new tests.
5. Run `/review`; fix findings or state explicitly why a finding is acknowledged and unfixed.
6. Self-check against CLAUDE.md §6 checklist and §3 data-safety rules.
7. PR description: what changed, why, and human verification steps written for a non-coder ("open file X, click Y, expect Z").
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

- Max task size: one PR reviewable in one sitting. If a task grows past that, split it in BACKLOG.md before continuing.
- Never "improve" things outside the task while passing by. File a new BACKLOG entry instead.
- The orchestrator re-reads acceptance criteria before accepting any PR: tests passing is necessary, criteria met is the standard.
- Any test weakened, skipped, or deleted must be named in the PR description with the reason. Silent test changes are grounds for rejection.

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
