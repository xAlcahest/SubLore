# Gate 2, wave 4, lens V1 — what the fixes broke

**Scope.** The fix diff only: `git diff eca9806` over the working tree, which is where wave 3 and the
consolidation pass live (they are uncommitted). `git diff eca9806..HEAD` is empty, so the brief's
literal command reads nothing; the diff reviewed here is the 25-file, 1996/626 working-tree change
whose code half is `src-tauri/src/{lib,dialog,main}.rs`, `src-tauri/src/video/{mod,player,surface/mod}.rs`,
`src/components/VideoStage.tsx`, `src/hooks/useStartupFiles.ts`, `src/types/video.ts`.

**Questions applied.** L1: did a correction break something that worked. L2: can any of it lose the
user's work.

**Platform.** Everything below is reasoning over the diff plus two compiled `rustc` experiments run on
Linux. No Sublore binary was built or driven for this review. Nothing here is a Windows claim.

---

## Findings

### 1. BLOCKER — the close gate deadlocks itself on its own error path, leaving a window that can never be closed

`src-tauri/src/lib.rs:176`

```rust
let dirty = unsaved_work(app_handle);
match decide_close(&mut gate(), &label, dirty) {
```

`gate()` returns a `MutexGuard`. A temporary created in a `match` scrutinee lives until the end of the
`match`, so **`GATE` stays locked for every arm**, including `CloseAction::Ask` and
`CloseAction::AskAgain`, which call `ask_before_closing` at `lib.rs:183` and `lib.rs:190`.

`ask_before_closing` re-enters that same lock on two paths, both of which run synchronously on the
calling thread:

- `lib.rs:431`: `if let Err(error) = asked { … clear_gate(); }`. `clear_gate` is `*gate() = None`
  (`lib.rs:383-385`), so any `Err` out of `dialog::ask_close` locks `GATE` while it is already held.
- `dialog.rs:85-94`: `answer_worker` builds `Delivery::new(answer)`, moves it into the thread closure,
  and calls `Builder::spawn(…)?`. When the OS refuses the thread, `spawn` drops that closure, which
  drops the `Delivery`, whose `Drop` (`dialog.rs:62-70`) delivers `Cancel` **inline on the caller's
  thread**. The callback's first statement is `mark_acting(&acted_label)` (`lib.rs:412`), and
  `mark_acting` calls `gate()` (`lib.rs:364-365`). Same lock, same thread.

`std::sync::Mutex` is not reentrant. Verified rather than assumed, with two standalone programs:

- The guard really is held inside the arm: a `try_lock` from a second thread inside the match arm
  returns `Err`, and returns `Ok` after the match. Printed `lock still held inside the match arm: true`.
- The re-entry really hangs rather than panicking: the same shape (`match decide(&mut gate()) { Ask =>
ask_before_closing() }` with `clear_gate()` inside) ran under `timeout 5` and exited 124.

**Failure the user sees.** The X button stops working. The main loop is blocked inside the
`CloseRequested` handler, so the window does not close, does not repaint, and no dialog appears. The
session still holds the unsaved subtitle. The only way out is SIGKILL, and the edits go with it. That
is CLAUDE.md §3's failure-mode budget inverted: a Sublore defect costing the user's work.

**This is new.** At `eca9806` the same state was two `AtomicBool`s, `GATE_OPEN` and `CLOSING`, whose
`store` cannot block, and `dialog.rs` had no `answer_worker` (it called `std::thread::spawn(move ||
deliver(answered))` from inside the GTK response handler, `eca9806:src-tauri/src/dialog.rs:77`). The
deadlock is created by the interaction of two wave-3 fixes: replacing the atomics with
`Mutex<Option<Gate>>`, and adding the pre-started answer worker with its drop-guard.

**Reachability.** Both triggers are error paths (thread creation refused under RLIMIT_NPROC or memory
pressure; `run_on_main_thread` failing). They are rare. They are also exactly the paths the fix added
in order to _handle_ failure gracefully, and the handling is what hangs. Note also that the same wide
lock scope means `asr::shutdown` and `shutdown_video` at `lib.rs:178-179` run with `GATE` held.

**Recommended correction.** Take the decision and drop the guard before acting on it:

```rust
let action = { decide_close(&mut gate(), &label, dirty) };
match action { … }
```

A `let` binding ends the temporary at the semicolon. Add a unit test that calls `clear_gate()` from
inside a scope shaped like the handler and asserts it returns; today nothing in `lib.rs:613-675` drives
the static at all, only the pure `decide_close`.

---

### 2. SERIOUS — a working `gpu-context` pin was removed for every X11-only machine, and no automated check would notice

`src-tauri/src/video/player.rs:227`, decided at `player.rs:63-75`

At `eca9806` the option was unconditional on Linux whenever `wid` was set:

```rust
init.set_option("gpu-context", "x11egl")?;
```

Now `gpu_context_for` returns `Some("x11egl")` only when `WAYLAND_DISPLAY` is set and non-empty, and
`None` otherwise, which leaves mpv's own `auto` probing in charge on every plain X11 session and under
Xvfb.

Three things make this a finding rather than a tidy-up:

- The justification (`player.rs:592-596`) is a measurement on _a python-xlib window_, not on Sublore's
  GDK native child surface created in `video/surface/linux.rs:37-44`. Those are different windows with
  different visuals and different parents. The measured claim does not carry to the code it licenses.
- The pin that was removed was the configuration under which the whole wdio suite has been green. The
  change widens `auto`'s reach to every user's X11 machine on the strength of one machine's probe.
- Nothing would catch the regression. `e2e/specs/video-surface.spec.js:192` and `:241` assert
  `mapState(surface.id) === "IsViewable" && childWindows(surface.id).length > 0`, and that file's own
  docstring at line 6 says a surface can report `IsViewable` while showing nothing. `pnpm e2e:wayland`
  covers only the branch that still pins, and by `.github/workflows/ci.yml:210-218` it deliberately does
  not run in CI. So the branch this change altered is the one with no picture assertion anywhere.

**Recommended correction.** Either keep the pin and let the hatch remove it, or produce the measurement
against Sublore's own surface (`pnpm e2e:wayland`'s harness with `WAYLAND_DISPLAY` unset, asserting a
painted frame rather than `IsViewable`) before the pin is dropped. As it stands the change is a platform
claim about machines nobody ran.

---

### 3. SERIOUS — `startup_files` still drops arguments in silence, in the commonest case there is

`src-tauri/src/lib.rs:76` and `lib.rs:78`

```rust
files.subtitle.get_or_insert_with(|| arg.to_owned());
…
files.video.get_or_insert_with(|| arg.to_owned());
```

The whole point of this fix is stated at `lib.rs:33-34` ("the dropped ones are handed back rather than
logged here") and `lib.rs:63-64` ("anything else was meant to be one, so it is named rather than dropped
in silence"). `get_or_insert_with` on an already-`Some` field does nothing and pushes nothing onto
`ignored`. So `sublore ep01.srt ep02.srt` opens `ep01.srt`, ignores `ep02.srt`, and writes no line about
it: not in `taken` (`lib.rs:139-143`), not in the `ignored` loop (`lib.rs:145-147`). A user who passes
two subtitles or two videos, or who globs a directory, gets exactly the silence this fix was written to
end, on the most likely input of all.

The check knows about the behaviour and works around it instead of asserting on it:
`e2e/scripts/startup-args-check.js:17-19` explains that it needs two launches "because `startup_files`
keeps only the first subtitle it accepts". That is a documented gap, not a covered one.

**Recommended correction.** In both arms, when the slot is already filled, push
`format!("{arg} (a {kind} was already named)")` onto `ignored`, and add a case to
`startup-args-check.js` asserting the line.

---

### 4. MINOR — a successful save can now raise a second dialog about a document that has nothing unsaved

`src-tauri/src/lib.rs:175`, `lib.rs:334-340`, `src-tauri/src/subtitle/mod.rs:413-420`

`unsaved_work` is now read for every close, the answered one included. It maps
`SessionState::Unknown` to dirty, and `session_state` returns `Unknown` whenever `slot.try_lock()` is
contended. At `eca9806` this could not happen after an answer: `CLOSING.swap(false)` short-circuited the
whole dirty check (`eca9806:src-tauri/src/lib.rs:138-142`).

So: user answers Save, `save_current` writes the file and marks the session clean, `close_window` posts,
and if any `subtitle_*` command happens to hold the session lock at the instant the `CloseRequested`
lands, `decide_close` sees `Acted(Clean)` plus `dirty` and returns `AskAgain` (`lib.rs:334-340`). A
second "Unsaved changes" dialog appears for a file that was just saved.

It terminates: the second Save reaches `save_current`, finds `!session.dirty()`, returns `Ok(None)`, and
`after_save` maps that to `Clean` (`lib.rs:495`). Cost is one confusing click, not data. Naming it
because it is a behaviour change none of the tests can see: every case in `lib.rs:613-675` passes `dirty`
as a plain `bool`, so `Dirty` and `Unknown` are indistinguishable to the suite, and the difference
between them is exactly what this arm now acts on.

---

### 5. MINOR — the empty string means opposite things in the two hatches added in this range, and neither can report a value it cannot decode

`src-tauri/src/main.rs:9-16`, `src-tauri/src/main.rs:35-40`, `src-tauri/src/video/player.rs:64-73`,
`src-tauri/src/video/player.rs:206-212`

- `SUBLORE_WEBKIT_WORKAROUNDS=` (set, empty) falls into the `_` arm and leaves the `/sys/module/nvidia`
  probe deciding (`main.rs:15`), asserted at `main.rs:89-99`.
- `SUBLORE_MPV_GPU_CONTEXT=` (set, empty) means "leave mpv alone" (`player.rs:66`), asserted at
  `player.rs:619-623`.

Two escape hatches added in the same range, same prefix, opposite readings of the same input. Whichever
is right, they should not disagree.

Separately, on the brief's question of whether the printed decision can disagree with the decision
taken: it cannot. `apply` is computed once at `main.rs:28` and used for both the `set_var` calls and the
`eprintln!`, and `chosen` likewise at `player.rs:204`. What _can_ disagree is the printed state of the
variable and the environment the user actually set: both read through `std::env::var(..).ok()`, so a
value that is not UTF-8 prints as `unset` (`main.rs:38`, `player.rs:209`) for a variable that is set.
That is the exact `OsString` lesson `startup_files` was rewritten to learn at `lib.rs:46-47`, not applied
to the two new hatches beside it.

---

### 6. MINOR — the mpv hatch turns a typo into no video at all

`src-tauri/src/video/player.rs:66-67`, applied at `player.rs:227`

Any non-empty value is forwarded verbatim: `Some(forced) => Some(forced.to_owned())`. mpv's initializer
rejects an unknown `gpu-context` name, `Mpv::with_initializer` fails, and `Player::new` returns a
`VideoError`, so the user gets no playback rather than a fallback and a warning. `player.rs:604-616`
exercises only `x11`, `x11vk` and `auto`, so the unrecognised case is untested as well as unhandled.
Validate against the names mpv accepts, or log the rejection and fall back to the probe.

---

### 7. MINOR — the new error handling in `useStartupFiles` is `console.error`, which no user can read

`src/hooks/useStartupFiles.ts:37` and `:63`

Both new handlers report to the devtools console. CLAUDE.md §6 asks for errors at boundaries to surface
to the UI as actionable messages, "never silent logs", and a release WebKitGTK webview has no console the
user can open. `openOne` is the more consequential of the two: it converts a rejected open into a
swallowed one, so a file named on the command line that fails to open produces a blank stage and no
statement anywhere the user can see. The old code (`eca9806`) was silent by intent and said so; the new
code is silent while claiming not to be, which is worse for the honesty rule in CLAUDE.md §9.

---

### 8. MINOR — the new `matchMedia` listeners are the one part of the N2c fix nothing exercises

`src/components/VideoStage.tsx:54-62`

Three `MediaQueryList` listeners are registered so that a live scale-factor change re-reports the region.
There are no frontend unit tests in this repo at all (no `*.test.*` under `src/`, no vitest in
`package.json`), and `e2e/scripts/scaled-surface-check.js:71-119` measures two _separate launches_ at
`GDK_SCALE=1` and `GDK_SCALE=2` rather than changing the ratio inside one. The listener whose entire
purpose is the live change is therefore never fired by anything. If `matchMedia` or the `dppx` query
ever throws, the throw happens inside the effect and takes the video stage down; nothing would catch it
before a user did.

---

## Checked and clean, stated precisely

These are the places I expected to find something and did not. Each was traced rather than skimmed.

- **The `CloseRequested` restructure did not remove the shutdowns.** The plan's named false positive.
  `CloseAction::Close` is returned from both `None => Close` (`lib.rs:322`) and `Acted(_) => Close`
  (`lib.rs:343-348`), and the arm at `lib.rs:177-180` calls `asr::shutdown` and `shutdown_video` on
  both. The gate path reaches `Close` through the second. Nothing lost.
- **`SessionAfter::Unproven` is sound, and I defeated the argument rather than restating it.**
  `discard_open_file` (`lib.rs:520-533`) returns `Unproven` only when `close_session(slot, true)` errs.
  Reading `close_session` (`subtitle/mod.rs:320-329`), with `discard == true` the only reachable error is
  `lock(slot)?`, and `lock` (`subtitle/mod.rs:534-542`) errs only on poison. Every editing command goes
  through the same `lock`, so after that failure no new edit can be committed. A dirty session at the
  close really is the abandoned work, and `Acted(_) => Close` is right to wave it through.
- **`after_save(Ok(_)) => Clean` does not close over live work.** `Ok(None)` from `save_current`
  (`subtitle/mod.rs:437-456`) means `!session.dirty()`. `NoDocument` (`lib.rs:498-501`) means the slot is
  empty, and the only ways to empty it are `close_session` with `discard` or a clean
  `open_session` (`subtitle/mod.rs:290-297`), both of which refuse to drop dirty work.
- **The answer worker cannot outlive a write.** `save_open_file` completes `save_current` before
  returning `Answered::Close`, and `close_window` is only called after that (`lib.rs:419-424`), so the
  window never closes with a write in flight. `stall_after_answer` (`lib.rs:445-461`) sits between the
  two, after the write, which is where the late-edit check needs it.
- **Two gates over one document cannot overlap.** From `mark_acting` (`lib.rs:364`) to `mark_acted`
  (`lib.rs:372`) the phase is `Acting`, and `decide_close` returns `Wait(Held::Answer)` for it
  (`lib.rs:329-332`). A second answer while the first save is writing is not reachable.
- **An answer arriving after the window is gone is handled.** `close_window`'s `None` arm
  (`lib.rs:559-563`) logs and clears the gate rather than leaving a decision standing.
- **A panic on the answer thread does not strand the gate.** `crash::on_panic` ends with
  `std::process::exit(EXIT_PANIC)` (`src-tauri/src/crash/mod.rs:99`), so a panic between `mark_acting`
  and `mark_acted` kills the process instead of leaving a permanently unclosable window. This is the
  only reason finding 1 is the sole deadlock in the file.
- **`RunEvent::ExitRequested` never calls `prevent_exit` (`lib.rs:200`), and that is currently safe.**
  Nothing in `src-tauri/src/` or `src/` calls `AppHandle::exit`, so the event only fires after the last
  window has already been closed, which the gate controls. It becomes a data-loss path the moment a
  menu, tray or frontend quit is added. Worth a BACKLOG line, not a finding against this diff.
- **`pixels_over`'s rewrite is a fix, not a regression.** `src-tauri/src/video/surface/mod.rs:53-70`.
  At divisor 1 with integral inputs the new edge-difference span equals the old rounded length. Where
  they differ (`577`/`1025` over 2: old 513, new 512) the new one matches the page's own rule at
  `VideoStage.tsx:38-44`, and the old one hung a pixel past the stage. Both sides now compute
  `round(right*r) - round(left*r)`.
- **`pixels` and `pixels_over` losing `pub` compiles.** `platform` is a child module of `surface`
  (`surface/mod.rs:6-13`), and a child can reach its parent's private items.
- **Nothing in the range writes to user media or to `fixtures/`.** `SAFE_OPTIONS`
  (`video/player.rs:33-51`) disables `config`, `load-scripts`, `save-position-on-quit`,
  `resume-playback`, `watch-later-options`, `sub-auto`, `audio-file-auto` and `access-references`, so a
  stray argument routed to `files.video` by `lib.rs:78` cannot make mpv write anything.
  `close-gate-late-edit-check.js:222-227` and `scaled-surface-check.js:72` copy into `mkdtemp` and edit
  the copy.
- **`startup_files` on the awkward inputs.** A directory, a dangling symlink and a nonexistent path all
  fail `Path::is_file()` (`lib.rs:60`) and are named in `ignored` unless they start with `-`. A file
  that disappears between the check and the open reaches `read_document` and fails there. A file the
  user can stat but not read passes `is_file()` and fails at open. None of these writes anything; the
  only complaint about them is finding 7's swallowed report.
- **The GTK dialog button geometry is not a live risk.** `e2e/lib/gtk-dialog.js:11` estimates 96px per
  button; the labels are `Save`, `Discard`, `Cancel` (`src-tauri/src/strings.rs:26-28`), well inside
  that under any normal theme, and both callers prove the click landed by watching the dialog go.

---

## One thing I could not settle from the diff

The four CI steps added at `.github/workflows/ci.yml:198-208` have never run on a GitHub runner, since
the change is uncommitted. The comment on the `e2e:scale` step says the `GDK_SCALE=2` window is 2048x1400
and that "this bare X server serves without clamping (verified locally)", but the step runs it under
`xvfb-run -s "-screen 0 1280x1024x24"`, a smaller screen than the one the local verification used. If
`toplevelByName` (`scaled-surface-check.js:53-59`) ever reads a clamped width, the
`doubles(single.toplevel.width, double.toplevel.width)` guard at `scaled-surface-check.js:162-170` fails
and takes the whole job red. That is the safe direction, but it is a prediction, not a measurement, and
the first push will settle it.
