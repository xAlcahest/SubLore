# Decisions — owner ruling 2026-08-29

All thirteen open questions raised in `post-v1-plan.md` under "Decisions due now" are decided. This file is the record; the backlog carries the work.

**Immediate execution order: 9, then 2, then 1, then the rest in plan order.** Decision 9 is an active data-loss defect, not a missing feature, so it jumps every functional milestone. Decision 2 is the prerequisite of decision 1.

---

---

## 14. How the video frame reaches the screen — owner ruling 2026-08-30

**Adopted as a working hypothesis, to be ratified formally at gate 2.** The X11 child window stays for v1.0, and M2.0's layer registry is built as permanent, not as a bridge to something else. The reasoning, the costs and the three conditions that would reverse it are in `x11-vs-render-api.md`, and those conditions stay written there as guards rather than being folded away here.

**Probe P6, shaping the surface, is parked.** It is not spent now. It becomes current when M16 or decision 1 makes it so, and not before. No new front is opened on this.

## 1. Video occlusion: the native surface hides for HTML layers

When a menu or a dialog opens over the video, the native surface hides, and it comes back when the layer closes. No native system menus, no separate popup windows.

**Why.** The surface raises above the webview on every region update (`surface/mod.rs:80`, `linux.rs:66`), and M2.0 puts a menu bar and a transcription dialog exactly there. Of the three ways out, native menus would mean giving up the shared look across Windows and Linux that the CSS chrome buys us, and separate windows are heavier and worse on tiling WMs. Hiding is cheap and keeps one implementation.

**Delivery.** E2E test: a menu opened over a playing video is visible. Belongs to M2.0. Depends on decision 2.

## 2. Show/hide with a loaded video, built now

Build the re-show path immediately, with a behavioural test that hides and re-shows while a video is already loaded, asserting on **the visible frame**, not on the internal state flag.

**Why.** `show()` is called in exactly one place, inside `video_open` (`video/mod.rs:106`), and its own comment warns it must run before mpv builds its video output, because mpv creates its window inside ours and leaves it unmapped if ours is (`surface/mod.rs:82-84`). Hide-then-show after opening has never been exercised. Decision 1 rests entirely on this working, and M2.0's rule that panels disappear with their provider only works one way without it.

**Delivery.** Precedes M2.0. Asserting on state instead of pixels would pass while the user sees black.

## 3. Windows E2E moves into the Windows activation milestone

The E2E input and window-inspection backend for Windows is not scattered work: it belongs to the Windows activation milestone, which is now mandatory before any sale or public release (CLAUDE.md platform policy). The `check` job keeps compiling Windows on every push.

**Why.** The suite drives the app with `xdotool` over XTEST and inspects windows with `xwininfo` (`e2e/lib/input.js:6-9`); neither exists on Windows. Doing this piecemeal inside feature milestones would spread half-finished platform work across all of them. Collected in one milestone it is finite, and the release gate makes it impossible to forget.

## 4. Bulk edit: composite history entry

One history entry holds a transaction of N child edits under a single label.

**Why.** Today an entry carries one `Splice` (`history.rs:37-39`), `Edit` names a single cue (`plan.rs:28-58`), and coalescing needs matching label and offset (`history.rs:192-195`), so edits on different cues never merge. The alternative, a family of range variants emitting one splice, only covers contiguous ranges; sparse selections and QA fixes across an episode are the actual use case.

**Delivery.** Test: edit scattered rows, one undo returns the document byte-identical.

## 5. Grid selection: active line and selection are separate

The cursor (single active line) and the selection (a set: single, shift for a range, ctrl for scattered) become two distinct pieces of state, both drivable from the keyboard. Bulk operations act on the selection.

**Why.** One React index does both jobs today (`CueList.tsx:85`). M2.5's criteria already name "play selection" and M5's QA will want to select every flagged row. Left alone, M2.5 ships a selection that means the cursor and M5 invents its own flagged-set, and the two never reconcile.

**Delivery.** Before M2.5 depends on it.

## 6. Text matching engine lives in the open core

The matcher and the ASS override-tag scanner go in an open-core crate. M5 consumes it. The closed module keeps only persistence (TM and termbase storage) and QA policy. Search and QA share fixtures.

**Why.** CLAUDE.md §4 requires the open core to be fully useful alone and forbids pro branches in the open repo. The comparison both features need is identical: find a source term in a line while ignoring override tags. Two engines would mean the free product cannot search and the two disagree on what counts as a match.

## 7. Subtitles on video: the translation, with a toggle

The video shows the translation document, with a toggle to show the source instead. The preview is fed from a shadow copy in the working folder. **Never save the user's file to produce a preview.**

**Why.** Subtitles are off by configuration today (`player.rs:41`) and nothing calls `sub-add`. With two documents open, something has to say which one is on screen, and answering that after M2.6 means reopening the finished two-document model. The shadow copy exists to remove the temptation of reloading the saved file, which would overwrite the user's file on every keystroke and eat the backup ring (CLAUDE.md §3).

## 8. Autosave: its own store, untouchable backups

Autosave gets a separate store, a naming convention the backup pruner cannot see, and its own retention policy. Recovery is offered when the app reopens after a crash. Overwrite backups of user files are never touched by a timer.

**Why.** The backup cap is ten (`backup.rs:21`) and pruning keys off the source filename (`backup.rs:208-216`), so an autosave timer sharing that store would delete the user's real safety copies in ten ticks. That is a data-safety regression against CLAUDE.md §3.3, and the kind of bug this project exists not to have.

## 9. Close gate: active defect, fixed first

Intercept `CloseRequested`. If anything is unsaved, ask save / discard / cancel, per dirty document, and honour the answer.

**Why.** There is no `prevent_close` anywhere in the repo; closing the window with unsaved edits throws them away silently. Dirty state is already tracked on both sides and nobody consults it on close. The dialog plugin is already a dependency. This is not a missing feature, it is a live data-loss path, and M2.6 doubles it by opening a second document.

**Delivery.** Before every other functional milestone.

## 10. Non-cue segments: half a day of written analysis now

Write the analysis now of whether `Edit` and `Expectation` can grow a `Meta` variant for `Style:` lines, script properties and attachments. Adjust the shape now if the answer says so. The feature itself stays at M14.

**Why.** `Edit` covers cues only (`plan.rs:28-58`) and everything else travels as uninterpreted metadata (`document.rs:81-92`). The crate is small and its only consumers are its own tests and twelve commands; once M5 and M6 depend on it, a second write path gets bolted alongside the first instead of replacing it. The analysis is cheap and its conclusion may be "no change needed", which is a fine result to have in writing.

## 11. Frames: no. Milliseconds, final

The product reasons in milliseconds. Every "pending a frame engine" reservation comes out of the plans. If a frame seam is ever needed, it lives in the player and nowhere else.

**Why.** There is no notion of framerate, frame or keyframe in production code, and none of v1's three formats is frame-based: SRT and VTT are milliseconds, ASS centiseconds. Carrying a phantom blocker inflated timing estimates without describing real work.

## 12. M2.4 audio provider: reuse the pattern, not the ASR code

M2.4 reuses the shape of the ASR path — ffmpeg discovery, background execution, progress, cancellation — and not its code. Extraction runs at full quality, behind a public API with a per-episode cache, and its lifetime is tied to the episode, not to a transcription run.

**Why.** The ASR extractor is private, writes into a scratch folder that deletes itself when the run ends (`scratch.rs:88-91`), and produces mono 16 kHz because whisper wants that (`sidecar.rs:285,302-304`). Fine for peaks, wrong for playing a selection or exporting a clip, and the lifetime is wrong in a way that would make the file vanish mid-session.

## 13. "New translation from source"

A command in M2.6 creates a new document inheriting the source's cues and timings with empty text. The source is read-only while translating. The first save asks for name and location with a sensible proposal (episode plus language). **The source is never modified or overwritten.**

**Why.** There is no new-document command, and save-as writes a copy elsewhere while leaving the session pointed at the original and still dirty, by declared choice (`subtitle/mod.rs:390-391`). A translator handed only a source file has no clean route to a target; the route they would find on their own goes through the unsaved-changes refusal and a Discard button pointed at the source file. That is the first gesture of their working day.
