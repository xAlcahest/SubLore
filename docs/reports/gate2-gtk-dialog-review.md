# Gate 2 — L6: `dialog.rs`, thread ownership and object lifetime

Lens L6 of gate 2. Scope `GATE_BASE=f0b0058` .. `GATE_HEAD=eca9806`, the whole of
`src-tauri/src/dialog.rs` (arrives whole in `fee26f8`) plus its two callers in `src-tauri/src/lib.rs`.

**Question:** is `src-tauri/src/dialog.rs` sound about which thread owns each GTK object and how long
each lives, on the platform where it actually runs?

**Verdict, one line:** the GTK mechanics are sound on Linux — the destroy-inside-the-handler is
correct, no GTK object crosses a thread, no nested main loop is entered, and the deadlock the module
was written to avoid really is absent. What is not sound is the _delivery_ of the answer: the
callback can be dropped without ever being called on both branches, the thread that carries it is
spawned with a call that panics rather than returns an error, and two of the comments that carry the
threading argument state a mechanism the dependency sources refute.

Everything behavioural below is **verified on Linux** (Fedora, GTK 3.24.52, gtk-rs 0.18.2). The
`#[cfg(not(target_os = "linux"))]` halves have never been executed anywhere; they are read, not run.

---

## What I checked

Read whole, line by line:

- `src-tauri/src/dialog.rs`, all 156 lines, both `cfg` branches.
- `src-tauri/src/lib.rs:128-166`, `:189-241`, `:245-335` — every call site of `ask_close` and
  `report_error`, and the thread each one runs on.
- `src-tauri/src/crash/mod.rs:80-99`, `:113-125`, `:174-195` — what happens to a panic on a spawned
  thread, and what the crash report records about it.
- `src-tauri/src/subtitle/mod.rs:389-451` and `crates/sublore-io/src/atomic.rs:1-76` — how long the
  delivery thread blocks and whether an abrupt exit during it can tear a file.

Dependency sources read rather than trusted (review-prompt rule):

- `gtk-0.18.2/src/widget.rs:226-238` — the `WidgetExt::destroy` safety contract.
- `glib-0.18.5/src/signal.rs:68-90` — how `connect_raw` owns the `Box<F>` and when it is freed.
- `tauri-2.11.5/src/app.rs:495-500` and `tauri-runtime-wry-2.11.4/src/lib.rs:196-255`, `:3335`,
  `:3432`, `:2117-2119` — what `run_on_main_thread` does, especially when the caller _is_ the main
  thread.
- `tauri-plugin-dialog-2.7.2/src/desktop.rs:215-250` and `src/lib.rs:286-349` — the Windows twin's
  real contract.
- `rfd-0.16.0/src/backend/gtk3/utils.rs:7-50` and `src/backend/win_cid/message_dialog.rs:242-247` —
  the claim the module's own doc makes about rfd, and what rfd does on Windows.

Ran, read-only, against the running system:

- `pkg-config --modversion gtk+-3.0` → 3.24.52; `gtk3-3.24.52-2.fc44` installed.
- `gresource extract /usr/lib64/libgtk-3.so.0.2420.32 /org/gtk/libgtk/ui/gtkmessagedialog.ui` — to
  settle whether `show_all()` on a `GtkMessageDialog` reveals widgets GTK keeps hidden.
- `git show f0b0058:src-tauri/Cargo.toml` and `git diff f0b0058 eca9806 -- src-tauri/Cargo.toml` —
  to confirm `gtk`/`gdkx11` predate the range.

Nothing was modified. The battery was not re-run; `docs/reports/gate2-battery-baseline.md` records
it green at `GATE_HEAD`, and nothing here contradicts that.

---

## Findings

### 1. `std::thread::spawn` panics instead of failing, at the moment the user has just said "keep my work" — **serious**

`src-tauri/src/dialog.rs:77`

```rust
std::thread::spawn(move || deliver(answered));
```

`std::thread::spawn` panics if the OS refuses the thread; only `std::thread::Builder::spawn` returns
`Err`. This call runs inside the GTK response handler, on the main thread, **after** the user chose
Save and **before** anything has been written. The project's own panic hook
(`src-tauri/src/crash/mod.rs:80-99`) does not unwind and leave the app running: it writes the crash
report and calls `std::process::exit(101)` at `:99`.

**Failure:** the user clicks Save on the close gate while the process is at its thread limit
(`RLIMIT_NPROC`/`pthread_create` returning `EAGAIN`, or memory exhausted — a long transcription with
its sidecar and the webview's own thread pool is the realistic shape). `spawn` panics, the hook
exits the process with code 101, and the edits the user just asked to save are never written. The
window's answer was "keep my work" and the outcome is that the work is gone. CLAUDE.md §3's failure
budget says a bug may cost annoyance, never data.

The codebase already knows the right pattern: `crash/mod.rs:183` uses
`std::thread::Builder::new().name(...).spawn(...)` and branches on `is_ok()`.

**Fix:** `Builder::new().name("sublore-close-gate").spawn(...)`, and on `Err` deliver the answer
some other way (or at minimum leave the window open, clear `GATE_OPEN`, and report through
`report_error`) instead of taking the process down.

Confidence: certain about the mechanism, likely about reachability — I did not reproduce thread
exhaustion.

### 2. On Windows `ask_close` cannot report a failure that leaves nobody ever asked — **serious**

`src-tauri/src/dialog.rs:120` (the `Ok(())`), against the contract stated at `src-tauri/src/dialog.rs:26`

The doc says: _"An error means nobody will ever be asked, so the caller has to keep the window open
and say so."_ `ask_before_closing` (`src-tauri/src/lib.rs:234-240`) is built entirely on that: the
only thing that clears `GATE_OPEN` when no answer will arrive is `ask_close` returning `Err`.

The Windows branch returns `Ok(())` unconditionally, and the delivery underneath it can be dropped:
`tauri-plugin-dialog-2.7.2/src/desktop.rs:222` discards the post with `let _ =`.

```rust
let _ = handle.run_on_main_thread(move || { ... std::thread::spawn(...) });
```

If that post fails — `Error::FailedToSendMessage` from
`tauri-runtime-wry-2.11.4/src/lib.rs:250-253`, i.e. the event-loop proxy is gone — the closure never
runs, the callback `F` is dropped without ever being invoked, and the plugin says nothing.

**Failure (Windows, never executed anywhere):** the gate opens, no dialog appears, no answer is ever
delivered, `ask_close` returns `Ok`, `GATE_OPEN` (`src-tauri/src/lib.rs:148`) stays `true` for the
life of the process. Every later close request hits `api.prevent_close()` and then finds the flag
already set, so no dialog is ever raised again. The window becomes unclosable and silent; the user's
only exit is killing the app, which loses the unsaved edits.

**Fix:** make delivery a property of the type rather than of the reachable paths — wrap the callback
in a guard whose `Drop` delivers `CloseAnswer::Cancel` if it was never called. That closes this and
finding 3 with one change and needs no Windows run to be correct.

### 3. The answer callback can be dropped without delivering on Linux too, and nothing notices — **serious**

`src-tauri/src/dialog.rs:46` (`DESTROY_WITH_PARENT`) and `src-tauri/src/dialog.rs:61-78`

Two ways `F` reaches its destructor without ever being called:

- **`DESTROY_WITH_PARENT`.** If GTK destroys the dialog because its parent died, `response` is not
  emitted — `gtk_widget_destroy` does not synthesise one. `answer` (`:61`) is still `Some`, and the
  `Box<F>` is freed when the closure finalises, dropping the callback unread.
- **A queued task that never runs.** `run_on_main_thread` returning `Ok` only means the message was
  accepted (`tauri-runtime-wry-2.11.4/src/lib.rs:250-253`); if the loop exits before
  `Message::Task` is dispatched, the boxed closure — and `F` inside it — is dropped.

In both cases `GATE_OPEN` stays `true` and the gate can never be raised again, exactly as in
finding 2.

**Failure:** I could not construct a trigger in the app as it stands. The only code that destroys the
main window is `close_window` (`src-tauri/src/lib.rs:296-317`), which runs _after_ the answer; a
second X-click while the gate is up is stopped by `prevent_close` and the flag, so it does not reach
a destroy. The second path only fires while the app is already exiting, where it costs nothing. I am
reporting it as a **suspicion**, not a demonstrated failure — and noting that it stops being a
suspicion the moment a second window exists, which decision 1 (owner-moved into M2.0 as T3) is
about, or the moment anything else can close the main window programmatically. A single-use
mechanism whose "exactly once" holds only because no current caller violates it is the class of
defect the gate exists to catch.

**Fix:** same guard as finding 2.

### 4. The comment justifying the worker thread states a mechanism the dependency source refutes — **minor**

`src-tauri/src/dialog.rs:74-76`

```rust
// Off the main thread, because acting on the answer writes a file and the main loop is
// the one thing that must not block: `close_window` posts back to it and would wait on
// itself.
```

`close_window` would **not** wait on itself. `AppHandle::run_on_main_thread`
(`tauri-2.11.5/src/app.rs:495-500`) goes to `send_user_message`
(`tauri-runtime-wry-2.11.4/src/lib.rs:235-255`), which short-circuits when the caller is already the
main thread and runs the message inline; `Message::Task(task) => task()` at `:3335`. Called from the
main thread it executes the closure immediately and returns. There is no self-post and no deadlock.

The decision to spawn is still right, for a different reason the file does not give: `save_current`
(`src-tauri/src/subtitle/mod.rs:437-441`) takes a **blocking** `slot.lock()` — the same lock
`session_state` at `:413-421` refuses to block on, with its own comment saying that waiting there
"would freeze the window mid-save (CLAUDE.md §7)" — and then writes a file. Doing that inside the GTK
response handler would hold the main loop for the whole save.

**Failure:** a maintainer tests the stated deadlock, does not find one (because there is not one),
concludes the comment is stale, and moves the delivery back onto the main thread. The gate then
freezes the window for the length of a save on a slow disk or a contended lock — a §7 responsiveness
regression that the comment was supposed to prevent.

The same wrong premise shows up one comment earlier: `:25` says `ask_close` "returns as soon as the
dialog is on its way". Its only caller is `ask_before_closing`, reached only from the
`CloseRequested` arm (`src-tauri/src/lib.rs:144-150`), which runs on the main thread — so the
closure runs _inline_ and the function actually returns after the dialog is up.

**Fix:** replace the deadlock claim with the real one (a blocking lock plus a file write must not run
on the main loop), and correct `:25`.

### 5. `report_error` does have a main-thread caller, which its own doc says it does not — **minor**

`src-tauri/src/dialog.rs:123-126` and the call site `src-tauri/src/lib.rs:307`

The doc says: _"both callers reach here while the window they would parent to is either closing or in
a state they are refusing to close"_, and `src-tauri/src/lib.rs:266-267` adds that blocking here
"would be the deadlock `ask_before_closing` exists to avoid". Both are written as if `report_error`
is always reached off the main thread.

`report_close_failure` at `src-tauri/src/lib.rs:307` is called from **inside** `close_window`'s
`run_on_main_thread` closure (`:298-317`) — that is the main thread. So `report_error` posts to the
thread it is already on.

**Failure:** none observable today; the inline short-circuit of finding 4 makes it benign, and the
dialog is simply built immediately. The defect is that a documented invariant is false as shipped,
in the one module whose entire justification is a threading argument. If the short-circuit ever
changes, or someone substitutes a blocking marshal, this line becomes a main-thread self-post from
inside a GTK dispatch.

**Fix:** say plainly that `lib.rs:307` is on the main thread and that `run_on_main_thread` runs
inline there, or hoist `report_close_failure` out of the main-thread closure so the doc becomes true.

### 6. Both parent lookups fail into `None` silently, degrading to the exact behaviour the module exists to escape — **minor**

`src-tauri/src/dialog.rs:41-43`, and the Windows twin at `src-tauri/src/dialog.rs:103-105`

```rust
let parent = handle
    .get_webview_window(&label)
    .and_then(|window| window.gtk_window().ok());
```

Two failures are swallowed with no log. `gtk_window()` is a channel round trip
(`tauri-runtime-wry-2.11.4/src/lib.rs:2117-2119`, `:3432`, macro at `:196-211`): if the runtime's
window map no longer holds the id, nothing is ever sent, the sender drops, and `getter!` maps it to
`FailedToReceiveMessage`. `.ok()` turns that into `None`.

A `None` parent means the dialog is built with a null parent — which the module doc at `:3-7` and the
commit message name as _the_ rfd limitation this module was written to fix — while `MODAL` at `:46`
is still set, so it becomes application-modal.

**Failure:** the window is dropped from the runtime map between `unsaved_work` reading the session
(`src-tauri/src/lib.rs:144`) and the posted closure running; the user gets an application-modal
dialog that is not transient for anything, can sit behind the main window, and is not logged
anywhere. Reachability is narrow — both failures mean the window is essentially gone — but nothing
tells anyone it happened.

On Windows the same silent degradation happens twice over: the plugin's own `parent()`
(`tauri-plugin-dialog-2.7.2/src/lib.rs:290-303`) also swallows both handle errors with
`if let (Ok, Ok)`.

**Fix:** `log::error!` on the fallback. The parent is the module's stated reason for existing;
losing it should not be silent.

### 7. The module doc and commit claim rfd's GTK thread is removed; it is not removed from the process — **minor**

`src-tauri/src/dialog.rs:3-11`

The doc: _"The plugin uses rfd, which starts a second thread the first time any dialog is shown and
iterates GTK on it for the rest of the process's life… Removing that thread is worth doing on its
own"_. `fee26f8`'s message is flatter: the change "removes that thread".

It removes it from the close-gate path only. `tauri_plugin_dialog::init()` is still registered at
`src-tauri/src/lib.rs:73`, `project::choose_path` still raises plugin dialogs, and
`crash::show_dialog` (`src-tauri/src/crash/mod.rs:174-190`) goes through the plugin too. Any session
where the user picks a project path still gets rfd's `GtkGlobalThread`
(`rfd-0.16.0/src/backend/gtk3/utils.rs:7-50`, one thread calling `gtk_main_iteration` in a loop for
the life of the process).

**Failure:** a reader of this file concludes the second GTK thread is gone and reasons about GTK
thread affinity on that basis — for example when debugging N1b's exit crash, which is precisely what
this file is at pains to say it did not fix.

This is the documentation half only. N1c (rfd's second GTK thread in `project::choose_path`) is filed
and explicitly out of this gate's scope; it is **not** re-filed here.

**Fix:** one word — "removes that thread from the close gate".

### 8. The delivery thread is unnamed, and the project's own crash report says so — **minor**

`src-tauri/src/dialog.rs:77`

`format_report` records `thread.name().unwrap_or("<unnamed>")` at `src-tauri/src/crash/mod.rs:119`.

**Failure:** the save path panics (a poisoned-lock recovery gone wrong, an I/O panic), the hook writes
the crash report, and the one line that would say the panic came from the close gate's delivery reads
`thread: <unnamed>`. Every other long-lived thread the app spawns for a reason is named —
`crash/mod.rs:183` names `sublore-crash-dialog`.

**Fix:** the same `Builder::new().name(...)` that finding 1 needs. One change closes both.

---

## Hunt items I found sound, and why

1. **`unsafe { dialog.destroy() }` at `:68` and `:141`, from inside the widget's own signal handler.**
   Sound. The binding's contract (`gtk-0.18.2/src/widget.rs:226-235`) is: _"you must NOT query the
   widget's state subsequently"_. The handler does not touch `dialog` after the call — it matches on
   `response`, which is `Copy`, and `deliver` was already moved out of the `RefCell` on the line
   above. The captured environment cannot be freed under the running handler either:
   `glib-0.18.5/src/signal.rs:80-87` connects through `g_signal_connect_data` with
   `destroy_closure::<F>` as the destroy notifier, which `g_cclosure_new` installs as a **finalize**
   notifier on the GClosure; the closure is kept alive by the handler reference the emission holds,
   so disposing the widget inside its own handler defers the `Box<F>` drop past the handler's return.
   This is also the canonical GTK3 idiom. I did not take the inline comment's word for any of it.
   Exercised on Linux: `e2e/scripts/close-gate-check.js` drives all three answers (Save, Discard, and
   Cancel via Escape, `:118-133`) and is green 12/12 in the baseline.

2. **The `RefCell<Option<F>>` single-take at `:61-63`.** Sound. GTK emits signals on the thread that
   owns the widget, so no `Sync` is needed and no two handlers can run concurrently. The `let-else`
   temporary `RefMut` is dropped before `destroy()` is reached, so even a re-entrant emission during
   dispose could borrow again and would find `None` and return — and there is no such emission:
   `GtkDialog` produces `response` from button clicks and from delete-event, not from dispose.

3. **The deadlock the module was written to avoid — my named false positive — is genuinely absent.**
   I verified rather than reasoned from shape. `ask_close` is reached only from `ask_before_closing`,
   only from the `CloseRequested` arm (`src-tauri/src/lib.rs:144-150`), which runs on the main
   thread; `send_user_message` therefore takes the inline branch and the dialog is built and shown on
   the main thread. Nothing anywhere enters a nested GTK main loop — `gtk::MessageDialog::run()` is
   never called, only `show_all()` — so the event loop is never re-entered from inside a dialog. The
   commit's `ThreadId(1)` / `ThreadId(23)` measurement is consistent with the sources: the GTK main
   loop _is_ the tao main thread, and `std::thread::spawn` gives a fresh thread. Claim upheld.

4. **Thread ownership of every GTK object.** Sound. `MessageDialog::new`, `add_button`, `set_title`,
   `connect_response`, `show_all` and `destroy` all execute either inside a `run_on_main_thread`
   closure or inside a signal handler dispatched by the main loop. Nothing GTK crosses to the spawned
   thread: `deliver` captures an `AppHandle` and a `String` and nothing else, and `CloseAnswer` is a
   plain `Copy` enum.

5. **`show_all()` on a `GtkMessageDialog`.** Sound on this system, despite the general "do not
   `show_all` a dialog" caveat. I extracted `/org/gtk/libgtk/ui/gtkmessagedialog.ui` from the
   installed `libgtk-3.so.0.2420.32`: `secondary_label` carries `no-show-all=1` and the 3.24 template
   has no image child, so `show_all` cannot reveal a widget GTK deliberately keeps hidden.

6. **Escape and the window manager's close button both deliver an answer.** Sound. `GtkDialog` turns
   its `close` binding and the delete-event into a `response`, and the catch-all `_ =>
CloseAnswer::Cancel` at `:72` covers `DeleteEvent` and every unknown response.
   `close-gate-check.js:132-133` drives exactly that path deliberately, to stay clear of the button
   geometry.

7. **A half-finished save cannot be torn by an abrupt exit.** Sound, and it matters because the
   delivery thread is detached. The ordering is `save_open_file` first, `close_window` after
   (`src-tauri/src/lib.rs:222-228`), so the write has returned before any close is requested; and
   `save_with_backup` (`crates/sublore-io/src/atomic.rs:59-76`, `write_atomic` at `:34-36`) is temp
   file in the destination's own directory, fsync, rename, so even `process::exit` mid-write leaves
   the user's file untouched and at worst a `.sublore-tmp-` file behind. CLAUDE.md §3.2 and §3.3 hold
   on this path.

8. **No phantom dependency.** `gdkx11 = "0.18"` and `gtk = "0.18"` are at `src-tauri/Cargo.toml:47-48`
   already at `f0b0058`, and `git diff f0b0058 eca9806 -- src-tauri/Cargo.toml` is empty. The
   bindings pair correctly with the installed GTK3 (gtk-rs 0.18.2 targets 3.24.x; the system has
   3.24.52).

9. **The rfd claim in the module doc is accurate on its facts.**
   `rfd-0.16.0/src/backend/gtk3/utils.rs:7-50` does spawn exactly one permanent thread that
   `gtk_init_check`s and then loops on `gtk_main_iteration`, and every desktop dialog in the plugin
   routes through it. Only the "removed" part is overstated (finding 7).

10. **The Windows twin does not block the main loop.** rfd's `AsyncMessageDialogImpl` for `win_cid`
    (`rfd-0.16.0/src/backend/win_cid/message_dialog.rs:242-247`) runs the modal on a `ThreadFuture`
    thread, and the plugin blocks on that future inside its own `std::thread::spawn`
    (`tauri-plugin-dialog-2.7.2/src/desktop.rs:225-226`). Half the twin's contract — "never block the
    main loop" — is preserved; the other half, "deliver exactly once", is finding 2. Compiled in CI,
    never run (CLAUDE.md §5.5).

11. **No `unwrap`, `expect` or `panic!` anywhere in `dialog.rs`** (CLAUDE.md §6). The one panicking
    call is `std::thread::spawn` itself, which is finding 1.

---

_Written on Linux, against `GATE_HEAD=eca9806`. Every behavioural statement above is a Linux
statement. The `#[cfg(not(target_os = "linux"))]` halves were read, not run, here or anywhere._
