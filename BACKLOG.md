# BACKLOG.md — Sublore build plan

Ordered milestones for v1.0 (scope: CLAUDE.md §1). Rules: work top to bottom; a milestone's tasks are detailed by the orchestrator when the milestone starts, using M0 as the template; every task gets acceptance criteria BEFORE implementation; the owner's checklist at the end of each milestone is the definition of done.

Status legend: `[ ]` open · `[~]` in progress · `[x]` verified-by-tests · `[✓]` owner-passed · `[!]` blocked

Behavioural verdicts in this file are **verified on Linux** unless they say otherwise. Windows compiles in CI; it is not verified until MW closes (CLAUDE.md platform policy, 2026-08-29).

**Decisions in force** (owner ruling 2026-08-29, full text in `docs/design/decisions.md`):

| # | Decision | Lands in |
|---|---|---|
| 1 | Video occlusion: native surface hides for HTML layers | M2.0 |
| 2 | Show and re-show with a loaded video, built now | N2 |
| 3 | Windows E2E backend | MW.1 |
| 4 | Composite history entry, one undo per operation | M2.5 (shape), M7 (use) |
| 5 | Active line and selection separated | M2.0 |
| 6 | Text matcher in the open core | M5 |
| 7 | Video shows the translation, source on toggle, shadow copy | M2.6 |
| 8 | Autosave in its own store, backups untouchable by timers | M11 (post-v1) |
| 9 | Close gate — active defect | N1 |
| 10 | Non-cue segments: written analysis now | N3 |
| 11 | Milliseconds, final. No frame vocabulary | M2.5 note, plan-wide |
| 12 | M2.4 reuses the ASR pattern, not its code | M2.4 |
| 13 | New translation from source | M2.6 |

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

## NOW — ahead of every functional milestone (owner ruling 2026-08-29)

Two items jump the queue by owner decision. Full reasoning in `docs/design/decisions.md`; the ruling settles all thirteen questions raised in `docs/design/post-v1-plan.md`. Order is fixed: N1, then N2, then M2.0.

- [ ] **N1 Close gate (decision 9).** Intercept `CloseRequested`; if anything is unsaved, ask save / discard / cancel per dirty document and honour the answer. **This is an active data-loss defect, not a missing feature:** there is no `prevent_close` in the repo, dirty state is tracked on both sides and nobody consults it on close, so closing the window throws unsaved edits away silently, against CLAUDE.md §3. The dialog plugin is already a dependency. M2.6 doubles the exposure by opening a second document.
  - AC: open a subtitle fixture, edit a cue, close the window: a dialog appears offering save, discard and cancel. Cancel leaves the app open with the edit still there and the file on disk untouched. Discard closes and leaves the file untouched. Save writes the edit and then closes.
  - AC: with nothing modified, closing the window exits straight away with status 0 and no dialog, and the existing shutdown checks still pass unchanged.
- [ ] **N2 Video surface show and re-show (decision 2).** Build the re-show path for the native surface with a video already loaded. Prerequisite of M2.0's occlusion handling (decision 1).
  - AC: open a video, hide the surface, show it again: the frame is visible and playback continues. The assertion is on the visible frame, not on an internal flag, because mpv creates its own window inside ours and leaves it unmapped if ours is (`video/surface/mod.rs:82-84`) — a state-only assertion would pass while the user sees black.
  - AC: hide and re-show ten times in a row leaves no orphan process and no leaked surface.
- [ ] **N3 Non-cue segments: written analysis (decision 10).** Half a day, output is a document, not a feature. Answer whether `Edit` and `Expectation` in `sublore-edit` can grow a `Meta` variant covering `Style:` lines, script properties and attachments, which today travel as uninterpreted metadata (`sublore-formats/src/document.rs:81-92`) while `Edit` covers cues only (`plan.rs:28-58`). Adjust the shape now if the answer says so; the feature itself stays at M14. Not blocked by N1 or N2 and can run alongside, but **must land before M5**, because once the closed modules depend on the crate a second write path gets bolted alongside the first instead of replacing it.
  - AC: a document in `docs/design/` states whether the shape changes, and if it does not, why not. "No change needed" is a valid and useful answer, in writing.

## M2 — Editor with video and waveform (tasks detailed 2026-08-28)

Goal: the free product's core: cue list, text editing, timing adjust against waveform, side-by-side source/target view.

- [ ] **M2.0 Shell redesign — blocks M2.4-M2.6.** Filed 2026-08-29 after the owner ran the app and rejected the interface. M0-M4 each bolted a horizontal band with a path field and a button onto one column, because that is the cheapest way to give an E2E spec a stable selector; the result is the union of five test harnesses. Rebuild the shell on Aegisub's structure with a modern finish, per `docs/design/shell-layout.md` and `docs/design/shell-mockup.html`. The engine is not touched: this is the frontend shell plus the file-dialog commands.
  - AC: opening a video or a subtitle goes through the system file dialog, reachable from the File menu and from the toolbar; no field for typing a path is left anywhere in the interface.
  - AC: video and waveform panels sit side by side in the top band, the current-line band under them, the cue grid across the bottom.
  - AC: transcription controls are not on screen until opened from the menu.
  - AC: with a video and a subtitle open, at 1024x700 and at 1920x1080, no element is clipped at a window edge and the page never scrolls horizontally (the clipped `Save copy to` label at 1024x700 is the regression fixture for this).
  - AC: the panel holding the video never scrolls, per the M0.2 constraint that the native X11 surface is placed from DOM coordinates and recomputed only on resize.
  - AC: the 27 existing E2E checks pass with selectors re-pointed and assertions unchanged; no assertion is weakened, skipped, or retargeted (CLAUDE.md §5.4).
  - AC (decision 1, occlusion): opening a menu or a dialog over a playing video hides the native surface, and closing it brings the frame back. E2E: with a video playing, open the File menu over the video rectangle — the menu is visible and the video is not covering it; close it and the frame returns. No native system menus and no separate popup windows: the CSS chrome is what keeps Windows and Linux looking the same. Depends on N2.
  - AC (decision 5, selection): the shell separates the active line (single cursor) from the selection (a set: single, shift for a range, ctrl for scattered), both drivable from the keyboard. Bulk operations act on the selection. Landing it here, before M2.5 leans on it, is the point.
- [x] **M2.1 Editable document model.** Mutation API over `sublore-formats` (edit cue text, edit times, insert, delete, split, merge) that keeps the lossless guarantee: everything the parser preserved stays preserved for untouched cues, and every mutation re-runs the tiling/coverage guard M1 added.
  - AC: mutating one cue in a fixture and saving leaves every other byte of the file identical; a mutation that would break segment coverage is refused with a structured error, never written; property test over random edit sequences never produces a document that fails the guard.
- [x] **M2.2 Undo/redo.** Single undo stack for every document mutation, with coalescing of consecutive typing into one entry.
  - AC: any sequence of edits can be undone back to the exact original bytes and redone forward to the exact edited bytes; undo depth is bounded and documented; typing a word is one undo step, not one per character.
- [x] **M2.3 Cue list UI with editing.** Virtualized cue list (index, start, end, text), inline text editing, keyboard navigation, dirty state, save/save-as.
  - AC: E2E: open the 2000-cue fixture, edit a cue's text, save, reopen, the edit is there and the rest is byte-identical; undo restores it; scrolling and typing show no visible lag (measured, budget CLAUDE §7: open under 1 s).
- [ ] **M2.4 Waveform.** Audio peaks extracted from the media (via the existing libmpv/ffmpeg path, off the main thread, cancellable) and rendered as a zoomable waveform with the playhead.
  - AC: peaks for a 60 s fixture appear within budget and match the audio (silence reads flat, the 440 Hz tone reads full); playhead tracks playback; zoom and scroll stay responsive; no main-thread blocking.
  - AC (decision 12, audio provider): reuse the ASR *pattern* — ffmpeg discovery, background execution, progress, cancellation — and not its code. Extraction runs at full quality behind a public API with a per-episode cache, and its lifetime is tied to the episode, not to a transcription run. The ASR extractor is private, produces mono 16 kHz because whisper wants that (`sublore-asr/src/sidecar.rs:285,302-304`), and writes into a scratch folder that deletes itself when the run ends (`scratch.rs:88-91`): fine for peaks, wrong for playing a selection, and it would vanish mid-session.
- [ ] **M2.5 Timing against the waveform.** Drag cue boundaries on the waveform, nudge with keyboard, snap to playhead; changes flow through the M2.1 mutation API and undo stack.
  - AC: E2E: drag a cue boundary, the model times change accordingly and save round-trips; nudge shortcuts move by the documented step; every timing change is undoable.
  - AC (filed 2026-08-29 from Aegisub's audio toolbar, `default_toolbar.json`): the keyboard command set is what makes timing fast, and dragging alone does not cover it. Previous/next line, play selection, play line, play before/after/begin/end of selection, play to end, lead in, lead out, and commit each have a shortcut and each is exercised by a behavioural test.
  - AC (decision 4, one undo per operation): `sublore-edit` grows a composite history entry — a transaction of N child edits under a single label. Today an entry carries one `Splice` (`history.rs:37-39`), `Edit` names a single cue (`plan.rs:28-58`), and coalescing needs matching label and offset (`history.rs:192-195`), so edits on different cues never merge. Test: edit scattered rows, one undo returns the document byte-identical. Range variants were rejected because sparse selections, not contiguous ranges, are the real case.
  - Note (decision 11): the product reasons in **milliseconds**, final. No frame, framerate or keyframe vocabulary, and no timing item carries a "pending a frame engine" reservation. If a frame seam is ever needed it lives in the player and nowhere else.
- [ ] **M2.6 Source/target side by side.** Two documents open at once (source and target), aligned by index, editing only the target.
  - AC: open two fixtures as source and target; rows align; editing the target never mutates the source file on disk; saving writes only the target.
  - AC (decision 13, how a translation is born): a **New translation from source** command creates a document inheriting the source's cues and timings with empty text; the source is read-only while translating; the first save asks for name and location with a sensible proposal (episode plus language). The source is never modified or overwritten. E2E: open only a source fixture, run the command, every cue and timing is carried over with empty text, translate two lines, save: a new file appears at the chosen path and the source fixture is byte-identical to before.
  - AC (decision 7, subtitles on video): the video shows the **translation**, with a toggle to show the source instead. The preview is fed from a shadow copy in the working folder; the user's file is **never** saved to produce a preview. E2E: with both documents open, pause on a time where a cue is active — the translation text is on the frame; toggle, and the source text is; neither file on disk changes, and no backup is created by toggling or typing.

**Status M2 part A, 2026-08-28:** M2.1-M2.3 on `main`. New crate `sublore-edit` (splice-based mutation planning, verification, undo/redo with explicit run boundaries, edit sessions). 289 Rust tests and 17 E2E checks green; the 2000-cue fixture opens in 68 ms against a 1000 ms budget with 26 rows in the DOM. Review caught and fixed three real defects: ASS cue deletion refused between blank runs, undo coalescing two deliberate edits into one step, and a global Ctrl+Z stealing native undo from text inputs. M2.4-M2.6 not started.

**Owner checklist M2:** open a real subtitle file with its video → edit some lines → adjust a cue against the waveform → undo a few times → save → reopen and confirm your edits are there and nothing else changed. Subtitle a 1-minute clip start to finish without another tool.

## M3 — Local transcription (tasks detailed 2026-08-28)

Goal: whisper.cpp sidecar producing editable, word-timestamped cues.

- [x] **M3.1 Sidecar process wrapper.** Run whisper.cpp as a child process (never in-process, never on the main thread): spawn, stream progress, cancel, reap. Build/vendor strategy for the binary decided and documented; CUDA never a hard dependency.
  - AC: a behavioral test runs a real transcription of a short audio fixture end to end and gets output; cancelling mid-run kills the child and leaves no orphan process (verified with a process check); the app stays responsive throughout; a missing or unrunnable binary produces a readable error, never a crash.
- [x] **M3.2 Model management.** Explicit, user-initiated model download with resume and integrity check; models live in the app data dir; nothing downloads on its own.
  - AC: no network request happens unless the user asks for a download (CLAUDE §1); an interrupted download resumes instead of restarting; a corrupt or truncated file is detected by checksum and refused, never handed to whisper; models are never committed to the repo.
- [x] **M3.3 Word timestamps to cues.** Convert whisper output (word-level timestamps) into subtitle cues in the M1 document model, with a documented, deterministic segmentation rule.
  - AC: transcribing the fixture produces cues whose times are inside the media duration and strictly ordered; the same input always produces the same cues; the result is a valid document that passes the M1 coverage guard and can be saved as SRT and reopened byte-identically.
- [x] **M3.4 Transcription UI.** Choose model, start, see progress, cancel. Runs off the main thread with visible, cancellable progress (CLAUDE §7).
  - AC: E2E: start a transcription on the fixture, progress advances, cancel stops it and the UI returns to a usable state with no orphan process; on completion the cues appear in the app; GPU/CPU selection is visible and CPU fallback works with no GPU present.

**Status 2026-08-28:** M3.1-M3.4 on `main` (developed in a parallel worktree, merged onto the editor and projects). New crate `sublore-asr`: whisper.cpp sidecar built by `scripts/build-whisper.sh` from the commit pinned in `whisper.pin` (never committed), model store with explicit resumable downloads, deterministic word-to-cue segmentation. 489 Rust tests, 27 E2E checks and the real-whisper suite (7 tests against the actual binary and model, transcription, cancellation, progress, determinism) all green. Review caught a model integrity hole: a corrupted model passed the length-only check and made whisper emit silently wrong text with exit 0; `resolve()` now verifies the sha256 and the UI offers a re-download. The pre-commit hook now blocks binaries, models and generated audio from ever entering the repo. Windows and CI unverified.

**Owner checklist M3:** download a model from inside the app → transcribe a short clip → watch progress → cancel one run and confirm nothing is left running → let one finish and see the cues appear, editable.

## M4 — Projects (tasks detailed 2026-08-28)

Goal: SQLite project (series → episodes → files) so memory has somewhere to live.

- [x] **M4.1 Schema and migrations.** One SQLite database file per project; schema for series, episodes, and the files attached to each episode (media path, subtitle paths, role); a versioned migration runner.
  - AC: creating a project produces a database at the chosen path with the current schema version; an automated test takes a database written at version N, migrates it, and verifies both the schema and every row survives (old db → migrate → verify, CLAUDE §2); a database from a newer version than the app is refused with a readable error, never silently altered.
- [x] **M4.2 Project lifecycle.** Create, open, close a project; add episodes; attach existing media and subtitle files to an episode by path.
  - AC: create a project, add two episodes with files, close and reopen: everything is still there with the same paths and order; attaching a file records only its path and metadata, never copies or moves the user's file; opening a database that is corrupt or not a Sublore project fails with a readable error and leaves it untouched.
- [x] **M4.3 Deletion safety.** Deleting a project or an episode removes only Sublore's own records and its own project folder contents.
  - AC: a behavioral test with real files on disk deletes a project whose episodes reference media and subtitles outside the project folder, then asserts every one of those user files still exists byte-identical (CLAUDE §3); no code path deletes outside the project folder.
- [x] **M4.4 Project UI, minimal.** Create/open a project, see its episodes and their attached files, add an episode, attach a file. No editing beyond that.
  - AC: E2E: create a project in a temp location, add an episode, attach a subtitle fixture, restart the app, reopen the project, the episode and its file are listed; errors surface as readable messages.

**Status 2026-08-28:** M4.1-M4.4 on `main` (developed in a parallel worktree, merged onto M2 part A). New crate `sublore-project` (SQLite store, versioned migrations, project lifecycle, deletion safety). 359 Rust tests and 22 E2E checks green after integration. Review caught two critical races in `Database::create`, both reproduced before the fix: a symlink could redirect the project database outside the folder the user chose, and two concurrent creates could delete the winner's finished project; both closed with an atomic claim plus `SQLITE_OPEN_NOFOLLOW`. Merge integration moved the project panel into a left sidebar so the cue list keeps its height. Windows and CI unverified.

**Owner checklist M4:** create a project → add an episode → attach a subtitle file and a video → close the app → reopen the project and find everything where you left it → delete the project and confirm your own video and subtitle files are still on disk.

## M5 — Termbase + QA (pro, closed module)

Goal: the product. Per-project glossary with approved renderings; QA pass flags every target line where a source term appears without its approved rendering.
Draft criteria: on the standard fixture episode, QA flags exactly the planted violations and nothing else; term added mid-series retro-flags earlier episodes on demand; module absent → core runs untouched, module present + licensed → features appear.

**Constraint (decision 6, 2026-08-29):** the text matcher and the ASS override-tag scanner live in an **open-core crate**, and M5 consumes it. The closed module keeps only persistence (TM and termbase storage) and QA policy. Search and QA share fixtures. The comparison both need is identical — find a source term in a line while ignoring override tags — and CLAUDE.md §4 requires the open core to be fully useful alone. Two engines would leave the free product unable to search, and the two would disagree about what counts as a match.

**Prerequisite:** N3 must be answered before this milestone starts.

## MW — Windows activation (mandatory before any sale or public release)

Goal: make Windows a verified platform instead of a compiled one. Filed 2026-08-29 with the platform policy change in CLAUDE.md; decision 3 puts the scattered Windows work here rather than inside feature milestones, where half-finished platform work would spread across all of them.

Today the `check` job builds on ubuntu and windows (`.github/workflows/ci.yml:18`), but the behavioural suite runs on ubuntu only (`:125-126`) and drives the app with `xdotool` over XTEST while inspecting windows with `xwininfo` (`e2e/lib/input.js:6-9`) — neither exists on Windows. So every behavioural verdict in this file covers Linux and nothing else.

- [ ] **MW.1 E2E backend for Windows.** Native input and window inspection behind the same harness interface, so specs stay platform-agnostic and assertions do not change.
  - AC: the full behavioural suite runs on Windows in CI and is green, with the same spec files and the same assertions as on Linux. A failure on either platform turns CI red.
- [ ] **MW.2 Platform hardening.** Work through what only Windows can show: the native video surface z-order (`video/surface/windows.rs` reasserts `HWND_TOP`), path and encoding handling, the crash dialog, the installer.
  - AC: the M0.2, M0.4, N1, N2 and M2.0 criteria are re-run on Windows and pass there, including the occlusion behaviour of decision 1.
- [ ] **MW.3 Owner checklist on Windows.** Every owner checklist in this file, run by the owner on a Windows machine from an installed build.
  - AC: the owner signs off. Until then no build ships to anyone.

**This milestone gates release.** v1.0 is not tagged and nothing is sold while it is open, however complete the rest of the plan is.

## M6 — Translation memory + licensing + release

Goal: exact and fuzzy TM across the whole project; offline license check; pay-once purchase flow via merchant of record; v1.0 tag.
Draft criteria: repeated lines in later episodes surface their earlier translation; fuzzy matches ranked and insertable; license file validates offline, invalid license degrades gently to free core; both installers pass the full owner checklist end to end.

**Prerequisite:** MW is closed. The v1.0 tag cannot precede Windows activation.

---

## Parking lot (explicitly not v1 — do not pull forward)

Karaoke/typesetting · diarization · built-in LLM translation beyond BYOK hook · macOS activation · auto-updater · batch CLI · Rust-side localized strings for the native crash dialog (v1 ships it English-only, owner decision 2026-08-23). Anything an agent wants to add lands here first and waits for the owner.
