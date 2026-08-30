# Gate 2 — L1: what the corrections themselves broke

Lens L1 of gate 2, per `docs/reviews/gate-2-plan.md` §2. Scope `GATE_BASE=f0b0058`..`GATE_HEAD=eca9806`,
nothing else. Paths below are relative to the repo root `/home/alcahest/git/SubLore`.

Question: did the three fixes in this range each break something that was working before them,
including each other?

The reference battery at `GATE_HEAD` is green (`docs/reports/gate2-battery-baseline.md`). Nothing in
this lens made it red: no file outside this report was touched, and the one experiment run
(`argtest.rs`, below) lives in the session scratchpad, not in the tree.

---

## What was checked

- `git diff f0b0058 eca9806` in full, then per commit with `git show` for `062f201`, `fee26f8`,
  `2b31f14`, `c7261a5` and the doc commits, so no claim below rests on a commit message.
- `src-tauri/src/lib.rs` whole file: the `CloseRequested` arm, `CLOSING`, `GATE_OPEN`,
  `close_window`, `ask_before_closing`, `startup_files`, `startup_files_command`.
- `src-tauri/src/dialog.rs` as it arrives in `fee26f8`, both `cfg` halves.
- `src-tauri/src/main.rs`, `src-tauri/src/video/player.rs`, `src-tauri/src/video/mod.rs`,
  `src-tauri/src/video/surface/{mod,linux,windows}.rs`, `src/components/VideoStage.tsx`,
  `src/types/video.ts`, `src/hooks/useStartupFiles.ts`, `src/App.tsx`.
- `e2e/lib/env.js`, `e2e/specs/video-surface.spec.js`, `e2e/scripts/close-gate-check.js`,
  `wayland-attach-check.js`, `scaled-surface-check.js`, `n1b-load-probe.js`, `package.json`,
  `.github/workflows/ci.yml` (unchanged in the range).
- **Dependency sources read, not trusted** (`docs/reviews/review-prompt.md`):
  `tauri-runtime-wry-2.11.4/src/lib.rs` — `send_user_message` (:235-256), `Message::Task(task) => task()`
  (:3335), `WindowMessage::Close` panic arm (:3491), the event-loop interception of
  `Message::Window(id, WindowMessage::Close)` (:4368), `on_close_requested` (:4440-4468),
  `on_window_close`; `tauri-2.11.5/src/window/mod.rs:1794-1801` (`close` vs `destroy`) and
  `tauri-2.11.5/src/manager/window.rs:166-176` (`tauri://close-requested`).
- One executable experiment, to turn a suspicion into a certainty: a five-line Rust program with
  `std::env::args().skip(1).filter(...)`, compiled with the system `rustc` and given the argument
  `/tmp/\xe9pisode.srt`. Result: `panicked at library/std/src/env.rs:871`, exit status 101.

---

## Findings

### 1 — blocker — a non-UTF-8 command-line argument kills the app before its window exists

**`src-tauri/src/lib.rs:75`** (`.manage(startup_files(std::env::args()))`), reached through
`src-tauri/src/lib.rs:38-59`. Introduced by `062f201`.

`std::env::args()` is documented to panic _during iteration_ on any argument that is not valid
Unicode, and it does: the run above exits 101 with
`called Result::unwrap() on an Err value: "/tmp/\xE9pisode.srt"`. `startup_files` drives exactly
that iterator at `lib.rs:43-46`; `.skip(1)` does not help, because `skip` still pulls each element
through `next()`, and the `filter` closure never gets a chance to reject the path.

**How it fails.** A Linux filename is a byte string, not UTF-8. `sublore /home/user/Épisode.srt`
where that file was created under a Latin-1 locale, or the same path handed over by a file manager
through the `.desktop` `%f` field, or the binary itself installed under a non-UTF-8 directory (then
even `argv[0]` is fatal): the iterator panics inside `run()` while the builder chain is still being
assembled, before `.setup()` and before the event loop. `crash::install()` has run
(`lib.rs:68`) so the panic hook fires, but `crash::attach` has not (`lib.rs:108`), so `APP` is unset
and `show_dialog` returns at its first guard (`src-tauri/src/crash/mod.rs:164-166`). The user gets
no window, no dialog, and a crash report at the fallback path. Before `062f201` the app never read
`argv` and started normally with the same file present; the user simply opened it from the bar.

This is the app's new front door and it is also the only channel WORKFLOW §4c leaves for driving the
app on a real display, so it is not an exotic path.

**Correction.** Iterate `std::env::args_os()` and convert each entry with `to_str()` / `into_string()`,
dropping the ones that are not valid Unicode rather than panicking — or carry them as `PathBuf` and
only lose the ability to name them in the JSON payload. Either way one unnameable argument must cost
that argument, not the launch.

---

### 2 — serious — `gpu-context=x11egl` is forced on every Linux launch, not only when a Wayland display is present, and it removes the gpu VO's context probing

**`src-tauri/src/video/player.rs:186-191`**. Introduced by `062f201`.

```rust
if let Some(wid) = config.wid {
    init.set_option("wid", wid)?;
    // `wid` is an X11 window id, and with a Wayland display in the environment mpv's
    // `gpu-context=auto` picks Wayland and draws past it. See docs/reports/n2b-probe.md.
    #[cfg(target_os = "linux")]
    init.set_option("gpu-context", "x11egl")?;
}
```

The comment states the trigger condition — _with a Wayland display in the environment_ — and the
code tests nothing. `WAYLAND_DISPLAY` is never read here. `SAFE_OPTIONS` (`player.rs:33-51`) does
not pin `vo`, so before this change the gpu VO probed its contexts in order and settled on whichever
worked; now it is pinned to one.

**How it fails.** A pure-X11 session whose GL stack offers GLX but no usable EGL-on-X11 platform:
VirtualBox and VMware guests, `ssh -X` / remote X, and older proprietary driver installations are
the concrete populations. There `x11egl` cannot create a context, `vo=gpu` fails to initialise, and
the user is dropped to whatever VO mpv probes next or to no picture at all while the transport
reports playback — which is the exact silent failure N2b was filed for, relocated to a different set
of machines. The condition the option exists to correct (a Wayland display present) is absent on
every one of them, so they pay the cost and get none of the benefit.

Nothing in the tree exercises the fallback: `e2e/scripts/wayland-attach-check.js` proves the option
is _needed_ under a compositor, `video-surface.spec.js` runs under Xvfb/llvmpipe where EGL exists,
and both are one GL stack each.

**Confidence: suspicion** on the failure itself — I have no GLX-only machine here to run it on. The
mismatch between the comment's stated condition and the unconditional code is certain, and so is the
loss of mpv's own probing.

**Correction.** Set the option only when `WAYLAND_DISPLAY` is non-empty, which is the condition the
comment already names; or set `gpu-context` to a list that keeps a fallback behind `x11egl`. Either
way, say in the code which of the two it is.

---

### 3 — minor — `window.close()` can be prevented where `destroy()` could not, and a prevented close leaves the app unclosable with its video already destroyed

**`src-tauri/src/lib.rs:296-322`** (`close_window`) with **`src-tauri/src/lib.rs:136-141`**.
Introduced by `2b31f14`.

`tauri-2.11.5/src/manager/window.rs:170-175` calls `api.prevent_close()` whenever the webview holds
a JS listener on `tauri://close-requested`, and that manager handler runs _before_ the app's own
`RunEvent::WindowEvent` handler inside `on_close_requested`
(`tauri-runtime-wry-2.11.4/src/lib.rs:4448-4462`). `destroy()` sent `WindowMessage::Destroy`, which
goes straight to `on_window_close` and cannot be prevented by anyone; `close()` sends
`WindowMessage::Close`, which goes through the full close-request sequence.

**How it fails.** The trigger is one line of future frontend code — `getCurrentWindow().onCloseRequested(...)`,
the ordinary way to hook a close in Tauri. With it present: the user answers Save, `close_window`
sets `CLOSING` and calls `close()`; the resulting `CloseRequested` consumes `CLOSING` and runs
`asr::shutdown` + `shutdown_video` (`lib.rs:138-141`); tauri then prevents the close. The window
survives with mpv and the native surface torn down, `GATE_OPEN` still `true` (nothing clears it on
the success path), and every later close request now falls into the `else` arm at `lib.rs:151-154`,
tears down again, and is prevented again. The app cannot be closed and cannot play video.

I checked: no such listener exists today (`grep` over `src/` finds `listen(` only in
`useVideoPlayer.ts:67` and `useTranscription.ts:186`, on custom events), so this is latent, not
live. It is written down because the constraint is new, undocumented and invisible — the comment at
`lib.rs:301-302` explains why `close` replaced `destroy` and says nothing about what `close` now
depends on.

**Correction.** One line at `lib.rs:301` recording that the frontend must not register a
`tauri://close-requested` listener, or a check that the close actually happened.

---

### 4 — minor — the error branch `fee26f8` added to `ask_before_closing` cannot run on Linux either, and it is what discards the dialog's real failure modes

**`src-tauri/src/lib.rs:234-240`**, against **`src-tauri/src/dialog.rs:36-81`**.

On Linux `dialog::ask_close` is `app.run_on_main_thread(closure)`, which is
`send_user_message(Message::Task(..))`; `send_user_message`
(`tauri-runtime-wry-2.11.4/src/lib.rs:235-247`) runs the task **inline and returns `Ok(())`** when
the caller is already the main thread. `ask_before_closing` is only ever called from the
`CloseRequested` arm (`lib.rs:149`), which _is_ the main event loop thread. So `asked` is
unconditionally `Ok` and the `if let Err` arm is dead — on Linux, not only on Windows, which
`gate-2-plan.md` §7 already notes for the `cfg(not(linux))` half.

**How it fails.** The failures that branch appears to guard live inside the closure and are thrown
away there: `handle.get_webview_window(&label)` returning `None` and `.gtk_window()` erroring both
collapse into a `None` parent at `dialog.rs:41-43` with no log and no return value. The dialog is
then built parentless and non-transient — precisely the rfd limitation `fee26f8` was written to
remove — while `ask_close` reports success and `GATE_OPEN` stays `true`. If the user does not find
that dialog, the X button is silently inert for the rest of the session. Before `fee26f8` the plugin
path had no error return at all, so the branch is not a regression in behaviour; it is a guard that
reads as protection and provides none.

**Correction.** Move the two `Option` failures inside the closure onto a path that reports them (a
channel, or the callback invoked with `Cancel` plus a log), and delete the unreachable arm — or keep
the arm and give `ask_close` something real to return. CLAUDE.md §6: no dead code.

---

### 5 — minor — `__NV_DISABLE_EXPLICIT_SYNC=1` is process-wide, so it reaches libmpv and the whisper sidecar, and only the webview's benefit was measured

**`src-tauri/src/main.rs:23`**. Introduced by `062f201`.

It is set before the `/sys/module/nvidia` test, so it applies on every Linux launch where the escape
hatch is not `"0"`, including machines with no NVIDIA hardware at all. Being an environment variable
of the process, it is read by libmpv in-process and inherited by the whisper.cpp sidecar, not only
by WebKit.

**How it fails.** No observed failure. The docstring's own justification is "it costs nothing", and
the measurement behind it is a webview luma range on one RTX 5070 Ti; nobody measured what the
driver does with it under mpv's `x11egl` output or under a Vulkan whisper build on the same driver.
CLAUDE.md §7 asks for measured, and §9 asks that assumed be labelled assumed. **Confidence:
suspicion** — the variable's documented effect is on the Wayland explicit-sync path, which
`GDK_BACKEND=x11` keeps us off, so the likely real cost is zero. The defect is that "costs nothing"
is stated as a fact about three components and was measured on one.

**Correction.** Either restrict it to the branch that already tests `/sys/module/nvidia`, or say in
the comment that the claim covers the webview only. This overlaps L7's brief; it is filed here
because the question "did the fix break something that was working" is what surfaced it.

---

## Hunt items found sound, and why

**`2b31f14`: is `shutdown_video` still guaranteed to run before the GTK window dies on all four
paths?** Yes, on all four, and I traced them through the runtime rather than the comment.
`on_close_requested` (`tauri-runtime-wry-2.11.4/src/lib.rs:4440-4468`) invokes the app callback and
only afterwards, if nothing prevented the close, calls `on_window_close`, which drops the tao window
and takes the GTK window with it. So the handler at `lib.rs:135-155` always runs while the window is
alive. Ordinary close: `else` arm, `lib.rs:151-154`. Gate close: `CLOSING` arm, `lib.rs:138-141` —
`window.close()` at `:304` sends `Message::Window(Close)` through the proxy even from the main
thread (`tauri-2.11.5/src/window/mod.rs:1794`, and the runtime's own `// NOTE: close cannot use
send_user_message`), and the event loop intercepts that variant at `lib.rs:4368` before
`handle_user_message` could reach its `panic!("cannot handle WindowMessage::Close on the main
thread")` at `:3491`. `ExitRequested` and `Exit`: `lib.rs:156-161`, unchanged by this range and
reached only after the window is already gone, exactly as before. `VideoState::shutdown`
(`src-tauri/src/video/mod.rs:110-116`) destroys the surface only after `player.shutdown()` reports
mpv gone, and every caller is on the main thread, so the thread-local `SURFACE` is reached from the
thread that owns it. The invariant in the comment at `lib.rs:129-130` still holds.

**The `if CLOSING … else if unsaved_work … else` chain: what is no longer checked.** On the
`CLOSING` arm the dirty check is skipped entirely, deliberately. I enumerated it and it is **not a
regression**: before `2b31f14` the gate path called `window.destroy()`, which produces no
`CloseRequested` at all, so the dirty check was not merely skipped there — the handler never ran.
The new arm checks strictly more than the old path did. What the new arm _could_ check and does not
is the interval between the answer being acted on (worker thread) and the close request arriving
(main thread), during which the webview is alive and can re-dirty the session. That window existed
before too, because `destroy()` is equally asynchronous — it also travels as
`Message::Window(Destroy)` through the proxy. It is a live question about the design, not about the
change, and `gate-2-plan.md` assigns it to **L5**; I am not re-filing it here. The plan's named false
positive for this lens is correct and I confirm it: the shutdowns were **moved**, not removed, and
both non-gate arms still call them.

**`fee26f8`: did removing the gate's use of rfd change when rfd's GTK thread first starts, and does
anything depend on it having started?** Nothing depends on it. The only remaining plugin consumers
are `project::choose_path` (`src-tauri/src/project/mod.rs:244-264`) and `crash::show_dialog`
(`src-tauri/src/crash/mod.rs:163-200`), and both create the thread lazily on their own first use;
neither reads any state the gate would have initialised, and `tauri_plugin_dialog::init()` is still
registered at `lib.rs:73` so the plugin state `crash/mod.rs:168` probes for is present either way.
The residue — that the gate now builds GTK on the main thread while the picker still drives GTK from
rfd's thread, so two threads can construct GTK objects concurrently where previously both dialogs
were rfd's — is the interaction the plan permits me to note and forbids me to re-file: it is
**N1c**, already open in `BACKLOG.md:114`, which states the same hazard in the same terms.

**`062f201`: which specs' timing assumptions moved under `SUBLORE_WEBKIT_WORKAROUNDS: "0"` in
`appEnv`?** None, and the direction is the safe one. `e2e/lib/env.js:26` makes every harness launch
behave the way _every_ launch behaved before `062f201` existed, because the mitigation it disables
was added in that same commit; the specs' timings were tuned against that behaviour and are
unchanged by it. On a runner with no `/sys/module/nvidia` the only variable actually skipped is
`__NV_DISABLE_EXPLICIT_SYNC`, which does nothing there. The one script that must not be scrubbed
opts out explicitly and says why: `e2e/scripts/wayland-attach-check.js:8-9, 90-95` bypasses `appEnv`
entirely, which also keeps `appEnv`'s `delete env.WAYLAND_DISPLAY` (`env.js:29`) from gutting the
check that exists to prove the Wayland fix. That is the trap this hunt item was pointed at and it
was avoided. The genuine cost — that no automated check now exercises the shipping configuration —
is L7's item, not a timing regression.

**`close-gate-check.js`: do 12 checks still run, and does the double-click still land after the cue
list paints when the file arrives via argv?** I counted the `check()` calls on the executed path:
seven in phase one (`:261, :270, :275, :283, :291, :296, :301`) and five in phase two (`:322, :330,
:335, :350, :357`) — 12, matching `EXPECTED_CHECKS` at `:39`. The setup is now strictly earlier
rather than later: the file is parsed as soon as the frontend mounts and invokes
`startup_files_command`, whereas before the parse could not begin until roughly 2000 ms plus the
`xdotool` typing of a ~60-character path. The same 3500 ms now covers only paint + parse + first
row. And if it ever stops being enough, the failure is loud, not silent: a document that was never
dirtied means no dialog, the app exits on the close request, and `waitForDialog` (`:79-97`) throws
its explicit "two causes look identical from here and both are failures" message. That is the right
shape.

**N2c: does `surface/linux.rs` still land in the same place where the surface already worked?** Yes,
and the arithmetic is exact rather than approximately right. Under Xvfb, `devicePixelRatio` is 1 and
`gdk::Window::scale_factor()` is 1, so `VideoStage.tsx:31-40` sends the CSS rectangle unchanged and
`pixels_over(1.0)` (`surface/mod.rs:51-66`) is the old `logical()` to the number — same code path,
same result, which is why the suite is a no-op here as the delivery claims. Under `GDK_SCALE=2` the
page sends CSS×2 and Linux divides by 2, giving GDK the same logical rectangle it used to get, so
that case is unchanged too — `c7261a5`'s own message says as much, and `scaled-surface-check.js`
guards it. On the owner's 1.5 display the page sends CSS×1.5 and GDK's factor is 1, so nothing is
divided out: the surface moves, which is the fix, not a regression. The guard at `:52-56`
(`divisor.is_finite() && divisor >= 1.0`) makes an absent or nonsense factor degrade to the old
behaviour rather than to a larger rectangle. The two remaining questions on this contract — the
`Math.round` versus `round-away-from-zero` disagreement across the IPC boundary, and
`devicePixelRatio` being read once per `report()` and never re-read when only the ratio changes —
are **L11's**, named in its brief, and I leave them there rather than duplicating the register entry.
So is `src-tauri/src/video/mod.rs:90`, still documenting `VideoRegion` as "in CSS pixels": I
confirmed it is still there and it is the finding already registered without a lens in
`gate-2-plan.md` §2b.

---

## What this lens could not see

The two findings above that carry a `suspicion` label (2 and 5) both turn on hardware this machine
is not: a GLX-only X11 stack, and an NVIDIA driver under mpv and whisper. Everything else in this
report was either read in the tree, read in the dependency source, or run.

Every behavioural statement here is a **Linux** statement. The Windows halves of `dialog.rs` and
`surface/windows.rs` were read and reasoned about; nothing in this report claims they were run,
because nothing has run them.
