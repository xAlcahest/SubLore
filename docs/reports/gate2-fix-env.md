# Gate 2 — Wave 3 fixes: the NVIDIA mitigation, its record, and the two CI gaps

**Cluster:** `src-tauri/src/main.rs`, `e2e/lib/env.js`, `.github/workflows/ci.yml`, `package.json`.
**Base:** `GATE_BASE=f0b0058`, `GATE_HEAD=eca9806`. Rows taken from `docs/reports/gate2-register.md`,
read together with the lens reports behind them (L7 `gate2-env-mitigation-review.md`, L4
`gate2-platform-claims-review.md`, L11 `gate2-n2c-region-review.md`, L3 `gate2-test-honesty-review.md`,
L1 `gate2-regressions-review.md`, L12 `gate2-docs-review.md`).

**Platform of every verdict below: Linux, on the owner's machine, under Xvfb.** Nothing here was run
on Windows and nothing here was run on the real display: the one run this cluster still needs on the
owner's own session is named in §3 and belongs to the orchestrator under WORKFLOW §4c.

**One new file was created outside my four:** `e2e/scripts/webview-paint-check.js`. The owner's
requirement was to build the instrument, and an instrument has to live somewhere; a new file cannot
conflict with another implementer's edits, which is what the ownership rule protects. It is wired in
through `package.json`, which is mine. Flagging it because it is outside the list I was given.

---

## 1. What changed

**`src-tauri/src/main.rs`**

- The escape-hatch parse became a pure function, `nvidia_workarounds_wanted(hatch, nvidia_module)`,
  with four unit tests. It accepts `0/false/no/off` to disarm (case-insensitive, trimmed) and
  `1/true/yes/on` to force the workarounds on where the probe cannot see the driver. An unrecognised
  value leaves the module probe deciding.
- `__NV_DISABLE_EXPLICIT_SYNC` moved inside the guard: both variables are now set by one decision,
  or neither is.
- One `eprintln!` records the decision — what it applied, what the hatch said, whether
  `/sys/module/nvidia` was there. This is the app's only record of which rendering path it chose.
- The 9-line measurement-heavy docstring is now 3 lines pointing at
  `docs/reports/n2b-collaudo-reale.md`; the `/sys/module` comment says in one line that the module
  is a proxy for "NVIDIA is drawing" and not the same question; the `set_var` block in `main` carries
  its single-thread precondition.

**`e2e/scripts/webview-paint-check.js`** (new, `pnpm e2e:webview`) — the armed-configuration check.
Launches the app twice, armed (nothing added to the session's environment, so the app's own detection
decides) and disarmed, and asserts five things: the decision line exists, it agrees with this
machine's `/sys/module/nvidia`, the armed run really did read the hatch as unset, **the window
painted**, and the hatch turns the workarounds off. Paint is the luma range of the window's own
pixels, captured with `import -window <id>` per WORKFLOW §4c, not a root grab. It refuses to run if
`WEBKIT_DISABLE_DMABUF_RENDERER` is already set in the shell, because such a run could not tell the
app's mitigation from the outside variable — the discrimination `real-session-check.mjs` lacks.

**`e2e/lib/env.js`** — the comment now says what the disarming costs (every caller tests a
configuration no user gets) and names the check that covers the armed one.

**`.github/workflows/ci.yml`** — the `e2e` job gained a `pnpm e2e:scale` step.

**`package.json`** — `e2e:webview` added.

## 2. Row by row

### `e2e/lib/env.js:26` — L7 F3, promoted to blocker — **fixed, with one half still owed on the real display**

The instrument exists and runs: `pnpm e2e:webview`. Run under Xvfb on this machine, where
`/sys/module/nvidia` is present, so the armed launch took the branch and applied both variables —
the app itself said so: `sublore: webview workarounds applied (SUBLORE_WEBKIT_WORKAROUNDS=unset,
/sys/module/nvidia present)`. The window measured 16..235, which is the same reading
`n2b-collaudo-reale.md` recorded for the interface on real hardware. Five of five checks.

**What proves it can fail.** A discrimination experiment, with the build's exit status checked
explicitly rather than chained behind `&&` (WORKFLOW §4c): `mitigate_nvidia_webview` was taken out of
`main`, `pnpm e2e:build` was run and its exit status confirmed `0`, the rebuilt binary was confirmed
by hand to no longer print the decision line, and the check then failed at its first assertion:
`webview paint check failed: the armed launch recorded the rendering path it chose`. The mitigation
was restored and the binary rebuilt; the first restore silently did not happen (an interactive `cp`
prompt), which is exactly the trap §4c describes, so the restore was verified by grep and the binary
by running it, before the check was re-run green.

The measure separates flat from painted: a synthetic flat image through the same signalstats parse
reads `YMIN=YMAX=126`, range 0, against the window's 219. The threshold is 32.

**What is still owed.** On this machine under Xvfb, llvmpipe renders and **both** configurations
paint, so this run proves the mitigation path executes and the window is not blank — it does not
prove the mitigation is what makes the window paint. Only the owner's real display can show that,
and the check prints the pair for exactly that purpose: armed paints, disarmed goes flat, and it
says so in words. That run is the orchestrator's, per §4c.

**One limitation, said out loud:** a luma range cannot tell the app's interface from a dev-mode
binary's error page. Run against a plain `cargo build` binary before `pnpm e2e:build`, this check read
0..255 and passed. It asserts "not blank", not "the right page".

### `.github/workflows/ci.yml:196` (L11 F2) and `package.json:19` (L3 #6) — **fixed**

`pnpm e2e:scale` now runs in the `e2e` job. Verified by running the exact command the step runs,
`xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e:scale`: green, 5/5, toplevel 1024x700 then
2048x1400, surface 736x159 then 1472x320. The 2048x1400 window is larger than the screen and this
bare X server serves it without clamping, so the step needs no bigger screen than its neighbours.
The job already installs everything it needs (`x11-utils`, `python3-xlib`, the video fixture).

What can fail: L11 traced it — delete `* ratio` from `VideoStage.tsx` and the surface stops doubling,
which is the assertion at `scaled-surface-check.js:162-169`. That check's own first-assertion defect
was fixed in parallel by another implementer; my step runs their fixed version.

### `src-tauri/src/main.rs:14` (L4 #1) and `:26` (L7 F1) — the module is the wrong signal — **not fixed as stated, deliberately; the three consequences are fixed**

L7 offered two corrections. I did not take the narrowing, and the reason is a measurement:

```
/sys/class/drm/card1/device/driver -> amdgpu     card1-DP-1..3, HDMI-A-1: all disconnected
/sys/class/drm/card2/device/driver -> nvidia     card2-DP-4: connected, card2-DP-5: connected
```

On this machine the narrow signal (a _connected_ connector on an NVIDIA-driven card) would still
fire, so it would not regress the owner. But on a PRIME-offload laptop where the panel hangs off the
integrated GPU while NVIDIA renders the webview, the connected connector belongs to the iGPU and the
narrow signal would **disarm** the mitigation — turning today's unmeasured slowdown into the blank
window of `n2b-collaudo-reale.md:9`. Under CLAUDE.md's own failure-mode ordering that trade goes the
wrong way, so the broad signal stays and is now named as a proxy in one line at the site, the
decision is recorded (F7), and the hatch works in both directions (F6). A user the mitigation
over-fires on can now find out from the app's own stderr and turn it off; a user it under-fires on
can force it on.

**This is a judgement call on a row the owner may want to rule on differently.** The data he needs to
overrule me is the block above: narrowing is cheap and would work here, and its cost is paid by
somebody else's laptop.

### `src-tauri/src/main.rs:20` (L7 F4, F6) — the hatch — **half fixed**

Fixed: the exact-string contract. `false`, `no`, `off`, `OFF`, `0` now disarm, and `1/true/yes/on`
force the workarounds on where `/sys/module` is invisible (a container, a Flatpak sandbox). Proved by
four unit tests; proved that they can fail by compiling the _old_ behaviour with the same tests in a
scratch file — 2 of 4 fail, on `false/OFF/" 0 "` and on the uppercase force-on values.

**Not fixed: the documentation.** F4's correction is a paragraph in `README.md` beside the
`SUBLORE_FORCE_PANIC` one, and `README.md` is not in my file list. The variable a shipping user might
need is still documented nowhere they will look. This needs one paragraph naming the variable, the
accepted values (`0/false/no/off`, `1/true/yes/on`) and what it turns off.

### `src-tauri/src/main.rs:27` (L7 F2) — the cost — **not fixed**

The check now prints, for both configurations, the time from spawn to the first capture that is not
flat. That is an instrument, not the answer. Four runs on this Xvfb spread from 698 ms to 2102 ms
with no consistent ordering between armed and disarmed, so a single pair is not a measurement, and
under Xvfb it is llvmpipe being measured, not the driver the mitigation ships for. Cold start and
idle PSS on the real display, armed and disarmed, still have to be taken, and the §7 statement
belongs in the N2b entry of `BACKLOG.md`, which is not my file, and needs the owner's sign-off.

### `src-tauri/src/main.rs:23` (L1 #5, L7 F5) — `__NV_DISABLE_EXPLICIT_SYNC` outside the guard — **fixed**

It is inside now: one decision sets both variables or neither, so a machine with no NVIDIA module —
a CI runner, a pure AMD or Intel box, a VM — no longer has it set process-wide, and the whisper
sidecar and libmpv no longer inherit it there.

**Honest about the proof:** the branch that controls it is unit-tested, and the decision line reports
which way it went. There is no automated assertion that the variable is _absent_ from the process,
because a process's own `set_var` is invisible from outside — `/proc/<pid>/environ` shows the environ
at `execve`. That half is proved by reading the seven lines, not by a check.

### `src-tauri/src/main.rs:38` (L7 F7) — nothing records the rendering path — **half fixed**

The decision now goes to stderr, and `e2e/scripts/webview-paint-check.js` asserts it is there and
agrees with the machine. It does **not** reach `~/.local/share/com.sublore.app/logs/`, which is where
`README.md:133` sends a user with a problem: the log plugin is registered inside `run()` in
`src-tauri/src/lib.rs`, another implementer's file this wave, and carrying the decision into it means
`run()` taking it as an argument or a shared cell. Left undone rather than reached across.

### `src-tauri/src/main.rs:37` (L7 F8) — the `set_var` precondition — **fixed**

Two lines at the site: both writes run on the process's only thread, before anything is spawned and
before GTK reads the environment, and nothing that creates a thread may be added above them. It is a
comment, so nothing can prove it; it exists so that the day edition 2024 makes these `unsafe`, the
person wrapping them in `unsafe {}` reads what they are promising.

### `src-tauri/src/main.rs:4` (L12 #5) — docstring over the comment budget — **fixed**

Nine content lines with an RTX 5070 Ti, a driver version and two luma ranges are now three lines
naming the defect and pointing at `docs/reports/n2b-collaudo-reale.md`. The measurements did not move
into another comment; they stay in the report that took them.

## 3. What the orchestrator still has to run

On the owner's real display, in his own session, launch only — no keyboard, no mouse (§4c):

```
pnpm e2e:webview
```

Expected there, if the mitigation is doing its job: `armed` paints (a luma range in the hundreds) and
`disarmed` reads flat, followed by the line "Disarmed, this machine's window is flat: the mitigation
is what makes it paint." That output is the evidence the blocker asks for and it is the one thing
this cluster could not produce for itself. If the disarmed run also paints, the mitigation is not
what makes the window visible on that machine any more, and the whole N2b conclusion needs revisiting.

`WEBKIT_DISABLE_DMABUF_RENDERER` must not be exported in that shell; the check refuses to run if it
is, and says why.

## 4. Deliberately not done, and why

- **`pnpm e2e:webview` is not in a CI job.** `ubuntu-latest` has no NVIDIA module, so the branch under
  test cannot be taken there at all, the armed and disarmed launches would be byte-identical
  environments, and it would need `imagemagick` added to the job plus a pixel measurement never taken
  on that runner. The exclusion and its reason are in the script's own header, which is the standard
  L3 applies. It is two lines to add if the owner wants it anyway — an apt package and a step — and
  the assertion would still be real there (a no-NVIDIA Linux user is a shipping configuration).
- **No test was weakened, skipped or retargeted.** Nothing was deleted; the four unit tests are new.

## 5. State of the battery, and one hazard

- `cargo fmt --check`: clean. `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test -p sublore --bin sublore`: 4/4, the new tests.
- `cargo test --workspace`: **red**, 52 passed / 2 failed, both in `src-tauri/src/dialog.rs`
  (`a_dialog_that_goes_away_unanswered_answers_cancel`,
  `a_callback_dropped_before_any_dialog_answers_cancel`, both panicking with `an answer: Disconnected`).
  That file is another implementer's, mid-edit while I ran it. Not caused by anything in this cluster,
  and it needs re-running once their work lands.
- `pnpm lint`: clean. `pnpm format:check`: three files fail, all another implementer's
  (`close-gate-check.js`, `real-session-check.mjs`, `wayland-attach-check.js`); mine pass.
- `pnpm e2e:scale` and `pnpm e2e:webview`: green under Xvfb, verified on Linux.

**The hazard:** this wave's implementers share one working tree and one `target/`. My discrimination
experiment left a binary without the mitigation in `target/debug/sublore` for roughly two minutes,
and at one point `cargo clippy` failed on a `log::info!` in `video/player.rs` that was mid-edit in
another lane. Any behavioural verdict taken during that window is unreliable. The tree ends with a
current `pnpm e2e:build` binary that prints the decision line, verified after the last edit.

## 6. Noticed while passing, not touched (WORKFLOW §4)

- An untracked file literally named `--help` sits in the repo root. It looks like a redirected
  `pnpm --help`. Not mine; it was there before this wave started.
- `real-session-check.mjs` still sets `WEBKIT_DISABLE_DMABUF_RENDERER` from the outside, so it cannot
  discriminate the app's mitigation from its own variable (L7 F3's correction (c)). That file belongs
  to another implementer this wave and was being edited while I worked; if their pass does not remove
  it, the row stays open.
- `e2e/README.md` has no entry for `scaled-surface-check.js`, `wayland-attach-check.js` or the new
  `webview-paint-check.js`. Its spec table predates all three. Not my file.
