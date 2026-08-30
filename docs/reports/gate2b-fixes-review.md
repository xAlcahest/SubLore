# Gate 2, wave 4 round two — V1', what round two broke

Lens: what the round-two corrections themselves broke, whether any of them can lose the user's work,
and whether the deadlock fix is actually provable. Read on Linux, against the working tree as it
stands (`HEAD == eca9806`, all of waves 3 and 3b uncommitted).

Nothing was edited in this repo. The two cargo runs below used `CARGO_TARGET_DIR` pointing into the
session scratchpad and a `tar`-copy of the tree, so `target/debug/sublore` was never rebuilt: it is
still the `pnpm e2e:build` binary the round-two battery left behind (mtime 21:36, unchanged after my
runs). WORKFLOW §4c is intact for whoever runs the battery next.

---

## 0. The headline: the deadlock fix is real, and only one test proves it

**Verdict: the fix is correct, and exactly one of the two tests offered as proof can fail against
the deadlocked code. It is a source-text scan, not an execution.**

I ran the discrimination rather than reading the claim. A copy of the tree with `lib.rs:184` put back
to the round-one shape (`match decide_close(&mut gate(), &label, session) {`), built in an isolated
target dir:

```
failures:
    tests::the_close_handler_takes_no_gate_guard_of_its_own
test result: FAILED. 59 passed; 1 failed; 0 ignored
```

Against the shipping tree the same two tests are green (`2 passed; 0 failed; 58 filtered out`).

So:

- `the_close_handler_takes_no_gate_guard_of_its_own` (`src-tauri/src/lib.rs:749`) **does**
  discriminate. Both of its assertions fail on the reverted shape: `decide_close_now(` is absent from
  the handler, and `gate()` appears in it at handler-relative line 12 preceded by a space, which the
  filter at `lib.rs:766-771` does not exclude. This is the whole proof of the blocker.
- `a_close_decision_leaves_the_gate_unlocked_for_the_arm_that_acts_on_it` (`lib.rs:728`) **passed on
  the deadlocked tree**, in the same run. It cannot fail there, because it exercises
  `decide_close_now`, a function the deadlocked code did not have; the deadlock lived in the handler
  and this test never touches the handler. It is a useful guard on the new helper and it is not
  evidence about the defect. `docs/reports/gate2b-fix-gate.md` §1 presents both under "What proves
  it" without separating them, and its own quoted revert run ("58 passed; 2 failed") is consistent
  with mine: one failure was the mpv implementer's in-flight test, not a second gate failure.

The gate's charter is that a fix under review pressure gets read for what it broke. On the deadlock
itself, it broke nothing I can find — see §1. What it did not do is put an executable guard on the
defect class; §2 is that gap.

---

## Findings

### 1. MINOR — the shape that caused the blocker still exists twice in the same file, and the test written to prevent it does not look there

`src-tauri/src/lib.rs:392`, `src-tauri/src/lib.rs:401`, `src-tauri/src/lib.rs:749-779`

The blocker was a `MutexGuard` built in a `match` scrutinee, which lives for every arm, in a function
whose arms re-enter the same lock. `mark_acting` (`lib.rs:390-397`) and `mark_acted`
(`lib.rs:399-408`) both still have exactly that shape:

```rust
match gate().as_mut() {
    Some(open) if open.label == label => open.phase = Phase::Acting,
    _ => log::warn!(...),
}
```

Safe today, and I checked why rather than assuming it: neither arm calls anything that reaches
`GATE` — the assignment is a field write, and `log::warn!` goes to tauri-plugin-log's file target.
But the guard that round two shipped is scoped by construction to the `CloseRequested` handler: the
test slices the source between `RunEvent::WindowEvent` and `RunEvent::ExitRequested`
(`lib.rs:751-758`) and asserts only about that slice. Two functions below, the identical shape sits
unguarded, in the same lock, in the file this gate has already been bitten in once.

**Failure scenario.** Someone adds a `clear_gate()` or a `decide_close_now()` call to the `_` arm of
`mark_acting` — the arm that fires when the gate went away under an answer, which is precisely where
a future implementer would want to recover. The main thread deadlocks on its own lock the first time
the recovery arm is taken, the window can never be closed, and `cargo test` stays green because
`the_close_handler_takes_no_gate_guard_of_its_own` never reads those lines.

The cheap version of the fix is to widen the slice, or to scan every `fn` in the file that names
`gate()` for a guard held across a `match`. I am not proposing the code; I am naming that the guard
is narrower than the defect it was written for.

### 2. SERIOUS — `SUBLORE_MPV_GPU_CONTEXT` is the only way off an unconditional pin, and it is documented nowhere a user will look

`src-tauri/src/video/player.rs:53-70` (the hatch const at `:56`, `gpu_context_for` at `:67`), `README.md:140` and `:142`, `docs/reports/gate2b-fix-hatch.md`

Round two did two things that pull against each other:

- `gate2b-fix-mpv.md` restored the unconditional pin and states plainly that "the hatch
  `SUBLORE_MPV_GPU_CONTEXT` is the way off it". The hatch is now the entire recovery route for any
  machine where `x11egl` is the wrong answer.
- `gate2b-fix-hatch.md` closed the closure audit's `main.rs:20` row ("the hatch is documented nowhere
  a user will look") by adding a README paragraph — for `SUBLORE_WEBKIT_WORKAROUNDS` only. I grepped:
  `README.md` names `SUBLORE_FORCE_PANIC` (`:140`) and `SUBLORE_WEBKIT_WORKAROUNDS` (`:142`) and does
  not name `SUBLORE_MPV_GPU_CONTEXT` anywhere. Neither does `docs/design/`, nor `e2e/README.md`. The
  only prose mention in the tree is `BACKLOG.md:126`, in a residue list.

So the audit row was closed for one hatch while the same gap was left open on the hatch the same
round made load-bearing, and no report says so.

**Failure scenario.** A user on a stack where the pinned `x11egl` does not work opens a video, sees
no picture, and has no discoverable way to change it. The app prints the decision to its log
(`player.rs:232-238`), which tells him which path was taken and not that there is a variable that
changes it. Nothing in the shipped product mentions the variable's existence.

### 3. SERIOUS — with the narrowing gone, an mpv that lacks `x11egl` stops the whole app from starting, not just the video

`src-tauri/src/video/player.rs:86-97`, `src-tauri/src/video/mod.rs:131-142`,
`src-tauri/src/lib.rs:162-165`

`set_gpu_context` protects the hatch from a typo but deliberately propagates the pin's own refusal:

```rust
Err(error) if context == GPU_CONTEXT_PIN => Err(error),
```

mpv builds `gpu-context` as a choice over the contexts compiled in, so on a libmpv built without
EGL/X11 the name `x11egl` is refused at `set_option` time. Trace the error: `Mpv::with_initializer`
fails → `Player::new` returns `Err` → `video::setup` returns `Err` (`video/mod.rs:138-141`) →
`lib.rs:162-165` returns it out of the Tauri `setup` hook → `Builder::build()` fails → `run()` fails
→ `main()` returns `Err`, which Rust prints to a stderr a desktop launch has nowhere to show. The
user gets no window at all, no dialog, and no message.

This is a **restoration, not a creation**: `eca9806` pinned unconditionally too. What round two
removed is the round-one escape — the narrowed `gpu_context_for` returned `None` on an X11 session,
so such a machine started and probed. And `a_pin_mpv_refuses_reaches_the_caller`
(`player.rs:689-698`) now codifies "reaches the caller" as intended behaviour without stating what
the caller does with it, which is kill the launch. The fix for V1 #6 stops one step short of the case
with the identical user-visible cost, in the same function.

Low likelihood on mainstream distros; total consequence when it hits. Flagging for a ruling, not
proposing a change.

### 4. MINOR — V1 finding 5's first half is still open, and each round-two report points at the other

`src-tauri/src/video/player.rs:68`, `src-tauri/src/main.rs:14-24`, `README.md:142`,
`BACKLOG.md:126`

`gate2b-fix-hatch.md` states the settled rule ("an empty value means the variable is unset"), writes
it into `nvidia_workarounds_wanted`'s docstring and the README, pins it with
`an_empty_value_decides_exactly_what_no_value_decides` (`main.rs:131`), and says: "**This is the one
item that needs another implementer's change before the finding is closed.**"
`gate2b-fix-mpv.md` never mentions finding 5. `player.rs:68` still reads `Some("") => None`, and
`an_empty_hatch_hands_the_choice_back_to_mpv` (`player.rs:649-653`) now asserts the opposite rule as
correct. Two hatches with the same prefix still read the same input in opposite directions, and the
README now documents one of the two readings as if it were the rule.

It is filed at `BACKLOG.md:126` under N5, so it is not lost. Naming it because the gate's exit
condition wants every row closed or owner-ruled, and this one reads as closed if you only read the
two reports.

**One thing nobody noted, and it will cost the next round.** The pin's own headline discrimination in
`gate2b-fix-mpv.md` §1 — "unpinned (`SUBLORE_MPV_GPU_CONTEXT=`) … the surface has no children" — is
run _through_ the empty-value semantics that N5's first row wants changed. Fix N5 as `main.rs` ruled
and that experiment's mechanism disappears; the only remaining route to `auto` becomes
`SUBLORE_MPV_GPU_CONTEXT=auto`. Whoever takes N5 has to update the reproduction with it.

### 5. MINOR — `BACKLOG.md`'s residue list still carries a defect round two fixed

`BACKLOG.md:128`

N5 lists as open residue: "`player.rs` — an unrecognised `gpu-context` value is forwarded verbatim,
mpv's initializer rejects it, and the user gets **no video at all** rather than a fallback and a
warning." That is V1 finding 6, and round two fixed it: `set_gpu_context` (`player.rs:86-97`) falls
back to the pin and warns (`player.rs:254-261`), proved by
`a_context_mpv_refuses_falls_back_to_the_pin` (`player.rs:672-687`) and by the before/after build pair
in `gate2b-fix-mpv.md` §2.

**Failure scenario.** A later session works the N5 list, reads that row, finds the fallback already
there, and either wastes the pass or — worse — "restores" the described behaviour because the
backlog says that is what the code does. The residue list is the surviving record of this gate; a
stale row in it is a wrong instruction to the next reader.

### 6. MINOR — the mpv hatch still reports a set value as `unset`, in the file round two rewrote

`src-tauri/src/video/player.rs:228`

`let hatch = std::env::var(GPU_CONTEXT_HATCH).ok();` — a value Rust cannot decode reads as `None`, so
`player.rs:232-238` prints `SUBLORE_MPV_GPU_CONTEXT=unset` for a variable the user did set. The
decision is unaffected (undecodable → pin, same as unset), so this costs only the log line — but that
line is the first thing a "no picture" report is read for, and it lies. `gate2b-fix-hatch.md` fixed
exactly this in `main.rs:51` with `var_os` + `hatch_report`, named `player.rs:209` as carrying the
same defect, and left it because the file belonged to another implementer that round; the mpv
implementer, who did own the file, rewrote the function above it and did not carry the lesson across.
Filed at `BACKLOG.md:127`.

---

## Data loss: nothing found in the round-two diff, and here is what I checked

The close gate exists so a Sublore defect costs annoyance and never data (CLAUDE.md §3). I traced
every path round two touched.

- **Nobody holds `GATE` across a call that retakes it.** All call sites: `decide_close_now`
  (`lib.rs:326-328`), `mark_acting` (`:392`), `mark_acted` (`:401`), `clear_gate` (`:411`), plus the
  tests. `decide_close_now`'s guard is a temporary in the function body's tail expression, so it is
  dropped before the function returns and `CloseAction` is `Copy` — no borrow escapes. The handler
  arms therefore run with the gate free, which is what `ask_before_closing`'s two re-entrant paths
  need: `mark_acting` when `answer_worker` (`dialog.rs:80-95`) fails to spawn and `Delivery::drop`
  (`dialog.rs:62-70`) delivers `Cancel` inline on the calling thread, and `clear_gate` at
  `lib.rs:455` on the same path. I confirmed that inline-drop mechanism by reading `answer_worker`:
  `carrier` is moved into the closure, and a refused `spawn` drops the closure on the caller's
  thread. The round-one blocker was real and this is the right fix for it.
- **`GATE` and the subtitle session lock are never nested.** `session_now` (`lib.rs:416-422`) takes
  and releases the slot via `session_state`'s `try_lock` (`subtitle/mod.rs:413-420`) before
  `decide_close_now` is called; `save_open_file` and `discard_open_file` take the slot only from the
  answer thread, after `mark_acting` has released `GATE`. No lock-order inversion exists to invert.
- **Releasing the guard earlier than round one did opens no new window.** The two shutdown calls in
  the `Close` arm (`lib.rs:186-187`) now run without the gate; they are main-thread only, they do not
  pump the GTK loop (`Player::shutdown` joins with a drain timeout), and `decide_close` has already
  set `*gate = None`, so a re-entrant close would decide from scratch and find the same state. The
  `Ask` arm still calls `api.prevent_close()` before `ask_before_closing`, so the window cannot be
  let go between the decision and the dialog.
- **`Acted(Unproven)` closes over a dirty session, and the justification holds.** I checked the
  comment's claim rather than accepting it: `close_session` (`subtitle/mod.rs:320-330`) with
  `discard: true` can only fail at `lock(slot)?`, which fails only on poisoning
  (`subtitle/mod.rs:534-541`), and every editing command goes through that same `lock`. So after a
  failed discard nothing new can enter that session, and the dirty content at the close really is the
  work the user chose to lose.
- **`startup_files`'s rewrite writes nothing.** `lib.rs:49-89`: the slot and kind are chosen first,
  a full slot pushes an `ignored` line, `ignored` is logged in `setup` (`lib.rs:154-156`). No file is
  opened, moved or created. The two ignored lines are asserted behaviourally at
  `e2e/scripts/startup-args-check.js:271-275` and `:276-280`, both of which can fail for a cause the
  check constructs (two real `.srt` files in a controlled `cwd`).
- **`VideoStage`'s try/catch cannot leak a listener.** `src/components/VideoStage.tsx:58-70`: only
  queries whose `addEventListener` returned are pushed to `ratioQueries`, and the cleanup
  (`:81-83`) iterates that same array. A throw costs the three ratio listeners and nothing else; the
  `ResizeObserver`, the resize listener and the first `schedule()` all still run.
- **`useStartupFiles` changed comments only**, and the comments now say what is true: the failure
  reaches the log and no further. That is honesty, not a fix, and the file says so.
- **No `std::env::set_var` runs after a thread exists.** Grepped the whole of `src-tauri/src` and
  `crates`: three call sites, all in `main.rs:57`, `:58`, `:78`, all before `sublore_lib::run()`.

## Re-derived rather than inherited

The brief says the closure audit is fallible and one row is known wrong. I re-derived that row
instead of taking the harness implementer's correction on trust.

`docs/reports/gate2-closure-audit.md:77` says the ffmpeg gate holds the suite for "a capability
nothing uses". Read `crates/sublore-asr/src/tools.rs:76-98`: ffmpeg is resolved from `PATH` (or
`SUBLORE_FFMPEG_BIN`) and `crates/sublore-asr/src/sidecar.rs:291` spawns it to extract the WAV the
transcription reads. `asr.spec.js` drives a real transcription. The dependency is real, the audit row
is wrong, and `e2e/wdio.conf.js:33`'s replacement — `requireTool("ffmpeg", "extract the audio the
transcription spec transcribes")`, at module scope, with `requireTool` already imported at `:9` — is
correct on both counts. The correction is filed at `BACKLOG.md:132`.

I also checked the mpv revert on its own terms rather than on its story. The narrowing keyed on
`WAYLAND_DISPLAY`, but libwayland's `wl_display_connect(NULL)` falls back to `wayland-0` under
`XDG_RUNTIME_DIR` when that variable is unset, so mpv can reach a compositor on a session the
narrowing would have read as X11-only. The condition was a proxy for a decision mpv makes from two
variables, and removing it is the safer side. `docs/design/x11-vs-render-api.md:23` and
`BACKLOG.md:94` describe the pin as unconditional and are correct again; their `player.rs:186-192`
line references are stale (the code is at `player.rs:248-262` now), which is cosmetic.

## What I did not check

- **Windows.** Nothing here is a Windows claim. `gpu_context_for` and `set_gpu_context` compile on
  Windows behind `allow(dead_code)` and their tests are not `cfg`-gated to Linux, so on Windows the
  suite asserts a Linux-only decision; that is harmless but it is not coverage. The `check` job is
  the only thing standing behind the Windows half of this diff.
- **The behavioural battery.** I did not re-run `pnpm e2e`, the close-gate checks or the scale check;
  deliberately, so `target/debug/sublore` stays the round-two `pnpm e2e:build` artefact. Every runtime
  claim above is from reading code plus the two isolated `cargo test` runs.
- **The live device-ratio change** in `VideoStage`. Still fired by nothing, as
  `gate2b-fix-harness.md` states and `BACKLOG.md:131` files. I confirmed the gap, not closed it.
- **The pin against real EGL-less hardware.** `gate2b-fix-mpv.md` §4 says that case was simulated at
  the mpv level only and that Sublore's own webview goes down with the EGL vendor library. That
  disclosure is accurate about its own limits; finding 3 above is a reading of the error path, not a
  measurement.
