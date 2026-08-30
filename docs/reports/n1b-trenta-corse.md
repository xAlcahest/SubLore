# N1b — thirty runs of the close gate after the GTK dialogs, 2026-08-30

After the close gate's rfd dialogs were replaced with GTK dialogs created on the main thread, `pnpm e2e:close-gate` was run 30 times on Linux, under `xvfb-run`, in six batches of five sequential runs, each batch on its own X server (displays 401-405, 411-415, 421-425, 431-435, 441-445, 451-455). **One run died with SIGSEGV. The N1b acceptance criterion is not met.** The criterion asks for 30 runs with no SIGSEGV and no other non-zero exit; this battery produced one crash and four further failures, so it fails on both halves.

## The counts

| batch     | passed | SIGSEGV | setup | other |
| --------- | ------ | ------- | ----- | ----- |
| :401-:405 | 5      | 0       | 0     | 0     |
| :411-:415 | 4      | 0       | 0     | 1     |
| :421-:425 | 4      | 0       | 0     | 1     |
| :431-:435 | 4      | 0       | 0     | 1     |
| :441-:445 | 4      | 0       | 0     | 1     |
| :451-:455 | 4      | 1       | 0     | 0     |
| **total** | **25** | **1**   | **0** | **4** |

## The crash

Second run on display :452, on the save branch, the same check as before:

```
close gate check failed: save exited the app with status 0
exit was {"code":null,"signal":"SIGSEGV"}
```

Same branch and same failing check as the two occurrences in `n1b-segfault-uscita.md`. Whether it is the same stack is **unverified**: no core dump was inspected for this run, so the only evidence here is the exit signal. It is a crash on the way out after save, and nothing beyond that is established.

The rate moved from 1 in 12 (and 2 in roughly 17 across that afternoon) to 1 in 30. That is not evidence that it moved. If the old rate of about 1 in 11 were still exactly what it is, one crash or fewer in 30 runs would come up roughly a quarter of the time. The battery is not large enough to tell a real reduction from an ordinary run of luck, and the criterion is binary anyway: one SIGSEGV is a failure.

## Harness setup failures: zero

No run failed to reach the behaviour under test. Every one of the 30 launched the app, opened the fixture, dirtied the document and raised the dialog. The counts above are counts of the check's own verdicts, not of runs that never started.

## The four other failures

None of these is the defect under test, and none of them is explained. They are recorded here rather than waved off.

- **:413, "timed out after 15000ms waiting for the app to exit after discard"** and **:433, "timed out after 15000ms waiting for the app to exit after save"**. The answer landed and the dialog closed; the process had not exited 15 seconds later. Not a crash: the process was alive, not dead.
- **:421, "no process survived discard — survivors: 1243086"**. The app exited cleanly, and one member of its process group was still alive 10 seconds after that. A lingering child, not a signal.
- **:443, "timed out after 10000ms waiting for the dialog to close after save"**. The click was aimed at the Save button by estimated coordinates (`clickDialogButton` sizes slots by the runner's theme and says so in its own comment) and the dialog was still there 10 seconds later. The likeliest reading is that the click missed the button, which is a harness weakness, not app behaviour. Likeliest is not verified.

Three of the four are timeouts, and the six batches ran in parallel on one machine, six X servers and six app instances at once. Load is a plausible contributor to the two exit timeouts and to the survivor check. Plausible, again, is not measured: nothing in this experiment separates a slow machine from a slow shutdown.

That parallelism also means this was not 30 consecutive runs in the shape the acceptance criterion asks for. It was six independent sequences of five. For the crash the distinction does not matter, since one crash fails either shape; for the four timeouts it matters, and they should be re-run sequentially before anyone decides what they are.

## What this proves and what it does not

Proved, on Linux, under Xvfb: moving the close gate's dialogs onto the main thread did not eliminate the exit crash. The gate's behaviour itself is intact in all 30 runs that got that far: the dialog is raised on close, discard leaves the file untouched, save writes the file. The crash lands after the write, as before, so nothing observed here put user data at risk.

Not proved:

- That the rate changed at all. See above.
- That the remaining crash has the cause described in `n1b-segfault-uscita.md`. No backtrace was taken this time.
- Anything about Windows or macOS. This battery ran on Linux only. Windows compiles in CI and has never had this check run against it; a green compile is not a behavioural result. macOS is deferred and was not touched.
- That a real session is now free of rfd's second GTK thread. It is not: N1c is still open, and `project::choose_path` still goes through the plugin's blocking pickers, so opening a project folder arms the same condition for the rest of the session. This check never opens the picker, so this battery says nothing about that path.
