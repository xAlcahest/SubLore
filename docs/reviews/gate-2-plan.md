# Gate 2 — the review, ready to fire

Written before N2c merged, so that at merge time nobody has to invent it. Owner ruling 2026-08-30
(WORKFLOW §4a) defines this gate; `docs/reviews/review-prompt.md` is the template every lens brief
is built from and none of them may skip.

Nothing in this file is a finding. It is the apparatus.

---

## 1. Scope, stated exactly

### The commits

Seven merged commits, all of which went in under the gate regime with **no dedicated review, by
choice of regime**, plus the N2c delivery. This gate is the first eyes on all of them.

| commit    | what it carries                                                                  |
| --------- | -------------------------------------------------------------------------------- |
| `062f201` | fix: attach mpv and paint the webview in a real Wayland session — code + harness |
| `fee26f8` | refactor: native GTK dialogs on main thread for close gate — code                |
| `3657241` | test: N1b load probe, and the measurement that downgrades N1b — probe + docs     |
| `323026b` | docs: gate 2 decision document, refuted hypotheses, three new defects            |
| `2b31f14` | fix: close the window instead of destroying it after the gate is answered — code |
| `5332875` | docs: N2c definitive criterion and P3 scale report                               |
| `18fe5f3` | docs: gate 2 review register, and N2c's open suspect                             |
| N2c       | the fractional-scaling delivery, **not yet merged when this plan was written**   |

`f0b0058` ("docs: gate 1 status") is the parent of `062f201` and is therefore the gate base. It is
outside the gate: gate 1 already covered everything up to and including it.

### The files

Code:

- `src-tauri/src/lib.rs` — the close path, `CLOSING`, `GATE_OPEN`, `startup_files`
- `src-tauri/src/dialog.rs` — new, 156 lines, never reviewed by anyone but its author
- `src-tauri/src/main.rs` — the NVIDIA webview mitigation and its escape hatch
- `src-tauri/src/video/player.rs` — `gpu-context=x11egl`
- `src/hooks/useStartupFiles.ts` (new), `src/App.tsx`

Harness:

- `e2e/lib/env.js`, `e2e/specs/video-surface.spec.js`, `e2e/scripts/close-gate-check.js`
- `e2e/scripts/wayland-attach-check.js` (new), `e2e/scripts/real-session-check.mjs` (new),
  `e2e/scripts/n1b-load-probe.js` (new)
- `package.json`, `.github/workflows/ci.yml`

Documentation:

- `BACKLOG.md`, `WORKFLOW.md`, `docs/design/decisions.md`, `docs/design/x11-vs-render-api.md`
- `docs/reports/n1b-segfault-uscita.md`, `n1b-sessanta-corse.md`, `n1b-trenta-corse.md`,
  `n2b-collaudo-reale.md`, `n2b-probe.md`, `n2b-stato.md`, `n2c-p3-scala.md`

N2c adds, as measured in the working tree before merge (the merged diff is authoritative, this is
the shape to expect): `src-tauri/src/video/surface/mod.rs` (`SurfaceRegion::pixels`,
`pixels_over`, seven unit tests), `surface/linux.rs`, `surface/windows.rs`,
`src/components/VideoStage.tsx`, `src/types/video.ts`, `src-tauri/src/video/mod.rs`,
`e2e/scripts/scaled-surface-check.js` (new), `package.json`, and the report/BACKLOG text.

### Deliberately out of scope

- **Anything at or before `f0b0058`.** Gate 1 covered it. A defect found there is filed in
  BACKLOG.md, not counted against this gate.
- **Decision 1** (occlusion handling). Owner ruling moved it into M2.0 as T3. Not in this gate.
- **N1c** (rfd's second GTK thread in `project::choose_path`). Filed, open, deliberately not fixed
  in this range. A lens may note where it interacts with code in scope; it may not re-file it.
- **The known gap at N1**: an inline cue editor holding uncommitted text leaves the backend session
  clean, so that text is lost on close. Filed, explicitly out of scope, needs decision 1's shape.
  Do not re-file.
- **Fixed `sleep()` waits in `close-gate-check.js`.** Filed as small debt at N1. Note them where
  they change behaviour, do not re-file them as new.
- **macOS.** Deferred by CLAUDE.md. The only rule is that nothing may _block_ a later port; flag
  mac-blocking design, never missing mac testing.
- **Style.** Prose length, naming taste, and keeping refuted hypotheses on the record are not
  findings. CLAUDE.md §9 asks for the refuted hypotheses explicitly.

---

## 2. The lenses

Twelve lenses, twelve delegated agents. Four are the standing lenses of WORKFLOW §4a, one is named
in advance by the owner, seven are what this scope earns.

**One lens was cut.** The draft carried a thirteenth, "orphaned harness code and unguarded fixes",
covering dead exports, a prerequisite enforced for nothing, and fixes that no CI job runs. It does
not earn its own agent: a fix with no check and a check that cannot fail are the same defect —
automation that proves nothing — and one agent should own the question "what does the suite
actually prove". Its hunt list is folded into **L3** in full, and nothing from it is dropped.

Every brief carries the shared material below. Every lens reads the diffs with git rather than
trusting any description, including this one.

### Shared brief material (goes into all twelve, per `docs/reviews/review-prompt.md`)

- Name the project rules by section, not "review this": CLAUDE.md **§3** (data safety), **§5**
  (how work is verified, especially §5.4 never fake a pass and §5.5 platform), **§6** (code
  quality), **§7** (performance budgets), **§9** (honesty and the platform on every verdict); and
  WORKFLOW **§4** (drift control, including its rule that no test is weakened, skipped or deleted silently), **§4a**, **§4c** (driving the app).
- **Read the dependency sources, do not trust docstrings.** The N1 review's threading verdict came
  from reading `rfd`, `tauri-plugin-dialog` and `tauri-runtime-wry`. This gate needs `gtk-rs` 0.18,
  `tao`, and `tauri`'s `run_on_main_thread` read the same way.
- The tests are under review alongside the code. Three empty assertions and a branch that passed on
  a missed click were found there last time.
- Severity on every finding: **blocking / serious / minor**, with `file:line` and the recommended
  correction.
- **"Nothing found" is not an available answer.** If you found nothing you did not look hard enough.
- **Write the full report to the named path under `docs/reports/` BEFORE finishing, then stop.**
  The closing message is not the report. A missing or empty file is a failed review whatever the
  closing message claims. Do not repeat your summary; write the file and terminate.
- Do not modify any file except your own report.

---

### L1 — What the corrections themselves broke _(standing)_

**Report:** `docs/reports/gate2-regressions-review.md` · **model:** opus

**Question:** did the three fixes in this range each break something that was working before them,
including each other?

**Hunt list**

- `2b31f14` changed `close_window` from `window.destroy()` to `window.close()` and moved
  `asr::shutdown` + `shutdown_video` out of `close_window` into the `CloseRequested` arm
  (`src-tauri/src/lib.rs:136-155`, `:289-322`). Walk every path that reaches `CloseRequested`: the
  ordinary close, the gate close, `ExitRequested`, `Exit`. Is `shutdown_video` still guaranteed to
  run **before** the GTK window dies on all four, given that `lib.rs:129-130` still asserts in a
  comment that mpv's child window must be gone first?
- The `if CLOSING … else if unsaved_work … else` chain: a consumed `CLOSING` skips the dirty check
  entirely. Enumerate what used to be checked on that path and no longer is.
- `fee26f8` replaced `tauri-plugin-dialog` with `dialog.rs` on Linux only. `tauri_plugin_dialog::init()`
  is still registered (`lib.rs:73`) and `project::choose_path` still uses it. Did removing the
  gate's use of rfd change **when** rfd's GTK thread first starts, and does anything now depend on
  it having started?
- `062f201` set `SUBLORE_WEBKIT_WORKAROUNDS: "0"` inside `appEnv` (`e2e/lib/env.js`), changing the
  environment of **every** wdio spec and every script using `appEnv`, not only the ones the commit
  was about. Which specs' timing assumptions moved under it? The commit's own text says the
  workarounds cost 373 ms against 186 ms to reach React state.
- `close-gate-check.js` lost the type-the-path setup and its 2000+1500 ms waits became a single
  3500 ms wait. Same total, different phase boundaries: does the double-click at `FIRST_CUE_TEXT`
  still land after the cue list paints when the file arrives via argv instead of the Open button?
- N2c's `surface/linux.rs` now divides by `self.window.scale_factor()` where it previously used
  `region.logical()`. On the machines where the surface already worked — Xvfb at factor 1, the
  owner's Wayland session — does it still land in the same place?

**Counts as a finding:** any behaviour that worked before `062f201` and does not now; any ordering
invariant stated in a comment that the new code no longer satisfies.
**Does not count:** the deliberate removals the commit messages name and justify — argue with those
in L3 and L10.
**Most likely false positive:** reading the `CloseRequested` restructure as having _removed_ the
shutdowns. They moved; both branches of the new chain still call them. A lens reporting "asr no
longer shuts down on the gate path" without following the flag has misread it.

---

### L2 — Data-loss paths _(standing)_

**Report:** `docs/reports/gate2-data-loss-review.md` · **model:** opus

**Question:** can any path introduced in this range end with the user's subtitle work gone,
truncated, or written to a file they did not name?

**Hunt list**

- `save_open_file` (`lib.rs:245-264`): `Ok(_) => true` treats `Ok(None)` — "the session was clean" —
  as a successful save and closes the window. Trace the case where `session_state` returned
  `Unknown` because `try_lock` was busy (`subtitle/mod.rs:413-421`), the gate opened on it, and
  `save_current`'s blocking lock then finds a session that is genuinely clean versus one
  concurrently mutated.
- `discard_open_file` (`lib.rs:280-287`) returns `true` even when `close_session(.., true)` errored,
  and the caller then sets `CLOSING` and closes. Is there a state where the discard failed, the
  session is still dirty, and the close proceeds anyway with the flag waving it through?
- The answer callback runs on a **detached** `std::thread::spawn` (`dialog.rs:77`, inside
  `connect_response`). The file write and its backup happen there, with no join and no handle.
  What happens to a half-finished save if the main loop reaches `Exit` first? Cross-check
  CLAUDE.md §3.2 (temp + fsync + rename) and §3.3 (backup before overwrite) in
  `subtitle::save_locked` / `backup_root`.
- `startup_files` (`lib.rs:38-59`) plus `useStartupFiles` (`src/hooks/useStartupFiles.ts`): the app
  now opens files named on argv with no confirmation. Does opening a subtitle mutate anything on
  disk — mtime, lock file, backup — before the user has asked for a save? §3.1 says source media is
  read-only; establish that the subtitle is read-only until an explicit save.
- Any non-subtitle extension becomes `files.video` (`lib.rs:54`). A `.srt` typo'd as `.str`, a
  `.txt`, a stray argument: what does `openVideo` do with it, and can it write anything?
- `n1b-load-probe.js`, `close-gate-check.js` and `scaled-surface-check.js` all copy a repo fixture
  into a temp dir and edit the copy. Confirm none of them ever writes into `fixtures/` itself.

**Counts as a finding:** any reachable sequence ending in lost or altered user data, however
improbable, plus any write to a path the user did not name.
**Does not count:** loss of in-flight editor text not yet committed to the backend session — the
known gap filed at N1, explicitly out of scope.
**Most likely false positive:** flagging `save_current`'s poisoned-lock recovery
(`subtitle/mod.rs:441-447`) as unsound. Its docstring at `:423-436` gives the whole-document-swap
argument; a finding must defeat that argument, not restate that poison recovery is generally risky.

---

### L3 — What the suite actually proves _(standing, absorbs the cut lens)_

**Report:** `docs/reports/gate2-test-honesty-review.md` · **model:** sonnet

**Question:** does every check in the touched tests fail for a cause the test itself constructs;
were the deleted assertions replaced by something equally strong; and which fix in this range has
no check that CI runs?

**Hunt list — assertions**

- `e2e/specs/video-surface.spec.js`, the whole of `062f201`'s edit. The pixel assertion
  (`saturation(...) > PICTURE`) was removed from four places and replaced with
  `mapState(...) === "IsViewable" && childWindows(surface.id).length > 0`. The file's own original
  docstring said a surface "can report `IsViewable` while showing nothing at all", and BACKLOG's N2
  acceptance criterion states the assertion must be "on the visible frame, not on an internal flag,
  because … a state-only assertion would pass while the user sees black". Judge whether the
  replacement satisfies that AC, whether the substitution was declared per WORKFLOW §4 ("any test
  weakened … must be named in the delivery description"), and whether `childWindows().length > 0`
  can fail for anything the test does — it is established at `before()` and never torn down between
  cycles.
- The `before()` hook's new hard check (`if (childWindows(surface.id).length === 0) throw`) replaced
  a 15 s `waitFor`. Is a single instantaneous read there racy, and if it never fails, does it assert
  anything?
- `wayland-attach-check.js`: `check("the app window appeared", toplevel !== null)` — `waitFor`
  already threw if it was null, so the condition is guaranteed true. Exactly the WORKFLOW §2.5b
  pattern ("no assertion on a constant") inflating a counter whose stated job is catching removed
  assertions. Audit all four checks the same way, including `attached !== null` at `:126-130` after
  a `.catch(() => null)`.
- `scaled-surface-check.js`: `EXPECTED_CHECKS = 5`. It compares the same app at `GDK_SCALE` 1 and 2
  and asserts the surface doubles. Can each of the five fail? Can the two runs be confounded by
  anything other than the scale — window placement, a failed second launch, a surface that never
  appeared in either run?
- `EXPECTED_CHECKS` counters everywhere in scope: `close-gate-check.js` (12, unchanged across both
  of this range's edits), `wayland-attach-check.js` (4), `scaled-surface-check.js` (5). Does each
  count match the number of checks that can actually fail, and do 12 still execute in
  `close-gate-check.js` after the argv rewrite?
- `n1b-load-probe.js` asserts nothing by design and ends `process.exit(0)` unconditionally. Fine as
  a probe — confirm nothing in CI or any script treats its exit status as a verdict.

**Hunt list — coverage and dead harness (the folded lens)**

- `saturation()` in `e2e/lib/pixels.js`: `video-surface.spec.js` was its last consumer and `062f201`
  removed the import. `real-session-check.mjs` reimplements the measurement inline rather than
  calling it; `wayland-attach-check.js` only mentions it in a comment. Establish whether the export
  now has any caller. CLAUDE.md §6: no dead code. The right finding may be "consolidate", not
  "delete" — the N2b reports depend on that measure.
- `requireFfmpeg()` is still called unconditionally at `e2e/wdio.conf.js:34`, making ffmpeg a hard
  prerequisite of the whole suite for a capability the suite no longer uses.
- `pnpm e2e:wayland` (added by `062f201`) and `pnpm e2e:scale` (added by N2c) appear in **no CI job**:
  `.github/workflows/ci.yml` runs `pnpm e2e`, `e2e:shutdown` and `e2e:close-gate` only.
  `e2e:wayland` is the sole automated proof that `gpu-context=x11egl` fixes N2b and needs a Wayland
  socket a runner will never have. What guards those two fixes on every future push?
- Same question, one line each, for: the `startup_files` path, the NVIDIA mitigation, `dialog.rs`,
  and N2c's `pixels_over`. Which can regress silently, and does anything say so out loud?
- `close-gate-check.js` and `n1b-load-probe.js` each carry their own copy of the GTK button geometry
  (96 px width, 12 px gap, slot indices). Two copies of one fragile constant.
- Anything in `e2e/README.md` that now describes a harness that no longer exists.

**Counts as a finding:** any check whose condition the surrounding code guarantees; any assertion
removed without a named, argued replacement of comparable strength; dead exported code; a
prerequisite enforced for nothing; a fix with no check that CI runs and no statement that it has
none.
**Does not count:** fixed `sleep()` waits, already filed as known debt — note them, do not re-file.
A check that legitimately cannot run in CI (the Wayland one) is not itself the finding; the absence
of any _statement_ that it cannot is.
**Most likely false positive:** treating the pixel-assertion removal as automatically illegitimate.
It carries a measurement (2 appearances in 10 under llvmpipe with mpv attached all 10 times) and a
stated relocation of the proof to real hardware. Rule on whether that measurement supports the
conclusion, not on the fact that an assertion disappeared.

---

### L4 — Platform claims checked on one machine only _(standing)_

**Report:** `docs/reports/gate2-platform-claims-review.md` · **model:** sonnet

**Question:** which behavioural claim in this range holds only on the author's specific hardware,
driver, compositor or display, and is written as though it holds generally?

**Hunt list**

- `mitigate_nvidia_webview` (`src-tauri/src/main.rs`): measured on "an RTX 5070 Ti with driver
  610.57.04", one machine, one driver, applied to every Linux user whose `/sys/module/nvidia`
  exists — including hybrid-graphics laptops rendering on Intel or AMD, and users on driver versions
  where upstream expects `__NV_DISABLE_EXPLICIT_SYNC` alone to be enough, which the comment itself
  says. Does the detection identify "NVIDIA is drawing" or only "the module is loaded"?
- `init.set_option("gpu-context", "x11egl")` (`video/player.rs:185-191`) is now unconditional on
  Linux whenever a `wid` is handed over, including under Xvfb/llvmpipe and on non-NVIDIA GPUs where
  `auto` was previously correct. Is there evidence it was tried anywhere but this machine?
- `docs/reports/n2c-p3-scala.md`: 144 DPI, KWin, rootless XWayland, `K = 4/3`, DP-5/DP-4 — all one
  desk. Which conclusions generalise (the `tao` `AtomicI32` reading, which is source-derived) and
  which do not (everything measured through `spectacle`)?
- N2c's own verification is "on the owner's 1.5 display" and nowhere else, and
  `scaled-surface-check.js` says in its header that it cannot produce a fractional ratio at all.
  Is the delivery's language faithful to that, everywhere it appears?
- **The Windows halves nobody has run.** `dialog.rs`'s `#[cfg(not(target_os = "linux"))]`
  `ask_close` and `report_error` compile and have never executed. There `ask_close` can never
  return `Err`, so `ask_before_closing`'s error branch (`lib.rs:234-240`) is unreachable — correct
  there, or merely compiling? Same question for N2c's `surface/windows.rs` `region.pixels()`.
- `close-gate-check.js` and `n1b-load-probe.js` compute GTK button positions from a hardcoded 96 px
  width and 12 px gap. That is the author's GTK theme and font size.

**Counts as a finding:** a claim stated without its platform; a mitigation keyed on a signal that
does not mean what the code assumes on hardware other than the author's.
**Does not count:** honest, explicitly-scoped statements ("Verified on Linux. Windows compiles in
CI and has never had these checks run against it") — those are the standard, not a defect. The
bare-"verified" sweep across documents belongs to **L12** alone; do not duplicate it here.
**Most likely false positive:** demanding macOS coverage. Flag mac-blocking design, not missing mac
testing.

---

### L5 — The close path and the single-use `CLOSING` flag _(named in advance by the owner)_

**Report:** `docs/reports/gate2-close-path-review.md` · **model:** opus

**Question:** can the `CLOSING` flag ever wave through a close that should have been gated, or stay
standing where it should have been consumed?

**Hunt list**

- `lib.rs:136-155`, `:192-197`, `:290-330`, in full. Build the state machine of
  `GATE_OPEN` × `CLOSING` × session-dirty and enumerate every reachable pair.
- `GATE_OPEN` is never cleared on the success path — not in `close_window`'s `Ok` branch, not in the
  `CLOSING` arm of `CloseRequested`. If the close request is raised and the window nevertheless
  survives, `CLOSING` has been consumed and `GATE_OPEN` is stuck true: the next close prevents
  itself and raises no dialog. Is that state reachable, and what does the user see?
- Order between `CLOSING.store(true)` (`lib.rs:303`) and `window.close()` (`:304`): a user X-click
  already queued in the event loop can consume the flag before `close()`'s own request arrives.
  Trace both interleavings.
- `CLOSING` is a process-global static, not per-window, while the handler matches on `label`.
  Single-window today — say plainly what breaks the day a second window exists, since decision 1 is
  queued in M2.0.
- The dirty check is skipped when `CLOSING` is set. Between the answer being acted on (worker
  thread) and `CloseRequested` arriving (main thread) the webview is alive: can the frontend
  re-dirty the session in that interval, and would the flag then discard it silently?
- `dialog::ask_close` returns `Ok` when the closure is merely **posted** to the main thread. If that
  closure panics or never runs, `connect_response` never fires, `GATE_OPEN` stays true, and nobody
  is ever asked again. Check the failure modes of `handle.get_webview_window(&label)` and
  `.gtk_window()` — `dialog.rs:41-43` lets both fail into a `None` parent without a word.
- The `RefCell<Option<F>>` single-take in `connect_response` versus `CLOSING`'s `swap`: two separate
  single-use mechanisms guarding one decision. Confirm they cannot disagree.
- Cross-check `docs/reports/n1b-sessanta-corse.md`'s own account of this fix, which says the missing
  clear was caught by self-check and fixed. Verify the fix **as shipped**, not as described.

**Counts as a finding:** any reachable state where a dirty session closes without the gate, or where
the gate becomes permanently unraisable.
**Does not count:** "the flag is redundant because the app exits right after" — the report already
concedes that and consumes it anyway.
**Most likely false positive:** claiming `swap(false)` is racy because two `CloseRequested` events
could interleave. They cannot: `CloseRequested` is delivered on the single main event loop thread,
so the two arms never run concurrently. The real races are main-thread-versus-worker-thread.

---

### L6 — `dialog.rs`: thread ownership and object lifetime

**Report:** `docs/reports/gate2-gtk-dialog-review.md` · **model:** opus

**Question:** is `src-tauri/src/dialog.rs` sound about which thread owns each GTK object and how
long each lives, on the platform where it actually runs?

**Hunt list**

- `dialog.rs` in full — 156 new lines standing exactly where `project::choose_path`'s documented
  deadlock lives.
- `unsafe { dialog.destroy() }` at `:68` and `:141`: destroying the widget from inside its own
  signal handler while GTK is still dispatching it. Read gtk-rs 0.18's `WidgetExt::destroy` safety
  contract and GTK3's own rules. Do not take the inline comment's word for it — the review-prompt is
  explicit that dependency sources get read, not trusted.
- `std::thread::spawn(move || deliver(answered))` at `:77`: an unnamed, unjoined thread that
  outlives the dialog and calls back into `AppHandle`. What if it panics? What if the app is torn
  down while it runs?
- `report_error` builds a parentless dialog on the main thread via `run_on_main_thread` and returns
  before it is shown; both callers are already off the main thread. Confirm no path calls
  `report_error` **from** the main thread, which would post to itself.
- `DESTROY_WITH_PARENT` plus `MODAL` with a parent from `get_webview_window(label).gtk_window()`
  (`:41-46`): what happens to the pending answer if the parent dies first?
- The `#[cfg(not(target_os = "linux"))]` twin at `:83` and `:146`: does it preserve the same
  contract — deliver exactly once, never block the main loop — and does its unconditional `Ok(())`
  leave a real failure mode unreported on Windows?
- `gtk = "0.18"` and `gdkx11 = "0.18"` were already in `src-tauri/Cargo.toml` before this range.
  Confirm that, so nobody files a phantom new-dependency finding, and check the GTK3-versus-binding
  version pairing against what the running system provides.

**Counts as a finding:** a GTK object touched off the main thread, a use-after-destroy, a callback
that can be dropped without delivering, or an unhandled panic on a spawned thread.
**Does not count:** stylistic objection to writing GTK directly instead of using the plugin. That
trade is argued in the commit and is the owner's to relitigate.
**Most likely false positive:** asserting that the deadlock the module was written to avoid is still
present, because `run_on_main_thread` appears in the code. The measurement in the commit message
(`ThreadId(1)` for the handler, `ThreadId(23)` for the delivery) is checkable — verify or refute it
rather than reasoning from the shape.

---

### L7 — Process-wide environment mutation and the NVIDIA mitigation

**Report:** `docs/reports/gate2-env-mitigation-review.md` · **model:** opus

**Question:** is `main.rs`'s environment surgery safe, correctly targeted, and does it cost the user
anything that was measured?

**Hunt list**

- `mitigate_nvidia_webview` (`src-tauri/src/main.rs`): three `std::env::set_var` calls. Confirm the
  crate edition (`src-tauri/Cargo.toml` says `edition = "2021"`) and that no thread exists yet at
  that point; state what breaks if the edition moves to 2024, where `set_var` is `unsafe`. Check the
  ordering against `gtk_init` and against `GDK_BACKEND=x11` at `main.rs:9`.
- The escape hatch accepts only the exact string `"0"`. `SUBLORE_WEBKIT_WORKAROUNDS=false`, `=no`,
  `=off`, empty — all enable the workarounds. Is that contract documented anywhere a user would
  find it?
- `WEBKIT_DISABLE_DMABUF_RENDERER=1` pushes WebKit off its accelerated buffer path. CLAUDE.md §7 has
  four budgets: cold start < 2 s, idle < 400 MB, UI responsiveness, file open < 1 s. Was any of them
  measured with the variable set? §7 requires a change that regresses a budget to say so and wait
  for owner approval. The commit already measures a 373 ms versus 186 ms input-latency cost, which
  is a §7 responsiveness number sitting in a commit message rather than in a budget statement.
- `appEnv` in `e2e/lib/env.js` now sets `SUBLORE_WEBKIT_WORKAROUNDS: "0"` for every harness launch.
  Consequence: **no automated check ever exercises the shipping configuration on a machine with the
  NVIDIA module loaded.** Meanwhile `real-session-check.mjs:108` sets `WEBKIT_DISABLE_DMABUF_RENDERER`
  by hand from the outside instead of letting the app's own mitigation do it, so the one
  real-hardware run also bypassed the code under test. Establish whether the mitigation as shipped
  has ever run.
- `GDK_BACKEND=x11` is set in both `main.rs` and `appEnv`; check they cannot disagree.
- `/sys/module/nvidia` as the detection signal: nouveau, a loaded-but-unused module, containers,
  `prime-run` setups.

**Counts as a finding:** a mitigation that fires where it should not; an unmeasured cost against a
§7 budget; a code path no test can reach because the harness disables it.
**Does not count:** the choice to read `/sys/module` rather than probe GL — argued in the comment
and cheap. Challenge the _meaning_ of the signal, not the mechanism.
**Most likely false positive:** calling `set_var` unsound outright. Before `tauri::Builder` runs, in
a single-threaded `main`, on edition 2021, it is defined behaviour. The finding, if any, is about
the edition boundary and the ordering against GTK initialisation.

---

### L8 — `startup_files` as an input surface, and its frontend consumer

**Report:** `docs/reports/gate2-startup-files-review.md` · **model:** sonnet

**Question:** does argv-driven file opening handle hostile, malformed and merely surprising input
the way CLAUDE.md §6 requires of a boundary?

**Hunt list**

- `startup_files` (`lib.rs:38-59`): `.skip(1)` assumes argv[0] is the program name — true for a
  direct exec, check the packaged launcher and the `.desktop` invocation. `!a.starts_with('-')`
  drops any file whose name begins with a dash. `is_file()` silently ignores a path the user named
  but which does not exist — no message, nothing in the log. TOCTOU between `is_file()` at startup
  and the open seconds later in the webview.
- Extension classification by `to_lowercase()` + `ends_with`: everything not `.srt/.vtt/.ass/.ssa`
  becomes the **video**. Two videos, two subtitles, ten files, a FIFO, a device node, a 40 GB file,
  a filename containing a newline — trace each.
- `startup_files_command` (`lib.rs:62`) is registered in `invoke_handler` (`:105`) and the state
  `manage`d at `:75`, before `.setup()`. Confirm the state is present for the first invoke and that
  calling the command twice cannot give different answers.
- `useStartupFiles` (`src/hooks/useStartupFiles.ts`): the `try/catch` covers only `invoke`.
  `await openVideo(...)` and `await openSubtitle(...)` sit outside it inside a
  `void (async () => …)()`, so a rejection there is an unhandled promise rejection with nothing
  shown to the user, and a failing video open means the subtitle is never opened at all.
  CLAUDE.md §6: errors surface as actionable messages, never silent logs.
- The `started` ref versus the `[openVideo, openSubtitle]` dependency array: confirm the guard is
  what actually prevents a re-run, and that the empty catch on `invoke` does not also swallow a real
  backend error.
- i18n: CLAUDE.md §9 forbids hardcoded user-facing strings. Does this path produce any?

**Counts as a finding:** an unhandled rejection, a swallowed error, a file opened that the user did
not name, or a classification that misroutes a real user's file.
**Does not count:** "the app should validate that the video is really a video" — format sniffing is
not v1 scope. The question is whether a wrong guess is _reported_.
**Most likely false positive:** treating argv as an attacker-controlled channel of the same severity
as network input. It is not: the person typing the command line is the user. Rate these as
robustness and error-reporting defects, not security ones.

---

### L9 — The harness on the owner's real display

**Report:** `docs/reports/gate2-real-session-review.md` · **model:** sonnet

**Question:** does `real-session-check.mjs` obey the rule that `062f201` introduced in the very same
commit?

**Hunt list**

- WORKFLOW §4c, added by `062f201`: "synthetic input … never used on the owner's real display"; "on
  the real display, three things are allowed: launching the app, passing it files as command-line
  arguments, and capturing its own window"; "capture the window, not the screen".
- `e2e/scripts/real-session-check.mjs`, committed by that same commit: `typeText(FIXTURE)` at `:125`,
  `clickIn(live, POINTS.videoField)` at `:124`, `clickIn(..., POINTS.videoOpen)` at `:126`,
  `clickIn(appWindow(), POINTS.transport)` at `:139`, and `raise()` calling `xdotool windowraise` +
  `windowactivate` at `:65-68`. It runs on `:0`, with `spawn(..., [])` — no argument, despite
  `startup_files` existing precisely so it would not need the keyboard. The rule and the script
  contradict each other inside one commit. Decide which one is wrong and say so.
- `windowSaturation` (`:71-99`) captures the **whole composited desktop** with `spectacle -f` and
  crops afterwards, which is the screen and not the window, and puts whatever else was on the
  owner's screen into `/tmp`. The crop PNGs are never removed: `rmSync` at `:97` deletes only the
  full-screen capture, and only on the success path.
- `const REPO = "/home/alcahest/git/SubLore"` at `:27`, absolute and committed; dynamic `import()`
  of harness modules by absolute path; `path.join(REPO, "target/debug/sublore")` instead of
  `requireAppBinary()`, so the guard message about the Vite-dev-URL binary never fires.
- No `check()`, no `EXPECTED_CHECKS`, no non-zero exit — it prints a table. Not in `package.json`,
  not in CI. Test, probe, or committed scratch file? §4c permits probes, but then it must say so as
  loudly as `n1b-load-probe.js` does.
- `spawnSync` for the ffmpeg crop at `:84` — status never checked, so a failed crop falls through to
  a confusing "no saturation from" error.

**Counts as a finding:** synthetic input on a display the harness did not create; a committed
absolute path; an unbounded artifact left on disk; a script whose category is not declared.
**Does not count:** the workarounds themselves (the `4/3` factor, raise-before-capture) — honest
measurements of a hostile environment.
**Most likely false positive:** condemning `raise()`/`windowactivate` as banned synthetic input.
§4c bans typing, clicks and key presses; window management is arguably outside it — though §4c also
says window capture "needs no raise and no focus". Report the tension; do not assert a rule the file
does not contain.

---

### L10 — The N1b evidence chain

**Report:** `docs/reports/gate2-n1b-evidence-review.md` · **model:** sonnet

**Question:** does the committed instrument actually measure what the reports and BACKLOG claim it
measured?

**Hunt list**

- `docs/reports/n1b-sessanta-corse.md:9` names the probe `n1b-branch-probe.mjs`. **No file by that
  name exists anywhere in the repo's history** — verified with
  `git log --all --diff-filter=A --name-only`. The committed script is `e2e/scripts/n1b-load-probe.js`,
  added in `3657241`. Establish whether the numbers came from the file that shipped or from a script
  never committed. N1b's closing criterion in BACKLOG is written in terms of the **committed** name.
- `clickDialogButton` in `n1b-load-probe.js:50-57`: `buttonWidth = 96`, gap 12, offset 24,
  `slot = { save: 2, discard: 1 }`. GTK buttons are sized to their labels — "Save", "Discard",
  "Cancel" are not equal width. If the estimate drifts, a "save" run clicks Discard, and the discard
  branch is exactly the one that never crashed. That would produce "0 SIGSEGV in 60 save runs" for
  the wrong reason. Verify the geometry against the real dialog, or say it is unverified.
- The probe swallows every exception (`catch {}` at `:118`) and always `process.exit(0)`. Only
  `phase` distinguishes a run that answered the dialog from one that missed the button. Did the
  60-run aggregation filter on `phase === "done"`? The report's table has a "reached the end" column
  for the sequential battery and **no such column** for the six-stream battery that produced the
  verdict.
- `killGroup(pgid)` in `finally` (`:120-128`) can kill the app during the exact exit window the
  probe exists to observe.
- WORKFLOW §4c's discrimination rule ("a discrimination experiment proves the rebuild happened
  before it measures … check the build's exit status explicitly"), added in `323026b`. Was it
  honoured for the 60-run battery on the delivered binary? `wayland-attach-check.js:141-146` claims
  a removal experiment for `x11egl`; check whether that one states its build status.
- `n1b-sessanta-corse.md`'s own table: the delivered binary got **3** sequential `close-gate` runs
  where the first binary got 30, while N1b's second acceptance criterion in BACKLOG reads "thirty
  sequential runs of `pnpm e2e:close-gate` stay clean". N1b is marked `[x]`. Does the evidence meet
  its own written criterion?
- The arithmetic: 2/30 per-run rate, 60 runs, `(28/30)^60 ≈ 1.6%`. It checks out — verify the
  independence assumption and that the "after" battery ran under the same six-way load, since load
  is the stated condition of the defect.

**Counts as a finding:** an instrument that cannot distinguish "the branch ran and did not crash"
from "the branch never ran"; a claimed measurement whose apparatus is not in the tree; a criterion
declared met by weaker evidence than it names.
**Does not count:** the probabilistic framing itself. "The crash did not occur in the battery built
to make it occur" is exactly the honesty CLAUDE.md §9 asks for.
**Most likely false positive:** reading the probe's silent `catch` as an error-swallowing defect. It
is a probe and says so at `:4`. The defect, if any, is in what the _aggregator_ did with `phase`.

---

### L11 — N2c: the region contract as a public interface

**Report:** `docs/reports/gate2-n2c-region-review.md` · **model:** opus

**Question:** does N2c move both platforms' halves of the region contract together, and does its
evidence match its own three acceptance criteria?

**Hunt list**

- N2c's BACKLOG entry and `docs/reports/n2c-p3-scala.md`. The three ACs: the surface covers the
  stage on a 1.5 display, measured against a screenshot of the app's own window; a unit test pinning
  the conversion including a fractional ratio and the 16-bit X11 clamp; the E2E suite green under
  Xvfb where the change must be a **no-op**.
- CLAUDE.md §6: the IPC region contract is a public interface, so "changing them means updating
  every consumer in the same PR". The consumers: `src/components/VideoStage.tsx` (now multiplies by
  `window.devicePixelRatio` before reporting), `src/types/video.ts` (the doc comment changed from
  "CSS px" to "Native device px"), `video::video_set_region`, `surface/mod.rs`
  (`pixels()` / `pixels_over()`), `surface/linux.rs` (now `pixels_over(scale_factor())`),
  `surface/windows.rs` (now `pixels()`). Verify every one moved in the same change, and that the
  Windows change is labelled compiled-not-run.
- **Two rounding rules on two sides of one contract.** `VideoStage.tsx` uses `Math.round`, which
  rounds half **up** (`Math.round(-2.5) === -2`); the Rust side has a test named
  `halves_round_away_from_zero`. Establish whether they agree, and whether negative coordinates are
  reachable (a stage scrolled or laid out off the left edge of the viewport).
- `#[cfg_attr(target_os = "linux", allow(dead_code))]` on `pixels()` and `#[cfg_attr(windows, allow(dead_code))]`
  on `pixels_over()`: each platform compiles a function it never calls. What catches a defect in the
  half this platform does not use, beyond the unit tests? Confirm the unit tests exercise both.
- `a_nonsense_divisor_is_ignored_rather_than_applied`: what counts as nonsense — zero, negative,
  NaN, infinity — and what does `scale_factor()` actually return on a display where GDK is confused?
- `sizes_never_reach_the_window_api_as_zero` against `types/video.ts`'s "Zero in either dimension
  hides the surface". Two invariants, or a contradiction? Trace `an_empty_region_is_recognised_before_it_is_converted`
  and the derived-visibility logic N2 built in `video/mod.rs`.
- `devicePixelRatio` is read once per `report()`. It changes when the window is dragged between
  monitors of different scale — the report itself names DP-5 and DP-4 on that desk. Does anything
  re-report the region when only the ratio changed and the element's CSS size did not? A
  `ResizeObserver` will not fire for that.
- The 16-bit X11 clamp: at 1.5 on a large display, does any coordinate now exceed what X accepts,
  and what happens at the boundary?
- The Xvfb no-op claim: `devicePixelRatio` is 1 there, so a green suite proves nothing about the
  fix. Does the delivery say so, as the AC requires? `scaled-surface-check.js`'s header does say so
  for itself — check the BACKLOG status text and the report say it too.
- The verification screenshot: §4c says capture the window (`import -window <id>`), not the screen.
  Check which was used and whether any synthetic input was involved (see L9).
- **The open suspect**, carried from `n2c-p3-scala.md`: twelve seconds after launch the toplevel
  measured 800x600 against a logged inner size of 1024x700. If N2c's work does not explain it, does
  it survive on the record? A geometry change that silently absorbs an unexplained resize is a
  defect in the making — and `e2e/lib/paths.js` hardcodes `windowWidth = 1024` / `windowHeight = 700`
  as a frozen contract that `findToplevel` searches by, so a self-resizing window is a harness
  problem too.

**Counts as a finding:** one platform moved without the other; an AC declared met by evidence that
cannot show it; the open suspect quietly dropped; a rounding or clamping disagreement across the
IPC boundary.
**Does not count:** the absence of a Windows behavioural run. That is policy, not a defect, provided
it is labelled.
**Most likely false positive:** re-deriving the mechanism and disagreeing with the report's
conclusion that the multiplier must come from the page. The `tao` source argument (`AtomicI32`,
`as f64`) is checkable — check it before contradicting it.

---

### L12 — Documentation that outruns the code

**Report:** `docs/reports/gate2-docs-review.md` · **model:** sonnet

**Question:** do the documentation commits in this range describe the tree that exists?

_Why this earns an agent: four of the seven merged commits are documentation, and the whole gate
regime rests on BACKLOG and the reports being trustworthy. A lens list that only reads Rust leaves
more than half the gate unreviewed._

**Hunt list**

- `3657241`, `323026b`, `5332875`, `18fe5f3` in full, plus the doc portions of `062f201`, `2b31f14`
  and N2c.
- Every file path, function name and script name cited in `docs/reports/n1b-*.md`, `n2b-*.md`,
  `n2c-p3-scala.md`, `docs/design/x11-vs-render-api.md`, `docs/design/decisions.md`: does each
  exist, at the cited line, saying what is claimed? Start with `n1b-branch-probe.mjs` (see L10),
  then `VideoStage.tsx:26-33`, `surface/linux.rs:63-64`, `surface/mod.rs:51`, and
  `tao-0.35.3/src/platform_impl/linux/window.rs:431`.
- Statuses versus criteria. N1b is `[x]` with an AC of thirty sequential `close-gate` runs and a
  delivered-binary table showing three. N2 is `[x]` with an AC demanding a visible-frame assertion
  that `062f201` then removed from the spec — the AC and the spec no longer agree, and nothing in
  the range updated the AC.
- WORKFLOW §4a's gate-2 line as edited by `18fe5f3` still opens "After N2c, immediately before the
  owner's manual checklist" while the register text and the NOW block both say decision 1 moved into
  M2.0 as T3. Read the whole paragraph for sentences that disagree with each other.
- **CLAUDE.md §9 wording sweep, owned by this lens alone.** Grep every document touched in the range
  for a bare "verified" with no platform on it, and for any claim of Windows _behaviour_ as opposed
  to Windows _compilation_.
- Comments carrying numbers. §6 caps comments at 1-2 lines per guard or block. Check `main.rs`'s
  mitigation docstring, `dialog.rs`'s module doc, `video-surface.spec.js`'s new inline block,
  `wayland-attach-check.js:132-146` (fifteen lines asserting an experiment), and
  `scaled-surface-check.js`'s header. Are these measurements that belong in a report rather than
  inline?

**Counts as a finding:** a citation that does not resolve; a status that outruns its own criterion;
two documents in the range contradicting each other; a bare platform-free verdict.
**Does not count:** prose style, length, or keeping refuted hypotheses on the record — §9 wants
exactly that.
**Most likely false positive:** flagging the long explanatory comments without checking §6's actual
wording ("max 1-2 lines per guard/block, reference the issue number") against what the comment is
attached to. A module-level doc comment is not a guard-clause comment: `dialog.rs`'s file-level ones
are probably legitimate, a six-line block inside a `before()` hook probably is not.

---

## 3. Coverage check

Every code file changed in the range is claimed by at least two lenses.

| file                                                  | lenses            |
| ----------------------------------------------------- | ----------------- |
| `src-tauri/src/lib.rs` (close path, `CLOSING`)        | L1, L2, L5        |
| `src-tauri/src/lib.rs` (`startup_files`)              | L2, L8            |
| `src-tauri/src/dialog.rs`                             | L2, L4, L6        |
| `src-tauri/src/main.rs`                               | L4, L7            |
| `src-tauri/src/video/player.rs`                       | L1, L4            |
| `src-tauri/src/video/surface/*`, `video/mod.rs`       | L1, L11           |
| `src/hooks/useStartupFiles.ts`, `src/App.tsx`         | L2, L8            |
| `src/components/VideoStage.tsx`, `src/types/video.ts` | L11, L1           |
| `e2e/lib/env.js`                                      | L1, L7            |
| `e2e/specs/video-surface.spec.js`                     | L1, L3            |
| `e2e/scripts/wayland-attach-check.js`                 | L3, L4, L10       |
| `e2e/scripts/real-session-check.mjs`                  | L4, L7, L9        |
| `e2e/scripts/n1b-load-probe.js`                       | L3, L10           |
| `e2e/scripts/close-gate-check.js`                     | L1, L3, L10       |
| `e2e/scripts/scaled-surface-check.js`                 | L3, L11           |
| `package.json`, `.github/workflows/ci.yml`            | L3                |
| `e2e/lib/pixels.js` (callers)                         | L3                |
| BACKLOG, WORKFLOW, `docs/design/*`, `docs/reports/*`  | L4, L10, L11, L12 |

---

## 4. The workflow shape

Follows WORKFLOW §4a (many lenses in parallel, each with its own hunt list, each writing its report
to a file and then terminating) and §4b (gate reviews are always delegated, start from
`docs/reviews/review-prompt.md`, and the caller reads the file, never the closing message).

### Wave 0 — freeze (orchestrator, minutes)

1. N2c merges to main. **New code stops.** Documentation, M2.0 preparation and planning continue;
   a gate freezes merges and nothing else.
2. Record `GATE_HEAD` (the N2c merge commit) in this file, next to `GATE_BASE=f0b0058`. Every lens
   brief carries both, verbatim, so twelve agents cannot disagree about what they are reading.
3. Run the full battery once and record the result, so a lens finding "the suite is red" can be
   told apart from a lens breaking something.

### Wave 1 — twelve lenses, all parallel (delegated)

All twelve run at once. N2c has merged by Wave 1, so nothing has to wait for it; L11 is not a second
wave any more.

- **opus:** L1, L2, L5, L6, L7, L11 — the lenses that reason about concurrency, lifetime and
  data paths.
- **sonnet:** L3, L4, L8, L9, L10, L12 — the lenses that read evidence, documents and harness code
  against a checklist.

Each agent: reads its brief, reads the diffs with git, writes `docs/reports/gate2-<slug>-review.md`,
**then terminates**. Per §4b, an agent that has written its file and keeps talking is stopped and
treated as finished, and nothing after the file is read. The orchestrator calls `TaskStop` on every
agent when its file exists; no idle or resumable agents survive the wave.

The orchestrator does not review anything in Wave 1. It reads twelve files.

### Wave 2 — dedup and triage (orchestrator, not delegated)

Read all twelve files. Never a closing message. An agent whose report file is missing or empty has
failed and its lens is re-run, not waived.

Deduplication rule, applied in this order:

1. Key every finding by `file:line` **plus defect class**. Same key from two lenses collapses into
   one register entry naming both lenses. Agreement raises confidence; it never raises the count.
2. Same defect at different lines (the GTK button geometry duplicated in two scripts, a bare
   "verified" in four documents) collapses into one entry with a list of sites and one fix.
3. Two lenses reaching **opposite** verdicts on the same code is not split down the middle and not
   averaged: a third read adjudicates, delegated, with both reports as input, and its verdict is
   what enters the register.
4. A finding a lens marked as its own "most likely false positive" still enters the register, with
   the lens's argument for why it is real. The orchestrator rules; the lens does not get to
   pre-dismiss itself.

Output: `docs/reports/gate2-register.md` — one row per surviving finding, with severity, sites,
finding lenses, and the fix owner. This register is the gate's ledger and the thing the exit
condition is checked against.

### Wave 3 — fixes (delegated implementers)

Findings are grouped into clusters that do not share files, per WORKFLOW §5, and one implementer
takes each cluster. Every finding is **fixed, or explicitly ruled on by the owner**. Not triaged,
not deferred with a note (WORKFLOW §4a). A finding the owner rules on is recorded in the register
with the ruling and the date.

Nothing else is touched while passing by (WORKFLOW §4, "never improve things outside the task"): an out-of-scope defect noticed during a fix
becomes a BACKLOG entry, not a silent extra change.

### Wave 4 — adversarial verification (delegated, after the fixes, never the fixer)

This is where the adversarial pass sits: **after** the fixes exist and **before** the gate opens.
WORKFLOW §4b: "A review's own fixes get reviewed. Corrections written under review pressure are new
code, and the next pass hunts explicitly for what they broke." The second N1 review found a blocker
created by a fix from the first one.

Two delegates, parallel, neither of which wrote any of the fixes and neither of which shares a
context with an implementer:

- **V1 — what the fixes broke.** `docs/reports/gate2-fixes-review.md`, opus. Reads only the fix
  diff (`git diff $GATE_HEAD..HEAD`), with L1's question and L2's question applied to it. Carries
  the same refusal: nothing found is not an answer.
- **V2 — closure audit.** `docs/reports/gate2-closure-audit.md`, sonnet. Takes the register row by
  row and, for each, re-runs the original hunt item against the current tree. Verdict per row:
  _closed_ / _not closed_ / _owner-ruled_. It does not accept a fix's own description as evidence;
  it looks at the code. A row it cannot verify is _not closed_.

If V1 or V2 produces a blocking or serious finding, Wave 3 repeats for that finding and Wave 4 runs
again on the new fixes. There is no bound on the loop other than the exit condition.

### Agent count

12 (Wave 1) + up to 1 adjudicator (Wave 2, only on a contradiction) + N implementers (Wave 3,
clustered by file ownership) + 2 (Wave 4), and Wave 4 repeats per round. Every one of them writes a
file and terminates.

---

## 5. The commands, verbatim

Set these once, in every brief, so nobody reconstructs them under pressure.

```sh
cd /home/alcahest/git/SubLore
GATE_BASE=f0b0058          # parent of 062f201; gate 1 covered everything up to here
GATE_HEAD=<n2c merge sha>  # filled in at Wave 0, once N2c is on main
```

The whole gate range, one diff:

```sh
git diff $GATE_BASE $GATE_HEAD
git diff --stat $GATE_BASE $GATE_HEAD
git log --oneline $GATE_BASE..$GATE_HEAD
```

Per commit, message and diff together:

```sh
git show 062f201   # mpv attach + webview paint + harness
git show fee26f8   # native GTK dialogs
git show 3657241   # N1b load probe
git show 323026b   # gate 2 decision document
git show 2b31f14   # close instead of destroy
git show 5332875   # N2c criterion + P3 report
git show 18fe5f3   # gate 2 register
git show $GATE_HEAD  # N2c
```

Only the code, or only the harness, or only the documents:

```sh
git diff $GATE_BASE $GATE_HEAD -- src-tauri/src src
git diff $GATE_BASE $GATE_HEAD -- e2e package.json .github
git diff $GATE_BASE $GATE_HEAD -- BACKLOG.md WORKFLOW.md docs
```

The single files that carry the most unreviewed code:

```sh
git diff $GATE_BASE $GATE_HEAD -- src-tauri/src/lib.rs
git show fee26f8 -- src-tauri/src/dialog.rs      # the file arrives whole in this commit
git diff $GATE_BASE $GATE_HEAD -- src-tauri/src/main.rs
git diff $GATE_BASE $GATE_HEAD -- src-tauri/src/video
git diff $GATE_BASE $GATE_HEAD -- e2e/specs/video-surface.spec.js
```

Files added in the range, which have no "before" to diff against and must be read whole:

```sh
git diff --diff-filter=A --name-only $GATE_BASE $GATE_HEAD
```

Checking a citation actually resolves (L10, L12):

```sh
git log --all --diff-filter=A --name-only --pretty=format: | grep -i '<name>'
grep -rn '<cited path or symbol>' docs/ e2e/ src/ src-tauri/src/
```

The §9 platform sweep (L12 only):

```sh
git diff $GATE_BASE $GATE_HEAD -- BACKLOG.md docs | grep -n '^+' | grep -i 'verified'
```

The fix diff, for Wave 4:

```sh
git diff $GATE_HEAD..HEAD
```

**Ordering, when anything is rebuilt** (WORKFLOW §4c, which has cost two debugging sessions): the
E2E binary is built **last**. `cargo test`, `cargo clippy --all-targets` and `pnpm build` all run
before `pnpm e2e:build`, never after. And a discrimination experiment checks the build's exit status
explicitly — never `pnpm e2e:build >/dev/null 2>&1 && echo ok`.

---

## 6. Exit condition

The gate opens when **all** of these are true. Each is written so it can fail.

1. **Twelve report files exist under `docs/reports/`**, one per lens, each non-empty and each
   containing at least one finding with a severity and a `file:line`. A missing or empty file is a
   failed review, whatever its agent said (§4b). A file whose entire content is "nothing found" is a
   failed review.
2. **`docs/reports/gate2-register.md` exists** and every finding from all twelve reports appears in
   it exactly once, deduplicated by the Wave 2 rule, with the lenses that raised it named.
3. **Every register row is `closed` or `owner-ruled`.** No row reads triaged, deferred, or
   "acceptable for now" without an owner ruling recorded next to it with its date (§4a: fixed, or
   explicitly ruled on by the owner).
4. **V2's closure audit marks every row closed or owner-ruled**, having re-checked each against the
   tree rather than against the fix's description. A row V2 could not verify counts as not closed
   and the gate stays shut.
5. **V1 reports no blocking and no serious finding** against the fix diff. If it does, Wave 3 and
   Wave 4 repeat.
6. **The full battery is green on Linux**, in the §4c order: `pnpm format:check`, `pnpm lint`,
   `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
--workspace`, `pnpm build`, then `pnpm e2e:build`, then `pnpm e2e`, `pnpm e2e:shutdown`,
   `pnpm e2e:close-gate`, `pnpm e2e:scale`. `pnpm e2e:wayland` runs on a Wayland session or is
   recorded as not run, with the reason.
7. **The Windows `check` job is green** on the gate head. Green means compiled. It is not reported
   as working behaviour (CLAUDE.md §5.5).
8. **Every acceptance criterion touched by a fix still passes**, and any test weakened, skipped or
   deleted during Wave 3 is named with its reason (WORKFLOW §4). A silent test change reopens the gate.
9. **BACKLOG.md is updated**: any finding that became a new task is filed, and any status this gate
   showed to be overstated (N1b's `[x]`, N2's visible-frame AC) is corrected or explicitly defended
   in writing.

Failure modes this list is built to catch, stated so they cannot be waved through: a lens returning
a closing message and no file; a register that quietly loses the minor findings; a fix declared done
by its own author; a green suite standing in for the fix that no suite runs; and "verified" with no
platform on it.

---

## 7. What this gate cannot see

Written down here rather than discovered later.

- **The gate runs on Linux.** Every behavioural verdict it produces is a Linux verdict. CLAUDE.md
  §9: write "verified on Linux", never a bare "verified".
- **The Windows branch of `src-tauri/src/dialog.rs` has never been executed anywhere.** It compiles
  in CI and that is all. `#[cfg(not(target_os = "linux"))] ask_close` can never return `Err`, so
  `ask_before_closing`'s error branch is unreachable on Windows — a lens can reason about it, and no
  lens can observe it. The same is true of `report_error`'s Windows twin.
- **The Windows half of the N2c region contract** (`surface/windows.rs` taking `region.pixels()`
  as native) has never run either. Its correctness rests on a unit test and on reading, not on a
  display.
- **The NVIDIA mitigation as shipped may never have run anywhere.** The harness disables it through
  `appEnv`, and the one real-hardware script sets the variable from the outside instead. L7 exists
  to establish that; if it turns out to be true, no wave of this gate can fix it by testing — only
  by running the app with the mitigation armed.
- **N2c's fractional-scale behaviour is provable only on the owner's 1.5 display.**
  `scaled-surface-check.js` says so in its own header: a fractional `devicePixelRatio` cannot be
  produced under Xvfb, measured two ways. The gate can check the arithmetic, the contract and both
  platforms' halves. It cannot see the pixels.
- **N1b is a probability, not a proof.** The battery that produced its verdict would let an unfixed
  defect through about one time in sixty. This gate can audit the instrument and the arithmetic; it
  cannot lower that number.
- **macOS is deferred.** Nothing here has been built or run there, by policy. The only thing the
  gate looks for is design that would block a later port.
- **The full matrix — the behavioural suite green on Windows too — is the exit condition of the
  Windows activation milestone, not of this gate.** That milestone gates any sale or public release.
  This gate opening does not.
