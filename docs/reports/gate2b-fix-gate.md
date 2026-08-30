# Gate 2, wave 4 fixes — the close gate and the command line

Files owned this round: `src-tauri/src/lib.rs`, `src/hooks/useStartupFiles.ts`. The behavioural check
that covers the command line, `e2e/scripts/startup-args-check.js`, was extended to assert the new
line; nothing else was touched.

**Platform: everything below was run on Linux.** No Windows claim is made anywhere in this report.

---

## 1. BLOCKER — the close gate deadlocked itself. Fixed.

**What was wrong.** `lib.rs:176` read `match decide_close(&mut gate(), &label, dirty)`. The
`MutexGuard` was a match-scrutinee temporary, so `GATE` stayed locked for every arm, including the
two that call `ask_before_closing`. That function re-enters the same lock on the same thread on two
paths: `clear_gate()` when `dialog::ask_close` returns `Err`, and `mark_acting` when the OS refuses
the answer thread and `Delivery::drop` delivers Cancel inline. `std::sync::Mutex` is not reentrant.

**What changed.**

- `decide_close_now(label, session)` now owns the locking. It takes the guard, decides, and drops it
  before returning an owned `CloseAction`. The handler calls that and never names `gate()` itself, so
  there is no guard in scope for any arm to hold.
- The shutdown calls in the `Close` arm (`asr::shutdown`, `shutdown_video`) no longer run with the
  gate held either. Same cause, same fix.

**What proves it.**

- `the_close_handler_takes_no_gate_guard_of_its_own` reads this file's own source, slices the
  `CloseRequested` handler out of it, and fails if the handler contains a bare `gate()` call or stops
  going through `decide_close_now`. Discrimination experiment: with the handler put back to
  `match decide_close(&mut gate(), &label, session)` the test fails
  (`test result: FAILED. 58 passed; 2 failed`, the second failure being another implementer's
  `video::player::tests::every_x11_wid_gets_the_pinned_context`, see the note at the end). Restored,
  it passes.
- `a_close_decision_leaves_the_gate_unlocked_for_the_arm_that_acts_on_it` drives the real static: it
  takes a decision through `decide_close_now`, asserts `GATE.try_lock()` succeeds afterwards, then
  makes the same re-entrant calls the `Ask` arm makes (`mark_acting`, `clear_gate`) and asserts the
  gate state they leave. A guard that outlived the decision fails the `try_lock` rather than hanging
  the suite.
- The deadlock itself was reproduced first-hand before the fix, as a standalone program with the same
  shape (guard in the scrutinee, `clear_gate()` in the arm): under `timeout 5` it exited 124.
- The close path was then run end to end with the fix in: `pnpm e2e:close-gate` 12/12 on display 95
  and `pnpm e2e:close-gate-late-edit` 8/8 on display 96, one display number each, never reused.

**A note on what is not covered.** Both triggers are error paths (a refused thread, a failed post to
the main thread), and neither is reachable from a behavioural run. The source test is what stands
guard over the shape; the runtime test is what stands guard over the functions.

## 2. `lib.rs:76` — a second file of the same kind was dropped in silence. Fixed.

**What changed.** `startup_files` no longer uses `get_or_insert_with`, which does nothing at all when
the slot is full. The slot and its kind are picked first, and a full slot pushes
`"<arg> (a <kind> was already named)"` onto `ignored`, which `setup` already logs. `sublore ep01.srt
ep02.srt` now opens the first and says out loud what it did with the second.

**What proves it.**

- `a_second_file_of_the_same_kind_is_named_rather_than_dropped` passes two subtitles and two videos
  and asserts both the files taken and the exact two `ignored` lines. Discrimination: with
  `get_or_insert_with` put back, the test fails.
- `e2e/scripts/startup-args-check.js` now passes a second real subtitle in its second launch and
  asserts the log line, so the workaround its header used to describe is an assertion instead. The
  check counts 7 checks where it counted 6.
- Live, on this machine: `pnpm e2e:startup-args` 7/7 on display 93, and again 7/7 on display 98 after
  the final rebuild. The app's own log carries
  `[WARN] command line: ignored ep02.srt (a subtitle was already named)`.
- Discrimination on the behavioural check itself, run properly: `startup_files` reverted,
  `pnpm e2e:build` run with its exit status printed (`build(no fix) exit=0`, so the binary really was
  rebuilt), then the check on display 94, which failed on exactly the new assertion and nothing else
  (`startup args check failed: a second subtitle is named in the log rather than dropped in silence`).
  Restored, rebuilt (`build(final) exit=0`), green again.

## 3. `lib.rs:175` — `Unknown` and `Dirty` were the same `bool`. Fixed as far as it is safe to fix.

**What changed.** `unsaved_work(app) -> bool` is now `session_now(app) -> SessionState`, and
`decide_close` takes the three-valued state instead of a `bool`. The suite can now express the case
the arm acts on.

**What did not change, on purpose.** `Unknown` still counts as unsaved everywhere `Dirty` does, the
answered gate included, so the second dialog the finding describes is still possible. This is a
ruling, not an oversight, and the reason is that the alternatives are worse:

- Closing on `Unknown` after an answer reopens N1. `session_state` returns `Unknown` precisely when a
  subtitle command holds the session lock, which is the instant an edit is being committed. That is
  the late-edit case, not a benign coincidence.
- Holding the close instead of asking (a `Wait`) wedges the window forever when the lock is poisoned,
  since `try_lock` keeps failing. One extra click is cheaper than an unclosable window.

The cost stands as the finding described it: a click, in a race, never data. If the owner would
rather pay differently, the arm to change is `Acted(Clean)` plus `Unknown`, and it is now one line.

**What proves it.** `a_session_that_cannot_be_read_is_asked_about_like_a_dirty_one` asserts `Ask` on a
fresh gate and `AskAgain` on an answered one, both from `SessionState::Unknown`. Before this change
no test could state that case at all; a change of the ruling now has to change a test that says so.

## 4. `useStartupFiles.ts:63` — reported. NOT fixed.

**State: not fixed.** A file named on the command line that fails to open still reports only to
`console.error`, which a release WebKitGTK webview gives the user no way to read.

**Why.** Making it an actionable message needs somewhere to render it: the hook would have to hand
the failure to its caller, `src/App.tsx` would have to render it, and the copy would have to go in
`src/i18n/en.ts` because §9 forbids hardcoded user-facing strings. Neither of those files is mine
this round, and a value returned by the hook that nobody renders would be dead code carrying the same
silence. There is also no frontend test framework in this repo (no vitest, no `*.test.*` under
`src/`), so the fix has no unit-level way to fail either; it would need a wdio spec.

**What I did do in the file.** The comments no longer claim the silence was ended. The `invoke` catch
says plainly that it reaches the log and no further and that the user sees an app that opened
nothing; `openOne`'s doc says the guard exists against an unhandled rejection, that both callbacks
wired today put their own failure on screen and resolve, and that a rejection which does reach the
catch goes to the log alone. That is honesty (§9), not a fix, and it is counted as neither.

**What the fix would be**, for whoever owns those files next: `useStartupFiles` returns the failed
path (or takes a reporter), `App.tsx` renders it in the existing `app__error` alert region, and the
sentence lives in `en.ts`.

---

## Battery

Ordered per WORKFLOW §4c: everything that compiles Rust or the frontend ran before `pnpm e2e:build`,
and no `cargo` command ran after it.

| What                                        | Result                                                          |
| ------------------------------------------- | --------------------------------------------------------------- |
| `rustfmt --check src-tauri/src/lib.rs`      | clean                                                           |
| `cargo clippy --all-targets`                | no warnings, no errors                                          |
| `npx tsc --noEmit`                          | clean                                                           |
| `npx eslint` on both changed JS/TS files    | clean                                                           |
| `npx prettier --check` on both              | clean                                                           |
| `cargo test --lib`                          | 59 passed, 1 failed, and the failure is not in my files (below) |
| `pnpm e2e:build`                            | exit 0, printed and checked                                     |
| `pnpm e2e:close-gate`, display 95           | 12/12                                                           |
| `pnpm e2e:close-gate-late-edit`, display 96 | 8/8                                                             |
| `pnpm e2e:startup-args`, display 93 then 98 | 7/7 both times                                                  |

Display numbers 93, 94, 95, 96 and 98 were used once each, never reused inside this battery.

**The one red test is not mine.** `video::player::tests::every_x11_wid_gets_the_pinned_context` fails
in `src-tauri/src/video/player.rs`, which another implementer is editing in parallel for V1's finding 2. It did not exist in my first run of the suite this session and appeared partway through; I did not
touch that file and did not investigate it further.

## Not committed

Nothing was committed, per the brief.
