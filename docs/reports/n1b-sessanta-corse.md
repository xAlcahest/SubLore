# N1b — sixty sequential runs, one branch at a time — 2026-08-30

The owner's reading grid was fixed before the numbers existed: if the crash appears only on the save branch, that difference is where to look; if it appears on both, the save-specific suspicion dies and `destroy()` comes back into it; if it never appears in sixty, the real rate is lower than feared and N1b is downgraded to a known defect with a revised closing criterion, without spending an attempt blind.

**It never appeared. The third branch applies.**

## What was run

A probe, not the check: `n1b-branch-probe.mjs` launches the app with a subtitle passed on the command line, dirties the first cue, asks for the close, answers with one button, and records what happened. It asserts nothing. Sixty runs, **sequentially**, one app at a time, no parallel load, alternating save and discard so that any drift in the machine over the half hour would fall on both branches rather than on one.

| branch  | runs | reached the end | non-zero exit | signal | core dump |
| ------- | ---- | --------------- | ------------- | ------ | --------- |
| save    | 30   | 30              | 0             | 0      | 0         |
| discard | 30   | 30              | 0             | 0      | 0         |

Every run reached the `done` phase, which means the dialog was raised, the button was answered, and the process exited on its own with status 0. `coredumpctl` was queried for each app's pid after it exited; none produced a core.

## What sixty clean runs do and do not establish

They bound the rate rather than prove its absence. Zero in sixty puts the per-run probability under roughly 5% with 95% confidence, and no lower.

That bound is still informative, because there is something to compare it against. Under the rfd binary, sequential runs of `close-gate-check.js` produced two crashes in about seventeen runs. If that rate were unchanged, zero in sixty would be about a one-in-two-thousand outcome. So the sequential rate did fall, and the earlier statement in `n1b-trenta-corse.md` that nothing showed the rate had moved was made against the parallel battery, which is a different condition and remains true of that one.

Two things must not be read into this:

- **The probe is not the check.** `close-gate-check.js` runs cancel and discard against one app instance and then starts a second instance for save; every crash yet seen landed on that second instance. The probe uses one instance per run with a single dialog, structurally like the check's save phase but not identical to a full check run. Sixty clean probe runs are not sixty clean check runs.
- **This says nothing about Windows or macOS.** It ran on Linux, under Xvfb.

## The isolating battery: same probe, six streams at once

Two variables separated the sequential battery above from the parallel one in `n1b-trenta-corse.md` — the load and the harness — so neither could be blamed. The same probe was run again, sixty times, in six concurrent streams. **Load is the only thing that changed.**

| branch  | runs | SIGSEGV | core dump |
| ------- | ---- | ------- | --------- |
| save    | 30   | **2**   | 2         |
| discard | 30   | 0       | 0         |

Both crashes carry the same stack as every one before them — `_gdk_x11_display_queue_events` inside `gtk_main_iteration_do` — with one GTK thread in the process and no `rfd` frame anywhere.

Two findings, and they are compatible rather than competing.

**Load is a condition of the defect.** Sixty sequential runs produced nothing; sixty runs of the same probe against the same binary, six at a time, produced two crashes. Nothing else differs between the batteries.

**Under load, the defect is save-specific.** Two in thirty on the save branch against zero in thirty on discard, in the same battery, interleaved through the same streams. This is the first branch of the owner's reading grid, reached from the third: the crash did not appear at all until the machine was busy, and when it appeared it appeared only where the file gets written.

Those two together say what the next attempt should aim at. Save and discard take the identical path out — the same dialog, the same `close_window`, the same `window.destroy()` — and differ in one thing: save writes a file and a backup on the worker thread before asking for the close. That work delays the destroy relative to the dialog's own GTK teardown, and load lengthens the delay. The suspicion is no longer "save is special"; it is that save is slow, and that the race is lost when the destroy arrives late.

## Revised closing criterion

The old criterion — thirty consecutive clean runs — is now known to be unable to fail: the defect does not reproduce sequentially at all. It is replaced by a battery that can:

- **Sixty save-branch runs of the probe under six-way parallel load, with zero SIGSEGV and zero core dumps.** At the rate measured here, two in thirty, an unfixed defect survives that battery about one time in sixty.
- The existing `close-gate-check.js` stays armed and unchanged, and thirty sequential runs of it stay clean. It is not the instrument that can prove a fix, but it is the one that guards the behaviour.

For CI this matters more than the sequential number suggested: the runner is small and busy, which is the condition under which this reproduces.
