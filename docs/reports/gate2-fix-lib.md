# Gate 2, Wave 3 — `src-tauri/src/lib.rs` and `src/hooks/useStartupFiles.ts`

Cluster: both blockers of the gate, two serious rows and nine minor ones, all sited in the two files
above. `GATE_BASE=f0b0058`, `GATE_HEAD=eca9806`. Everything below was run on Linux under Xvfb.
Nothing here is a Windows claim.

Files I changed: `src-tauri/src/lib.rs`, `src/hooks/useStartupFiles.ts`. Files I created:
`e2e/scripts/close-gate-late-edit-check.js`, `e2e/scripts/startup-args-check.js`. Nothing else.

---

## The work I inherited, and what I did with it

The brief said the tree carried an unfinished argv fix and that the second blocker was untouched.
That was stale: the working tree already held the whole `CLOSING` rewrite, a `Mutex<Option<Gate>>`
replacing the two atomics, a `decide_close` function with six unit tests, and a
`SUBLORE_CLOSE_ANSWER_DELAY_MS` hook naming a harness file that did not exist. I read it against the
lens reports rather than against its own comments.

**Kept, because it is right:** `args_os`/`OsString` with the dropped arguments carried out and
logged; the gate as one piece of state instead of two flags; `decide_close` as a pure function
taking the state as an argument, which is what makes the state machine testable at all; the
debug-only stall hook, modelled on `crash::force`; the `NoDocument` arm; the note on `window.close()`
being preventable; `openOne` in the hook.

**Corrected:**

- `Gate.decided: Option<Decided>` conflated _the dialog is on screen_ with _the answer is being
  acted on_. That is exactly the split L5's row `lib.rs:192` asks for, and it was not made. Replaced
  with `Phase::{Asking, Acting, Acted(SessionAfter)}`, set at the three points that actually move.
- `CloseAction::Wait` logged `"a close decision is still in flight"` for all three of its causes,
  including the one where another window owns the gate. A log line that is the only diagnostic a
  user gets for a dead X button has to be true. It is now `Wait(Held)` and says which case it is.
- `ask_before_closing`'s docstring said "the answer arrives on the main thread". `dialog.rs:77`
  spawns a thread for it. Rewritten to what the code does.
- The `NoDocument` classification sat inline in `save_open_file`, where nothing can test it. Pulled
  out as `after_save`, which is pure and has two tests.
- `useStartupFiles`'s `invoke` catch was silent everywhere, which is the pattern L8's finding 2
  names. It logs now.

**Added:** the two behavioural checks below, two discrimination experiments, a test that fails if
the frontend ever registers a `tauri://close-requested` listener, and seven more unit tests.

---

## Blockers

### `src-tauri/src/lib.rs:75` — a non-UTF-8 argv entry panics before any window exists — **fixed**

`std::env::args_os()`, and each argument is classified as an `OsStr`. An argument the IPC payload
cannot carry costs that argument and is named in the log, never the launch. The type is now part of
the guard: `startup_files` takes `Iterator<Item = OsString>`, so putting `std::env::args()` back at
the call site does not compile.

**Proved by:** `tests::an_argument_that_is_not_unicode_costs_that_argument_and_is_named` (unit), and
`e2e/scripts/startup-args-check.js`, 4/4, which launches the real binary through `sh` so that one
argument reaches `execve` as the raw byte `0xE9`, then asserts the window comes up, the app closes
cleanly, the dropped argument is named in the log, and the valid subtitle beside it was still taken.

**Discrimination run.** Reverting only the call site to
`startup_files(std::env::args().map(OsString::from))`, rebuilding (exit status checked explicitly,
WORKFLOW §4c), and running the check:

```
BUILD_RC=0
thread 'main' panicked at library/std/src/env.rs:871:51:
called `Result::unwrap()` on an `Err` value: "/tmp/sublore-e2e-argv-sLpyKT/s\xE9rie.srt"
the app exited (code 101) before its window appeared.
ARGV_RC_WITHOUT_FIX=1
```

Note what this experiment also shows: the unit test alone would **not** have caught it, because it
calls `startup_files` directly. The behavioural check is what covers the call site.

### `src-tauri/src/lib.rs:138` — an edit committed while the gate's save is in flight is closed away — **fixed**

`unsaved_work` is now read on every `CloseRequested`, an answered one included, and `decide_close`
decides from that plus the gate's phase. An answer marks the session's state at the moment it was
acted on; if the session is dirty again when the close arrives, that is work committed after the
answer and the gate asks again (`CloseAction::AskAgain`) instead of closing over it.

The hang that `CLOSING` was introduced to prevent does not come back, and it is worth saying which
property does that. The old flag was consumed by the arm that read it, so one answer waved through
exactly one close. The gate now does the same thing in the `Acted` arm (`*gate = None`), so a second
close after an answered one asks from scratch. What it no longer does is trust that flag as proof
that the session is clean.

`SessionAfter::Unproven` is the one case where a dirty session at the close does not ask again: a
discard whose `close_session` failed. That failure is only reachable on a poisoned lock, and
`subtitle::lock` refuses a poisoned lock to every editing command, so no newer edit can exist. The
dirty session there is the work the user chose to lose one dialog earlier.

**Proved by:** seven unit tests over `decide_close`, and `e2e/scripts/close-gate-late-edit-check.js`,
8/8, which drives the real app: edit a cue, close, click Save, commit a second edit while the save
is still in flight, and require the app to ask again and the second edit to reach the disk. The
interval is a race in production, so the app holds it open on request through
`SUBLORE_CLOSE_ANSWER_DELAY_MS` (debug builds only, like `SUBLORE_FORCE_PANIC`). That the hold
actually armed is check 2 of the script: without it the run would prove nothing while looking green.

**Discrimination run.** Removing only the `AskAgain` arm, so the answered close skips the dirty check
the way `CLOSING` did:

```
BUILD_RC=0
  ok  the app was still running when the second edit was committed
  ok  the app really held its answer open across that edit
late-edit gate check failed: the edit committed after the answer was asked about instead of closed away
LATE_EDIT_RC_WITHOUT_FIX=1
```

and the unit test with it: `left: Close, right: AskAgain`. The two setup checks passing before the
failure is the part that matters: the interval was really entered, and the edit was really made
inside it.

---

## Serious

### `src-tauri/src/lib.rs:192` — the gate outlives the dialog it documents — **fixed for the part this file owns, with a remainder named below**

The state now has three phases and the code sets each one where it happens: `Asking` when the gate
goes up, `Acting` at the top of the answer callback (the dialog destroys itself before that callback
runs, so this is the first instant with nothing on screen), `Acted` on the main thread immediately
before `window.close()`. A close request held during any of them is logged with which one held it.
The docstring's claimed lifetime is now the code's lifetime.

**Proved by:** `an_answer_being_acted_on_holds_the_window_and_says_which_case_it_is` and
`a_gate_still_on_screen_holds_the_window_without_raising_a_second_one`.

**Not fixed, and I am not going to pretend otherwise:** an answer worker that blocks forever still
holds the window shut. `save_current` takes the blocking session lock deliberately, and if another
command never releases it the gate stays in `Acting` for the life of the process. I did not add a
timeout, because every automatic release is worse than the wedge: letting the close through discards
the work the gate exists to protect, and raising a second dialog puts two saves on the same session.
Bounding the worker is `dialog.rs:77`'s row (`Builder::spawn`, and a `catch_unwind` around
`deliver`), which is another implementer's file.

**One correction to L5 and L2 while I am here.** Both reason that a panic inside the worker leaves
`GATE_OPEN` true for the life of the process and the window permanently unclosable. That path is not
reachable in this tree: `crash::on_panic` (`src-tauri/src/crash/mod.rs:99`) ends in
`std::process::exit(101)`, so a panicking worker takes the process down rather than leaving a wedged
window. The blocking-lock half of the scenario is real; the panic half is not. This is why I did not
add a drop guard: it would have been dead code.

### `src-tauri/src/lib.rs:197` — process-global flags against a per-window handler — **fixed**

`Gate` carries the label it was raised for, and `decide_close` compares it. An answer for one window
is never an answer for another; a close request for a window that does not own the gate is held and
logged as such, not waved through.

**Proved by:** `one_windows_answer_never_closes_another_window`.

---

## Minor

| row                                                                    | state | what changed, and what proves it                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------------------------------------------------------------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lib.rs:194` single-use flag has no check that can fail                | fixed | `an_answer_waves_through_one_close_and_never_a_later_one`. Still not reachable behaviourally in a single-window app, which exits on the close it waved through; the unit test is the check, and that is stated rather than papered over.                                                                                                                                                                                                                             |
| `lib.rs:234` unreachable "dialog could not be raised" recovery         | fixed | The comment claimed a refusal that cannot happen. It now says the branch is unreachable from this caller because `ask_close` posts to the main thread it is already on and that runs inline. The branch stays: `ask_close` returns a `Result` and discarding it would be worse than handling it. No test, because there is no reachable behaviour to pin. The other half of L1's fix, making `dialog.rs:41-43`'s silent `None` parent reportable, is in `dialog.rs`. |
| `lib.rs:258` "save failed" for a document that was closed              | fixed | `after_save` classifies `NoDocument` as nothing to save and closes; every other error still keeps the window and shows why. `a_document_closed_under_the_gate_is_nothing_to_save_and_not_a_failed_save`, `a_write_that_failed_keeps_the_window_and_carries_its_reason`.                                                                                                                                                                                              |
| `lib.rs:304` `window.close()` is preventable where `destroy()` was not | fixed | The constraint is recorded at the call. It also has a check that can fail: `the_frontend_registers_no_close_requested_listener` walks `src/` and fails if any file registers one. It is a source-invariant test, which is unusual here, and it is the only thing in the repo that can catch the trap being stepped on.                                                                                                                                               |
| `lib.rs:307` `report_error` has a main-thread caller its doc denies    | fixed | `report_close_failure`'s doc now says plainly that it is reached from inside `close_window`'s main-thread closure and that a post from the main thread runs inline. The mirror-image false doc at `dialog.rs:123-126` is not my file.                                                                                                                                                                                                                                |
| `lib.rs:43` no dropped argv element is logged                          | fixed | `StartupArgs.ignored` is carried out of `startup_files` and logged in `setup`, which is the first point a logger exists. Unit tests plus both argv behavioural checks, which read the log file after exit.                                                                                                                                                                                                                                                           |
| `lib.rs:45` a real file whose name starts with `-` is dropped unread   | fixed | `is_file()` decides; the dash only suppresses the _report_ for things that were never files. `a_file_whose_name_starts_with_a_dash_is_still_the_file_the_user_named`, and behaviourally in `e2e/scripts/argv-startup-check.js`.                                                                                                                                                                                                                                      |
| `useStartupFiles.ts:34` the catch's comment mischaracterizes it        | fixed | Comment corrected, and the branch now logs instead of returning in silence. **No test:** the repo has no frontend unit-test runner, and adding one is a new dependency, which WORKFLOW §3 says to stop for rather than decide alone.                                                                                                                                                                                                                                 |
| `useStartupFiles.ts:42` no local catch around the two opens            | fixed | `openOne` wraps each call and reports. Same missing proof, same reason. Both are latent contract gaps today: neither callback can reject, as L8 established.                                                                                                                                                                                                                                                                                                         |

---

## Battery, run on Linux under Xvfb

Static, after the last edit:

```
fmt: clean          clippy: exit 0          cargo test --workspace: 529 passed, 0 failed
eslint: clean       prettier --check .: clean
```

Behavioural, on a binary built by `pnpm e2e:build` after everything that compiles Rust or the
frontend (WORKFLOW §4c), with the build's exit status checked explicitly at `0`:

```
wdio:            8 passed, 8 total
shutdown:        5/5 checks
close gate:      12/12 checks
late-edit gate:  8/8 checks    (new, mine)
startup args:    4/4 checks    (new, mine)
argv startup:    3/3 checks    (not mine; ran it green here)
```

Baseline for comparison is `gate2-battery-baseline.md`: 502 tests and the same behavioural set. The
27 extra unit tests are mine and the parallel implementers'.

Mid-run, `cargo test` twice showed two failures in `dialog::tests`, a module that does not exist at
`HEAD` and was being written in `src-tauri/src/dialog.rs` by the parallel implementer while I ran.
They were green by my last run. Recorded because a red suite in a transcript should never be left
unexplained.

---

## Noticed, out of scope, for the orchestrator

1. **Neither of my two scripts is wired into `package.json` or CI.** Both are outside my file list.
   Unwired, they are checks no job runs, which is the same defect the register files as
   `package.json:19` and `.github/workflows/ci.yml:196`. Suggested entries:
   `"e2e:close-gate-late-edit": "node e2e/scripts/close-gate-late-edit-check.js"` and
   `"e2e:startup-args": "node e2e/scripts/startup-args-check.js"`. `e2e/README.md`'s table does not
   list them either.
2. **Two scripts now cover the same argv rows.** `e2e/scripts/argv-startup-check.js` appeared in the
   tree during my run, from a parallel implementer, covering rows 75, 43 and 45. Mine covers row 75
   with two assertions theirs does not have (the app came up, as a counted check rather than a
   thrown timeout; and the valid subtitle beside the bad argument was still the one taken) and
   leaves the dash and missing-path rows to unit tests. They should be one file. I kept mine because
   the blocker is my row and the discrimination experiment above was run against it; I did not
   delete a file outside my list.
3. **A stray untracked file named `--help` sits at the repo root** (`?? --help` in `git status`). It
   is not the app's doing, which never writes outside what the user opened; it looks like a shell
   redirection accident from a harness run. It must not be committed.
4. **An answered close that arrives while the session lock is momentarily busy reads `Unknown`,
   which counts as dirty, and asks a second time about a document the user just saved or discarded.**
   That is the safe direction and I left it: a needless question costs a click, a skipped one costs
   the work. Recorded so nobody reads it later as a bug.
5. `dialog.rs`'s silent `None` parent (`:41-43`) and its own false threading doc (`:74-77`,
   `:123-126`) are the other halves of my rows 234 and 307 and belong to that file's owner.
