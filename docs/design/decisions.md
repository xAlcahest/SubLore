# Decisions — owner ruling 2026-08-29

All thirteen open questions raised in `post-v1-plan.md` under "Decisions due now" are decided. This file is the record; the backlog carries the work.

**Immediate execution order: 9, then 2, then 1, then the rest in plan order.** Decision 9 is an active data-loss defect, not a missing feature, so it jumps every functional milestone. Decision 2 is the prerequisite of decision 1.

---

---

## 14. How the video frame reaches the screen — owner ruling 2026-08-30

**RATIFIED, final — owner ruling 2026-09-01.** No longer a working hypothesis: the X11 child window is the answer for 1.0, occlusion is solved by hiding the surface for HTML layers (decision 1), and no render API is opened before M16. T8 is unblocked, and with it M2.4, M2.5, M2.6 and MW.2. The three reversing conditions in `x11-vs-render-api.md` stay written as guards for a post-1.0 reader; none of them fires before M16.

The X11 child window stays for v1.0, and M2.0's layer registry is built as permanent, not as a bridge to something else. The reasoning, the costs and the three conditions that would reverse it are in `x11-vs-render-api.md`, and those conditions stay written there as guards rather than being folded away here.

**Probe P6, shaping the surface, is parked.** It is not spent now. It becomes current when M16 or decision 1 makes it so, and not before. No new front is opened on this.

## 1. Video occlusion: the native surface hides for HTML layers

When a menu or a dialog opens over the video, the native surface hides, and it comes back when the layer closes. No native system menus, no separate popup windows.

**Why.** The surface raises above the webview on every region update (`surface/mod.rs:99-101` — `set_region`, moved here by `c7261a5`'s 146-line addition; `linux.rs:66-67`), and M2.0 puts a menu bar and a transcription dialog exactly there. Of the three ways out, native menus would mean giving up the shared look across Windows and Linux that the CSS chrome buys us, and separate windows are heavier and worse on tiling WMs. Hiding is cheap and keeps one implementation.

**Delivery.** E2E test: a menu opened over a playing video is visible. Belongs to M2.0. Depends on decision 2.

## 2. Show/hide with a loaded video, built now

Build the re-show path immediately, with a behavioural test that hides and re-shows while a video is already loaded, asserting on **the visible frame**, not on the internal state flag.

**Why.** `show()` is called in exactly one place, inside `video_open` (`video/mod.rs:106`), and its own comment warns it must run before mpv builds its video output, because mpv creates its window inside ours and leaves it unmapped if ours is (`surface/mod.rs:98-99`, moved here by `c7261a5`'s 146-line addition). Hide-then-show after opening has never been exercised. Decision 1 rests entirely on this working, and M2.0's rule that panels disappear with their provider only works one way without it.

**Delivery.** Precedes M2.0. Asserting on state instead of pixels would pass while the user sees black.

## 3. Windows E2E moves into the Windows activation milestone

The E2E input and window-inspection backend for Windows is not scattered work: it belongs to the Windows activation milestone, which is now mandatory before any sale or public release (CONTRIBUTING.md platform policy). The `check` job keeps compiling Windows on every push.

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

**Why.** CONTRIBUTING.md §4 requires the open core to be fully useful alone and forbids pro branches in the open repo. The comparison both features need is identical: find a source term in a line while ignoring override tags. Two engines would mean the free product cannot search and the two disagree on what counts as a match.

## 7. Subtitles on video: the translation, with a toggle

The video shows the translation document, with a toggle to show the source instead. The preview is fed from a shadow copy in the working folder. **Never save the user's file to produce a preview.**

**Why.** Subtitles are off by configuration today (`player.rs:41`) and nothing calls `sub-add`. With two documents open, something has to say which one is on screen, and answering that after M2.6 means reopening the finished two-document model. The shadow copy exists to remove the temptation of reloading the saved file, which would overwrite the user's file on every keystroke and eat the backup ring (CONTRIBUTING.md §3).

## 8. Autosave: its own store, untouchable backups

Autosave gets a separate store, a naming convention the backup pruner cannot see, and its own retention policy. Recovery is offered when the app reopens after a crash. Overwrite backups of user files are never touched by a timer.

**Why.** The backup cap is ten (`backup.rs:21`) and pruning keys off the source filename (`backup.rs:208-216`), so an autosave timer sharing that store would delete the user's real safety copies in ten ticks. That is a data-safety regression against CONTRIBUTING.md §3.3, and the kind of bug this project exists not to have.

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

---

## 15. One ASR model for all four languages — owner ruling 2026-08-30

Sublore ships **`whisper-large-v3-turbo`** (MIT) as the single model for Japanese, Korean, Chinese and English. Language-specific fine-tunes are excluded: insufficient evidence or technical risk — dated bases, quantisation the authors advise against, foreign runtimes. Parakeet stays on the record as an English reference that cannot be shipped.

**Why.** Two sweeps, `docs/research/asr-anime.md` and `docs/research/asr-ko-zh-en.md`, are the source. Between them: no anime ASR exists — the models labelled anime are trained on visual-novel voice, dry studio recordings with no music, no effects, no overlap, so "anime" there names an acting style and not an audio source; the strongest of them are unshippable anyway, one under a corpus that forbids commercial use of any model derived from it and the others under no licence at all; and on anime-style audio plain `large-v3` beats kotoba-whisper, parakeet-ja and reazonspeech. Korean has nothing shippable that beats Whisper. Chinese does have a real candidate, and it is not taken: see below.

**Chinese punctuation is a post-ASR problem**, parked beside re-punctuation rather than solved by a second model. This is the one place the sweep found a shippable in-architecture win — `Belle-whisper-large-v3-zh-punct`, Apache-2.0, roughly halving Mandarin CER with native punctuation — and the ruling declines it anyway, because a per-language model is a UI, a download, an in-app licence and a verification matrix, not a model swap.

## 16. Boundary tuning moves to an external VAD — owner ruling 2026-08-30

whisper.cpp's built-in Silero VAD is **not** the route for this domain. Cue-boundary tuning becomes "external VAD with timestamp remapping", parked as a post-v1 pipeline task, not a flag.

**Source, stated honestly.** The ruling cites empty transcriptions documented on Japanese audio with background music, from the kotoba-whisper-v2.2 card. That statement **could not be confirmed**: both the v2.2 and the v2.1 cards were read on 2026-08-30 and neither mentions Silero, any VAD, or that failure mode. The decision stands on the owner's authority; the citation is owed and this paragraph stays until it arrives or is withdrawn.

**What this changes in what was already written.** `asr-anime.md` presented the built-in `--vad` as a free lever, zero new dependencies. That recommendation is superseded here. The measurement behind it is untouched and still holds — segmentation moves the number and source separation does not — but the instrument changes.

## 17. A save that wedges holds the window, on purpose — owner ruling 2026-08-31

`save_current` takes the session lock and waits. If another command never releases it, the close gate stays in `Acting` for the life of the process and the window cannot be closed. **This is accepted, and no timeout is added.**

**Why.** Every automatic release is worse than the wedge. Forcing the close over a save in flight throws away the work the gate exists to protect, and raising a second dialog puts two saves in a race on one session. Data safety wins over responsiveness; a window that will not close is visible and recoverable, work that vanished is neither.

**What is owed instead.** A post-v1 parking-lot item: after a threshold, say so — a "still saving" indicator, so that an unbounded wait is at least loud. The wait stays unbounded; it stops being silent.

## 18. The NVIDIA signal stays broad — owner ruling 2026-08-31

`main.rs` looks for `/sys/module/nvidia`, which answers "is the module loaded", not "is NVIDIA drawing". **The broad signal is kept.**

**Why.** The asymmetry decides it. A false positive costs the slower rendering path, which is an annoyance. A false negative costs a blank window, which is a dead product at launch. `SUBLORE_WEBKIT_WORKAROUNDS` exists in both directions for whoever is on the wrong side of the guess. Reopened only on a real report from a hybrid laptop, not on the theory of one.

## 19. The code answers before the owner does — owner ruling 2026-08-31

When a question is "how should the editor behave here", it is not a question for the owner until Sublore's own code has been read for the answer. Much of the behaviour is already decided and written down; asking about it spends the owner's time on something the repository already says.

**Why.** A decision was put to the owner about whether committing an unchanged field should dirty the document, when the answer was already in `session.rs:80` and in a test called `committing_an_unchanged_field_is_not_an_edit`. The question was asked from a log instead of from the code.

**How to apply.** Read the code. If it settles the question, follow it and say where it was settled. If it does not, decide it, state the reason here, and only then take it to the owner if the reason is one he owns.

## 20. Two classes of data, two homes — owner ruling 2026-09-01

Derived data goes in the app's cache directory. Irreplaceable data stays in the project folder.

**Derived** means regenerable from a file the user already has: waveform peaks, thumbnails, indexes. It lives in the app cache, keyed by a hash of the source file, under a size cap, and deleting all of it costs the user nothing but time. **Irreplaceable** means the only copy: the timestamped backups CONTRIBUTING.md §3.3 requires before any overwrite. Those stay in the project folder, and nothing automatic deletes them.

**Why.** The peaks cache was the question; the answer is a rule, so the next derived artefact does not come back as another question. It also keeps the project folder small enough to be copied, synced or mailed without dragging a cache along.

**How to apply.** Before adding any new stored artefact, ask which class it is. Derived: app cache, hashed key, capped, freely deletable. Irreplaceable: project folder, never deleted by cleanup logic. Settles the peaks cache for M2.4 and the backup root that `sublore-io/src/backup.rs` and `subtitle/mod.rs` disagree about today.

## 21. The pro repo, and an ABI that survives a compiler upgrade — owner ruling 2026-09-01

The closed modules live in their own private repository, named in the working notes and deliberately not here: this file is public, and the repository's name is not something the open core needs to know.

The interface is designed before any M5 code exists: **N8, a design task with a written document and a review**, and its constraints are fixed now rather than discovered later. Modules are dynamically loaded libraries behind a versioned interface that is stable across compiler versions — a C ABI or an equivalent, never bare Rust types across the boundary. Loading performs a version handshake; when the module is absent or the version does not match, the app degrades cleanly to the free core and says so. The interface crate `sublore-module-api` lives in the public repo and is part of the open core.

**Why.** Rust has no stable ABI, so a module built by a different compiler than the app is undefined behaviour waiting for a user to find. A handshake that fails into the free product is also the honest failure mode for a paid module: the free core must stay fully usable, per CONTRIBUTING.md §4.

**How to apply.** N8 comes before M5. The matcher and the ASS override-tag scanner stay in the core (decision 6). No licence logic, no `isPro` branch, ever, in the public repo.

## 22. The build runs in waves, with the Windows lane parallel throughout — owner ruling 2026-09-01

The wave plan is adopted as proposed. T2 runs alone, owning all of `src/` and every spec, because nothing in the shell parallelises until it has split `App.tsx` and `App.css` into per-region files. The Windows activation lane runs beside the shell lane from the first wave to the last, because it touches no file under `src/`. A colour token layer is defined inside T2, so T3 through T7 write `var(--...)` and never a literal. The restyle is one pass after T7. `EXPECTED_TESTS` is split per platform before the Windows CI wave, because it is one integer today and any Linux-only check would fail the Windows run.

**Why.** The measured shape of the dependency graph, not a preference: the serial spine runs harness geometry, T2, T3, T6, T5, T8, T9, then M2.4 to M2.6, and its length is what sets the finish date. Everything not on that spine is worth running beside it.

**How to apply.** One task per branch. Before scheduling two tasks in the same wave, check they do not both edit a shared file — `src/App.tsx`, `src/App.css`, `src/i18n/en.ts`, `src-tauri/src/lib.rs`, `e2e/wdio.conf.js`. The layout pixel constants outside `src/` (`e2e/lib/paths.js`, `e2e/scripts/picker-thread-check.js`) have one owner per wave and are re-measured with a screenshot as evidence, because the scripts that read them gate CI and sit outside `EXPECTED_TESTS`, so they go red with no counter to warn anybody.

## 23. Maximum autonomy — owner ruling 2026-09-01

The project advances on its own. The full contract is `WORKFLOW.md` §8; this is the record that the owner made the call and the date he made it.

In force after the decision block of the same ruling is delivered. Code no longer waits for approval: a green battery, a self-reviewed diff and an autonomous merge replace it. Gates run and open themselves. Anything not covered by the block follows the recommendation, applied and recorded here as an autonomous decision the owner can reverse by reading. Four things still stop the march: an ambiguity about data safety that no written rule settles, a choice that would move the open-core or licensing boundary, a technical block still standing after two serious attempts with a written report, and any irreversible action outside the repository.

**How to apply.** Autonomy widens who decides. It changes nothing about what may be claimed: reports on file, verdicts carrying their platform, positive proof in tests, measured numbers, and never a green for the wrong reason.
