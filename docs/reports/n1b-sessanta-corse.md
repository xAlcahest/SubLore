# N1b — sixty sequential runs, one branch at a time — 2026-08-30

The owner's reading grid was fixed before the numbers existed: if the crash appears only on the save branch, that difference is where to look; if it appears on both, the save-specific suspicion dies and `destroy()` comes back into it; if it never appears in sixty, the real rate is lower than feared and N1b is downgraded to a known defect with a revised closing criterion, without spending an attempt blind.

**It never appeared. The third branch applies.**

## What was run

A probe, not the check: `e2e/scripts/n1b-load-probe.js` launches the app with a subtitle passed on the command line, dirties the first cue, asks for the close, answers with one button, and records what happened. It asserts nothing. Sixty runs, **sequentially**, one app at a time, no parallel load, alternating save and discard so that any drift in the machine over the half hour would fall on both branches rather than on one.

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

**Correction, gate 2 (2026-08-30):** unlike the sequential battery above, this table carries no "reached the end" column. The probe's only signal for how far a run got is `phase`, and every catch in it is silent (`e2e/scripts/n1b-load-probe.js:118-119`), so a run that missed the dialog button under six-way X11 focus contention and timed out at `phase: "answer"` or `"exit"` would print zero SIGSEGV and zero core dump — indistinguishable here from a run that reached `"done"` and simply did not crash. Nothing committed to the repository can recompute that column for this battery: the code that ran the probe sixty times across six concurrent streams and collected these numbers was never committed (no script or `package.json` entry does it), so the raw per-run JSON this table was built from does not exist in the tree to re-check. The "2 in 30 on save, 0 in 30 on discard" split, and the "load is a condition of the defect, save is not special" conclusion built on it below, stand as measured but with this gap on the record rather than silently assumed closed. The later judgment table below, added the same day once the fix landed, does carry a "done" figure for both its batteries and is not affected by this gap.

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

## The cure, and the battery that judged it — later the same day

The second attempt was authorised against the measured hypothesis rather than against a guess. One shape was available. Hiding the dialog instead of destroying it would have been the cleaner sequencing, but `allWindows()` reads the whole window tree and a hidden window stays in it under its own name, so `close-gate-check.js` would still find it and go red — and no assertion gets weakened to make a fix look good. The dialog keeps being destroyed.

That left the other end of the path. `close_window` called `window.destroy()`, which tears down the GTK window directly and skips the close sequence `tao` runs for a window carrying a webview. That is the one difference between the path that crashes and the ordinary close that `shutdown-check.js` exercises constantly without ever crashing. Save is not special: it lengthens the window in which the thing goes wrong, and load lengthens it further.

The change is `window.close()` instead, with the gate's answer recorded in a `CLOSING` flag so the close request it raises is allowed through without asking again — no second dialog, no loop. `asr` and the video surface are shut down in `CloseRequested`, where every other close already does it, rather than in a private order of this path's own.

A self-check of the diff then found the flag was never cleared. It does not bite today, because the app exits straight after, but a flag that says "this close is already decided" and stays standing would let a later close skip the gate in silence — the exact shape of the loss the gate exists to prevent. It is now consumed by the handler that reads it, so it waves through one request and never a second. That change touches the gate path, so the judge was run again on the binary being delivered rather than on the one it was written against.

| judge                                             | first binary                     | delivered binary                 |
| ------------------------------------------------- | -------------------------------- | -------------------------------- |
| 60 save-branch probe runs, six concurrent streams | 0 SIGSEGV, 0 core dumps, 60 done | 0 SIGSEGV, 0 core dumps, 60 done |
| sequential `pnpm e2e:close-gate`                  | 30 green, 0 red                  | 3 green, 0 red                   |
| `pnpm e2e:shutdown`, same close path              | 5/5                              | 5/5                              |

At the rate measured before the change — two in thirty on this exact battery — an unfixed defect survives sixty runs about one time in sixty. That is the strength of this evidence and it is worth stating plainly rather than calling the defect dead: **the crash did not occur in the battery built to make it occur**, on Linux, under Xvfb. Nothing here says anything about Windows or macOS.

## Correction, 2026-08-30, from gate 2's closure audit

This report named the probe `n1b-branch-probe.mjs`, which is what it was called in a scratch
directory while the batteries ran. **No file of that name has ever existed in the repository**: the
script was committed as `e2e/scripts/n1b-load-probe.js`, and the name here has been corrected to it.

The orchestration around it — sixty runs, six concurrent streams, one display number each, a CSV of
outcome, exit status, signal and core presence — was a shell loop typed at the terminal and never
committed. So the numbers in this report are reproducible only by rebuilding that loop from the
description above. The probe itself is committed and is what N1b's closing criterion names; the
driver is not, and saying so is the difference between a reproducible result and a remembered one.
