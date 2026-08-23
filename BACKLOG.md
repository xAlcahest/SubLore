# BACKLOG.md — Sublore build plan

Ordered milestones for v1.0 (scope: CLAUDE.md §1). Rules: work top to bottom; a milestone's tasks are detailed by the orchestrator when the milestone starts, using M0 as the template; every task gets acceptance criteria BEFORE implementation; the owner's checklist at the end of each milestone is the definition of done.

Status legend: `[ ]` open · `[~]` in progress · `[x]` verified-by-tests · `[✓]` owner-passed · `[!]` blocked

---

## M0 — Skeleton that proves the stack (fully specified, start here)

Goal: a running Tauri 2 app on Windows and Linux that plays a video with libmpv and has green CI. This milestone exists to kill stack risk first: if libmpv embedding fails, we learn it in week one, not month three.

- [~] **M0.1 Repo bootstrap.** Tauri 2 + Rust core + TS frontend, workspace layout, lint/format hooks, README stub.
  - AC: `npm run tauri dev` opens a window titled Sublore on Windows and Linux; CI runs lint + build on both and is green.
  - Status 2026-08-22: implementation merged to `main` locally; full check battery (install, format, lint, build, fmt, clippy, cargo build) plus live app launch with window title verified on Linux. Remote publication deferred by owner, so the CI-green and Windows halves of the AC are unverified.
- [~] **M0.2 libmpv embedding.** Video renders inside the app window via libmpv, with play/pause and seek.
  - AC: open fixture `fixtures/video/sample.mkv`; video plays with audio; pause, seek to 0:30, frame changes accordingly; app closes cleanly with no orphan process.
  - Status 2026-08-22: implemented (libmpv2 crate, X11 wid embedding via native child window, XWayland on Wayland); 14 behavioral tests green against real libmpv; independent live verification under Xvfb passed every AC (video advances, pause freezes byte-identical, seek lands at 30.0, WM_DELETE_WINDOW close exits 0 with no orphans). Audio verified to decode (AAC track selected); audibility needs the owner's speakers. Windows and CI runs still unverified (no remote, no Windows machine).
- [ ] **M0.3 Packaging smoke.** CI produces installable artifacts (msi/exe, deb + AppImage).
  - AC: artifact from CI installs and runs M0.2's behavior on a clean VM of each OS.
- [~] **M0.4 Crash safety baseline.** Panic/error handler; log file in app data dir; no console spam in release.
  - AC: forced error path shows an actionable dialog and writes the log; app restarts normally after.
  - Input from M0.2 (2026-08-22): an external client hard-destroying the toplevel via XDestroyWindow segfaults the app in libgobject (normal WM_DELETE_WINDOW close is clean, exit 0). Not a user-reachable path through normal close; assess and handle or explicitly accept during this task.
  - Status 2026-08-23: implemented and verified live on Linux. Forced panic writes a crash report, shows a native dialog, exits 101; normal close exits 0 with no report; the app relaunches cleanly afterwards; release build emits nothing on stdout. XDestroyWindow segfault explicitly accepted and recorded in README (a panic hook cannot see SIGSEGV, and at M0 the app writes no user data). Windows paths and CI unverified: no remote, no Windows machine.
- [ ] **M0.5 E2E smoke harness.** tauri-driver + WebDriver behavioral test asserting the app launches with window title Sublore, wired into CI. Filed from M0.1: title verification there was manual (Xvfb + xdotool), WORKFLOW §2 wants it automated.
  - AC: a CI job on Linux runs the E2E smoke; the test fails if the window title is not "Sublore"; removing the harness or skipping the test turns CI red.
  - AC (filed from M0.2): the same job opens `fixtures/video/sample.mkv` and fails if the native video surface is not sized over the stage element, and fails if closing the window leaves a non-zero exit status or a surviving process. M0.2's suite drives the player headless only, so `video::setup` and `VideoSurface::create` have no automated coverage today.
  - Harness note from M0.2: `xdotool search --name Sublore` also matches GTK's 10x10 group-leader window and lists it first, so select the toplevel by its 1024x700 geometry. Close it with an ICCCM-conformant WM_DELETE_WINDOW ClientMessage (python-xlib helper): `xdotool windowquit` proved a no-op in this WM-less Xvfb environment, and `xdotool windowclose` is XDestroyWindow, which bypasses the app's close path entirely (and currently segfaults it, see M0.4).

**Owner checklist M0:** install from CI artifact on your machine → open a video → play, pause, seek → close → reopen. Everything behaves; nothing left running in task manager.

## M1 — Subtitle formats, lossless

Goal: open and save SRT, ASS, VTT without destroying anything, including files from the wild.
Draft criteria: round-trip fixtures (CRLF/LF, BOM, overlapping cues, non-Latin scripts, malformed lines) byte-stable for supported fields; atomic writes + rolling backups per CLAUDE.md §3; malformed input produces a readable error, never a crash or a silent fix.

## M2 — Editor with video and waveform

Goal: the free product's core: cue list, text editing, timing adjust against waveform, side-by-side source/target view.
Draft criteria: edit 2,000-line file with no visible lag; every edit undoable; timing changes audible/visible against waveform; owner can subtitle a 1-minute clip start to finish without touching another tool.

## M3 — Local transcription

Goal: whisper.cpp sidecar producing editable, word-timestamped cues.
Draft criteria: model download is explicit and resumable; transcription runs off the main thread, shows progress, cancels cleanly; Vulkan used when present, CPU fallback verified; output loads straight into the editor as cues.

## M4 — Projects

Goal: SQLite project (series → episodes → files) so memory has somewhere to live.
Draft criteria: create/open project; episodes retain their files and state across restarts; schema migration test green; deleting a project never touches user media or subtitle originals.

## M5 — Termbase + QA (pro, closed module)

Goal: the product. Per-project glossary with approved renderings; QA pass flags every target line where a source term appears without its approved rendering.
Draft criteria: on the standard fixture episode, QA flags exactly the planted violations and nothing else; term added mid-series retro-flags earlier episodes on demand; module absent → core runs untouched, module present + licensed → features appear.

## M6 — Translation memory + licensing + release

Goal: exact and fuzzy TM across the whole project; offline license check; pay-once purchase flow via merchant of record; v1.0 tag.
Draft criteria: repeated lines in later episodes surface their earlier translation; fuzzy matches ranked and insertable; license file validates offline, invalid license degrades gently to free core; both installers pass the full owner checklist end to end.

---

## Parking lot (explicitly not v1 — do not pull forward)

Karaoke/typesetting · diarization · built-in LLM translation beyond BYOK hook · macOS activation · auto-updater · batch CLI. Anything an agent wants to add lands here first and waits for the owner.
