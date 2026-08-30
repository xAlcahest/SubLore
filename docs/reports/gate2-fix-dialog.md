# Gate 2 — Wave 3 fixes: `src-tauri/src/dialog.rs`

Cluster: the close gate's dialog module. Every register row whose site is `src-tauri/src/dialog.rs`,
serious and minor. One file changed, `src-tauri/src/dialog.rs`, and nothing else.

`GATE_BASE=f0b0058` · `GATE_HEAD=eca9806`. Written on Linux (Fedora, GTK 3.24.52, gtk-rs 0.18.2).
Every behavioural statement below is a Linux statement; the `#[cfg(not(target_os = "linux"))]` half
is still compiled and never run, here or anywhere.

## What the rows had in common

Four of them were one defect wearing four faces: **the answer was carried by a path that could fail
or be dropped after the question had already been consumed.** `std::thread::spawn` panics rather
than returning an error, and it was called _after_ the `FnOnce` had been taken out of its cell and
the dialog destroyed, so there was no dialog left to re-ask and no callback left to answer with. The
same shape, one layer over, let the callback reach its destructor without ever being called: GTK
destroying the dialog with its parent, a main-thread task that is never dispatched, and on Windows
the plugin's own `let _ = run_on_main_thread(...)` dropping the closure in silence.

So the fix is one mechanism, not four patches:

1. **`Delivery<F>`**, a carrier that owns the callback and answers `Cancel` from its `Drop` if it
   was never answered. Delivery stops being a property of which paths happen to be reachable and
   becomes a property of the type. Answering twice is impossible (the callback is taken on the first
   answer), and a lost dialog now says so in the log instead of leaving the gate standing open.
2. **`answer_worker`**, which creates the thread that will act on the answer **before anything is
   asked** and hands back a channel. The one fallible operation on the path now fails while the
   question is still unasked, where the caller's existing contract already covers it ("an error
   means nobody will ever be asked"). Nothing on the answer path can panic any more: there is no
   `std::thread::spawn` left in the file, and the thread is named `sublore-close-answer`.

Both branches, Linux and non-Linux, go through both. The GTK response handler and the plugin
callback now hold nothing but a `Sender<CloseAnswer>`; dropping either one answers `Cancel`.

## The rows

### Serious

**`dialog.rs:77` — `std::thread::spawn` panics instead of failing, inside a GTK C trampoline, after
the answer has been consumed (L2 F3, L6 #1). Fixed.**
The spawn moved out of the response handler and to the top of `ask_close`, as
`std::thread::Builder::new().name(...).spawn(...)?`. If the OS refuses the thread the function
returns `Err` before the dialog exists, which is the case `ask_before_closing` already handles by
keeping the window and clearing the gate. The user cannot lose an answer they have given, because
the refusal happens before they are asked. Proved by the tests below plus the plain reading that the
file now contains no panicking call at all.

**`dialog.rs:77` — the delivery thread is unnamed and the crash report says so (L6 #8). Fixed.**
`ANSWER_THREAD = "sublore-close-answer"`, matching the shape of `crash/mod.rs:183`. Pinned by a test
that reads `std::thread::current().name()` from inside the callback.

**`dialog.rs:46` — the answer callback can be dropped without delivering on Linux (L6 #3). Fixed.**
The GTK closure holds a `Sender`. `DESTROY_WITH_PARENT` destroying the dialog without a response, or
a posted task that is never dispatched, both drop that sender; the worker's `recv` fails, the carrier
drops, and `Cancel` is delivered. L6 reported this as a suspicion it could not trigger in the app as
it stands, and I did not manage to trigger it either. It is fixed structurally, and the structure is
tested.

**`dialog.rs:120` — the Windows branch returns `Ok` unconditionally while its delivery can be
dropped (L6 #2). Fixed in code, compiled only, never executed.**
The plugin still discards its own post (`tauri-plugin-dialog-2.7.2/src/desktop.rs:222`), so the
dialog may still never be raised. What changed is the consequence: dropping the plugin's closure
drops the sender, which answers `Cancel`, which keeps the window and re-arms the gate, instead of
leaving `GATE_OPEN` true for the life of the process and an unclosable window. The `Ok(())` is now
honest, and the code says why at the return.
**This is a compile-time claim on Windows and nothing more.** What is genuinely executed there is
the delivery machinery: the five tests below are platform-independent and CI runs
`cargo test --workspace` on `windows-latest`, so `Delivery`'s exactly-once and answer-on-drop
behaviour is _run_ on Windows. The plugin call, the dialog, the button mapping and the drop of the
plugin's closure are not, on Windows or anywhere.

### Minor

**`dialog.rs:3` — the module doc claims rfd's GTK thread is removed (L6 #7). Fixed.**
It now says the thread is removed from the close gate only, names the two paths that still raise
plugin dialogs (`project::choose_path`, `crash::show_dialog`), and points at N1c.

**`dialog.rs:41` — a missing dialog parent is accepted in silence (L5 #5, L6 #6). Fixed.**
`log::warn!` on both branches when the parent is `None`. A null parent is the rfd behaviour the
module exists to escape; losing it should not be something you discover from a screenshot.

**`dialog.rs:74` — the comment justifying the detached thread states a mechanism that does not exist
(L5 #7, L6 #4). Fixed.**
The self-wait claim is gone. The reason recorded now is the real one: acting on the answer takes a
blocking lock and writes a file, which the main loop must not do (CLAUDE.md §7). The related
overclaim at the old `:25` ("returns as soon as the dialog is on its way") is corrected too: on
Linux the only caller is already the main thread, `run_on_main_thread` runs the task inline there,
and the dialog is up before `ask_close` returns.

**`dialog.rs:84` — the Windows-only paths compile but have executed zero times (L4 #3). Not a
defect, and still true.** L4 recorded it as correct disclosure rather than a finding. Nothing I did
changes it: no Windows path in this file has ever run. See the honesty paragraph in the row above
for the one part that CI does execute there.

### One row that is not mine, and what I did about it

L6 #5 (`report_error` has a main-thread caller its own doc says does not exist) is filed at
`src-tauri/src/lib.rs:307`, which belongs to the lib.rs implementer. The half that lives in my file
is fixed: `report_error`'s doc now says that one of its two callers is on the main thread and that
the post runs inline there. The lib.rs half (`:266-267`'s deadlock claim, or hoisting
`report_close_failure` out of the main-thread closure) is untouched.

## What proves it

Five unit tests in `src-tauri/src/dialog.rs`, all platform-independent, all green:

```
cargo test -p sublore --lib dialog::      5 passed
cargo test -p sublore --lib               54 passed, 0 failed
cargo clippy --all-targets -- -D warnings exit 0
cargo fmt --check                          clean
```

- `the_answer_reaches_the_callback`
- `a_dialog_that_goes_away_unanswered_answers_cancel`
- `a_callback_dropped_before_any_dialog_answers_cancel`
- `an_answered_gate_is_never_answered_a_second_time`
- `the_answer_is_acted_on_by_the_named_worker_and_not_by_the_asking_thread`

**They can fail. Two discrimination experiments, each with the mutation applied, the suite run, and
the file restored from a copy:**

| mutation                                              | result                                                                                                                                                                     |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Drop for Delivery` no longer calls `deliver(Cancel)` | 2 failed, 3 passed: `a_dialog_that_goes_away_unanswered_answers_cancel` and `a_callback_dropped_before_any_dialog_answers_cancel` both fail with `an answer: Disconnected` |
| `.name(ANSWER_THREAD)` removed from the builder       | 1 failed, 4 passed: `left: None, right: Some("sublore-close-answer")`                                                                                                      |

The non-Linux branch was type-checked by temporarily swapping the two `cfg` guards on `ask_close` so
that the plugin version is the one compiled on this machine: `cargo clippy --all-targets -D warnings`
exit 0, then the file restored. That is a type check against the plugin's Linux backend, not a
Windows run.

The exactly-once assertion in `an_answered_gate_is_never_answered_a_second_time` was not mutated: the
property is carried by `Option<F>` plus `FnOnce`, and I could not write a mutation that breaks it and
still compiles. It stands as a guard against a future `Drop` that answers unconditionally, and I am
saying plainly that it is the weakest of the five.

## What I could not prove, and why

**The behavioural close-gate check does not run green in this working tree, and it does not
implicate this change.** `xvfb-run -a pnpm e2e:close-gate` fails at the first step, before any of its
twelve checks: "the app exited (code 0) instead of asking", which is its own way of saying that
nothing was dirty when the close arrived, so the gate correctly did not open.

Discrimination, with both build exit statuses checked explicitly and never chained behind a silent
`&&` (WORKFLOW 4c):

1. `pnpm e2e:build` (exit 0) with my `dialog.rs`, then the check: fails as above.
2. `git show HEAD:src-tauri/src/dialog.rs` restored over mine, `pnpm e2e:build` (exit 0), then the
   check: **fails identically.** Same message, same step.
3. My version restored, rebuilt (exit 0), run again with `md5sum` on the binary before and after to
   rule out another agent's `cargo test` overwriting it mid-run: unchanged, same failure.

So the close gate's dialog is not what is broken. What is broken is upstream of it: the document
never reaches the screen. `xvfb-run -a pnpm e2e` in the same tree is **6 of 8 spec files red**, with
`cue list editing` failing on "timed out after 30000ms waiting for the first cue row to appear" and
transcription specs failing alongside. The same suite was 8/8 green at `GATE_HEAD` a few hours ago
(`gate2-battery-baseline.md`). The tree is being edited and rebuilt by parallel Wave 3 implementers
while I run, so I am reporting this as the state of the shared tree, not as a verdict on anyone's
change.

**Consequence for this cluster, stated honestly:** the GTK dialog path (button mapping, the destroy
inside the handler, save writing the file and the window then closing) is **not** re-verified
behaviourally by me. It is unchanged in shape from the version that was 12/12 green at `GATE_HEAD`;
only the delivery of the answer behind it was rewired, and that rewiring is what the unit tests
cover. The close-gate check has to be re-run by the orchestrator once the tree is whole, and until it
is green nobody should call this cluster behaviourally verified.

## Noticed, out of scope, for BACKLOG

- **The tree is red beyond this file.** 6/8 wdio spec files fail and `pnpm e2e:close-gate` cannot
  reach its first assertion, both because the cue list never appears. Candidate worth checking first,
  as a suspicion and nothing more: the in-flight `src/components/VideoStage.tsx` adds
  `window.matchMedia("(min-resolution: …dppx)")` with `addEventListener` in an effect that runs on
  every render of the stage, and a throw there would blank the React tree, which is exactly the
  symptom. I did not test it and I did not touch the file.
- **A panic inside `deliver` still ends the process, and no `catch_unwind` here could change that.**
  L2 recommended wrapping the delivery body in `catch_unwind`. I did not: the panic hook
  (`crash/mod.rs:99`) calls `std::process::exit(101)` before any `catch_unwind` could resume, so the
  wrapper would be unreachable code (CLAUDE.md §6 bans that). If the project wants a panic on the
  answer thread to leave a live, re-armed window instead of an exit, that is a decision about the
  hook's exit policy, not about this module.
- **`ask_close`'s `Err` path is compiled, not exercised.** It needs the OS to refuse a thread. I did
  not construct thread exhaustion, and the register did not ask for it. The failure mode it replaces
  (a panic in a C trampoline with the answer already consumed) is gone by construction: the call
  that could fail now happens before the question.
