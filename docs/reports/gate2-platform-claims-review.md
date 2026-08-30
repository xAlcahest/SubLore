# Gate 2 — L4: platform claims checked on one machine only

Scope: `f0b0058..eca9806`. Question: which behavioural claim in this range holds only on the
author's specific hardware, driver, compositor or display, and is written as though it holds
generally?

## What I checked

- `src-tauri/src/main.rs` (`mitigate_nvidia_webview`), read whole, against every place it is
  referenced: `e2e/lib/env.js`, `docs/reports/n2b-collaudo-reale.md`, `BACKLOG.md` N2b.
- `src-tauri/src/video/player.rs:174-195` (the `gpu-context=x11egl` option) against
  `docs/reports/n2b-probe.md` and `docs/reports/n2b-collaudo-reale.md`.
- `docs/reports/n2c-p3-scala.md` in full, and every place its numbers are cited
  (`src-tauri/src/video/surface/mod.rs`, `BACKLOG.md` N2c, `docs/design/x11-vs-render-api.md`).
- `e2e/scripts/scaled-surface-check.js` and `e2e/scripts/wayland-attach-check.js` headers, and
  `.github/workflows/ci.yml`'s `e2e` job, to see which of the two run automatically anywhere.
- `src-tauri/src/dialog.rs`'s `#[cfg(not(target_os = "linux"))]` halves of `ask_close` and
  `report_error`, against `lib.rs:216-238`'s `ask_before_closing`, and
  `src-tauri/src/video/surface/windows.rs`'s `set_region`, against the Linux side of the same
  contract in `surface/mod.rs`.
- `e2e/scripts/close-gate-check.js` and `e2e/scripts/n1b-load-probe.js`'s hardcoded GTK button
  geometry (`buttonWidth = 96`, gap `12`), and how a wrong click is caught downstream.
- `BACKLOG.md`'s N2b and N2c status lines against the scripts and reports they cite, for whether the
  platform scoping stated there survives contact with the actual test wiring.

## Findings, most severe first

### 1. [serious] The NVIDIA DMABUF mitigation fires on every Linux machine with the module loaded, including ones where it should not, and the repo's own evidence says so

**File:** `src-tauri/src/main.rs:14-29`

```rust
#[cfg(target_os = "linux")]
fn mitigate_nvidia_webview() {
    ...
    std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
    if std::path::Path::new("/sys/module/nvidia").exists() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}
```

The doc comment above it (lines 4-13) states the measurement plainly: "Measured on an RTX 5070 Ti
with driver 610.57.04" — one machine, one driver — and from that one measurement derives a rule
applied unconditionally to every Linux user for whom `/sys/module/nvidia` exists. That file exists
whenever the `nvidia` kernel module is loaded, which says nothing about which GPU is actually doing
the drawing: on any NVIDIA Optimus/PRIME hybrid-graphics laptop where WebKitGTK renders through the
integrated Intel or AMD GPU, the module is loaded and this still fires, forcing off the DMABUF
renderer on a stack that never had the bug the workaround exists for.

This is not a hypothetical reading of the code — the repo has already measured this exact
imprecision, just in the other direction. `docs/reports/n2b-collaudo-reale.md:47` (added in this
range, `062f201`): "The detection keys on the NVIDIA module being loaded, which is true on this
machine even under Xvfb, where rendering is llvmpipe and the workaround is not needed." The fix
applied there was `SUBLORE_WEBKIT_WORKAROUNDS=0` baked into the E2E harness's `appEnv`
(`e2e/lib/env.js:26`) — but that only covers the one environment the author personally tests in.
The hybrid-laptop case is the same failure mode (module loaded, wrong GPU drawing) and has no
harness to catch it, no escape hatch turned on by default, and is not mentioned anywhere in
`BACKLOG.md` or the design docs added in this range.

**Failure:** a user on an NVIDIA Optimus laptop, GDM/desktop session with WebKitGTK rendering
through the Intel iGPU (the common default on such laptops), launches Sublore. `/sys/module/nvidia`
exists because the discrete GPU's module is loaded (even if unused, or used only for the discrete
GPU should a game demand it). `WEBKIT_DISABLE_DMABUF_RENDERER=1` is set before the webview is
created, disabling a renderer that was working fine on the iGPU, for a bug that only exists on the
NVIDIA proprietary driver's DMABUF path. Never exercised by CI or the local E2E harness — both force
the workaround off — so this exact branch has run, as shipped, only on the one RTX 5070 Ti box.

### 2. [serious] `gpu-context=x11egl` is forced for every `wid`-backed player on Linux, verified only on the author's NVIDIA+EGL stack, with no escape hatch if it is wrong elsewhere

**File:** `src-tauri/src/video/player.rs:186-191`

```rust
if let Some(wid) = config.wid {
    init.set_option("wid", wid)?;
    #[cfg(target_os = "linux")]
    init.set_option("gpu-context", "x11egl")?;
}
```

This is unconditional whenever the app embeds video with a native window id — i.e. on every
non-headless Linux launch — and it replaces mpv's own `auto` selection, which the codebase itself
says is normally correct (`docs/reports/n2b-probe.md:26`, `SurfaceRegion`'s own history). The only
evidence behind pinning `x11egl` specifically is `docs/reports/n2b-probe.md`, run "on Linux, Fedora,
real Wayland socket present... mpv 0.41.0" — the same single machine as finding 1 — by invoking the
`mpv` binary directly against a python-xlib X11 window, not the app itself, and not on any
non-NVIDIA GPU. `player.rs`'s own comment (line 36 of the probe doc, echoed at
`docs/reports/n2b-probe.md:36`) accepts that "if EGL is ever missing, mpv fails to build its output
and `video_open` returns an error" as "the acceptable direction for this failure" — but that
acceptance was written against the hypothetical of EGL being absent, not against the case where EGL
is present but a different `gpu-context` was what `auto` would correctly have picked on that user's
driver stack (e.g. non-NVIDIA Wayland-native EGL setups, or older/nouveau/AMD combinations that
`auto` handles today and `x11egl` has never been tried against). Unlike the NVIDIA mitigation above,
there is no `SUBLORE_`-prefixed escape hatch for this option at all — a user on hardware where
`x11egl` is wrong has no way to opt back into `auto` without rebuilding.

**Failure:** a Linux user on a GPU/driver stack where mpv's `auto` gpu-context selection would have
picked something other than `x11egl` (or where `x11egl` itself is broken, e.g. certain Mesa/EGL
combinations not covered by the single Fedora+NVIDIA probe) opens any video. `video_open` returns an
error — CLAUDE.md §1 lists "video playback... via embedded libmpv" as v1.0 scope, not a feature that
may fail outright depending on which GPU the user owns — and there is no environment variable to
fall back to the previous, working `auto` behaviour.

### 3. Windows-only `dialog.rs` and `surface/windows.rs` code paths compile but have run zero times — correctly disclosed, not a new finding on its own

**Files:** `src-tauri/src/dialog.rs:83-121`, `:146-155`; `src-tauri/src/video/surface/windows.rs:57-59`

Confirmed: the non-Linux `ask_close` (`dialog.rs:84-121`) always returns `Ok(())` — there is no
path in it that returns `Err`, so `ask_before_closing`'s error arm (`lib.rs:233-238`,
`GATE_OPEN.store(false, ...)` plus the log line) is unreachable on that platform as shipped.
Similarly `surface/windows.rs:59` now calls `region.pixels()` (divisor 1.0) instead of the removed
`region.physical()`, matching what `docs/reports/n2c-p3-scala.md` says the Windows side needs, but
that correctness is asserted by reading the code, not by running it.

This is not counted as a finding on its own: CLAUDE.md's platform policy (2026-08-29) and every
BACKLOG entry in this range say plainly that Windows compiles and is not behaviourally verified
until the MW milestone, and nothing in the diff claims otherwise for these two files — I searched
`BACKLOG.md`, `docs/design/*` and the N2c report for a Windows-verified claim on either path and
found none. Recorded here because the hunt list asked the question directly: the answer is "merely
compiling," not "correct there," and MW.2's own acceptance criterion (`BACKLOG.md:213`,
"`video/surface/windows.rs` reasserts `HWND_TOP`") already names this file for exactly that reason.

## Hunt items checked and found sound

- **`docs/reports/n2c-p3-scala.md`.** Explicitly separates what generalises (the `tao`
  `AtomicI32` reading at line 22, sourced from `tao-0.35.3/src/platform_impl/linux/window.rs:431`,
  a fact about the dependency, not the display) from what does not (everything measured through
  `xrandr`/`spectacle`/the app on `:0`, all labelled "measured on the owner's session" or "on that
  display"). The one loose end it carries — the 800x600 `xwininfo` reading that did not reproduce
  — is left open rather than explained away ("one clean launch is not an explanation," line 75).
  This is the honest, explicitly-scoped writing the brief says is the standard, not a defect.

- **`e2e/scripts/scaled-surface-check.js` and `e2e/scripts/wayland-attach-check.js` headers.** Both
  state their own scope in the file a reader would open to check the claim: the scale check's
  header says "N2c's own criterion is met on the owner's 1.5 display, and nowhere else" and that it
  "does not prove N2c" on purpose; the Wayland check's header says outright "Needs a real Wayland
  socket, so it runs on a machine with a Wayland session and is **not part of the headless Linux CI
  job**." I checked `.github/workflows/ci.yml`'s `e2e` job (lines 124-196) and confirmed neither
  `pnpm e2e:scale` nor `pnpm e2e:wayland` appears there — only `pnpm e2e`, `e2e:shutdown` and
  `e2e:close-gate` run in CI. This matches what the scripts themselves say, so `BACKLOG.md`'s
  "verified-by-tests on Linux" language for N2b/N2c (lines 93, 104), read together with "and
  separately verified... on the owner's own Wayland session," is not a claim stated without its
  platform — the platform is named twice, and the one-hop-away script header confirms it rather
  than contradicting it.

- **Hardcoded GTK button geometry in `close-gate-check.js:124-125` and
  `n1b-load-probe.js:52-53`** (`buttonWidth = 96`, gap `12`). This is a real portability
  assumption — a different GTK theme sizing buttons differently would change where the correct
  click lands — but both scripts disclose it as an estimate at the point of definition
  (`close-gate-check.js:113-115`: "these numbers are an estimate; the caller proves the click
  landed by watching the dialog disappear") and both are structured so a wrong click cannot pass:
  `waitForDialogGone` throws if the dialog does not close, and if a miscalculated click instead
  lands on a _different_ button (e.g. Cancel instead of Discard), the assertions after it —
  `state.exit === null` after cancel, `readFileSync(...).equals(original)` after discard, the
  block-diff check after save (`close-gate-check.js:270-347`) — are keyed on the actual observed
  outcome (file bytes, exit code), not on "a dialog went away," so a wrong-button click fails loud
  rather than passing for the wrong reason. This is the self-verifying structure the brief's
  "does not count" bar describes, not a silent platform-only pass.

## Note on scope

I did not re-litigate `docs/reports/n2c-p3-scala.md`'s own honesty (already covered above) or
duplicate L12's bare-"verified" sweep. I did not treat "Windows unverified" as a finding on its own
anywhere in this range — CLAUDE.md's platform policy makes that the explicit, correct standard for
this milestone, not a defect (see the brief's own "does not count" and "most likely false positive"
notes). Findings 1 and 2 above are not about missing macOS or Windows coverage; they are about a
mitigation and an option keyed on signals (`/sys/module/nvidia` existing; one machine's `mpv
--gpu-context` probe) that the repo's own evidence already shows do not mean what the code assumes
once you leave the author's own NVIDIA desktop, while still targeting the declared primary platform.
