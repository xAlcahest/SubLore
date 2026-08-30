# Gate 2 — L9: the harness on the owner's real display

**Scope:** `GATE_BASE=f0b0058`, `GATE_HEAD=eca9806`. Question: does
`e2e/scripts/real-session-check.mjs` obey WORKFLOW §4c, the rule that `062f201` introduced in the
very same commit that added the script.

## What I checked

- Read WORKFLOW.md §4c in full (`WORKFLOW.md:75-79`) and confirmed its exact wording against
  `git show 062f201 -- WORKFLOW.md`: the rule was added by `062f201`, in the same commit as
  `e2e/scripts/real-session-check.mjs` (170 new lines) and `e2e/scripts/wayland-attach-check.js`
  (181 new lines).
- Read `e2e/scripts/real-session-check.mjs` in full (170 lines).
- Read `e2e/lib/input.js` in full to confirm what `typeText`/`clickAt`/`focusWindow` actually send
  (real XTEST events, not synthetic DOM events — the file says so itself at `input.js:3-6`).
- Read `docs/reports/n2b-collaudo-reale.md` in full — the report `062f201` also commits, which
  narrates the incident that produced §4c and the fixes applied afterward.
- Compared `real-session-check.mjs` against its sibling `wayland-attach-check.js`, added by the
  same commit, to see whether both scripts were brought into line with the new rule or only one.
- Confirmed `e2e/lib/paths.js`'s `requireAppBinary()` contract (`paths.js:37-40`) and checked which
  scripts under `e2e/scripts/` use it versus construct the binary path themselves
  (`grep -rn requireAppBinary e2e/`).
- Checked `package.json`'s `scripts` block, `.github/workflows/ci.yml`, and `e2e/README.md`'s
  script inventory for any registration of `real-session-check.mjs`.
- Read `n1b-load-probe.js:1-16` for the header-declaration precedent §4c and the plan brief point
  to.
- Confirmed the `spectacle -f` full-screen capture, the `rmSync` cleanup call, and the `spawnSync`
  ffmpeg-crop call by reading `real-session-check.mjs:71-99` line by line.
- Checked `dataHome` (per-run `XDG_DATA_HOME` tempdir) and `OUT` (per-process screenshot tempdir)
  for any cleanup call anywhere in the file (`grep -n "dataHome\|rmSync\|mkdtempSync"`).

## Findings, most severe first

### 1. `real-session-check.mjs` sends real XTEST input on the owner's live display — the exact incident that produced the rule it was committed alongside — while its sibling script, added in the same commit, was fixed not to. (Serious)

**Where:** `e2e/scripts/real-session-check.mjs:124-126` (`clickIn(live, POINTS.videoField)`,
`typeText(FIXTURE)`, `clickIn(live, POINTS.videoOpen)`) and `:139`
(`clickIn(appWindow(), POINTS.transport)`), reached via `spawn(path.join(REPO,
"target/debug/sublore"), [], ...)` at `:103` — the app is launched with an **empty** args array,
not the fixture on argv.

WORKFLOW §4c (`WORKFLOW.md:77-78`), added by `062f201`:

> Synthetic input belongs in an isolated server. … They are never used on the owner's real
> display… On the real display, three things are allowed: launching the app, passing it files as
> command-line arguments, and capturing its own window. Nothing else. `startup_files` in
> `src-tauri/src/lib.rs` exists so that a real-session check can reach a loaded document without
> touching the keyboard.

`docs/reports/n2b-collaudo-reale.md`, committed by the same `062f201`, names the actual incident
this rule exists to prevent, in its own "Method notes" section:

> On a live compositor `xdotool` typing goes to whichever window holds the X focus, and it landed
> **in the owner's own window** during the first attempt. Hence today's rule: synthetic input only
> inside isolated servers; on the real display only launch, arguments, screenshots. (`:59`)

The same report then describes fixing the _other_ script the commit added:

> **`wayland-attach-check.js` no longer types.** It passes the fixture as a command-line argument,
> which is what `startup_files` was built for, so the input race above cannot reach it. (`:71`)

I verified that claim against the tree: `wayland-attach-check.js:91` is
`spawn(requireAppBinary(), [videoFixture], {...})` and a `grep` for `typeText\|clickAt\|clickIn`
in that file returns nothing — it never sends synthetic input at all.

`real-session-check.mjs` was not given the same treatment. It still opens the app with no argv
(`:103`, `[]`), then reaches the video-open UI the way `wayland-attach-check.js` used to: by typing
the fixture path into a field and clicking Open, at `:124-126`, live on whatever display the
process inherits (`DISPLAY` and `WAYLAND_DISPLAY` are not touched — the `env:` object at `:108`
only adds `WEBKIT_DISABLE_DMABUF_RENDERER` and `XDG_DATA_HOME` on top of `...process.env`). Every
piece of context in the repo — `n2b-collaudo-reale.md:3`'s "Fedora, KWin Wayland, XWayland `:0`
rootless" and the script's own comments about `spectacle`/compositing — places this script's
target display as the owner's actual desktop session, not an isolated Xvfb.

**Why it fails:** this is not a hypothetical mis-click. It is the literal failure mode that already
happened once with this exact tool chain (`xdotool type` following whichever window holds X
focus), on this exact machine, in this exact commit's own incident report. The script does call
`focusWindow(live.id)` at `:123` immediately before typing — `XSetInputFocus` via `xdotool
windowfocus --sync` — which is a real mitigation, but it is not the mitigation §4c actually
specifies. §4c's rule is categorical ("never used on the owner's real display"), not "call
`XSetInputFocus` first"; a compositor can steal focus back between `focusWindow()` returning and
`typeText()` running (a notification, an autoraise from another app, KWin's own focus-stealing
prevention interacting with `--sync`), and nothing in the script re-checks focus immediately before
the keystrokes land. If that race fires again, `FIXTURE` — a filesystem path — gets typed into
whatever window the owner is actually looking at, exactly as described in the incident report this
same commit carries.

### 2. `windowSaturation` captures the whole desktop, not the app's window, directly against §4c's stated method and reason. (Serious)

**Where:** `e2e/scripts/real-session-check.mjs:71-99`, specifically the `spectacle -f` call at
`:73` and the crop-by-coordinates that follows at `:80-86`.

§4c (`WORKFLOW.md:79`):

> **Capture the window, not the screen.** Under rootless XWayland `x11grab` on the root window
> reads black whatever the app draws; `import -window <id>` reads the window directly and needs no
> raise and no focus.

`windowSaturation` does the opposite: `spectacle -f -b -n -o <full> -d 200` captures the entire
composited desktop (`-f` is Spectacle's full-screen mode) into a temp file, then crops the app's
rectangle out of it in a second `ffmpeg` pass (`:80-86`) using a hand-derived `K = 4/3` composited-
to-X scale factor documented at `:44-45`. This is precisely the "capture the screen and crop"
approach the rule singles out as unnecessary and inferior to `import -window <id>`, which the rule
says "needs no raise and no focus." Because this script captures the wrong surface, it _needs_
`raise(top.id)` / `raise(live.id)` before every single measurement (`:118`, `:135`, `:141` — three
times per run) purely to compensate — churn the rule's own preferred method would not require, and
each `raise()`/`windowactivate --sync` call is itself an action sent to the live compositor,
immediately before the input sequence in finding 1.

Consequence beyond rule compliance: every full-screen capture (`${OUT}/${tag}-full.png`) contains
whatever else was on the owner's screen at the moment `spectacle` ran — not scoped to the app, on
an unattended script that runs against the owner's actual desktop.

### 3. The temp files this script creates are never fully cleaned up. (Minor)

**Where:** `e2e/scripts/real-session-check.mjs:42` (`OUT`, a per-process tempdir), `:97`
(`rmSync(full, { force: true })`), `:102` (`dataHome`, a per-run `XDG_DATA_HOME` tempdir).

`rmSync` at `:97` removes only the full-desktop screenshot (`full`), and only on the path that
reaches it — after the `SATAVG` regex match succeeds (`:93-96` throws first on a failed match, so a
failing run leaves `full` behind too). The **cropped** window PNG (`win`, `${OUT}/${tag}.png`,
written at `:83-86`) is never removed by anything in this file, and neither is `dataHome`, the
per-run `XDG_DATA_HOME` directory created at `:102` and handed to the app as its data directory. A
single 3-run invocation (the loop at `:161-169` runs `i` from 1 to 3) calls `windowSaturation`
three times per run (`empty`, `paused`, `playing`), so it leaves 3 `dataHome` directories and up to
9 cropped PNGs under `os.tmpdir()` unconditionally, plus any `full` screenshots from runs that
threw. Nothing else in the repository references the `n2b-real-*` prefix — `grep -rn "n2b-real"`
outside this file returns nothing — so nothing sweeps these up later either.

### 4. The script's own category — check, probe, or scratch file — is never declared, and it is wired into none of package.json, CI, or the README. (Serious)

**Where:** `e2e/scripts/real-session-check.mjs:1-18` (header docstring), whole file (no `check()`,
no `EXPECTED_CHECKS`, no explicit non-zero exit).

§4c permits probes on the real display, but the plan brief's own standard for that — matched
against `n1b-load-probe.js`, added by a later commit in the same range — is that a probe must
declare itself as loudly as that one does:

> **This is a probe, not a check. It asserts nothing.** (`n1b-load-probe.js:4`)

`real-session-check.mjs`'s header (`:1-18`) documents the _environment_ it works around at length
(rootless XWayland, the 4/3 scale, WebKit's DMABUF failure) but never states what kind of artifact
the file itself is. It has no `check()` calls, no `EXPECTED_CHECKS` counter, and no explicit
`process.exit()` of any kind — the loop at `:161-169` just logs a table and falls off the end.
(An uncaught throw inside `oneRun`, e.g. `windowSaturation`'s "no saturation from …" at `:95`,
_would_ propagate to a non-zero Node exit code, but that is incidental to error handling, not a
declared pass/fail contract.)

Consistent with that, the script is absent from every place a reader would look to learn what it
is: `package.json`'s `scripts` block only defines `e2e`, `e2e:build`, `e2e:shutdown`,
`e2e:close-gate`, `e2e:wayland`, `e2e:scale` — no entry for this file; `.github/workflows/ci.yml`
never mentions it (expected, since it needs a real display CI will never have — but nothing says
so, the way `scaled-surface-check.js`'s header does for its own real-display limitation); and
`e2e/README.md`'s script inventory has no row for it at all. A reader who finds this 170-line
committed file has to reverse-engineer, from the workaround commentary alone, whether it is safe to
run, what it proves, and whether a failure means anything.

### 5. A hardcoded absolute path bypasses the shared binary-resolution guard every other script uses. (Minor)

**Where:** `e2e/scripts/real-session-check.mjs:27` (`const REPO =
"/home/alcahest/git/SubLore"`), used at `:28-29` (two dynamic `import()` calls) and `:103`
(`path.join(REPO, "target/debug/sublore")`).

Every other script under `e2e/scripts/` resolves the app binary through
`requireAppBinary()`/`appBinary` from `e2e/lib/paths.js` (`n1b-load-probe.js:29,67`;
`wayland-attach-check.js:30,91`; `shutdown-check.js:18,45`; `close-gate-check.js:28,143`;
`scaled-surface-check.js:28,73` — confirmed by `grep -rn requireAppBinary e2e/`). That helper does
two things this script's hand-built path skips: it honours `CARGO_TARGET_DIR`
(`e2e/lib/paths.js:11-13`), and it fails with an actionable message before spawning anything if the
binary is missing or was produced by a plain `cargo build` rather than `pnpm e2e:build`
(`paths.js:37-40`, "plain `cargo build` produces a binary that loads the Vite dev URL and is
unusable here"). `real-session-check.mjs:103` constructs the same path by string concatenation and
spawns it directly; if the binary is stale or missing, that specific diagnostic never fires and the
script instead fails downstream, inside `waitFor(() => appWindow(), ...)` at `:116`, with a much
less informative timeout.

### 6. `spawnSync` for the ffmpeg crop discards its exit status, so a crop failure surfaces as a misleading error elsewhere. (Minor)

**Where:** `e2e/scripts/real-session-check.mjs:84-86`.

```js
spawnSync("ffmpeg", ["-hide_banner", "-y", "-i", full, "-vf", `crop=${crop}`, win], {
  timeout: 20000,
});
```

`run.status`/`run.stderr` are never inspected (contrast the `spectacle` call two lines above at
`:73-79`, which does check `run.status`). If the crop fails — bad geometry from the `K` factor, a
truncated `full` screenshot, ffmpeg missing a filter — `win` ends up missing or empty, and the
_next_ `ffmpeg … signalstats` call against `win` (`:87-91`) fails to find `SATAVG=` in its output
and throws `no saturation from ${win}` at `:95`. That error reports the symptom of the second
command against a file whose real problem was created by the unchecked first one.

## Hunt items I checked and found sound, or found as a reportable tension rather than a clear violation

- **`raise()` / `windowactivate` as possible banned "synthetic input" (`:65-68`).** §4c's ban is
  worded specifically around "typing, clicks and key presses" (`WORKFLOW.md:77`) and its "three
  things allowed" list names launch/argv/capture without explicitly covering window-management
  calls either way. I am not asserting §4c forbids `raise()` outright — the text does not say so.
  But it is not free of the rule's spirit either: `raise()` sends `xdotool windowraise` and
  `windowactivate --sync` to the live compositor of the owner's real desktop three times per run,
  and it exists here _only_ because the capture method chosen (finding 2) needs it — the rule's own
  preferred method, `import -window <id>`, "needs no raise and no focus" per `WORKFLOW.md:79`. So
  the tension is real even if the letter of the rule is ambiguous on this one call; I report it as
  a tension per the brief's guidance rather than as an outright violation.
- **`gtk = "0.18"` / other-lens dependency concerns** — out of scope for L9, not checked here.
- **The commit's own honest scoping in `n2b-collaudo-reale.md`.** The report is candid about the
  incident and about which fixes were applied (`wayland-attach-check.js` no longer types,
  `main.rs` mitigation verified three launches out of three) — that candour is the standard CLAUDE.md
  §9 asks for and is not itself a finding. It sharpens findings 1 and 2, though: the same document
  narrates the fix being applied to one script and not mentions the other needing it, which is why
  I read `real-session-check.mjs` as an omission rather than a considered exception.
- **`focusWindow(live.id)` at `:123`.** Verified this is a real, if incomplete, mitigation —
  `e2e/lib/input.js:11-13` documents it as `XSetInputFocus`, which does work without a window
  manager and does affect where subsequent `xdotool type`/`click` calls land. I did not find it
  sufficient to clear finding 1 (see the failure-mode argument there), but it is not absent or a
  no-op either — it is exactly the kind of best-effort guard that motivated §4c to be written as a
  categorical ban rather than "focus first and then it's fine."
