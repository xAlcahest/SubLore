# Gate 2 · L5 — the close path and the single-use `CLOSING` flag

Lens L5 of the gate-2 review (`docs/reviews/gate-2-plan.md` §2, named in advance by the owner).
Scope `GATE_BASE=f0b0058 .. GATE_HEAD=eca9806`, read with `git diff` and `git show`, not from any
description. Rules applied: CLAUDE.md §3, §5, §6, §9; WORKFLOW §4, §4a, §4b.

**Platform:** everything below is reasoning about Linux code plus reading of the dependency sources.
Nothing here was run. The Windows halves of `dialog.rs` are read, never executed, per §7 of the plan.

---

## What I checked

1. **`src-tauri/src/lib.rs:128-166`, `:189-241`, `:245-335` in full**, plus the whole of
   `src-tauri/src/dialog.rs` (156 lines, arriving whole in `fee26f8`), and the diff of `lib.rs`
   across the range (`git diff f0b0058 eca9806 -- src-tauri/src/lib.rs`).
2. **The state machine of `GATE_OPEN` × `CLOSING` × session-dirty**, enumerated below, with every
   clear site of both flags traced to the branch that reaches it.
3. **The dependency sources, read rather than trusted** (review-prompt.md is explicit about this):
   - `tauri-2.11.5/src/webview/webview_window.rs:2217` → `window/mod.rs:1794` → `dispatcher.close()`.
   - `tauri-runtime-wry-2.11.4/src/lib.rs:2274-2281`: `close()` deliberately bypasses
     `send_user_message` and posts `WindowMessage::Close` through the tao proxy, because
     `handle_user_message` **panics** on that message (`:3491-3493`). So `window.close()` returns
     `Ok` when the request is _queued_, never when the window is gone.
   - `:4368-4370` → `on_close_requested` (`:4438-4467`): it drops its `windows.0.borrow()` before
     invoking the callback, so re-entering `handle_user_message` from inside our handler cannot
     panic on a double borrow; and `on_window_close` (`:4469-4475`) only sets `inner = None` and
     leaves the map entry in place.
   - `:235-255` `send_user_message`: **when the caller is already the main thread the task is run
     inline and `Ok(())` is returned unconditionally.** This is load-bearing for findings 4 and 6.
   - `:3367-3376`, `:3432` `WindowMessage::GtkWindow`: nothing is sent on the channel when the
     window's `inner` is `None`, so `gtk_window()` errors for a window that is already closing.
4. **The single-use mechanisms against each other**: the `RefCell<Option<F>>` take at
   `dialog.rs:63-66` versus `CLOSING.swap` at `lib.rs:138`, including whether the `RefMut` is still
   alive when `dialog.destroy()` runs at `:68`.
5. **Ordering** between `CLOSING.store(true)` (`lib.rs:303`) and `window.close()` (`:304`), both
   interleavings against a user X-click already queued in the loop.
6. **The frontend side of the interval**: `src/components/CueList.tsx:348` (`onBlur={() => void
commit()}`), `subtitle/mod.rs:320-330` `close_session`, `:400-420` `session_state`, `:436-452`
   `save_current`, `:534-541` `lock`.
7. **The fix as shipped against its own account**: `docs/reports/n1b-sessanta-corse.md:63,65` and
   the `2b31f14` commit message, checked line by line against the tree.
8. **What automation covers it**: `e2e/scripts/close-gate-check.js:240-370` (all twelve checks) and
   a search for `#[cfg(test)]` in `lib.rs` and `dialog.rs`.

### The state machine, enumerated

`GATE_OPEN` is set only at `lib.rs:148`; cleared at `:231` (cancel / failed save), `:239`
(unreachable, finding 4), `:314` (window already gone) and `:327` (`report_close_failure`).
`CLOSING` is set only at `lib.rs:303`; cleared at `:138` (consumed) and `:305` (`close()` failed).

| GATE_OPEN | CLOSING | session         | reachable?                                                      | what happens on the next `CloseRequested`                                                                             |
| --------- | ------- | --------------- | --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| false     | false   | clean           | yes                                                             | `:151` shutdown, window closes. Correct.                                                                              |
| false     | false   | dirty           | yes                                                             | `:144` prevent, gate raised. Correct.                                                                                 |
| true      | false   | dirty           | yes                                                             | prevented, **no dialog raised**. Correct while the dialog is on screen; silently dead once it is not — **finding 2**. |
| true      | false   | clean           | yes (after a Save completes, before `close_window`'s task runs) | `:151` shutdown, window closes without consuming the flag. Benign: the process exits.                                 |
| true      | true    | clean           | yes                                                             | `:138` consumes, shutdowns run, window closes. The intended path.                                                     |
| true      | true    | **dirty again** | yes                                                             | `:138` consumes and **never consults `unsaved_work`** — **finding 1**.                                                |
| false     | true    | any             | no                                                              | `CLOSING` is only set at `:303`, which is reached only with `GATE_OPEN` still true.                                   |

---

## Findings, most severe first

### 1 — serious · `src-tauri/src/lib.rs:138` · the answered-gate arm skips the dirty check over an interval the dialog no longer covers

`if CLOSING.swap(false, Ordering::SeqCst)` waves the close through without ever calling
`unsaved_work`. The flag's justification is that the answer just made the session clean. That is
true at the instant the answer is acted on, and the code then leaves a live, editable window open
across a gap it does not close.

**How it fails.** Dirty document, user clicks X, gate appears, user clicks **Save**.

1. `dialog.rs:68` destroys the dialog _inside_ the response handler, dropping the modal grab. The
   webview is interactive and focused again from this instant.
2. `dialog.rs:77` spawns a detached thread; `save_open_file` (`lib.rs:245`) runs `backup_root` +
   `subtitle::save_current`, which serialises the document, writes a backup, writes a temp file,
   fsyncs and renames. Tens of milliseconds at best; seconds on a large file or a network mount.
   `save_locked` (`subtitle/mod.rs:389-398`) then marks the session **clean**.
3. `close_window` (`lib.rs:296`) posts to the main thread, arms `CLOSING` at `:303`, and calls
   `window.close()` at `:304`. Per `tauri-runtime-wry-2.11.4/src/lib.rs:2274-2281` that only
   **queues** `WindowMessage::Close` through tao's proxy, so at least one further main-loop
   iteration runs with the window alive and the flag armed.
4. Any edit committed between steps 1 and 4 re-dirties the session. This does not need an unusual
   user: `src/components/CueList.tsx:348` commits on `onBlur`, so returning focus to the cue list
   and leaving the field is enough, and `subtitle_set_text` blocks on the session mutex, so a commit
   issued during the save is _guaranteed_ to land after `mark_saved`, not before.
5. The queued `CloseRequested` arrives, takes `lib.rs:138`, and the window closes. The edit is gone
   with no gate, no dialog and no log line.

CLAUDE.md §3: "a Sublore bug may cost the user annoyance, never data."

**Honest note on novelty.** This is not a regression introduced by `2b31f14`. Before it,
`close_window` called `window.destroy()`, which emits no `CloseRequested` at all, so the dirty
check was skipped just as completely by a different mechanism. `2b31f14` moved the hole, it did not
open it. It is reported here because L5 owns the close path as it now stands and this range is the
first outside reading of it.

**Recommended correction.** Do not treat the flag as proof of cleanliness. Either re-run
`unsaved_work` inside the `CLOSING` arm and fall back to the gate when it says dirty, or record the
session revision the answer was given at and compare it in the arm. The flag then means "this close
was decided", which is what it is named for, rather than "the session is clean", which it cannot
know.

---

### 2 — serious · `src-tauri/src/lib.rs:189-197`, `:231`, and `src-tauri/src/dialog.rs:68` · `GATE_OPEN` outlives the dialog it documents, and only the detached worker can clear it

`GATE_OPEN`'s docstring (`lib.rs:189-191`) states the invariant as "one gate at a time … a second
close request while the dialog is up raises a second dialog". The flag does not have that lifetime.
`dialog.rs:68` destroys the dialog _before_ `dialog.rs:77` spawns the thread that acts on the
answer, and every site that clears `GATE_OPEN` (`lib.rs:231`, `:239`, `:314`, `:327`) is reached
only _after_ that thread has finished. Between `dialog.rs:68` and the worker's completion there is
no dialog on screen and `GATE_OPEN` is true.

**How it fails.** In that stretch every close request takes `lib.rs:144-150`: `api.prevent_close()`
fires, `GATE_OPEN.swap(true)` returns true, and nothing is raised, shown, or logged. The X button is
silently dead with no dialog to explain it.

The damaging version: `save_open_file` → `subtitle::save_current` takes a **blocking**
`slot.lock()` (`subtitle/mod.rs:441-447`) — deliberately blocking, unlike `session_state`'s
`try_lock`. If another command is holding the session mutex for a long time (`subtitle_open`
reading a very large file, or `save_with_backup` on a stalled network mount), the worker never
reaches `close_window`, `GATE_OPEN` is never cleared, and **the window can no longer be closed by
its X button at all** — no dialog, no message, no log. The user's only remaining exit is killing the
process, which destroys exactly the work the gate exists to protect. There is no timeout anywhere on
this path.

**Recommended correction.** Split the two meanings the flag is carrying — "a dialog is on screen"
and "a close decision is in flight" — and give the swallowed close request a visible response
(re-raise, or a message) instead of a silent `prevent_close`. At minimum, log the swallowed request:
today the silent branch at `:148` is indistinguishable from a hung app.

---

### 3 — serious (latent; no cost today) · `src-tauri/src/lib.rs:131-155`, `:192`, `:197` · both flags are process-global while the handler is per-window and never checks `label`

The handler binds `label` at `:132` and uses it only to pass to `ask_before_closing` at `:149`.
Nothing compares it against the window the flags were armed for; `GATE_OPEN` and `CLOSING` are
plain process-wide statics. The brief asks for this to be said plainly because decision 1 is queued
into M2.0 as T3.

**How it fails the day a second window exists.** Window A's gate is answered with Save; `:303` arms
`CLOSING`; before the queued request is dispatched the user clicks X on window **B**, which has its
own unsaved work. B's `CloseRequested` reaches `:138` first, consumes A's flag, skips the dirty
check, and **B closes with its edits gone**. A's own request then arrives with the flag already
spent. Separately, while A's dialog is up, B's X-click is swallowed by the `GATE_OPEN` guard at
`:148` with no dialog and no feedback — finding 2, one window over.

**Recommended correction.** Key both flags by window label, or state in the code that this module is
single-window and make the M2.0 T3 task own the change. Today this costs nothing; it is written down
so that the multi-window task does not inherit it silently.

---

### 4 — minor · `src-tauri/src/lib.rs:234-240` · the "the dialog could not be raised" recovery can never run

`ask_before_closing` is called from exactly one place, `lib.rs:149`, inside the `RunEvent` callback,
which runs on the main thread. On Linux `dialog::ask_close` (`dialog.rs:37`) forwards to
`AppHandle::run_on_main_thread`, and `tauri-runtime-wry-2.11.4/src/lib.rs:239-249` shows that when
the caller **is** the main thread the task is executed inline and `Ok(())` is returned
unconditionally. On every other platform `ask_close` ends with a bare `Ok(())` at `dialog.rs:120`.
So `asked` is `Ok` on every reachable path and the `if let Err` branch is dead.

That matters twice over. CLAUDE.md §6 bans dead code; and the branch is the only thing that would
clear `GATE_OPEN` when nobody can be asked, so the recovery it advertises is untested and, as
written, untestable. Its comment at `:235-237` ("reporting it would need the same main thread that
just refused to take the question") describes a refusal that cannot happen.

**The same reading is good news and should be recorded:** because the task runs inline, `ask_close`
returning `Ok` on Linux means the dialog was actually built and `show_all()`-n before the call
returned, not merely posted. The brief's concern that "`ask_close` returns `Ok` when the closure is
merely posted" does not hold on the only path that calls it. It _would_ hold for any future caller
on a worker thread.

**Recommended correction.** Either delete the branch and state the invariant ("called only from the
main-thread close handler, where posting cannot fail"), or make it real by having `ask_close` report
whether the dialog was constructed.

---

### 5 — minor · `src-tauri/src/dialog.rs:41-43` · a missing parent is accepted in silence, reproducing the defect the module was written to remove

```rust
let parent = handle
    .get_webview_window(&label)
    .and_then(|window| window.gtk_window().ok());
```

Both failures collapse into `None` with no log. A `None` parent builds the dialog with a null
parent — precisely the rfd behaviour the module's own doc comment calls out at `dialog.rs:6-7` and
`:38-40` as the reason it exists: not modal to the window, able to sit behind it. The user then has
`GATE_OPEN` true, no visible dialog, and an X button that does nothing (finding 2).

`gtk_window()` can genuinely fail: `tauri-runtime-wry-2.11.4/src/lib.rs:3376` skips the whole
`WindowMessage` arm when the window's `inner` is `None`, so nothing is sent on the reply channel and
the getter errors. **On today's gate path this is not reachable** — `ask_close` runs synchronously
inside `on_close_requested`, where `inner` is still `Some`. It becomes reachable the moment
`ask_close` acquires a second caller. Reported as minor for that reason, not waived: a silent
`.ok()` on the one lookup whose failure recreates the bug being fixed deserves a line.

**Recommended correction.** `log::warn!` on the `None`, or treat it as a raise failure.

---

### 6 — minor · `src-tauri/src/lib.rs:194-197` and `e2e/scripts/close-gate-check.js:240-370` · the single-use property of `CLOSING` has no check that can fail

`lib.rs` and `dialog.rs` contain no `#[cfg(test)]` module; the gate's state machine has no unit
coverage at all. `close-gate-check.js` covers cancel → second dialog → discard → exit (`:261-301`)
and save → exit (`:322-359`). None of its twelve checks exercises the property the consumption at
`:138` exists for: that a close _after_ an answered gate still asks. It cannot, in a single-window
app that exits immediately after the close — which is exactly what
`docs/reports/n1b-sessanta-corse.md:65` concedes ("It does not bite today, because the app exits
straight after").

So a correction written under self-check pressure shipped with no automated check that can fail for
a cause the suite constructs. CLAUDE.md §5.2 makes behavioural tests the primary layer. This overlaps
L3's charter; it is filed here because it is a property of this flag.

**Recommended correction.** A unit test over the flag transitions (they are testable if the arms are
factored into a function taking `(closing, gate_open, dirty)`), or an explicit written statement that
the property is unverifiable until a second window exists.

---

### 7 — minor · `src-tauri/src/dialog.rs:74-77` · the comment justifying the detached thread states a mechanism that does not exist

> "Off the main thread, because acting on the answer writes a file and the main loop is the one
> thing that must not block: `close_window` posts back to it and would wait on itself."

The first half is right and sufficient. The second half is false:
`tauri-runtime-wry-2.11.4/src/lib.rs:239-249` runs a `Message::Task` **inline** when the caller is
the main thread, so `close_window` called from the main thread would re-enter, not wait. The
decision to spawn is correct — the write must not run on the main loop (CLAUDE.md §7) — but the
reason recorded next to it is not the reason. CLAUDE.md §9 applies to comments the next
implementer will reason from.

**Recommended correction.** Keep the file-write argument, drop the self-wait claim.

---

## Hunt items I found sound, and why

- **The `swap(false)` is not racy between two `CloseRequested` events.** Confirmed as the brief
  predicted: `on_close_requested` (`tauri-runtime-wry-2.11.4/src/lib.rs:4438`) is called from
  `handle_event_loop` on the single main loop thread, so the two arms of the chain at `lib.rs:138-155`
  never run concurrently. I did not file this.
- **The shutdowns did not disappear from the gate path.** `2b31f14` moved `asr::shutdown` +
  `shutdown_video` out of `close_window` and into both non-gated arms of `CloseRequested`
  (`lib.rs:139-140` and `:152-153`). Both branches call them; the `CLOSING` arm is one of the two.
  The ordering invariant asserted at `lib.rs:129-130` still holds for the gate path: `close()` posts
  through the proxy, `on_close_requested` runs the callback (so `shutdown_video` runs), and only then
  does `rx.try_recv()` fall through to `on_window_close` setting `inner = None`. The GTK window dies
  after the surface, not before. (L1 owns this; recorded here because the state machine depends on it.)
- **`CLOSING` cannot be left standing in a way that costs anything.** It is set only at `lib.rs:303`,
  and only after `get_webview_window` returned `Some`; the `close()` that immediately follows either
  fails (and `:305` clears it) or queues a request that `on_close_requested` will deliver, because
  the window's map entry survives `on_window_close`. The one path where it could survive — the user's
  own X-click consuming it first — ends with the process exiting. `ExitRequested`/`Exit`
  (`lib.rs:156-161`) never clear it, which is harmless for the same reason. Finding 1 is about the
  flag being consumed _correctly_ and the check it skips, not about it being left standing.
- **`GATE_OPEN` is cleared on every failure path of `close_window`.** `close()` returning `Err` →
  `:305-307` → `report_close_failure` → `:327`. Window gone → `:314`. `run_on_main_thread` posting
  failure → `:318-321` → `:327`. `n1b-sessanta-corse.md:65` says the missing clear was caught by
  self-check and fixed; verified as shipped, not as described — the consumption is
  `CLOSING.swap(false, Ordering::SeqCst)` at `lib.rs:138` and every clear site above is present.
  Finding 2 is about the _success_ path's window of silence, not about these.
- **The two single-use mechanisms cannot disagree.** `dialog.rs:63-66` takes the `FnOnce` out of the
  `RefCell` exactly once, so `deliver` runs at most once, so `close_window` runs at most once, so
  `CLOSING` is armed at most once per gate, and `swap` consumes it once. Checked the subtle part
  too: the `RefMut` temporary from `answer.borrow_mut()` in the `let ... else` at `:63` is dropped at
  the end of that statement, so `unsafe { dialog.destroy() }` at `:68` cannot re-enter the handler
  into a second `borrow_mut` panic. (Whether `destroy()` itself is sound inside its own signal
  handler is L6's question, not mine.)
- **A WM-closed dialog still delivers an answer.** GTK answers Escape and the window-manager close
  with `ResponseType::DeleteEvent`, which the `_ =>` arm at `dialog.rs:72` maps to `Cancel`, which
  clears `GATE_OPEN` at `lib.rs:231`. No path drops the answer silently except the
  `DESTROY_WITH_PARENT` case at `dialog.rs:46`, where the parent dying takes the dialog with it
  without a response — and the parent can only die through `close_window`, which cannot run while the
  gate is unanswered. Not filed.
- **`api.prevent_close()` is correctly absent from the `CLOSING` arm**, so the answered close
  actually proceeds; and `shutdown_project` is correctly left to `ExitRequested`/`Exit`
  (`lib.rs:156-161`), which fire after the last window closes.
- **`save_open_file` treating `Ok(None)` as success is sound from this lens's angle.**
  `save_current` (`subtitle/mod.rs:441-452`) takes the blocking lock and re-reads `session.dirty()`
  under it, so `Ok(None)` means the session was genuinely clean at that instant — the gate having
  opened on a merely busy `try_lock` (`:412-420`) is the case it exists for. Whether it is sound
  against a concurrently-mutated session is L2's; from here, the flag is not being armed over a
  session known to be dirty. What is unsound is the _interval after_ that instant — finding 1.
- **`discard_open_file` returning `true` on a failed `close_session` is not an L5 defect.**
  `close_session` (`subtitle/mod.rs:320-330`) refuses a poisoned lock via `lock()` (`:534-541`), so
  the session can survive the discard while the close still proceeds — but the user asked for the
  edits to go, so the flag waving that close through matches their instruction. L2 owns whether the
  surviving session is a problem.
