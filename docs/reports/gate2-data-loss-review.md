# Gate 2 — L2, data-loss paths

**Lens:** L2 (standing), `docs/reviews/gate-2-plan.md` §2.
**Scope:** `GATE_BASE=f0b0058` .. `GATE_HEAD=eca9806`, and nothing else.
**Question:** can any path introduced in this range end with the user's subtitle work gone,
truncated, or written to a file they did not name?
**Platform:** every behavioural statement below is reasoned from the tree and from the dependency
sources. Nothing here was observed by running the app. Where I ran anything it was one isolated
Rust program in a scratch directory, and it is named at the finding that used it.

---

## 1. What I checked

Code, read whole rather than in diff form where the file is new:

- `src-tauri/src/lib.rs` in full — `startup_files`, `startup_files_command`, the `CloseRequested`
  arm, `CLOSING`, `GATE_OPEN`, `ask_before_closing`, `save_open_file`, `discard_open_file`,
  `close_window`, `report_save_failure`, `report_close_failure`.
- `src-tauri/src/dialog.rs` in full, both `cfg` halves.
- `src-tauri/src/subtitle/mod.rs` — `open_session`, `close_session`, `apply_edit`, `save`,
  `save_locked`, `session_state`, `save_current`, `save_as`, `lock`, `current`, `read_document`,
  `backup_root`.
- `crates/sublore-edit/src/session.rs` — `EditSession::open/apply/undo/redo/commit/to_bytes/
mark_saved/dirty/revision`.
- `crates/sublore-io/src/atomic.rs` in full — `save_with_backup`, `Target::resolve`,
  `resolve_symlink`, `replace`, `fill`, `create_temp`, `rename`. Out of range and unchanged, read
  because the gate's save path goes through it and CLAUDE.md §3.2 and §3.3 have to be checked
  against it, not against a docstring.
- `src-tauri/src/crash/mod.rs` — `install`, `on_panic`, `report_path`, `show_dialog`, to establish
  what a panic before `setup` actually does to the user.
- `src-tauri/src/main.rs`, `src-tauri/src/video/player.rs` (`SAFE_OPTIONS`, `validate_path`),
  `src-tauri/src/video/mod.rs` and the N2c surface/region diff, to establish whether anything on
  the new argv path can write.
- `src/hooks/useStartupFiles.ts`, `src/App.tsx`.

Harness, for the "writes into `fixtures/`" question and for writes the user did not ask for:

- `e2e/scripts/close-gate-check.js`, `e2e/scripts/n1b-load-probe.js`,
  `e2e/scripts/scaled-surface-check.js`, `e2e/scripts/wayland-attach-check.js`,
  `e2e/scripts/real-session-check.mjs`, `e2e/lib/env.js`, `e2e/lib/input.js`.

Dependency sources, read rather than trusted (review-prompt.md, and the shared brief):

- `tauri-runtime-wry-2.11.4/src/lib.rs:2277-2290`, `:4368-4373`, `:4438-4467` — to establish that
  `window.close()` is an asynchronous proxy message that re-enters the handler as
  `RunEvent::WindowEvent::CloseRequested`, and that `window.destroy()` (the pre-range call) went
  straight to `on_window_close` with no `CloseRequested` at all. This is what makes the `CLOSING`
  arm reachable and what makes finding 2 a live decision rather than a hypothetical.
- `gtk-0.18.2/src/auto/dialog.rs:614-639` — `connect_response` installs an
  `unsafe extern "C" fn response_trampoline` with no `catch_unwind`, which is what makes finding 3
  a process death rather than a caught error.

One experiment, in the scratchpad, no repo file touched: a five-line Rust program that collects
`std::env::args()` and is handed a non-UTF-8 argument. Result: `rc 101`,
``panicked at library/std/src/env.rs: called `Result::unwrap()` on an `Err` value``. That is
finding 1's mechanism, confirmed rather than recalled.

The battery baseline in `docs/reports/gate2-battery-baseline.md` was read first. I ran no part of
the suite and changed no file except this report.

---

## 2. Findings, most severe first

### F1 — blocker — a non-UTF-8 argument kills the app before it opens a window

**`src-tauri/src/lib.rs:75`**, with the iteration at **`src-tauri/src/lib.rs:43-45`**.

```rust
.manage(startup_files(std::env::args()))
```

`std::env::args()` panics during iteration if any argument is not valid Unicode. `startup_files`
iterates it. So the panic happens inside the `tauri::Builder` chain, before `.build()`, before
`.setup()`, and therefore before `crash::attach` has set `APP`.

**How it fails.** A Linux filename is a byte string, not UTF-8. Legacy media collections carry
Latin-1 and Shift-JIS filenames routinely — exactly the material a subtitle translator works on.

1. The user runs `sublore /media/série.srt`, where `série.srt` is Latin-1 on disk (`\xe9`), by
   typing it, by shell completion, or by dropping it on a launcher.
2. `startup_files` iterates `std::env::args()` and panics on the second argument.
3. `crash::install` (`lib.rs:68`) has already replaced the panic hook, so `on_panic`
   (`crash/mod.rs:80-100`) runs: it appends a report to `std::env::temp_dir()/sublore-crash.log`,
   then `show_dialog` returns immediately at `crash/mod.rs:164-166` because `APP` has not been set
   yet, and `crash/mod.rs:99` calls `std::process::exit(101)`.
4. The stderr line that would explain it is behind `#[cfg(debug_assertions)]`
   (`crash/mod.rs:89-92`), so a release build prints nothing.

The user double-clicks or types a command and the app exits silently with status 101. There is no
window, no dialog, no message, and the only trace is a file in the temp directory they have no
reason to look in. The file is untouched, so this is not lost work — it is a crash on the primary
platform, which the plan's severity rule calls a blocker on its own.

**New in this range.** `git grep env::args f0b0058 -- src-tauri crates` finds only
`crates/sublore-asr/src/bin/fake_whisper.rs`; at `eca9806` it also finds `lib.rs:75`. `062f201`
introduced it.

**Confirmed, not assumed.** The scratchpad program above reproduces exit 101 on `/tmp/\xe9pisode.srt`.

**Recommended correction.** `std::env::args_os()`, and classify on `OsStr`. If a `String` is wanted
for the IPC payload, `args_os().skip(1).filter_map(|a| a.into_string().ok())` at least degrades to
"that file is ignored" instead of "the app does not start" — but silently dropping the very file
the user named is its own defect, so the honest version keeps the `OsString`, and reports a path
it cannot represent rather than dying on it.

---

### F2 — blocker — an edit committed while the gate's save is in flight is closed away without a gate

**`src-tauri/src/lib.rs:138-141`** (the `CLOSING` arm), with
**`crates/sublore-edit/src/session.rs:130-132`** (`mark_saved` does not bump the revision) and
**`src-tauri/src/dialog.rs:68`** (the dialog is destroyed before the answer is acted on).

The `CloseRequested` chain is:

```rust
if CLOSING.swap(false, Ordering::SeqCst) { asr::shutdown(...); shutdown_video(...); }
else if unsaved_work(app_handle) { api.prevent_close(); ... }
else { ... }
```

A consumed `CLOSING` skips the dirty check entirely. The flag is set at `lib.rs:303` by a closure
posted from the answer thread, and `window.close()` at `:304` is an asynchronous proxy message
(`tauri-runtime-wry-2.11.4/src/lib.rs:2277-2281`), so `CloseRequested` arrives at least one event
loop turn later. Between the answer being acted on and that arrival the webview is alive and the
session is writable.

**How it fails.**

1. The session is dirty. The user clicks the X. The gate dialog goes up (`lib.rs:144-150`).
2. The user clicks **Save**. `dialog.rs:63` takes the answer, `dialog.rs:68` destroys the dialog —
   which releases the GTK modal grab, so the main window becomes interactive again — and
   `dialog.rs:77` spawns the detached thread that will do the work.
3. That thread runs `save_open_file` → `subtitle::save_current`, which takes the blocking lock and
   runs `save_with_backup`: a full copy of the old file into the backup store, then a temp file,
   `sync_all`, `rename`, `sync_dir`. Two whole-file I/Os and two fsyncs. On a large ASS file, a
   network share, or an encrypted home directory this is measurable in seconds, not microseconds.
4. During step 3 the user edits a cue and presses Enter. `subtitle_set_text` →
   `apply_edit` → `lock(slot)` blocks behind the save.
5. The save finishes. `save_locked` calls `session.mark_saved()`, which delegates to
   `history.mark_saved()` and **does not touch `revision`** (`session.rs:130-132`, versus
   `commit`'s `self.revision = self.revision.saturating_add(1)` at `session.rs:140`). The guard
   drops.
6. The blocked `apply_edit` now acquires the lock. `check_revision` passes, because the frontend's
   revision is still current — the gate's save was invisible to it. The edit applies. The session
   is **dirty again**, and the frontend has been told so: the command returns a `CuePatchDto` with
   `dirty: true`.
7. The posted closure runs on the main thread: `CLOSING.store(true)`, `window.close()`.
8. `on_close_requested` (`tauri-runtime-wry-2.11.4/src/lib.rs:4438-4466`) emits `CloseRequested`.
   `CLOSING.swap(false)` returns true. **The dirty check at `lib.rs:144` is not reached.** The
   window closes and the app exits.

The user's committed edit is gone. No dialog, no warning, no log line. The same happens with
**Discard** if the user starts editing again before the window dies, and there it is worse, because
after a discard the session is empty and the edit lands on a fresh document that is then thrown
away.

**Honest qualification, because it changes the fix and not the severity.** Before `2b31f14` the
same edit was also lost: `close_window` called `window.destroy()`, which
`tauri-runtime-wry-2.11.4/src/lib.rs:4371-4373` routes straight to `on_window_close` with no
`CloseRequested` and therefore no dirty check either. So this is not a regression that `2b31f14`
introduced. It is a hole that `2b31f14` moved into a place where closing it costs three lines, and
then did not close: the new structure has a dirty check on that very path and steps over it. The
comment at `lib.rs:136-137` says the flag exists so it can "wave through exactly one request" —
what it does not say is that the one request it waves through is allowed to carry away work
committed after the answer.

**Recommended correction.** In the `CLOSING` arm, re-check `unsaved_work` before letting the close
through: if the session went dirty again since the answer, `api.prevent_close()`, clear
`GATE_OPEN`, and let the gate ask again. That is the only branch where the session can have changed
under the answer, and asking twice costs a click while not asking costs the work — which is the
argument `unsaved_work`'s own docstring at `lib.rs:199-200` already makes for the `Unknown` case.

---

### F3 — serious — the one operation on the answer path that can panic does so inside a GTK C trampoline, after the answer has been consumed

**`src-tauri/src/dialog.rs:77`**, reached from `dialog.rs:62-78`.

```rust
std::thread::spawn(move || deliver(answered));
```

`std::thread::spawn` panics if the OS refuses the thread; `std::thread::Builder::spawn` is the
form that returns `io::Result`. This crate already uses the `Builder` form everywhere else it
creates a thread — `video/player.rs:210-220` and `crash/mod.rs:183-187` both name the thread and
handle the error — so this is also the odd one out against the surrounding pattern that CLAUDE.md
§6 asks new code to follow.

**How it fails.**

1. The process is at its thread limit: `RLIMIT_NPROC` reached, a container `pids.max` hit, or
   memory pressure with a whisper sidecar and the mpv event thread already running. This is not
   exotic on the machine of someone transcribing a series while doing something else.
2. The session is dirty, the gate is up, the user clicks **Save**.
3. `dialog.rs:63` takes the answer out of the `RefCell` — **the single-use closure is now
   consumed and cannot be delivered by any later response**.
4. `dialog.rs:68` destroys the dialog, so there is no longer anything on screen to answer.
5. `dialog.rs:77` panics. `gtk-0.18.2/src/auto/dialog.rs:615-628` shows the closure is invoked from
   `unsafe extern "C" fn response_trampoline`, with no `catch_unwind` anywhere between. The panic
   hook runs first and `crash/mod.rs:99` calls `std::process::exit(101)`; had it not, unwinding out
   of an `extern "C"` frame aborts.

The user clicked Save and the process died with the document never written and the backup never
taken. Because step 3 already consumed the answer, there is no recovery path even in principle: the
dialog is gone and the closure is gone.

The narrower non-fatal cousin of the same defect: if `deliver` itself panics inside the detached
thread — anywhere in `save_current`, `save_with_backup`, or the edit engine — the thread dies
without ever reaching `close_window` or the `GATE_OPEN.store(false)` at `lib.rs:231`. `GATE_OPEN`
stays `true` for the life of the process, so `lib.rs:148`'s `swap(true)` returns true on every
later close and no gate is ever raised again, while `lib.rs:145` prevents each close. The user's
window stops responding to the X button with no explanation, and the only way out is killing the
process, which takes the unsaved work with it. I could not construct a specific panic inside
`deliver` from the current tree, so I record that half as a **suspicion** and the `spawn` half as a
finding: `spawn` panicking needs no bug at all, only a busy machine.

**Recommended correction.** `std::thread::Builder::new().name("sublore-close-answer").spawn(...)`,
and on `Err` deliver `CloseAnswer::Cancel` inline (or clear `GATE_OPEN` and report) so the window
stays open with the work intact instead of the process dying. Wrap the body of `deliver` in
`catch_unwind` so a panic there also ends in "window stays, gate re-armed" rather than in a
permanently ungated, unclosable window.

---

### F4 — serious — a committed script types into whatever window holds focus on the owner's live desktop

**`e2e/scripts/real-session-check.mjs:123-126`** and **`:139`**, with `e2e/lib/input.js:50-52`.

```js
focusWindow(live.id);
clickIn(live, POINTS.videoField);
typeText(FIXTURE);
clickIn(live, POINTS.videoOpen);
```

`typeText` is `xdotool type` with no `--window` (`e2e/lib/input.js:51`), i.e. XTEST synthetic input
that follows the X input focus. `clickAt` (`input.js:28-38`) moves the real pointer to root
coordinates and clicks. The script runs on the owner's live KWin/XWayland session — its own header at
`real-session-check.mjs:1-18` says so, describing rootless XWayland, a compositor screenshot and
"the window opens behind whatever is already there" — where `XSetInputFocus` is a request the
compositor is free to override, and where any notification, screen locker or window activation
between `:123` and `:126` redirects the keystrokes.

**How it fails, and it already has.** `docs/reports/n2b-collaudo-reale.md:59` records it in the
repo's own words: on a live compositor xdotool typing goes to whichever window holds the X focus,
"and it landed **in the owner's own window** during the first attempt". The payload here is a
47-character absolute path plus two pointer clicks at computed coordinates. Landing in a text
editor with an unsaved buffer, a terminal, or a chat window, that is an uncommanded modification of
a document Sublore has nothing to do with — and the two clicks can land on anything, including a
control that commits it.

The sharp part is that `062f201` added **both** the WORKFLOW §4c rule ("synthetic input … never
used on the owner's real display"; on the real display only launching, passing files as arguments,
and capturing the window) **and** this script, which does the forbidden thing, on the forbidden
display, for a file it could have passed as an argument — `startup_files` was added in the same
commit precisely so it would not have to. The script even spawns with `[]` at `:103`.

L9 owns the rule violation. I file it here because from the data-safety angle it is the one thing
in this range that can alter data outside Sublore's own files, and CLAUDE.md §3.5 says Sublore
touches only files the user explicitly opened and its own project folder — a committed harness
script that types into arbitrary windows is the sharpest available violation of that spirit.

**Recommended correction.** Pass the fixture as `spawn(binary, [FIXTURE])` at `:103` and delete
`:124-126` outright; that is what `startup_files` exists for. For `:139` (the transport click) there
is no argv equivalent, so either the run stops before it and says so, or the script moves to Xvfb
and stops claiming to measure the real session.

---

### F5 — minor — the gate reports "save failed" for a document that was closed, and refuses to close

**`src-tauri/src/lib.rs:252-263`**, with `src-tauri/src/subtitle/mod.rs:543-549`.

`save_open_file` treats every `Err` from `save_current` as a failed save: it logs, raises an error
dialog through `report_save_failure`, and returns `false`, which keeps the window open. But
`save_current` reaches `current(&mut guard)?` (`mod.rs:448`), which returns
`SubtitleErrorCode::NoDocument` when the slot is `None` — a state that means _there is nothing to
save_, not _the save failed_.

**How it fails.** The gate can open on `SessionState::Unknown`, which is a merely busy lock
(`mod.rs:413-421`). Suppose the lock is busy because `subtitle_close(discard: false)` is running on
a clean document — the user hit "close document" and the X within the same instant. The gate opens,
the user clicks Save, `save_current` blocks, gets the lock after `close_session` set `*guard = None`,
and returns `Err(NoDocument)`. The user is shown a dialog saying their save failed, the window
refuses to close, and nothing was ever at risk. Two lines later, the same "nothing to write" state
arriving as `Ok(None)` closes cleanly (`lib.rs:256-257`).

No data is lost — the direction of the error is the safe one — which is why this is minor and not
worse. It is still a false alarm on the one dialog the user must be able to trust.

**Recommended correction.** Match `NoDocument` alongside `Ok(_)` and close, or better, have the
gate's save distinguish "nothing to write" from "the write failed" in its return type instead of
folding both into `bool`.

---

## 3. Hunt items I found sound, and why

Rule 4 of the brief: these are checked, not skipped.

**`save_open_file`'s `Ok(_) => true` on the `Unknown`-lock path (hunt item 1).** Traced all four
resolutions of a gate that opened on a busy lock. `save_current` takes the _blocking_ lock, so it
waits out whatever held it and then reads the real state. Dirty → written. Genuinely clean →
`Ok(None)` → close, and closing is right, there is nothing on disk to protect. Concurrently mutated
→ the mutation lands first, `save_current` sees dirty, writes it. Session removed → `Err(NoDocument)`,
which is F5 and is not a loss. The one ordering that does lose work is the mutation landing
_after_ the save, and that is F2, filed against the flag rather than against this function.

**`discard_open_file` returning `true` after `close_session` errored (hunt item 2).** With
`discard: true`, `close_session` (`mod.rs:320-330`) skips the dirty check, so the only way it
returns `Err` is `lock(slot)` failing on a poisoned mutex (`mod.rs:534-541`). The outcome is: the
session is left in place and still dirty, `true` is returned, `CLOSING` is set, the window closes.
That is loss — of edits the user explicitly asked to lose, one dialog earlier. The docstring at
`lib.rs:278-279` states exactly that contract and the code matches it. Sound. The asymmetry with
`save_current` (which does recover poison, deliberately) is correct: save recovers because it is the
last chance to _keep_ the work, discard has nothing to keep.

**Atomicity and backup on the gate's save path (hunt item 3, CLAUDE.md §3.2 and §3.3).** Read
`crates/sublore-io/src/atomic.rs` rather than the docstrings. `save_with_backup:60-77` archives the
existing destination _before_ touching it and aborts the whole save if the archive fails.
`replace:148-164` creates a temp file in the destination's own directory with `create_new`
(`create_temp:233-254`, so an existing name is never opened or truncated), writes, `sync_all`
including the length (`fill:186-189`, `sync_all` at `:188`), renames, then `sync_dir`. `Target::resolve:88-132` refuses a
non-regular destination, and `resolve_symlink:137-144` writes _through_ a symlink instead of over
it. §3.2 and §3.3 hold.

**The detached save thread versus the main loop reaching `Exit` (hunt item 3, second half).** The
window cannot close underneath an in-flight save: while the answer thread holds the session mutex,
`unsaved_work` → `session_state` → `try_lock` fails → `SessionState::Unknown` → not `Clean` →
`api.prevent_close()` at `lib.rs:145`. And if the process is killed outright mid-write (logout,
SIGKILL, the N1b exit crash), the atomic sequence above means the destination is either entirely
the old file or entirely the new one; the worst residue is a `.sublore-tmp-<pid>-<n>` file next to
the user's subtitle, which `atomic.rs:18-20` names as a deliberate, self-describing trade. No
truncation is reachable. This is the part of the brief I most expected to break and it does not.

**`save_current`'s poisoned-lock recovery — my named most-likely false positive
(`subtitle/mod.rs:441-447`).** The brief required that a finding here defeat the docstring's
argument at `:423-436` rather than restate that poison recovery is risky. I went to check whether
the argument survives `EditSession::apply`, and it does, but not for the reason the docstring
gives. `apply` (`session.rs:78-89`) calls `self.history.record(...)` **before** `self.commit(...)`,
so "history is only touched after the new document exists" is true only of `edited.document`, not
of `self.document`. The guarantee that actually matters is the one in `commit`
(`session.rs:135-141`): `self.document = document` is a single move that cannot panic, so a panic
anywhere in the mutation leaves `self.document` holding one whole document or the other, and
`to_bytes()` therefore always writes a whole file. A panic inside `diff::views` after that move
leaves `views` and `revision` stale while `document` is new, but `views` and `revision` are the
UI's model and the UI is about to be destroyed; the bytes written are still a whole document
containing the user's own edit. **I am not filing this.** The conclusion holds; the sentence
supporting it is imprecise, which belongs to L12 if anywhere.

**`startup_files` and disk mutation before an explicit save (hunt item 4).**
`open_session` (`mod.rs:283-317`) refuses outright while the open file is dirty, then reads through
`read_document` (`mod.rs:631-668`), which does `fs::metadata`, a size check, and `File::open` — read
only, no create, no truncate, no lock file, no backup. Nothing is written when a subtitle is opened.
The subtitle stays read-only until the user saves. §3.1's spirit holds on the new argv path.

**Everything non-subtitle becoming `files.video` (hunt item 5).** Traced whether a misrouted file
can be written. It cannot. `video_open` → `Player::open` → `validate_path`
(`video/player.rs:465-477`) requires `is_file()` — which is false for FIFOs, device nodes and
directories — and canonicalises. libmpv is initialised with `SAFE_OPTIONS`
(`video/player.rs:33-51`): `config=no`, `load-scripts=no`, `save-position-on-quit=no`,
`resume-playback=no`, `watch-later-options=""`, `sub-auto=no`, `audio-file-auto=no`,
`access-references=no`. So mpv reads the user's `~/.config/mpv` not at all and writes no
watch-later state. A `.txt`, a `.str` typo or a stray argument produces an error, never a write.
The same `is_file()` in `startup_files` (`lib.rs:45`) already excludes FIFOs and device nodes before
the frontend sees them. Sound — and worth saying plainly, because "we hand an arbitrary argv entry
to a media player" is the shape of a much worse defect than the one that is actually there.

**The harness never writing into `fixtures/` (hunt item 6).** `close-gate-check.js:246-252` and
`:311-313` read the fixture and `copyFileSync` it into a fresh `mkdtempSync` directory, then launch
the app on the copy; the original is only ever `readFileSync`'d, as the comparison baseline.
`n1b-load-probe.js:59-64` does the same. `scaled-surface-check.js:73` and
`wayland-attach-check.js:91` pass `fixtures/video/sample.mkv` **directly**, not a copy, but that is
a video and the mpv lockdown above means it cannot be written; `real-session-check.mjs:43` does the
same, for the same file. No script writes into `fixtures/`.

Two notes attached to that, neither a finding: (a) the safety of passing the video fixture directly
rests entirely on the classification in `lib.rs:47-56` sending it to mpv rather than to the subtitle
session — the day someone points one of those scripts at a `.srt` fixture without copying it, a save
overwrites a repo fixture; (b) every launcher isolates `XDG_DATA_HOME` into a temp directory
(`close-gate-check.js:146`, `n1b-load-probe.js:70`, `scaled-surface-check.js:76`,
`wayland-attach-check.js:94`, `real-session-check.mjs:108`, `wdio.conf.js:21`), so no run writes
backups, logs or crash reports into the developer's real `~/.local/share/com.sublore.app`. `appEnv`
itself (`e2e/lib/env.js:18-31`) does **not** set `XDG_DATA_HOME`, so that isolation is each caller's
responsibility and currently every caller honours it.

**One more, outside my hunt list but on my subject, so it is recorded rather than filed:**
`real-session-check.mjs:71-99` captures the **whole composited desktop** with `spectacle -f` into
`/tmp`, and `rmSync(full)` at `:97` runs only after the saturation regex matched. On the `throw` at
`:94` a full-resolution screenshot of everything on the owner's screen stays in `/tmp`, and the
cropped PNG at `:83` is never deleted on any path. That is data written that nobody asked for, but
it is L9's finding by the plan's ownership table and I do not re-file it.

---

## 4. What this lens could not see

- Nothing here was executed. F2's window is a race whose width depends on the filesystem under the
  user's subtitle; I argue it from the code and from the wry source, not from a run. F3's trigger is
  a resource exhaustion I did not induce.
- F1 is the only finding whose mechanism I confirmed by running something, and what I ran was a
  five-line program in the scratchpad, not Sublore.
- The Windows halves of `dialog.rs` (`:83-121`, `:146-156`) were read for the data-loss question
  only. `ask_close`'s Windows twin delivers through `show_with_result` and cannot return `Err`, so
  `ask_before_closing`'s error branch at `lib.rs:234-240` is unreachable there; that is L4's and
  L6's, and neither half has ever been executed anywhere.
