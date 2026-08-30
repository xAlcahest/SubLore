# Gate 2 — L7: process-wide environment mutation and the NVIDIA mitigation

**Lens:** L7, `docs/reviews/gate-2-plan.md` §2.
**Scope:** `GATE_BASE=f0b0058`, `GATE_HEAD=eca9806`, nothing else.
**Platform:** every verdict below is a **Linux** verdict, taken on the owner's own machine. Nothing
here was run on Windows and the mitigation does not exist there (`#[cfg(target_os = "linux")]`).
**Battery:** not re-run. Nothing was edited except this file.

**Question:** is `main.rs`'s environment surgery safe, correctly targeted, and does it cost the user
anything that was measured?

Short answer: it is **safe** — `set_var` on edition 2021 before any thread exists is defined
behaviour and the ordering against GTK is right. It is **not correctly targeted** — the signal it
keys on is the presence of a kernel module, and on the very desk it was measured on that is already
a different question from "NVIDIA is drawing". And the cost is **measured in the one configuration
where the mitigation does not ship**, never against any §7 budget, and never put to the owner.

---

## What I checked

- `src-tauri/src/main.rs` whole file, and its diff in the range (`git diff f0b0058 eca9806 -- src-tauri/src/main.rs`).
- `src-tauri/Cargo.toml` and every `Cargo.toml` in the workspace for the edition; the absence of any
  `rust-toolchain*` file; `.github/workflows/ci.yml`'s Rust setup.
- Every `set_var` / `remove_var` and every `env::var` read in `src-tauri/src` and `crates/*/src`.
- Ordering: what runs before `main.rs:37-38`, what `sublore_lib::run()` does first (`lib.rs:67-73`),
  where the first thread is created, where GTK is initialised.
- `e2e/lib/env.js` whole file and its diff; every caller of `appEnv` (`wdio.conf.js`,
  `shutdown-check.js`, `close-gate-check.js`, `n1b-load-probe.js`, `scaled-surface-check.js`).
- `e2e/wdio.conf.js:19-34` — how `appEnv()` reaches the driver chain.
- `e2e/scripts/wayland-attach-check.js:1-21, 88-96` and `e2e/scripts/real-session-check.mjs:1-130` —
  the two scripts that launch the app outside `appEnv`.
- `.github/workflows/ci.yml` in full: which scripts CI actually runs.
- `README.md`, `e2e/README.md`, `BACKLOG.md`, `docs/design/decisions.md`, `docs/reports/n2b-collaudo-reale.md`,
  `docs/design/x11-vs-render-api.md`, `docs/design/m2-0-tasks.md` for where the escape hatch and the
  cost are documented, and for any owner ruling.
- The machine itself, because the mitigation reasons about hardware: `/sys/module`,
  `/sys/class/drm/card*/device/driver`, `glxinfo -B`, and `strings` on the installed
  `libwebkit2gtk-4.1.so.0.21.9` to confirm the variable is still honoured at WebKitGTK 2.52.5.

---

## Findings

### F1 — `/sys/module/nvidia` answers "is the module loaded", not "is NVIDIA drawing", and this machine already has two GPUs — **serious**, certain

`src-tauri/src/main.rs:26`

```rust
if std::path::Path::new("/sys/module/nvidia").exists() {
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
}
```

The mechanism is not the finding — the comment at `main.rs:24-25` argues for reading the module list
rather than probing GL and the brief rules that argument out of bounds. The **meaning of the signal**
is the finding.

The owner's own machine, read today:

```
/sys/class/drm/card1/device/driver -> /sys/bus/pci/drivers/amdgpu
/sys/class/drm/card2/device/driver -> /sys/bus/pci/drivers/nvidia
```

Two GPUs. On this box NVIDIA does happen to be the GL vendor (`glxinfo -B` reports
`OpenGL vendor string: NVIDIA Corporation`), so the mitigation is right here. But the two questions
have already come apart on the machine the measurement was taken on: "the nvidia module is loaded"
was true of a box where an AMD card is also present and driving four connectors.

**How it fails.** A hybrid-graphics laptop — the common Optimus shape, Intel or AMD driving the
panel, the NVIDIA card loaded for offload and idle — matches `/sys/module/nvidia` exactly. WebKit
there renders on the integrated GPU, where the DMABUF path allocates fine. The user nonetheless gets
`WEBKIT_DISABLE_DMABUF_RENDERER=1` on every launch, loses WebKit's accelerated buffer path for a
driver that is drawing nothing, and pays whatever that costs (F2: nobody knows what it costs on real
hardware). Nothing is logged (F7), nothing is documented (F4), and the user has no reason to suspect
an environment variable exists.

The delivery knew about this. `docs/reports/n2b-collaudo-reale.md:52` lists exactly this option —
"Detect more precisely, i.e. whether NVIDIA is driving _this display_ rather than merely installed" —
and rejects it as having "No cheap way found". That is a defensible engineering call. What is not
defensible is that the rejection never reached the owner as a decision (`docs/design/decisions.md`
contains no entry for it — grepped for `nvidia`, `WebKit`, `DMABUF`, `latency`, `373`: zero hits),
and the code carries no note that the condition is a proxy for the thing it wants to know.

**Correction.** Either narrow the signal — the connector actually backing the display has its driver
at `/sys/class/drm/card*/device/driver`, no GL context and no subprocess needed, which meets the
comment's own cheapness bar — or keep the broad signal and say so in one line at `main.rs:26`, log
the decision (F7), and document the escape hatch (F4) so the users this over-fires on can find it.

---

### F2 — the cost was measured in the one configuration where the mitigation does not ship, and never against a §7 budget — **serious**, certain

`src-tauri/src/main.rs:27`, `BACKLOG.md:94`, `docs/reports/n2b-collaudo-reale.md:65`,
`e2e/lib/env.js:22-26`

CLAUDE.md §7 is "measured, not vibed" and ends: "A PR that regresses a budget states it explicitly
and waits for owner approval." Turning off WebKit's DMABUF renderer is precisely the class of change
§7 exists for. What the range contains instead:

- **The one number that exists** is `373 ms` versus `186 ms` for a keystroke to reach React state
  (`BACKLOG.md:94`, `n2b-collaudo-reale.md:65`, repeated in `e2e/lib/env.js:24-25` and in
  `wayland-attach-check.js:12-14`). It was taken while diagnosing why `asr.spec.js` failed **under
  Xvfb** — that is the whole subject of `n2b-collaudo-reale.md` §4 and §5, and `asr.spec.js` runs
  nowhere else. Under Xvfb the renderer is llvmpipe and the mitigation is switched off in the
  shipping harness (`e2e/lib/env.js:26`). So the only cost figure in the repo describes the
  configuration the mitigation **does not ship in**. What it costs an NVIDIA user, who is the only
  person it ships to, is unmeasured.
- **Cold start and idle memory were not measured at all**, with or without the variable, and the repo
  says so in two places: `docs/design/x11-vs-render-api.md:183` ("no before-number exists anywhere in
  the repo") and `docs/design/m2-0-tasks.md:1321`, which records idle memory as **"never measured"**.
  So there is not even a baseline the change could be judged against.
- **No owner approval is recorded.** `docs/design/decisions.md` has nothing. The commit message of
  `062f201` is a single line with no body (`git log -1 --format=%B 062f201`), so the plan's phrase
  "sitting in a commit message" is inaccurate: the number lives in BACKLOG prose, which is worse,
  because BACKLOG.md:94 frames it as a **harness** problem ("which is what made `asr.spec.js` fail
  under Xvfb; the harness sets the escape hatch") and never as a cost borne by users.

**How it fails.** A user on an NVIDIA card opens Sublore and every interaction is slower than the
app's own budget assumes, by an amount nobody has measured, with no statement anywhere that this was
traded away and no owner sign-off on the trade. If cold start crosses 2 s or idle memory crosses
400 MB because the webview lost its accelerated path, the project finds out at the M2.0 budget task
and cannot attribute it.

**Correction.** Measure the three §7-relevant numbers on the real display with the mitigation armed
and disarmed — cold start, idle PSS, and the same keystroke-to-React figure — and write them into
the N2b entry as a §7 statement. If any budget moves, it goes to the owner as §7 requires. If they
do not move, that sentence is worth more than the current silence.

---

### F3 — the shipping configuration is exercised by exactly one script, that script is in no CI job, and nothing anywhere asserts the mitigation worked — **serious**, certain

Sites: `e2e/lib/env.js:26`; `.github/workflows/ci.yml:189-196`;
`e2e/scripts/wayland-attach-check.js:90-95`; `e2e/scripts/real-session-check.mjs:108`.

The plan's hypothesis for this lens was that "no automated check ever exercises the shipping
configuration" and that `real-session-check.mjs` "bypassed the code under test". **Both are wrong as
stated, and the corrected version is still a finding.** What the tree actually says:

- `appEnv` sets `SUBLORE_WEBKIT_WORKAROUNDS: "0"` (`env.js:26`), and `wdio.conf.js:24` copies that
  onto the runner's own environment, so every wdio spec, `shutdown-check.js`, `close-gate-check.js`,
  `n1b-load-probe.js` and `scaled-surface-check.js` launch the app with the mitigation **disarmed**.
  Those are all four scripts CI runs (`ci.yml:190`, `:193`, `:196`, plus the `check` job which runs
  no app at all).
- On top of that, CI runs on `ubuntu-latest`, which has no NVIDIA kernel module, so even with the
  hatch removed the `main.rs:26` branch would never be taken there. The branch is dead in CI twice
  over, structurally and permanently.
- `wayland-attach-check.js` **does** run the shipping configuration: it deliberately does not use
  `appEnv` (`:90-91`, `env: { ...process.env, XDG_DATA_HOME: dataHome }`), so the hatch is unset and
  `mitigate_nvidia_webview` arms. Its own header says so at `:12-13`. But it asserts map state and
  mpv's child window and **never a pixel** — the whole point of the narrowing recorded at
  `n2b-collaudo-reale.md:35`. A mitigation that stopped firing would leave that check green over a
  window painting nothing. And `pnpm e2e:wayland` appears in no CI job (`ci.yml` runs `pnpm e2e`,
  `e2e:shutdown`, `e2e:close-gate` only).
- `real-session-check.mjs:108` does **not** set `SUBLORE_WEBKIT_WORKAROUNDS`, so the app's own
  mitigation arms there too. What it does is set `WEBKIT_DISABLE_DMABUF_RENDERER: "1"` from the
  outside, which means the run produces the same rendering whether the mitigation fired or not. It
  cannot discriminate. Its header at `:15-17` describes the variable as an external workaround for a
  defect "filed separately", which is the state of the world before `main.rs` gained the mitigation
  in the same commit — the script was never updated to stop applying it.

Net: the code path that decides whether the app paints at all on the primary platform has **zero
automated coverage**, and no document in the range says it has none.

**How it fails.** `WEBKIT_DISABLE_DMABUF_RENDERER` is a WebKit escape hatch, not an API. It is
present in the installed library — `strings /usr/lib64/libwebkit2gtk-4.1.so.0.21.9` finds it, and
that is WebKitGTK 2.52.5 — but a distro upgrade that renames or drops it, or a change to the
`/sys/module` layout, or a refactor of `main.rs`, produces an app that opens a blank window on Linux
with the entire battery green, including `e2e:wayland`. The failure is silent and total: per
`n2b-collaudo-reale.md:9` the window painted "nothing at all".

**Correction.** Three things, none expensive. (a) State the gap in writing, in the N2b BACKLOG entry:
this fix has no check that CI runs. (b) Give `wayland-attach-check.js` — the one script that already
runs armed — a luma or saturation assertion on the app window, which is the measurement
`n2b-collaudo-reale.md:13-17` already used to prove the fix in the first place. (c) Delete
`WEBKIT_DISABLE_DMABUF_RENDERER` from `real-session-check.mjs:108` and let the app's own mitigation
do it, so that script measures the code instead of standing in for it.

---

### F4 — the escape hatch is documented nowhere a user will look, and its contract is exact-string — **serious**, certain

`src-tauri/src/main.rs:20`

```rust
if std::env::var("SUBLORE_WEBKIT_WORKAROUNDS").as_deref() == Ok("0") {
```

The comment at `main.rs:16-19` names the intended user in so many words: "Someone on a driver these
workarounds hurt rather than help can turn them off without rebuilding, which is what any driver
workaround owes its users". That user cannot find the variable. Grepping the whole repo for
`SUBLORE_WEBKIT_WORKAROUNDS` returns `main.rs`, `e2e/lib/env.js`, `BACKLOG.md:94`,
`docs/reports/n2b-collaudo-reale.md:67` and the gate plan. **Not `README.md`.**

That is not an oversight the project can shrug at, because the precedent is right there:
`README.md:140` documents `SUBLORE_FORCE_PANIC`, a **development-only** variable, in prose, with its
accepted values and the builds it applies to. The one variable that a shipping user might genuinely
need is the one that is missing.

The contract compounds it. Only the exact string `"0"` disarms. `SUBLORE_WEBKIT_WORKAROUNDS=false`,
`=no`, `=off`, `=disabled`, `=OFF`, or the empty string all leave the workarounds **armed**, and the
user who set one of them has no feedback of any kind (see F7 — nothing is logged either). A user
following a forum post that says "set it to false" gets the opposite of what they intended and no
way to tell.

**How it fails.** An affected user — F1 says that includes every hybrid-graphics owner — experiences
a slow interface, finds nothing in the README, guesses `=false` from habit, sees no change, and
concludes the app is slow. The escape hatch exists and is unreachable.

**Correction.** One paragraph in `README.md` beside the `SUBLORE_FORCE_PANIC` one, naming the
variable, the exact accepted value, and what it turns off. Either that, or accept any of
`0/false/no/off` and say so.

---

### F5 — `__NV_DISABLE_EXPLICIT_SYNC` is set on every Linux launch, outside the guard that decides the rest of the mitigation, and is inherited by every child process — **minor**, certain (the interaction with `x11egl` is a suspicion)

`src-tauri/src/main.rs:23`

The variable is set before the `/sys/module/nvidia` test and is therefore applied on machines with no
NVIDIA hardware at all — pure AMD, pure Intel, a VM, a CI runner. The docstring's justification
(`main.rs:12-13`, "it costs nothing and it is the step upstream expects to be enough on other driver
versions") is measured on one NVIDIA box; on a machine with no NVIDIA driver "costs nothing" is an
assumption, not the measurement the sentence sits inside.

Two consequences the range never reasons about:

1. **Inheritance.** `set_var` mutates the process environment, so every child inherits it: the
   whisper sidecar and the ffmpeg it drives (`crates/sublore-asr/src/sidecar.rs:291`, `:354`, spawned
   at `:482`), and WebKit's own web process. None of them was in scope when the variable was chosen.
2. **libmpv, in-process.** `video/player.rs` gained `gpu-context=x11egl` in this same range, putting
   mpv on an EGL path inside the process where `__NV_DISABLE_EXPLICIT_SYNC=1` is now global.
   Disabling explicit sync is an NVIDIA EGL/GLX knob; whether it affects mpv's presentation on that
   path is **unmeasured, and I am labelling this a suspicion rather than a defect** — I did not
   observe tearing or stale frames and I did not run the app. The point that stands without it is
   that two changes in one range touch the same EGL surface and neither mentions the other.

**Correction.** Move `__NV_DISABLE_EXPLICIT_SYNC` inside the `/sys/module/nvidia` guard, where the
docstring's own reasoning puts it — the sentence "it is the step upstream expects to be enough on
other driver versions" is a statement about NVIDIA driver versions, so it belongs on the NVIDIA
branch. That also makes the mitigation a single, guarded, describable decision instead of two.

---

### F6 — the escape hatch is one-way: there is no way to force the mitigation on where the module is invisible — **minor**, suspicion

`src-tauri/src/main.rs:20-28`

`SUBLORE_WEBKIT_WORKAROUNDS=0` turns the mitigation off. Nothing turns it on. The mitigation fires
only when `/sys/module/nvidia` is visible in the filesystem the process sees, and that is not the
same as "the driver is present": a container or a sandboxed package (Flatpak's default `/sys` is
restricted) can hide `/sys/module` from the app while NVIDIA is very much what draws the screen. In
that case the app opens the blank window of `n2b-collaudo-reale.md:9` with no in-app way out.

I am labelling this a **suspicion** and not a defect for one honest reason: Sublore ships no bundle
today (`pnpm e2e:build` is `tauri build --debug --no-bundle`, `package.json`), so no packaging that
would hide `/sys` exists yet, and a user can still export
`WEBKIT_DISABLE_DMABUF_RENDERER=1` themselves — WebKit reads it directly, the app is not in the way.
It belongs on the record because packaging is coming and because the workaround a user would need is
undocumented for exactly the same reason as F4.

**Correction.** When F4's README paragraph is written, make the variable symmetric —
`SUBLORE_WEBKIT_WORKAROUNDS=1` forces them on regardless of the probe — and document both values. It
is one extra comparison.

---

### F7 — the mitigation runs before logging exists, so nothing records which rendering path the app chose — **minor**, certain

`src-tauri/src/main.rs:38`, against `src-tauri/src/lib.rs:72` and `lib.rs:110`.

`mitigate_nvidia_webview()` is called at `main.rs:38`, before `sublore_lib::run()`. The log plugin is
registered inside it at `lib.rs:72` (`.plugin(log_plugin())`, "First in the chain, so anything logged
during setup already lands in the file"). So the mitigation **cannot** log, and does not: the app
makes one hardware-dependent decision about how it will draw, before anything else, and leaves no
trace of it.

**How it fails.** README.md:133 tells the user their log is at
`~/.local/share/com.sublore.app/logs/`, and that log is the project's whole diagnostic channel. A
user reports "the interface is sluggish" or "the window is blank". Nothing in the log says whether
`WEBKIT_DISABLE_DMABUF_RENDERER` was applied, whether `/sys/module/nvidia` was found, or whether the
user's `SUBLORE_WEBKIT_WORKAROUNDS` spelling took effect (F4). The first question support would ask
is unanswerable from the artifact the app produces for support.

**Correction.** Have `mitigate_nvidia_webview` return what it did — a small enum or a `bool` pair —
and log one line for it in `.setup()` next to the existing
`log::info!("Sublore {} starting on {}", ...)` at `lib.rs:110`. One line, and it also gives F3's
missing check something cheap to assert on.

---

### F8 — the single-thread precondition for `set_var` is nowhere in the code, and the edition is unpinned by any toolchain file — **minor**, certain

`src-tauri/src/main.rs:23`, `:27`, `:37`; `src-tauri/Cargo.toml:5`.

**The calls are sound today, and I want that on the record before the finding.** Verified rather than
assumed:

- `edition = "2021"` in `src-tauri/Cargo.toml:5`, and in all five workspace crates, so `set_var` is a
  safe fn and the 2024 `unsafe` rule does not apply.
- Nothing runs before `main`: grep for `ctor`, `lazy_static`, `#[used]` and `gtk::init` across
  `src-tauri/src` and `crates/*/src` finds none.
- No thread exists at that point. `sublore_lib::run()` begins with `crash::install()`
  (`lib.rs:68`), which only swaps the panic hook (`crash/mod.rs:47-52`) and spawns nothing; the
  crash-dialog thread at `crash/mod.rs:183-185` is created from the panic hook, far later. GTK is
  initialised inside Tauri's builder, after both `set_var` blocks.
- Every other environment read in the tree happens after `run()` starts —
  `crates/sublore-asr/src/tools.rs:117` and `:160`, `src-tauri/src/crash/force.rs:47`,
  `crates/sublore-io/src/fault.rs:75` — so there is no concurrent read racing the writes.
- Ordering against the other write is right: `GDK_BACKEND=x11` at `main.rs:37`, mitigation at `:38`,
  both before GTK picks a backend.

The finding is what happens next. There is **no `rust-toolchain.toml`** in the repo (checked), and CI
takes `dtolnay/rust-toolchain@stable` (`ci.yml:89`, `:164`) — a floating stable. The edition is
pinned per-crate, so the move to 2024 is a deliberate act, and it will be a **loud** failure: on
edition 2024 `std::env::set_var` is `unsafe` and these three lines stop compiling. That is why this
is minor rather than serious.

**How it fails.** The mechanical response to that compile error is to wrap the three calls in
`unsafe {}` and move on, because nothing in `main.rs` states what makes them safe. The precondition —
no thread has been created yet, and nothing may be moved above these lines — lives only in the head
of whoever wrote them. The day someone adds an initialisation step above `main.rs:37` that spawns a
thread (a crash handler that installs earlier, a logger, a single-instance socket), the `unsafe`
block is silently wrong and there is no comment to contradict them.

**Correction.** One line at `main.rs:36`, in the project's own comment budget: these run on the
process's only thread, before anything is spawned; nothing may be added above them.

---

## Hunt items I found sound, and why

- **`set_var` is not unsound.** The brief names this as my most likely false positive and it is
  correct: on edition 2021, in a single-threaded `main`, before `tauri::Builder`, this is defined
  behaviour. I verified the edition, the absence of pre-`main` initialisers, that `crash::install()`
  spawns no thread, and that every `env::var` reader in the tree runs later. F8 is about the edition
  boundary and the undocumented precondition only, exactly as the brief scoped it.
- **Ordering against `gtk_init` and against `GDK_BACKEND`.** `main.rs:37` sets `GDK_BACKEND=x11`,
  `:38` calls the mitigation, and GTK is initialised inside `sublore_lib::run()` well after both.
  Both variables are in place before any GTK, GDK or WebKit code has looked at the environment.
  Sound.
- **`GDK_BACKEND` cannot disagree between `main.rs` and `appEnv`.** Both set the literal `"x11"`
  (`main.rs:37`, `env.js:21`). `main.rs` sets it unconditionally, so it wins over anything the
  harness or the user's shell supplied — meaning they cannot disagree in a way that reaches GTK. The
  one consequence worth naming and not a defect: a caller passing `appEnv({ GDK_BACKEND: "wayland" })`
  would be silently overruled by the binary. That is the documented intent (`main.rs:33-34`), and no
  caller does it.
- **`Object.assign(process.env, appEnv())` at `wdio.conf.js:24` does not lose the `WAYLAND_DISPLAY`
  deletion.** I expected it to — `Object.assign` merges keys and cannot express a deletion, so the
  helper's `delete env.WAYLAND_DISPLAY` at `env.js:29` has no effect through that call site. It is
  covered because `wdio.conf.js:25` deletes it again explicitly on the next line. The comment at
  `:22-23` ("One rule, one place: `appEnv` owns it") is contradicted by the line under it, but
  `wdio.conf.js` is unchanged in this range (`git diff --name-only f0b0058 eca9806 -- e2e/wdio.conf.js`
  is empty; last touched by `d224f3c`), so it is out of scope and I am not filing it. Recording the
  refutation so nobody re-derives it.
- **`SUBLORE_WEBKIT_WORKAROUNDS` does propagate to the app through wdio.** Unlike the deletion, a set
  key survives `Object.assign`, so the whole wdio suite really does run disarmed. That half of the
  plan's hypothesis holds and is the substance of F3.
- **The `overrides` parameter is ordered correctly.** `env.js:27` spreads `...overrides` after
  `SUBLORE_WEBKIT_WORKAROUNDS: "0"`, so a caller that wanted to arm the mitigation could. None does,
  which is F3, but the helper is not the obstacle.
- **`.as_deref() == Ok("0")` handles a non-UTF-8 value correctly.** `env::var` returns
  `Err(NotUnicode)` there, the comparison fails, and the workarounds stay armed — the safe default,
  since armed is the state that makes the app visible. Sound.
- **The variable is still honoured by the installed WebKit.** `strings` on
  `/usr/lib64/libwebkit2gtk-4.1.so.0.21.9` (WebKitGTK 2.52.5) contains
  `WEBKIT_DISABLE_DMABUF_RENDERER`. The mitigation is not a no-op on this machine. That it is an
  undocumented escape hatch rather than an API is folded into F3, not filed separately.
- **`nouveau` does not trigger the mitigation.** Its module is `nouveau`, not `nvidia`, so
  `/sys/module/nvidia` is absent and a nouveau user gets nothing. Correct behaviour, since the GBM
  failure is the proprietary driver's.
- **No test-awareness leaked into production code.** The escape hatch is a general user-facing
  variable read once in `main.rs`; there is no `cfg!(test)`, no `SUBLORE_E2E` branch, and no
  debug-only path in the mitigation. This is the right shape and the delivery deserves the credit
  (`n2b-collaudo-reale.md:67` argues it explicitly).
- **The mitigation cannot be reached twice or re-entered.** It is called once from `main`, sets
  variables, and returns; there is no later caller and no state.
