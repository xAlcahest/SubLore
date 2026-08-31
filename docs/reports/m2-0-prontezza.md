# M2.0 readiness — can the milestone open at gate 2

Six delegated lenses read `docs/design/m2-0-tasks.md` end to end (1124 lines) against the tree as it
stands: `main` at `18fe5f3`, plus the uncommitted N2c work on `src/components/VideoStage.tsx`,
`src/types/video.ts` and `src-tauri/src/video/**`. The lenses were acceptance criteria, order and
dependencies, file ownership and parallelism, the contracts frozen in §2, citations, and scope
against CLAUDE.md §1 and `decisions.md` 1–14. Their findings are deduplicated here and every claim
below was re-checked at the file and line named. Nothing was executed; nothing was modified.

**Platform.** Every statement about the harness is about the Linux/X11 suite. No Windows claim is
made or implied.

---

## 1. The verdict

**M2.0 cannot open as written. It can open with the seven corrections in §2 applied first, and
those corrections are edits to the plan, not to the app.** The plan is sound in its architecture,
its scope and its incrementality: nothing it builds is outside CLAUDE.md §1, decision 14's ruling
that the layer registry is permanent is honoured, and the task graph's shape survives. What has
failed is its _factual base_. The plan was written on 2026-08-30 at 02:17, one minute after N2
merged and before N2b, N1b and the GTK dialog work landed, so it describes a repository that
stopped existing the same night. Its two most load-bearing frozen contracts — the video visibility
protocol in §2.1 and the close gate's route in §2.4 — both describe mechanisms that no longer
exist, and T3 is instructed to begin by reading a command signature that was never built. Opening
T1 on this document does not risk a slow milestone; it risks T3 inventing the second hide path that
N2 and §2.1 both forbid, and T5b turning six merged checks red in files no task owns.

---

## 2. Blockers — fix before T1 starts

### B1. §2.1 lines 129–140: the visibility protocol is written against a command N2 did not build

**What the plan says.** "Going hidden: send visibility false. Coming back: send the last measured
region **first**, then visibility true, in that order." "While the set is non-empty, `VideoStage`
keeps measuring on resize and stores the rectangle, and **sends no region**." "Backend command:
**N2's**. `video_set_visible { visible: bool }` is a guess... **T3 opens by reading N2's delivered
signature and writing it into this line**."

**What is true.** N2 delivered derived visibility and no visibility command at all.
`src-tauri/src/lib.rs:100-105` registers five video commands (`video_open`, `video_play`,
`video_pause`, `video_seek`, `video_set_region`) and none of them sets visibility.
`src-tauri/src/video/mod.rs:30-31` says so in its own words: "Visibility is derived, never set".
`wants_shown()` at `:56-59` is `video_open && !region_empty`; `settle()` at `:64-87` is the only
caller of show and hide; `apply_region` at `:265-280` skips the move when the rectangle is empty
and then settles. So there is no signature for T3 to read, and the plan has no branch for that.

Three consequences, in ascending order of damage:

1. The two-message ordering rule is dead: `apply_region` already applies geometry before it
   settles, inside one command.
2. "Sends no region while the set is non-empty" is not merely unimplementable, it is **backwards**.
   Leaving the region alone leaves `region_empty` false, so the surface stays mapped over the open
   menu. A T3 implementer following §2.1 literally ships a layer registry that never hides
   anything.
3. The escape hatch ("if N2 landed a different shape, N2 wins") plus the instruction to write a
   signature into the frozen contract is an invitation to add `video_set_visible`, which is the
   second hide path the same bullet forbids and a public-interface change that WORKFLOW §3 makes a
   STOP condition.

**Correction.** Replace the two transport bullets with the delivered mechanism, decided in the plan
rather than in T3: hide is one `video_set_region` carrying a zero-area rectangle; show is one
`video_set_region` carrying the last measured rectangle; geometry-before-visibility lives in
`apply_region` and not in the shell. Say explicitly that the panel keeps its DOM size and the hide
is a _reported_ rectangle, not a collapsed element, otherwise an implementer copies
`e2e/specs/video-surface.spec.js:40-50`'s `style.height = 0` and the layout jumps under the dialog.
Delete the "read N2's delivered signature" sentence. N2's own spec already states this is the
intended path for decision 1 (`video-surface.spec.js:13-16`).

**Second half of the same fix.** §2.1's "the effect that reacts to that boolean is the **only**
caller of the visibility command" no longer holds: `VideoStage` is already a caller of
`video_set_region` from three paths (`src/components/VideoStage.tsx:24-33`, `:41-44`, `:52`), and
after this correction hide and measure travel on one channel. A layer effect calling
`video_set_region` independently races the ResizeObserver, so any resize while a menu is open
re-sends a real rectangle and the surface reappears. Make the layer count an input to
`VideoStage`'s `report()` instead: report `HIDDEN` when the set is non-empty, the measured
rectangle when it is empty. That preserves the one-owner property the section exists to guarantee.

### B2. §4 line 296 and every guard line after it: the baseline is 33, not 27

`e2e/wdio.conf.js:17` reads `const EXPECTED_TESTS = 33`. Counting `it(` across the eight spec files
gives exactly 33: asr 5, editor 10, project 5, subtitle 3, title 2, video 2, video-empty 3,
video-surface 3. The plan's chain starts at 27 and every guard line is six low: line 354
("27 → 29"), 420 ("29 → 30"), 491 ("30 → 33"), 564, 617, 691, 901, and the §4 running total at 296.
The first three are at or below the value already on main, so a T1 implementer following its own
line writes 29 into the config and **silently disarms the guard for the six checks N2 added**. The
same 27 appears in §1 line 110, §0 line 47, T5b line 767 ("the 44"), T1b line 418 ("[existing: all
29]"), §8, and in BACKLOG.md's M2.0 acceptance criteria.

**Correction.** Rebase from 33: T1 35, T1b 36, T2 39, T3 43, T4 47, T5a 50, T5b 50, T6 50, T7 54,
T8 55. Correct the header's "all six specs" to eight. Restate §6's "`EXPECTED_TESTS` moves with the
checks" as **never decreases**, so this class of error fails loudly next time. Correct BACKLOG's
M2.0 AC in the same edit so the two documents do not carry two definitions of "the existing checks".

### B3. §0 fact 3 and §2.4: the close gate no longer opens its file through the shell

§0 lines 58–66 quote three constants verbatim as "the file says so itself". `fee26f8` deleted two of
them. `e2e/scripts/close-gate-check.js:41-43` now carries one point, `FIRST_CUE_TEXT = {750, 540}`,
under a comment in the singular. More than that, the gate no longer opens through the UI at all:
`:141-147` spawns the binary with the fixture in argv under the comment "The subtitle is passed as
an argument, never typed: see WORKFLOW.md 4c and `startup_files`".

What breaks:

- §2.4's premise that "four tasks re-derive the same two numbers" is one number, and the "both
  points are re-derived" criteria in T2 (line 483), T5a (line 678) and T5b (line 745) are vacuous.
- `TOOLBAR_OPEN_SUBTITLE`, frozen in §2.4 line 203 and promised by T5b, has nothing left to click.
  Asking T5b to give the gate a toolbar open route is not a re-point: it is **new behaviour in the
  most data-safety-sensitive script in the battery**.
- T5b lines 771–775 promise that "the 1,500 ms fixed wait after opening the file becomes a wait on
  the chooser toplevel disappearing". That wait was deleted with the typed open, and no chooser is
  involved. The waits today are `sleep(3500)` at :180 (webview paint plus the argv parse),
  `sleep(600)` at :186 and `sleep(2500)` at :191.

**Correction.** Rewrite §2.4 around one point plus the argv route. Delete `TOOLBAR_OPEN_SUBTITLE`
unless a task genuinely needs it, and state the decision explicitly: the gate keeps opening from
argv, which is cheapest and keeps it independent of the chooser. Rewrite T5b's N1-debt paragraph
against the waits that exist: the 3,500 ms setup wait is the one worth attacking, and the plan
should say whether a script with no DOM has anything to wait on instead.

### B4. Six merged checks live in two spec files no task owns

`e2e/specs/video-surface.spec.js` (3 checks) and `e2e/specs/video-empty.spec.js` (3 checks) were
added by N2 and appear in no "Files owned" list and nowhere else in the plan. Both reach the app
through the controls T5b deletes: `video-empty.spec.js:60,99,102,110` and
`video-surface.spec.js:116,122,130` use `.bar__input` and `.bar__button`, which §3 lists under
"Gone, gesture replaced". `video-surface.spec.js:40-50` drives hide and show by setting
`.stage__surface`'s height to 0, which is the same lever T3 rewires and the same element T5a
re-parents.

Under the plan's own §6 rule at line 1024 ("A task that finds the gate red and the file outside its
ownership stops with a BLOCKED report; that situation is a defect in this plan"), **T3 and T5b block
by construction**. These are also the only behavioural coverage the native surface has.

**Correction.** Add both specs to the Files owned of T3, T5a and T5b, with the same re-point rule as
the other four; name what replaces `.bar__input` as their readiness gate after T5b; and say whether
`setStageCollapsed` survives T3, because under a registry that hides by zero-area region, collapsing
the stage and opening a layer become the same lever and must not be allowed to mask each other. Add
a standing criterion naming both files to T3's and T5a's green-battery line.

### B5. Three criteria require a pixel signal that N2 measured and refused for CI

T1b criterion 2 (line 414), T3's Escape criterion (line 534-537) and T7 criteria 1 (line 869)
require "two spread samples 1.5 s apart both read in the live-frame range and differ" inside the
mocha suite. `e2e/specs/video-surface.spec.js:9-12` records the measurement that refused exactly
this: "under Xvfb with llvmpipe the frame is presented unreliably, measured at 2 appearances in 10
with mpv attached every time, which made this suite intermittent for a reason unrelated to the
code." Three further problems compound it:

- The instrument's name and unit are wrong. §2.5 line 226 says "the probe's spread measure" and
  "the probe script was never committed". `e2e/lib/pixels.js` is committed, `e2e/wdio.conf.js:8,34`
  already calls `requireFfmpeg()` on every run, and the delivered measure is ffmpeg's average
  saturation (`SATAVG`, `pixels.js:18,36`). The only recorded numbers on that scale are 5.86 live
  against 2.1 empty, taken on real hardware; `docs/reports/n2-probe.md`'s 0.3833/0.3850 pair is a
  different unit entirely.
- "Differ" carries no tolerance. The only recorded live pair differs by 0.4%, measured once.
- T1b's criterion requires `saturation()` to "return its refusal, not a number, when mpv's child
  window is absent". `pixels.js:36-87` has no such precondition; it lives in the callers.

**Correction.** Re-express the "the picture is alive" criteria the way N2 settled it: map state plus
the presence of mpv's child window inside the surface, in CI. Move the pixel measurement to the
real-session check and the owner checklist, with the platform written on it per CLAUDE.md §9. Change
§2.5's row to say the instrument exists, drop `(new)` from T1b's file list, name `SATAVG` instead of
"spread", and state whether T1b adds the mpv-child precondition into `pixels.js` or leaves it in the
callers. If the plan still wants a pixel assertion inside CI, that is a new investigation, not a
criterion to hand an implementer.

### B6. T2 line 442 declares `src-tauri/src/dialog.rs` new; it exists, and it is gate 2's declared lens

`src-tauri/src/dialog.rs` is 156 lines, added by `fee26f8`, holding the close gate's three GTK
message dialogs on the main thread. Its own module doc (`:1-11`) explains that Linux moved _off_
rfd because "the plugin uses rfd, which starts a second thread the first time any dialog is shown
and iterates GTK on it for the rest of the process's life, which GTK3 is not built for". T2 as
written creates that file and drops an rfd-driven file chooser into it. `WORKFLOW.md:55` names "the
close path and the single-use `CLOSING` flag" as gate 2's one pre-declared review lens; CLAUDE.md §3
puts it under data safety.

Compounding it, the plan has no node for **N1c**, open at `BACKLOG.md:113`, filed on 2026-08-30
against exactly the code T2 generalises: `src-tauri/src/project/mod.rs:244-257` still calls
`blocking_pick_folder` and `blocking_pick_file` through the plugin, and T2 turns that one call site
into four chooser kinds through the same plugin. Whichever of the two lands second pays for the
first, and T1's whole identification strategy assumes an rfd/GTK3 chooser toplevel.

**Correction.** Mark `dialog.rs` as existing and say what T2 adds beside `ask_close`, or give T2 its
own path. Then decide the order in §5 and put it to the owner: either N1c runs first and T2 inherits
a GTK-direct picker (in which case T1's by-title lookup is re-validated against the new picker), or
T2 builds the four choosers on GTK directly and closes N1c in the same delivery. Leaving it
undecided silently doubles one of the two tasks. T2's "zero production change" framing for T1 needs
the N1c caveat attached either way.

### B7. T4 criterion 10 and criterion 5: two things that cannot happen

"Ctrl+A marks all 2,000 rows" (line 594) is not observable in a virtualized grid: `CueList.tsx:17-19`
sets `ROW_HEIGHT = 28` and `OVERSCAN = 8`, and `:313-320` renders only `indices`, so at most about
60 rows exist in the DOM at any scroll position. No assertion can count 2,000 marked rows. The same
defect is in T7's Select-all table row.

Criterion 10 (line 607-611) then claims "the existing virtualization check still passes unchanged...
A selection that rendered the whole file would fail it." That check (`editor.spec.js:231-263`)
performs three scrolls and asserts `sample.rows <= 60`; it presses **no key at all**, so it cannot
fail because of a selection, and whether it even runs with a select-all in effect depends on the
order mocha happens to run the file's `it` blocks, which the plan does not fix.

**Correction.** Restate as something the DOM can answer: after Ctrl+A, every rendered row carries the
membership class at the top, at mid-file and at the bottom; the row count stays at or under 60 at
each of those positions; the cursor is still on its original row. Put the row-count assertion inside
the new check rather than leaning on one that never selects anything.

---

## 3. Serious findings

### S1. §5 line 986: "N2 is a hard predecessor of T3, and `BACKLOG.md` marks it `[ ]`: not started"

N2 merged as `d224f3c` and `BACKLOG.md:84` marks it `[x]`, verified-by-tests, with its gate-1
status. No task sits behind it. The fallback order at lines 989–991 ("if N2 has not landed when T2
finishes, run T4 before T3") is dead text that an implementer session will read as live scheduling
advice. What actually stands between today and T1 is **N2c and gate 2**, and neither appears
anywhere in the plan.

**Correction.** Redraw the head of the graph as `N2c → gate 2 → T1`. Delete the fallback-order
bullet. Keep T3-before-T4 on the `App.tsx` composition-root argument the section already gives. Note
that nothing becomes startable earlier: T3 is still behind T1, T1b and T2. What changed is that T3's
risk moved from schedule to contract, which is B1.

### S2. T1 and T1b say "Depends on. Nothing"; both are behind gate 2

The owner's 2026-08-30 ruling is N2c, then gate 2, then M2.0 starting at T1 (`BACKLOG.md:74`), and
`WORKFLOW.md:59` says a gate freezes merges of new code while planning and documentation keep
running. T1 and T1b are new harness code. State the gate as their predecessor in §5 and in both task
headers: written during the freeze if useful, merged when the gate opens.

### S3. N2c has no node, and it moves the unit the geometry criteria compare in

N2c is in the working tree now, modifying `src/components/VideoStage.tsx`, `src/types/video.ts` and
`src-tauri/src/video/**`. The diff resolves the rectangle to native pixels inside `report()`,
multiplying `getBoundingClientRect()` by `window.devicePixelRatio` before invoking. T3 and T5a both
own `VideoStage.tsx`; `src/types/video.ts`, whose header says the region contract is a public
interface, is owned by no task.

The criteria that compare the surface's X11 geometry with `.stage__surface`'s DOM rect "within the
existing 2 px tolerance" (T3 line 536, T5a line 676) are true only where the ratio is 1. Under Xvfb
that is always the case, so the Linux suite **cannot tell the correct wording from the wrong one**;
on the owner's 1.5-scaled display, after N2c, the comparison is the rect times the ratio and the
plan's wording is false. The citation `VideoStage.tsx:41-43` in §2.1 line 133 and T5a's overflow
paragraph moves by nine lines under the same diff.

**Correction.** Add N2c as a predecessor of T3 and T5a; add `src/types/video.ts` to T3's owned files
or freeze it explicitly; after N2c merges, state the region's unit once in §2.1 and once in §2.4 so
four tasks do not split between CSS and native pixels; re-word both geometry criteria and add a line
saying the Xvfb run cannot discriminate.

### S4. `STATUS_PREFIX` is named for the three-cue fixture in three criteria

T2 line 468, T5b line 727 and T7's table row at line 892 all say opening `basic-lf.srt` makes the
status line read the `STATUS_PREFIX` literal "the existing check asserts". `STATUS_PREFIX` exists
only at `editor.spec.js:44` and its value is `"SRT · 2000 cues · LF"`. The check being re-pointed
lives in `subtitle.spec.js`, whose constant is `LF_STATUS = "SRT · 3 cues · LF"` (`:16`), asserted
with `toBe` at `:129`. An implementer following the criterion literally writes an assertion that is
false for the fixture named in the same sentence, and the cheapest way out of a red test is to
weaken it, which is what §6 and CLAUDE.md §5.4 exist to prevent.

**Correction.** Name `LF_STATUS` in the three criteria that open `basic-lf.srt`; keep
`STATUS_PREFIX` only where the 2,000-cue fixture is the subject.

### S5. T3 criterion 2 presents new production behaviour as existing

The criterion (line 528-531) says `.transcribe__start` carries the `NO_VIDEO_REASON` literal in its
accessible name and adds "This is what today's `.asrbar__start` does (asr.spec.js:144)". It is not.
`asr.spec.js:144` asserts only `propertyOf(".asrbar__start", "disabled") === true`.
`src/components/TranscribeBar.tsx:117-124` gives the button no `title` and no `aria-label`, so its
accessible name is its own label text. `NO_VIDEO_REASON` names no key in `src/i18n/en.ts` (the asr
block at `:156-176` has none). So the criterion asserts a string its own delivery invents, which
means whatever the implementer writes becomes the literal and the criterion cannot fail.

**Correction.** Either add the `en.ts` key and its exact value to §2.3 now, so the criterion names a
string that predates the implementation, or drop the accessible-name clause and keep the disabled
assertion. Correct the sentence claiming this is today's behaviour either way.

### S6. §5 line 992: the T2/T4 parallel pair is not disjoint

Two collisions the plan does not account for beyond "two shared one-liners". First, both Files-owned
lists contain `src/App.css` outright: T2 for the `Choose…` button styling (line 442-446), T4 for
splitting the row classes (line 574-576). Second, T2's list has no `src/App.tsx`, but T2 cannot be
built without it: no component in this repo calls `invoke` (every `@tauri-apps/api` import is in
`src/hooks/*.ts`), and the existing chooser reaches its button as a prop, `onChoosePath={project.choosePath}`
at `src/App.tsx:47`. Adding `Choose…` to `VideoOpenBar` and `SubtitleBar` the repo's own way means
new props threaded through `App.tsx`, which is T4's file and T4's biggest structural edit.

**Correction.** Either drop the parallel claim, or move the App.tsx wiring for the four choosers into
T2 and freeze it before T4 starts, and say which task owns which half of `App.css`.

### S7. §5 line 997: the T6/T7 pair contradicts itself three ways

§5 says T6 and T7 "both edit `Shell.tsx`", but `Shell.tsx` is absent from T6's own Files owned (line
795), and T6 cannot rename `ProjectPanel.tsx` to `Rail.tsx` without editing whatever renders it. The
prescribed freeze ("freeze `Shell.tsx`, `App.css` and `en.ts` first") freezes the one file that
carries both deliveries: T7's entire delivery is mounting a menu bar strip in `Shell.tsx` and T6's is
swapping the rail component in it. Read as "nobody edits it" the pair cannot run; read as "the
orchestrator edits it first" it is an eleventh, unwritten task. The freeze list also omits
`e2e/wdio.conf.js`, which both tasks own.

**Correction.** Add `Shell.tsx` to T6 and run the pair sequentially, or spell out the pre-task that
cuts the two slots into `Shell.tsx`, and name `wdio.conf.js` in the freeze.

### S8. §4 line 303 defines `[owner checklist]` and no criterion ever uses it

Grep over the file: the string appears at line 303, where it is defined, and at line 426 as a
fallback. On no criterion. Since the same paragraph promises that anything so tagged "is stated as
unautomated in the M2.0 status (CLAUDE.md §9)", the absence of the tag means **the M2.0 status will
present the whole shell as machine-verified when its appearance was never checked by anything**: T5a's
colour tokens and dark-first palette, the arrangement being recognisable as the one the plan
specifies rather than merely satisfying five rect inequalities (delivery at :635-637), T6's rail
matching the mockup (:791-793), and the menu bar reading as a menu bar.

**Correction.** Tag the appearance halves: at minimum one `[owner checklist]` criterion on T5a and
one on T6, and list them in the M2.0 status as unautomated.

### S9. T4 has eight criteria and none of them says the two states look different

Every T4 criterion (lines 583-611) is phrased as which class name a row carries.
`docs/design/shell-layout.md:220` gives the reason for splitting them: "the cursor is drawn as an
outline on the row and membership as the filled row, and the two are visibly different states rather
than one style used twice". An implementation that gives `.cuelist__row--active` and
`.cuelist__row--selected` identical CSS satisfies all eight criteria while leaving the user unable to
tell the cursor from the selection, which is the whole of decision 5 from the user's side. Today one
class does both jobs (`CueList.tsx:318-321`).

**Correction.** Add one criterion: with rows 10–14 marked and the cursor on 14, the computed style of
row 14 differs from that of rows 10–13 in at least one named property (outline/border versus
background), and neither equals an unmarked row's.

### S10. T7 criterion 2 can pass with zero samples

"Sample the surface's map state every 50 ms from the click until the dialog is on screen: every
sample reads `IsUnMapped`" (line 871-874). If the dialog reaches the screen in under 50 ms the loop
takes one sample or none, and "every sample read `IsUnMapped`" is vacuously true. §9's honesty note
covers the residual gap between sampling and proof, not this. It is the same defect gate 1 found
inside N2, recorded at `BACKLOG.md:88`: "the 'clock is frozen' assertion could not fail because
`waitFor` returns on its first evaluation".

**Correction.** State a minimum sample count, assert the count as well as the values, and fail as
"did not sample" rather than passing when the transition was too fast to observe.

### S11. T8's cold-start threshold is never decided, and WORKFLOW §2.5 bans inventing one

T8 criterion 1 (line 960) says the script "exits non-zero if either is over budget". Idle memory has
its number (400 MB, stated). Cold start does not: §7's 2 s is a release-build figure on a mid-range
2020 laptop, T8 measures a debug build under Xvfb with software rendering and says so in its honesty
clause, and no number anywhere turns CI red. WORKFLOW §2.5 forbids a threshold that was not actually
measured, so the first implementer either invents one or blocks.

**Correction.** Make the probe produce the number: T8's probe session records the measured
debug-under-Xvfb cold start, the delivery writes that measurement plus a stated headroom into the
script as the CI threshold, and the M2.0 status says the §7 verdict is still owed on a release build.

### S12. T5a criterion 3 names a control that does not exist yet, and leaves its own element set open

Line 663-669 ends "The toolbar's save-copy-as control is the one this milestone regressed on, so it
is named in the failure message." There is no toolbar at T5a: T5a's delivery parks `VideoOpenBar` and
`SubtitleBar` unchanged (:629-642) and the toolbar arrives in T5b (:699-704). Separately, "every
element carrying a visible label satisfies `scrollWidth <= clientWidth + 1`" leaves the element set to
the implementer, so the narrowest defensible choice passes trivially and the criterion's strength is
decided after the fact by the person it constrains.

**Correction.** Move the named control to T5b. Replace "every element carrying a visible label" with
an enumerated selector list per task: at T5a the parked bars' controls and the status line, at T5b
the toolbar controls by name.

### S13. Three harness files carrying shell coordinates are owned by nobody

§4 line 291 promises "a battery that is green: the mocha suite, the close gate and the shutdown
check, all three", and §2.4 line 210 names only `close-gate-check.js` and `budget-check.js` as
consumers of `shell-points.js`. Three more files hold shell-dependent gestures:

- `e2e/scripts/real-session-check.mjs:52-56` clicks the video path field and open button as fractions
  of the 1024x700 layout (`videoField: 683/1024`, `videoOpen: 978/1024`). T5b deletes both, and this
  is the one script WORKFLOW §4c blesses for checks on the owner's real display.
- `e2e/scripts/n1b-load-probe.js:33-34` carries its own copy of `FIRST_CUE_TEXT = {750, 540}` under
  the comment "M2.0 must revisit this", and it is the script N1b's closing criterion is written in.
- `e2e/scripts/wayland-attach-check.js` is a fourth runner with its own package script that the "all
  three runners" rule does not count. It happens to survive T5b because it launches from argv and
  never clicks, but nothing in the plan says so.

**Correction.** Give the two coordinate-holding scripts an owning task (T5a or T5b) and make them read
`shell-points.js`; state that the wayland check is argv-driven and unaffected.

### S14. `src/hooks/useStartupFiles.ts` appears nowhere in the plan, and four tasks rewrite around it

Grep for "startup" over `m2-0-tasks.md` returns nothing. The hook is wired at `src/App.tsx:23` as
`useStartupFiles(open, subtitle.open)` and it is the only route by which the close gate
(`close-gate-check.js:141`) and the real-session check reach a loaded document; WORKFLOW §4c makes it
the rule for the owner's display. T3, T4, T5a and T5b all rewrite the composition root without being
told it is there. If it is dropped, the close gate fails as a 3,500 ms wait landing on an empty grid,
which reads as a timing flake rather than a deleted feature.

**Correction.** Add it to §0's inventory and to T5a's and T5b's owned files, with a one-line criterion
that a file named on the command line still opens after the rewrite.

### S15. `e2e/README.md` declares the selector contract and is owned only by T1b

`e2e/README.md:144` says of the class names "Renaming one breaks the harness", and :146-155 lists
roughly twenty names that §3 renames or deletes, including `.bar__input`, `.subbar__*`, `.project__path`
and `.cuelist__row--selected`. The file appears in exactly one Files-owned list, T1b's, which runs
before every rename. The milestone would end with a selector contract document describing a shell that
no longer exists.

**Correction.** Add `e2e/README.md` to T3, T5b and T6, each updating the names it moved, or state once
in §6 that whoever renames a selector updates that inventory in the same delivery.

### S16. Gate 4 applies to T2 and T5b and no task carries it

`WORKFLOW.md:57` requires a gate before **any** merge that touches saving, subtitle formats or the
open-core boundary, "whatever the regime, whatever the schedule". T2 adds the save-mode chooser and the
save destination route; T5b deletes `SubtitleBar` and re-points save, save-as, dirty, truncated and
discard onto the toolbar and status line, including the byte-comparison save checks. §6's standing rules
cite only the per-delivery review regime the owner replaced on 2026-08-30.

**Correction.** Add the gate-4 requirement to T2 and T5b as an explicit merge precondition, and note in
§6 that the per-delivery review the plan was written under is now the §2.5 self-check plus the gate
regime of WORKFLOW §4a.

### S17. §1's scope fence never mentions N1's known data-loss gap, which decision 1 was to settle

`BACKLOG.md`'s N1 entry files it: "an inline cue editor holding uncommitted text leaves the backend
session clean, so the window closes without asking and that text is lost. Covering it needs the gate to
consult the frontend, which is the HTML-dialog shape decision 1 will settle." Decision 1 is now T3. The
plan's scope fence (lines 98-103) discusses File > Close at length and does not mention the gap, so it is
neither in scope nor visibly still deferred. A data-loss path parked against a milestone that has arrived,
unmentioned in that milestone's plan, is exactly the silence CLAUDE.md §3 and §9 exist to prevent.

**Correction.** One line in §1 or T3: either the gap closes in M2.0 with its own criterion, or it stays
deferred with the named task that owns it.

### S18. T1b criterion 1's second clause has no instrument

"The two-toplevel guard still throws when a second instance is present" (line 412). Nothing is named that
produces a second toplevel, the harness owns exactly one app instance under the driver, and the clause has
no separate check counted against `EXPECTED_TESTS`. The cheapest way to satisfy it is to test the parser
rather than the situation, and nothing would catch its quiet removal.

**Correction.** Either state the route (spawn a second binary on the same display inside the check, assert
`findToplevel` throws, kill it, assert recovery) with its own guard count, or drop the clause and keep the
resize round trip, which is the instrument this task actually owes.

### S19. §0 fact 2 claims a precedent for typing into a native dialog; there is none

Fact 2 says the chooser is "a real X11 toplevel with a title we set, which `xdotool` can focus and type into
exactly the way `close-gate-check.js` already answers the N1 dialog". The gate answers that dialog by
focusing it and clicking estimated button coordinates (`close-gate-check.js:112-128`) or by pressing Escape
(`:132-137`). It never types into a native dialog, and nothing in this repo types into a `GtkFileChooser`
location bar. T1's "if it does not work, stop" clause is what keeps this honest, so this is a wrong premise
rather than an unmeetable criterion, but the criteria are written as if the gesture were proven.

**Correction.** Reword fact 2 to say what exists (focus, click, Escape on a native toplevel) and state that
entering text into a chooser is the unproven part T1 exists to answer.

---

## 4. Minor findings

- **T8 criterion 3** (line 964): "Whichever cold-start shape the probe settles on is the one implemented" is
  not falsifiable; all three listed outcomes satisfy it, including the one where the paint half is never
  measured. Keep it as a process note and let S11's measured threshold carry pass/fail.
- **T5b criterion 1 and T6 criterion 1** (:719-722, :806-808) count only `.shell input, .shell textarea`,
  while the BACKLOG AC is "no field for typing a path is left anywhere in the interface". Widen to
  `input, textarea, [contenteditable], [role="textbox"]`.
- **T3's hit-test clause** (:534-537) cannot fail for the occlusion T3 exists to prevent: `elementFromPoint`
  cannot see the native surface, which is an X11 child window and not a DOM node. Keep it, but say in §2.5
  that the hit test answers HTML occlusion only and the native surface is answered by map state.
- **§2.1 line 126** says "two specs poll that placeholder as their readiness gate". Three specs reference
  `.stage__empty` (video, video-empty, asr), and the readiness gate all of them actually poll is
  `.bar__input`, which T5b deletes. The conclusion (keep the placeholder mounted) survives; the justification
  does not.
- **§2.3 line 182** puts `.transcribe__backend` inside the dialog. `TranscribeBar.tsx:135-139` renders
  `.asrbar__backend` inside the status paragraph, after the run and only when a result exists: it is an
  output, not an input, and hiding it in a closed dialog contradicts §2.3's own seam.
- **§3's table** (:253-265) claims to be "the whole re-point job", and omits `.asrbar__phase`,
  `.asrbar__gpu-label` and `.asrbar__cues` (`TranscribeBar.tsx:107,136,160`), which T3 must necessarily
  touch. No assertion depends on them, so nothing breaks, but the sentence makes their absence a review trap.
- **§0 fact 1** ("Delete the boxes and the suite cannot reach anything") is no longer true: `startup_files`
  (`src-tauri/src/lib.rs:38-75`) gives any harness process a route to a loaded document, and the close gate
  already takes it. T1 stays first because the dialog is the route the product will have and the milestone's
  first AC asserts it, not because nothing else can reach a file.
- **T8's fixture loading** (line 951) drives the chooser with the T1 helper, which puts chooser teardown and
  input latency inside numbers that exist to answer CLAUDE.md §7. Load both fixtures from argv instead and
  keep `shell-points.js` for whatever still needs a click.
- **T5b's "Open budget, re-pointed not weakened"** paragraph (:767+) appears in none of T5b's nine criteria,
  so nothing in the done-list holds it; and its claim that the new window is "strictly wider than today's"
  depends on executing script in the page while a native chooser is up, which nothing in this repo has done.
  Promote it to a criterion and have T1's report answer the feasibility question.

---

## 5. What was checked and found sound

Coverage, so the reader is not guessing at it.

**Scope and decisions.** §1's scope fence holds against CLAUDE.md §1: nothing built is outside v1.0 scope,
nothing is scaffolded for later, and every exclusion maps onto M2.4, M2.5, M2.6 or M5. No non-goal is
touched. No licence logic, feature flag or `isPro` branch appears anywhere; the only pro-adjacent mention is
"row markers for QA — M5" inside the exclusion list. Decision 11 (milliseconds, final) is respected: no
frame, framerate or keyframe vocabulary anywhere in the file. Decision 14 is respected precisely: the layer
registry is built as the shell's single permanent owner with no bridge, interim or migration language, and
probe P6 is correctly not mentioned. Decision 5 lands where the decision says it lands, in a hook beside the
document state.

**§2.2 in full.** Every reference verified against current code: starting values match `useState(0)` at
`CueList.tsx:85`; `ROW_HEIGHT = 28` is line 17; `.cuelist__row--selected` today means the cursor and no spec
asserts on it, so the split is free as claimed; the three checks that click a row and wait for
`.cuelist__editor` are at `editor.spec.js:327, 511, 549`; the ctrl+z regression is at 543; index remapping
does live in `applyPatch` (`useSubtitleFile.ts:124`). The gesture criteria match `shell-layout.md:210-219`
row for row and each can fail on a plausible wrong implementation. Nothing in the recent merges touched the
grid.

**§2.5's other two rows.** `findToplevel` still matches title plus exact geometry (`e2e/lib/x11.js:74-88`)
with the duplicate guard at :80-86; CI's Xvfb screen is still `1280x1024` on all three E2E steps
(`ci.yml:190,193,196`); `paths.js:22-23` still hardcodes 1024x700; nothing in `e2e/` calls
`xdotool windowsize`. T1b's resize instrument is genuinely still owed, still blocking the two-size criteria,
and its BLOCKED branch for a WM-less resize is the right shape. There is still no hit-test helper and no
`e2e/lib/dom.js`.

**§3's selector map.** All 28 names in "Kept, byte for byte" exist in `src/`. All 38 on the renamed and gone
rows exist. `.cuelist__row--active` is correctly listed as new. The set of selectors quoted anywhere in
`e2e/` is a strict subset of the map, subject only to the three omissions in §4 above.

**T1's three criteria.** Each is observable and each can fail: the chooser is identified by a `WM_NAME` the
repo really sets (`strings.rs:18`), the path lands in a box that exists today, and the Escape criterion
proves the dialog was up before it proves nothing changed.

**T6's substitute routes.** `NO_SUCH_FILE` is reproducible through the chooser because attach stats the path
at attach time and maps the io error to `FileNotFound` (`crates/sublore-project/src/records.rs:263-274`;
`src/i18n/en.ts:63`); `NO_PROJECT_HERE` is a real pinned literal (`project.spec.js:17`). T6's byte-and-stat
comparisons, delete and restart criteria are honest re-points, and the rail-width criterion is a real
constraint a content-sized rail would fail. This section decides its question instead of forwarding it, and
decides it correctly.

**§2.4's replacement route.** `CUE_LIST_EMPTY` plus Enter works against the code as it stands: `.cuelist`
carries `tabIndex={0}` and the keydown handler (`CueList.tsx:304-312`), the sizer is `count * 28` px so a
click below the last row lands on the listbox, and Enter opens the editor on the active row (`:239-242`).
One ordering caveat: the behaviour the route relies on (a click below the last row changes neither state) is
contracted in §2.2 and built by **T4**, and T3 does not depend on T4. Either add the dependency or let T5a
make the switch after T4 lands.

**§1's File > Close reasoning.** `subtitle_close(state, discard: bool)` is at `src-tauri/src/subtitle/mod.rs:133-139`,
its single frontend caller passes `discard: true` at `useSubtitleFile.ts:184`, and the gate watches
`CloseRequested`, which that route never reaches. The exclusion is correctly argued (separately from S17).

**Line citations into production code.** Spot-checked and holding: `project/mod.rs:145,244`,
`subtitle/mod.rs:133`, `useSubtitleFile.ts:184`, `asr.spec.js:144`, `project.spec.js:162,207,247-249`,
`editor.spec.js:27,44,198,213,327,511,543,549`, `close-gate-check.js:39` (`EXPECTED_CHECKS = 12`),
`shutdown-check.js:32` (5). `Cargo.lock` has rfd 0.16.0 with `gtk-sys` and no `ashpd`, so §0 fact 2's
GTK3-chooser premise stands. Every new-file declaration except `dialog.rs` and `pixels.js` is still correct:
`dialog.js`, `dom.js`, `shell-points.js`, `budget-check.js`, `useLayers.ts`, `useSelection.ts`, `Shell.tsx`,
`Toolbar.tsx`, `MenuBar.tsx`, `EditBox.tsx`, `VideoPanel.tsx`, `TranscribeDialog.tsx` do not exist.

**Ordering that survives.** T4's dependency line, the sequential chain through the composition root, T6 and
T7 behind T5b, T8 behind T5b/T6/T7, and T8's ownership set (which collides with nothing). T5a's file list is
correctly disjoint from `VideoStage.tsx`, so its re-parenting does not collide with N2c.

**§7's open items.** "Video and waveform panels sit side by side" is genuinely still open; no ruling since
has touched it, and the recommendation to close that half of the AC at M2.4 is consistent with the
no-provider-no-panel rule the layout doc adopts. The remap-branch item is also still accurate: `package.json`
has no TypeScript test runner. Nothing in §7 should be struck.

**T8's idle-memory criterion and honesty clause.** The settle condition is stated as a rule that can fail,
whisper memory is excluded by construction rather than assumption, and the debug-build-under-Xvfb caveat is
exactly what CLAUDE.md §9 asks for.

---

## 6. What no lens could check

- **Nothing was executed.** No suite was run, no binary built, no measurement taken. Every statement above is
  read from source. Where the plan's own risk is "does this gesture work at all" (typing into a
  `GtkFileChooser`, `xdotool windowsize` under a WM-less Xvfb, executing script in the page while a native
  chooser is up), the answer is owed by T1 and T1b's probe reports and cannot be given here.
- **N2c is not merged.** Its effect on the geometry criteria (S3) is stated from the working-tree diff, which
  can still change before it lands. Treat S3 as a question until N2c is on main, then re-derive §2.1's unit
  and the two rect-versus-X11 criteria against what actually shipped.
- **Whether the plan's own claim to have applied 12 blocking and 23 serious findings is true** was not
  audited. The two critique reports it names exist; the correspondence between their findings and the
  document's current text was not re-walked, and gate 1's status already records this debt.
- **Appearance.** No lens can say whether the shell reads as the arrangement the plan specifies, whether the
  dark palette is right, or whether the rail matches the mockup. That is S8's point and it stays a person's job.
- **Windows.** Nothing in this report is a Windows claim. The E2E harness drives X11 only, and the milestone's
  behavioural verdicts will be Linux verdicts until the Windows activation milestone lands.
