# Verification of revision 3's corrections to `m2-0-tasks.md`

An adversarial pass over the corrections another agent applied to `docs/design/m2-0-tasks.md` against
`docs/reports/m2-0-prontezza.md`. Every quoted plan sentence was re-checked at the file and line it
names. Nothing was executed: `cargo`, `pnpm` and the E2E battery were not run, so nothing here is a
behavioural verdict on any platform. Two files were edited: the plan, and this report.

**Platform.** Every statement below about the harness is about the Linux/X11 suite, as in the
readiness report. No Windows claim is made or implied.

**Baseline re-derived independently.** `e2e/wdio.conf.js:17` reads `const EXPECTED_TESTS = 33`, and
`grep -c '^\s*it('` over the eight spec files gives asr 5, editor 10, project 5, subtitle 3, title 2,
video 2, video-empty 3, video-surface 3, total 33. The plan's chain and every guard line match.

---

## 1. The blockers

### B1 — the visibility protocol. **Closed**, after two repairs

The plan now says, at §2.1: "**There is no visibility command, and T3 must not add one.**
`src-tauri/src/lib.rs:100-104` registers exactly five video commands ... and `video/mod.rs:30-31`
states the rule in its own words, 'Visibility is derived, never set'."

True at every point. `lib.rs:100-104` is exactly `video_open`, `video_play`, `video_pause`,
`video_seek`, `video_set_region`; the quoted sentence is at `video/mod.rs:30`; `wants_shown()` is
`:57-59`; `settle()` is `:64-88`; `apply_region` is `:263-276` and applies geometry only when the
rectangle has area, then settles, exactly as the transport bullet says. The spec sentence the plan
leans on is at `video-surface.spec.js:14-16`. Cited `(prontezza B1, :33)` in five places.

Two things in the corrected text were wrong and are now fixed:

- The `report()` bullet claimed four callers "all of them through one `report()`". The unmount does
  not go through `report()`: it hands `HIDDEN` straight to the prop (`VideoStage.tsx:61`). Reworded
  to say three coalesced into one `report()` per frame (`:44-53`) plus the unmount, one
  `onRegionChange` either way; the citation `useVideoPlayer.ts:112-118` corrected to `:112-117`.
- §2.1's `surfaceWanted` bullet is revision 2 text that survived beside the new transport bullets,
  and read as if three inputs reached the wire. Added one sentence saying only `layerCount` does,
  because `videoLoaded` is the backend's own `video_open` (`video/mod.rs:57-59`). This is the exact
  hazard the section exists to prevent, and it was the one bullet left pointing the other way.

### B2 — the baseline is 33. **Closed**

§4: "**The baseline is 33, not 27.**" The chain 33 → 35 → 36 → 39 → 43 → 47 → 50 → 50 → 50 → 54 → 55
matches every per-task guard line, §8's totals and §6's restated rule ("it never decreases"). The
only surviving 27s and 29s are inside sentences that name them as revision 2's error. Verified count
above. `BACKLOG.md:136` still says 27; §1 says so and says it is owed, which is the honest handling
of a file this edit may not touch.

### B3 — the close gate's route. **Was not closed. Now closed**

§0 fact 3 and §2.4 are right: `close-gate-check.js:42-43` carries one point, `:141-147` launches with
the fixture in argv under the quoted comment, `TOOLBAR_OPEN_SUBTITLE` is deleted with the decision
stated, and T5b's N1-debt paragraph names the waits that exist (`sleep(3500)` at `:180`, `sleep(600)`
at `:186`, `sleep(2500)` at `:191`).

But **T2's close-gate criterion still read "the two points it clicks are re-derived"**, which is the
exact sentence B3 exists to kill, and it carried no citation. The plan therefore stated the same fact
two ways: "its one point" in §0 and §2.4, "the two points" in T2. Rewritten to name `FIRST_CUE_TEXT`
and say there is no second point, cited `(prontezza B3, :96)`.

### B4 — the two N2 specs. **Closed**, per the owner's ruling

`video-surface.spec.js` and `video-empty.spec.js` are in T3's and T5b's Files owned, each with the
obligation written into its own delivery. Their routes through the deleted controls are real:
`.bar__input` and `.bar__button` at `video-empty.spec.js:60,99,102,110` and
`video-surface.spec.js:116,122,130,135`. `setStageCollapsed` is answered correctly: the lever is
`style.height = 0` (`:41-50`), it produces a zero-area reported rectangle under the corrected
contract, and the spec's own proof that the DOM collapsed (`:53-62`) keeps a failed collapse from
reading as a successful hide. T5a's green-battery criterion names both files without claiming
ownership, which is what the owner's ruling leaves it.

### B5 — the pixel signal. **Closed**

`e2e/lib/pixels.js` exists (87 lines), `SATAVG` is at `:18` and `saturation()` at `:36`,
`wdio.conf.js:8,34` imports and calls `requireFfmpeg()` on every run, and `pixels.js:36-87` genuinely
carries no mpv-child precondition. `video-surface.spec.js:9-12` is quoted verbatim and correctly. The
5.86/2.1 pair is on the record at `BACKLOG.md:93`. Saturation is out of CI, the three criteria now
assert map state plus mpv's child window, and `pixels.js` is marked existing and removed from T1b's
new-file list.

### B6 — `dialog.rs`. **Closed**, after one line-number repair

`src-tauri/src/dialog.rs` is 156 lines, added by `fee26f8`, and its module doc at `:1-11` says what
the plan quotes, word for word. `WORKFLOW.md:55` does name the close path as gate 2's pre-declared
lens. `project/mod.rs:244-257` is `choose_path`, with `blocking_pick_folder` at `:252` and
`blocking_pick_file` at `:257`, so T2's "one plugin call site into four" is accurate.

The N1c paragraph cited `BACKLOG.md:113`, which is the two-killed-hypotheses bullet of N1b. N1c is at
`BACKLOG.md:114`. The readiness report made the same slip and the correction inherited it. Fixed.

### B7 — Ctrl+A and criterion 10. **Closed**

`CueList.tsx:17` is `ROW_HEIGHT = 28`, `:19` is `OVERSCAN = 8`, `:314` renders `indices` only, so the
DOM-answerable restatement is right. `editor.spec.js:231-263` is the virtualization check: three
scrolls, `expect(sample.rows).toBeLessThanOrEqual(60)` at `:258`, and it presses no key, exactly as
the plan now says. T7's Select-all row carries the same restatement.

---

## 2. The serious findings

- **S1 (graph head). Closed.** N2 is `[x]` at `BACKLOG.md:84`, N2b `:89`, N2c `:98`, N1b `:106`; the
  fallback-order bullet is deleted and named as deleted.
- **S2 (gate 2 as predecessor). Closed.** Stated in T1's and T1b's headers and in §5, against
  `BACKLOG.md:74` and WORKFLOW §4a.
- **S3 (N2c and the unit). Closed.** N2c is a closed node and a named predecessor of T3 and T5a;
  `src/types/video.ts` is T3's; the unit is stated once in §2.1 and once in §2.4; both geometry
  criteria say native device pixels and both carry the "Xvfb cannot discriminate" line. Verified in
  the tree: `VideoStage.tsx:30-41` resolves by `devicePixelRatio`, `src/types/video.ts:4,7` states the
  unit, and `video.spec.js:10,107-115` already compares `rect × dpr` inside a 2 px tolerance, so the
  criteria describe the instrument that exists.
- **S4 (`LF_STATUS`). Closed.** `subtitle.spec.js:16` holds `"SRT · 3 cues · LF"`, asserted with
  `toBe` at `:129`; `editor.spec.js:44` holds the 2,000-cue literal. Corrected in T2, T5b and T7's
  table.
- **S5 (the accessible name). Closed.** `asr.spec.js:144` asserts only the disabled property;
  `TranscribeBar.tsx:117-124` gives the button no `title` and no `aria-label`; `grep noVideoReason`
  over `src/` and `e2e/` returns nothing, so `en.asr.noVideoReason = "Open a video first."` genuinely
  predates the implementation. The "this is what today's button does" claim is corrected.
- **S6 (T2/T4 not disjoint). Closed.** `App.css` split by block, `App.tsx` moved into T2 and frozen
  before T4, `App.tsx:47`'s `onChoosePath={project.choosePath}` verified.
- **S7 (T6/T7). Closed.** `Shell.tsx` is in T6's files, the pair is sequential in §5 and §8, and
  `wdio.conf.js` is named in the shared list.
- **S8 (`[owner checklist]`). Closed.** One criterion on T5a (the arrangement and the palette), one on
  T6 (the rail against the mockup), both marked unautomated.
- **S9 (the two states look different). Closed**, citation repaired. The criterion is there and can
  fail. The quoted sentence is at `docs/design/shell-layout.md:219`, not `:220` as the readiness
  report and the correction both said. Fixed.
- **S10 (zero samples). Closed.** "At least 5 samples", the count asserted, failure as "did not
  sample". `BACKLOG.md:88` carries the gate-1 precedent the criterion cites.
- **S11 (cold-start threshold). Closed.** The probe produces the number, the delivery writes
  measurement plus headroom into the script, the §7 verdict stays owed on a release build.
- **S12 (named control, open element set). Closed**, after one repair. Moving the save-copy-as control
  to T5b is right: at T5a there is no toolbar. But the enumerated list written for T5a ended with
  `.status__document`, which does not exist until T5b renames `.subbar__status` to it (§3's own
  table). A criterion naming an absent selector cannot fail, which is the defect S12 was raised
  against. Replaced with `.status__transcribe`, the one status-line element T3 leaves mounted, with
  the reason stated inline.
- **S13 (three unowned scripts). Closed**, with a gap now stated. `real-session-check.mjs:52-56`
  carries `videoField: 683/1024` and `videoOpen: 978/1024`; `n1b-load-probe.js:33-34` carries its own
  `FIRST_CUE_TEXT`; `wayland-attach-check.js:91` and `scaled-surface-check.js:73` both spawn with the
  fixture in argv and contain no click, key or type call, so "unaffected" is verified rather than
  assumed. What no one had noticed: **T3 moves the stack and T5a is where the second copy of the
  point is folded away**, so the N1b battery run in between aims at the old layout. Nothing turns red
  (the probe is in no package script, no CI job and none of the three runners), so it is stated in T3
  as a delivery note rather than made into a dependency.
- **S14 (`useStartupFiles`). Closed**, after completing it. The hook was in T5a's and T5b's owned
  files but the readiness report also asked for §0's inventory, where only the Rust `startup_files`
  appeared. Added to §0 fact 1 with `src/App.tsx:23`.
- **S15 (`e2e/README.md`). Closed.** `:144` says "Renaming one breaks the harness" and `:146-157`
  lists the names §3 moves, including `.cuelist__row--selected` at `:155`. The file is in T3's, T5b's
  and T6's owned lists and §6 carries the standing rule.
- **S16 (gate 4). Closed.** On T2, on T5b, and in §6 and §8.
- **S17 (N1's open-editor gap). Closed.** §1 says it stays deferred and names T3 as the task that says
  so; T3 carries the criterion. `BACKLOG.md:81` is the entry it comes from.
- **S18 (two-toplevel clause). Closed**, after removing a duplicate. The clause is dropped and the
  drop is explained. The paragraph then repeated the "the guard stays exactly as it is" sentence
  already made ten lines above, with a different citation for the same code (`x11.js:81` in one,
  `:80-86` in the other). One citation kept (`:80-86`, which is the whole guard), the repetition
  removed.
- **S19 (typing into a native dialog). Closed.** Fact 2 now claims focus, an estimated click and
  Escape, which is what `close-gate-check.js:117-129` and `:136-139` do, and names the typed entry as
  the unproven gesture T1 exists to answer.

**The nine minor findings** are all applied and all cited by name. One of them, §0 fact 1, was applied
with a false replacement and is discussed in §3 below.

**One item from the readiness report's §5** ("what was checked and found sound") carried a live
question that nobody answered: the `CUE_LIST_EMPTY` route T3 adopts depends on a behaviour §2.2
contracts and **T4** builds, and T3 does not depend on T4. Verified in the tree that the route already
works today, so the fix is a statement rather than a dependency: `.cuelist` carries `tabIndex={0}` and
the keydown handler (`CueList.tsx:304-312`), the only pointer handlers are on a row and a row's text
(`:334`, `:351`), and Enter opens the editor on the active row (`:239-242`). Added to §2.4.

---

## 3. What I changed in the plan

Nine edits, all in `docs/design/m2-0-tasks.md`, each carrying the citation of the finding it belongs
to. The file is prettier-clean.

1. **§0 fact 1** said "All 33 checks reach their subject that way (`typeInto(...)` then a click on the
   adjacent open button)". False twice over: `title.spec.js`'s two checks open no file at all, and
   only three of the eight spec files call `typeInto` (`grep -c typeInto`: project 8, subtitle 6,
   editor 4, the rest 0). `video.spec.js:53-66` and `asr.spec.js:149-155` focus the box and type into
   it, `video-surface.spec.js:122-136` clicks and drives `xdotool type`, `video-empty.spec.js:99-111`
   goes through the input's own value setter. The conclusion survives; the sentence did not. Rewritten
   per spec file, and `useStartupFiles` added here for S14.
2. **§2.1's `surfaceWanted` bullet**: one sentence saying which of the three inputs reaches the wire
   (B1).
3. **§2.1's `report()` bullet**: the unmount does not go through `report()`; citation corrected (B1).
4. **§2.4**: the pre-T4 statement for the `CUE_LIST_EMPTY` route (readiness §5).
5. **§2.4**: `budget-check.js` is no longer asserted flatly as a `shell-points.js` consumer, since the
   same section's minor fix has T8 loading its fixtures from argv. It reads it only if it still
   clicks.
6. **T1b**: the duplicated "the guard stays" sentence removed, one citation kept, `paths.js:22-23`
   named for the 1024x700 default (S18).
7. **T2's close-gate criterion**: "the two points" to one point (B3).
8. **T2's N1c paragraph**: `BACKLOG.md:113` to `:114` (B6).
9. **T3**: the `n1b-load-probe.js` stale-copy note (S13). **T4**: `shell-layout.md:220` to `:219`
   (S9). **T5a**: `.status__document` to `.status__transcribe` in the enumerated element list (S12).

---

## 4. Still open, and what the owner has to rule on

- **The N1c ordering, which is a real decision and not a documentation gap.** T2 states the two
  options and says the order goes to the owner before T2 starts. It is the one thing in the plan that
  cannot be closed by any amount of editing: either N1c runs first and T1's by-title lookup is
  re-validated against a GTK-direct picker, or T2 builds the four choosers on GTK and closes N1c in
  the same delivery. Leaving it undecided doubles one of the two tasks.
- **`BACKLOG.md:136` still says "the 27 existing E2E checks".** It is the M2.0 acceptance criterion
  and it disagrees with `wdio.conf.js:17`. §1 records the debt. It is owed before T1 is written
  against that AC, and it is a one-line edit in a file this task may not touch.
- **The un-audited revision-2 claim.** The plan still says every blocking and serious finding from the
  two earlier critiques was applied, and no lens has re-walked that. §9 records it as open rather than
  inheriting it silently. Unchanged, and correct as handling.
- **`src-tauri/src/video/mod.rs:90` still documents `VideoRegion` as "in CSS pixels"** after N2c moved
  the unit, while `src/types/video.ts:4` on the other side of the same contract says native device px.
  A stale comment on a public interface, in source, so it is gate 2's or the next opener's, not this
  document's.
- **What no lens, including this one, could check.** Nothing was executed here either. Every gesture
  the plan hangs on T1 and T1b (typing into a `GtkFileChooser`, `xdotool windowsize` under a WM-less
  Xvfb, stamping the clock in the page while a native chooser is up) is still unproven, and the
  readiness report's §6 remains the honest list.
- `docs/reviews/gate-2-plan.md` shows as modified in `git status`. It was already dirty before this
  pass and was not touched by it.

---

## 5. Verdict

**M2.0 can open.** All seven blockers and all nineteen serious findings are addressed in the plan,
every correction traces to the report line that caused it, and the factual base now matches the tree
at `c7261a5`. B3 and S12 were not actually closed by the first correction pass, and both left a
criterion that could not fail, which is the defect class this repo has been bitten by three times;
both are closed now. What stands between the plan and T1 is gate 2 and the owner's N1c ordering call,
which is where the plan itself says they stand.
