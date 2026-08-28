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
- [~] **M0.5 E2E smoke harness.** tauri-driver + WebDriver behavioral test asserting the app launches with window title Sublore, wired into CI. Filed from M0.1: title verification there was manual (Xvfb + xdotool), WORKFLOW §2 wants it automated.
  - AC: a CI job on Linux runs the E2E smoke; the test fails if the window title is not "Sublore"; removing the harness or skipping the test turns CI red.
  - AC (filed from M0.2): the same job opens `fixtures/video/sample.mkv` and fails if the native video surface is not sized over the stage element, and fails if closing the window leaves a non-zero exit status or a surviving process. M0.2's suite drives the player headless only, so `video::setup` and `VideoSurface::create` have no automated coverage today.
  - Status 2026-08-23: harness implemented in `e2e/` (tauri-driver + WebdriverIO + Mocha) with four checks: window title, fixture opened, native video surface sized over the stage, close with exit 0 and no survivors. Suite verified green locally and proven to fail when each assertion is broken. A screen smaller than the 1024x700 window makes the video checks fail, so the Xvfb size is pinned in CI. CI job never executed: no remote.
  - Harness note from M0.2: `xdotool search --name Sublore` also matches GTK's 10x10 group-leader window and lists it first, so select the toplevel by its 1024x700 geometry. Close it with an ICCCM-conformant WM_DELETE_WINDOW ClientMessage (python-xlib helper): `xdotool windowquit` proved a no-op in this WM-less Xvfb environment, and `xdotool windowclose` is XDestroyWindow, which bypasses the app's close path entirely (and currently segfaults it, see M0.4).

**Owner checklist M0:** install from CI artifact on your machine → open a video → play, pause, seek → close → reopen. Everything behaves; nothing left running in task manager.

## M1 — Subtitle formats, lossless (tasks detailed 2026-08-23; implemented and verified-by-tests 2026-08-28)

Goal: open and save SRT, ASS, VTT without destroying anything, including files from the wild.

- [x] **M1.1 Subtitle core model + SRT.** Internal document model that preserves what it does not understand (raw payloads, ordering, quirks); SRT parser + serializer; the subtitle fixture tree is born here (`fixtures/subtitles/`, committed, with a `.gitattributes` rule keeping it byte-exact).
  - AC: every clean SRT fixture round-trips parse → serialize byte-for-byte (CRLF/LF mixes, BOM, blank-line quirks, numbering gaps, overlapping cues, non-Latin scripts); every malformed fixture yields a structured error with line number and reason, never a panic, never a silent fix.
- [x] **M1.2 VTT lossless.** WEBVTT parser + serializer: header, NOTE/STYLE/REGION blocks, cue settings, voice/inline tags preserved raw.
  - AC: same byte-stable round-trip guarantee over `fixtures/subtitles/vtt/`; malformed → structured error, never a panic.
- [x] **M1.3 ASS lossless.** .ass/.ssa parser + serializer preserving section order, Format lines, styles, and override tags as raw text.
  - AC: same byte-stable round-trip guarantee over `fixtures/subtitles/ass/`; malformed → structured error, never a panic.
- [x] **M1.4 Atomic save + rolling backups.** Temp file + fsync + rename per CLAUDE.md §3; before overwriting an existing subtitle file, a timestamped backup is kept (rolling cap as designed); no other code path deletes backups.
  - AC: a crash-injection behavioral test interrupts the save repeatedly and the destination is always either the old or the new content in full, never truncated or mixed; overwriting an existing file always leaves a timestamped backup; the rolling cap is enforced and tested.
- [x] **M1.5 Open/save wired into the app.** Minimal UI: open a subtitle file, show format + cue count, save-as; parse errors surface as readable UI messages.
  - AC: E2E: open an SRT fixture → format and cue count visible; save-as of a clean fixture produces a byte-identical file; open a malformed fixture → readable error message, app still alive and usable.

**Status 2026-08-28:** all five tasks implemented on `main`. Two new workspace crates: `sublore-formats` (lossless model + SRT/VTT/ASS) and `sublore-io` (atomic save, rolling backups, crash injection). 43 clean fixtures round-trip byte-for-byte and 23 malformed fixtures produce structured errors, verified independently in release profile as well as by the suites; 182 Rust tests and 7 E2E checks green. A release-only silent-corruption path found in review (the tiling guard was behind `debug_assert!`, plus a multi-byte character split that emptied the file) is fixed and covered by tests. Windows and CI unverified: no remote, no Windows machine.

**Owner checklist M1:** open a real .srt from your disk → cue count appears; save-as → the copy is byte-identical (fc/cmp); open a deliberately broken file → clear error, app keeps working.

## M2 — Editor with video and waveform (tasks detailed 2026-08-28)

Goal: the free product's core: cue list, text editing, timing adjust against waveform, side-by-side source/target view.

- [ ] **M2.1 Editable document model.** Mutation API over `sublore-formats` (edit cue text, edit times, insert, delete, split, merge) that keeps the lossless guarantee: everything the parser preserved stays preserved for untouched cues, and every mutation re-runs the tiling/coverage guard M1 added.
  - AC: mutating one cue in a fixture and saving leaves every other byte of the file identical; a mutation that would break segment coverage is refused with a structured error, never written; property test over random edit sequences never produces a document that fails the guard.
- [ ] **M2.2 Undo/redo.** Single undo stack for every document mutation, with coalescing of consecutive typing into one entry.
  - AC: any sequence of edits can be undone back to the exact original bytes and redone forward to the exact edited bytes; undo depth is bounded and documented; typing a word is one undo step, not one per character.
- [ ] **M2.3 Cue list UI with editing.** Virtualized cue list (index, start, end, text), inline text editing, keyboard navigation, dirty state, save/save-as.
  - AC: E2E: open the 2000-cue fixture, edit a cue's text, save, reopen, the edit is there and the rest is byte-identical; undo restores it; scrolling and typing show no visible lag (measured, budget CLAUDE §7: open under 1 s).
- [ ] **M2.4 Waveform.** Audio peaks extracted from the media (via the existing libmpv/ffmpeg path, off the main thread, cancellable) and rendered as a zoomable waveform with the playhead.
  - AC: peaks for a 60 s fixture appear within budget and match the audio (silence reads flat, the 440 Hz tone reads full); playhead tracks playback; zoom and scroll stay responsive; no main-thread blocking.
- [ ] **M2.5 Timing against the waveform.** Drag cue boundaries on the waveform, nudge with keyboard, snap to playhead; changes flow through the M2.1 mutation API and undo stack.
  - AC: E2E: drag a cue boundary, the model times change accordingly and save round-trips; nudge shortcuts move by the documented step; every timing change is undoable.
- [ ] **M2.6 Source/target side by side.** Two documents open at once (source and target), aligned by index, editing only the target.
  - AC: open two fixtures as source and target; rows align; editing the target never mutates the source file on disk; saving writes only the target.

**Owner checklist M2:** open a real subtitle file with its video → edit some lines → adjust a cue against the waveform → undo a few times → save → reopen and confirm your edits are there and nothing else changed. Subtitle a 1-minute clip start to finish without another tool.

## M3 — Local transcription

Goal: whisper.cpp sidecar producing editable, word-timestamped cues.
Draft criteria: model download is explicit and resumable; transcription runs off the main thread, shows progress, cancels cleanly; Vulkan used when present, CPU fallback verified; output loads straight into the editor as cues.

## M4 — Projects (tasks detailed 2026-08-28)

Goal: SQLite project (series → episodes → files) so memory has somewhere to live.

- [ ] **M4.1 Schema and migrations.** One SQLite database file per project; schema for series, episodes, and the files attached to each episode (media path, subtitle paths, role); a versioned migration runner.
  - AC: creating a project produces a database at the chosen path with the current schema version; an automated test takes a database written at version N, migrates it, and verifies both the schema and every row survives (old db → migrate → verify, CLAUDE §2); a database from a newer version than the app is refused with a readable error, never silently altered.
- [ ] **M4.2 Project lifecycle.** Create, open, close a project; add episodes; attach existing media and subtitle files to an episode by path.
  - AC: create a project, add two episodes with files, close and reopen: everything is still there with the same paths and order; attaching a file records only its path and metadata, never copies or moves the user's file; opening a database that is corrupt or not a Sublore project fails with a readable error and leaves it untouched.
- [ ] **M4.3 Deletion safety.** Deleting a project or an episode removes only Sublore's own records and its own project folder contents.
  - AC: a behavioral test with real files on disk deletes a project whose episodes reference media and subtitles outside the project folder, then asserts every one of those user files still exists byte-identical (CLAUDE §3); no code path deletes outside the project folder.
- [ ] **M4.4 Project UI, minimal.** Create/open a project, see its episodes and their attached files, add an episode, attach a file. No editing beyond that.
  - AC: E2E: create a project in a temp location, add an episode, attach a subtitle fixture, restart the app, reopen the project, the episode and its file are listed; errors surface as readable messages.

**Owner checklist M4:** create a project → add an episode → attach a subtitle file and a video → close the app → reopen the project and find everything where you left it → delete the project and confirm your own video and subtitle files are still on disk.

## M5 — Termbase + QA (pro, closed module)

Goal: the product. Per-project glossary with approved renderings; QA pass flags every target line where a source term appears without its approved rendering.
Draft criteria: on the standard fixture episode, QA flags exactly the planted violations and nothing else; term added mid-series retro-flags earlier episodes on demand; module absent → core runs untouched, module present + licensed → features appear.

## M6 — Translation memory + licensing + release

Goal: exact and fuzzy TM across the whole project; offline license check; pay-once purchase flow via merchant of record; v1.0 tag.
Draft criteria: repeated lines in later episodes surface their earlier translation; fuzzy matches ranked and insertable; license file validates offline, invalid license degrades gently to free core; both installers pass the full owner checklist end to end.

---

## Parking lot (explicitly not v1 — do not pull forward)

Karaoke/typesetting · diarization · built-in LLM translation beyond BYOK hook · macOS activation · auto-updater · batch CLI · Rust-side localized strings for the native crash dialog (v1 ships it English-only, owner decision 2026-08-23). Anything an agent wants to add lands here first and waits for the owner.
