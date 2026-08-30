# Gate 2b — harness cluster: `e2e/wdio.conf.js`, `src/components/VideoStage.tsx`

Two files, two findings from wave 4. Everything below was run on Linux (Fedora, X11 session, checks
under Xvfb). Nothing here is a Windows claim. Display numbers 641 to 647, one per run, never reused.

---

## 1. `e2e/wdio.conf.js:34` — the `requireFfmpeg()` gate (V2 row: **not closed**)

**Verdict: the gate stays, and it moves to the reason that is still true.** V2's premise is wrong,
and the run below is what shows it: ffmpeg is not a harness pixel tool any more, but it is still a
prerequisite of the suite, because `asr.spec.js` drives a real transcription and the app extracts
audio by spawning ffmpeg (`crates/sublore-asr/src/tools.rs:76-97` looks it up on PATH,
`crates/sublore-asr/src/sidecar.rs:291` runs it). Removing the gate would not free a machine without
ffmpeg to run the suite; it would only change how that machine finds out.

**What changed.** The import from `./lib/pixels.js` is gone, and the call is now
`requireTool("ffmpeg", "extract the audio the transcription spec transcribes")` — the same
prerequisite helper the harness already uses for `xdotool` and `xwininfo`
(`e2e/lib/paths.js:47-56`). The old message named a dead reason ("It measures whether the video
surface is showing a picture"); no spec has measured a pixel since the saturation assertions were
removed, and `grep` over `e2e/specs/` returns no hit for `saturation` or `pixels.js`.

The call stays at module scope, where the previous one was. I moved it into `onPrepare` first, and
the measurement below shows that would have been a regression, so it went back.

**What proves it.** Three runs, ffmpeg hidden from PATH by a directory of symlinks to every PATH
entry except ffmpeg (`command -v ffmpeg` fails, `node`, `pnpm`, `xdotool`, `xwininfo`, `python3`,
`xvfb-run` all resolve).

| run | display | configuration                                 | result                                                                                                                                                                                         |
| --- | ------- | --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A   | 642     | ffmpeg hidden, gate in `onPrepare`            | exit 1, hook error printed — **and all 8 spec files ran anyway**: 7 passed, `asr.spec.js` 5 failing                                                                                            |
| B   | 643     | ffmpeg hidden, gate at module scope (shipped) | exit 1, config load aborted, **zero specs ran**, one sentence: `E2E prerequisite missing: ffmpeg is not on PATH. The harness uses it to extract the audio the transcription spec transcribes.` |
| C   | 647     | ffmpeg present, shipped code                  | exit 0, 8/8 spec files, 33 tests, the `EXPECTED_TESTS` tally guard satisfied                                                                                                                   |

Run A is the load-bearing one and it settles both halves at once:

- **ffmpeg is genuinely needed by a spec.** With it hidden, `asr.spec.js` fails all five of its
  tests while every other spec file passes in full. So the gate is not vacuous, and it is not
  gating "for a dependency nothing needs".
- **The gate has to fire at config load.** WebdriverIO logs a throw from `onPrepare` and runs the
  specs regardless. That is exactly the failure the check exists to prevent: without it the run
  dies as `Expected: "tiny.en" / Received: "tiny"` and `timed out after 30000ms waiting for the
fixture to load and enable Transcribe` — five cryptic failures, ffmpeg named nowhere.

A discrimination note in the §4c sense: run B differs from run A only in where the call sits, and
the outcomes differ (0 specs vs 8). Run C differs from B only in PATH, and it is green. Every
`pnpm build` and `pnpm e2e:build` in this cluster had its exit status read on its own line, never
chained behind `&&`, and `target/debug/sublore`'s mtime was recorded before and after every
behavioural run (unchanged across each — no parallel implementer's build landed inside one).

**State: fixed.**

---

## 2. `src/components/VideoStage.tsx:57` — a throw in the effect took the page down (V1 finding 8)

**What changed.** The three `matchMedia` registrations are made one at a time inside a `try`, and
only the queries that registered go into the array the cleanup unsubscribes. A webview without
`window.matchMedia`, or with a `MediaQueryList` that has no `addEventListener` (WebKit gained it in
the Safari 14 generation, and WebKitGTK versions in the field vary), now costs the ratio listeners
and nothing else. The catch writes one `console.warn` and the effect continues to the
`ResizeObserver`, the resize listener and the first `schedule()`.

**What proves it.** A discrimination experiment on the real app, three builds, each with its exit
status read explicitly, each run on its own display. The injected fault is one temporary line in
the effect that makes `window.matchMedia` throw — the failure class the guard exists for — and it
was removed before the final build.

| run | display | code under test                               | `pnpm e2e:scale`                                                                          |
| --- | ------- | --------------------------------------------- | ----------------------------------------------------------------------------------------- |
| A   | 644     | guard present + injected throw                | **5/5, exit 0** — surface 736x159 at ratio 1, 1472x320 at ratio 2                         |
| B   | 645     | guard removed (wave-3 shape) + injected throw | **red, exit 1** — `timed out after 30000ms waiting for the native surface at GDK_SCALE=1` |
| C   | 646     | shipped code, no injection                    | **5/5, exit 0**                                                                           |

Run B also corrects the size of the original finding. V1 called it "takes the video stage down";
the window tree it printed shows worse. The `Sublore` toplevel is there at 1024x700 with one 1x1
child and no video surface at all: the throw escapes the effect during the mount commit, React
unwinds to the root with no error boundary above it, and the user gets an empty window. The check
that catches this is the one that waits for the native surface, which only ever appears because
`VideoStage` reported a region.

**What is still not exercised, stated plainly.** The `change` listener itself — the whole point of
the wave-3 addition — has still never fired. `scaled-surface-check.js` measures two separate
launches at `GDK_SCALE=1` and `GDK_SCALE=2`, so it proves the ratio is applied, never that a live
ratio change re-reports the region. Firing it needs either a frontend test runner with a fake
`matchMedia` (a new dependency: WORKFLOW §3, not mine to decide) or an XSETTINGS rig that flips
`Gdk/WindowScalingFactor` under a running app and a check that reads the surface geometry before
and after. I built neither. What this cluster delivers is that the code path cannot take the app
down, and that claim is measured.

**Honesty note on the `console.warn`.** It reaches a devtools console the release webview does not
offer: `log_plugin` (`src-tauri/src/lib.rs:212-231`) has a `LogDir` target and no webview target,
and nothing in `src/` calls the log plugin's `attachConsole`. So this line is readable by a
developer and by nobody else. I left it as a warn rather than inventing a UI surface, because the
user-visible consequence is that the video surface keeps its size until the next window resize —
there is no action for a user to take. It is the same gap V1 named as finding 7 against
`useStartupFiles.ts`, which is another cluster's file: a frontend-to-log bridge would close both at
once and belongs in BACKLOG, not here.

**State: fixed (the safety). Not covered: the live ratio change.**

---

## Noticed, not touched — outside this cluster's two files

- `e2e/lib/pixels.js`: `saturation()` now has no caller anywhere (`real-session-check.mjs` measures
  inline and says so at its line 23; `webview-paint-check.js` has its own luma reader). Only
  `requireFfmpeg()` still has one consumer, `webview-paint-check.js:189`, where its message —
  measuring whether the surface shows a picture — is still true. Dead code by CLAUDE.md §6, one
  deletion, someone else's file.
- `e2e/README.md:7-10` still explains ffmpeg to the reader as the harness's pixel instrument. The
  sentence about prerequisites being checked before any spec starts stays true; the reason beside
  it no longer is.
- `docs/reports/gate2-closure-audit.md`'s row for `e2e/wdio.conf.js:34` says the dependency is one
  "nothing needs". Run A above contradicts it. Worth correcting in the register when this cluster
  is folded in, so the next reader does not delete the gate on the strength of that line.

## Battery

Static, in the §4c order, exit statuses read individually: `pnpm lint` (0), `pnpm exec prettier
--check` on both files (0), `pnpm build` (0), `pnpm e2e:build` (0, binary mtime advanced each time).
No Rust changed in this cluster, so `cargo test` and `cargo clippy` were not run here — and by §4c
they must not run after `pnpm e2e:build`, which is what left the binary in place for the runs above.

Behavioural: `pnpm e2e` 8/8 spec files and 33 tests on display 647 (exit 0); `pnpm e2e:scale` 5/5 on
display 646 (exit 0); plus the four experiment runs on 641 to 645 tabulated above. Verified on
Linux.
