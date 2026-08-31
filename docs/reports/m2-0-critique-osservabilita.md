# M2.0 critique — observability lens

Adversarial read of `docs/design/m2-0-tasks.md` against CLAUDE.md §5.1 ("acceptance criteria
written first, in plain language, as observable behavior"), §5.4 (never fake a pass) and §9
(unverified work is presented as unverified). Read before writing, in this order: CLAUDE.md §1,
§3, §5, §6, §7, §9; `docs/design/shell-layout.md`; `docs/design/decisions.md`; `BACKLOG.md` M2.0
(read only); `docs/reports/n2-probe.md`; `docs/design/m2-0-tasks.md` in full; and the code the
plan makes claims about — `src/App.tsx`, `src/components/*.tsx`, `src/hooks/*.ts`,
`src/i18n/en.ts`, `src-tauri/src/project/mod.rs`, all six specs in `e2e/specs/`, `e2e/lib/x11.js`,
`e2e/lib/input.js`, `e2e/lib/paths.js`, `e2e/scripts/close-gate-check.js`, `e2e/wdio.conf.js`,
`package.json`, `.github/workflows/ci.yml`, `src-tauri/tauri.conf.json`.

**What this lens covers.** Whether each acceptance criterion in §4 of the task breakdown states a
behaviour someone can observe, and whether there is an instrument in this repo capable of
observing it. Nothing else: the layering design, the task ordering, the CSS, the i18n split and
the dependency graph were read for context and are not judged here.

**Platform.** Every claim below about the harness is about the Linux/X11 suite as it stands on
`main` at `8551bcf`. No code was run; the findings are read from source and are stated as such.

Nothing was modified. Seven blocking, seventeen serious, six minor.

---

## The shape of the problem, in one paragraph

The plan is unusually disciplined about _frozen assertions_ and unusually loose about _new_
criteria. §6 forbids weakening an existing check and §3 pins every selector, but the 49 acceptance
criteria in §4 are backed by 17 new checks, and the ones left over are disproportionately the
data-safety negatives ("nothing is written", "the file is untouched") and the milestone's own
headline claims (the shell arrangement, 1920x1080). Three of those criteria cannot be observed
with anything in `e2e/`, and one of them cannot be observed on the CI screen at all. That is the
thread running through the blockers below: not sloppy wording, but criteria written without
checking that an instrument exists.

---

## Blocking

### B1 — The 1920x1080 and resize criteria have no instrument, and the CI screen is too small

**Tasks.** T5a (both window sizes, resize 1024x700 → 1920x1080), T3 (resize with the dialog open),
T7 (resize with a dropdown open).

**Defect.** Four criteria across three tasks require the app window to be a size other than
1024x700, but `e2e/lib/x11.js:74` selects the app toplevel by _exact_ geometry
(`window.width === windowWidth && window.height === windowHeight`, 1024x700 from
`e2e/lib/paths.js:21`), so the moment the window is resized `findToplevel()` returns null and every
`before` hook and every `clickElement(toplevel, …)` in that file stops finding the app; and
`.github/workflows/ci.yml:190` runs Xvfb at `1280x1024`, on which a 1920x1080 window cannot exist.
No task in the plan owns `paths.js`, `x11.js`, `wdio.conf.js`'s environment or `ci.yml` except T8,
which lands last and for other reasons.

**Correction.** Add a prerequisite (call it T1b, alongside T1, no production change) that owns
`e2e/lib/x11.js`, `e2e/lib/paths.js` and `.github/workflows/ci.yml`:

- The Xvfb screen goes to at least 1920x1080 on every E2E job (`e2e`, `e2e:shutdown`,
  `e2e:close-gate`), and the comment at `ci.yml:187` is updated to say why.
- `findToplevel()` matches title plus "geometry is one of the sizes the suite drives", and a new
  `resizeWindow(id, w, h)` wraps `xdotool windowsize`; the duplicate-toplevel guard at
  `x11.js:81` is kept exactly as it is, because it is the leftover-instance guard.
- Every check that resizes restores 1024x700 before it returns, and that restore is asserted, so a
  later spec in the same session cannot inherit a window it was not written for.

Then T5a's criterion becomes observable: _"With a project, a video and the 2,000-cue fixture open,
at 1024x700: `document.documentElement.scrollWidth` equals its `clientWidth`, and every element
carrying a visible label has `scrollWidth <= clientWidth`. Resize to 1920x1080: the same two
statements hold, the video surface's X11 geometry matches the video panel's DOM rect within the
existing 2 px tolerance, and the frame is still advancing. Resize back to 1024x700: the surface
lands on the smaller rectangle."_

Until T1b exists, T5a's second criterion, T3's fourth and T7's fifth are unverifiable, and shipping
them as passed would be a §5.4 violation by omission.

### B2 — "The picture is alive" has no instrument either, and pulls an undeclared dependency

**Task.** T3 (and T7 inherits it).

**Defect.** T3's E2E section requires "the returned picture is alive by the probe's pixel-spread
measure, with the probe's precondition carried over". That measure lives in a scratchpad script
that was deliberately not committed (`docs/reports/n2-probe.md`, first paragraph). `e2e/lib/` has
`rootTree`, `allWindows`, `childWindows`, `mapState`, `findToplevel`, `findWindowsWithAppGeometry`
and nothing that captures or reads a pixel. T3's file list does not include `e2e/lib/x11.js` or any
new helper, and capturing an X region needs a binary the repo has never declared (`xwd`, `import`,
or ffmpeg) — CLAUDE.md §8 requires that to be stated in the PR before it arrives.

**Correction.** One of two, and the plan must pick:

1. T1 (already a harness-only task) also delivers `e2e/lib/pixels.js`: capture of a rectangle,
   the spread measure with the probe's exact formula, and the probe's precondition as a hard
   refusal (no measurement unless mpv's child window is present, `n2-probe.md`). The new binary is
   declared in T1's delivery with its licence and why `xwininfo` cannot do it. T3 then consumes it.
2. T3's criterion drops to what `mapState` can see — `IsUnMapped` while the layer is open,
   `IsViewable` over the panel rectangle afterwards, mpv's child window present — and the plan
   says in writing that "the frame is alive, not frozen" is **not** covered at M2.0 and names the
   task that brings it.

Option 2 is honest and cheap; what is not acceptable is the current state, where the criterion
reads as covered and no instrument exists.

### B3 — T5a breaks `close-gate-check.js` and does not own it

**Task.** T5a (T5b carries the repair).

**Defect.** `e2e/scripts/close-gate-check.js` drives the app by absolute coordinates with no DOM:
`SUBTITLE_PATH_FIELD = {x: 506, y: 73}`, `SUBTITLE_OPEN_BUTTON = {x: 676, y: 73}`,
`FIRST_CUE_TEXT = {x: 750, y: 540}` (lines 43-45). T5a re-parents everything into the new
frame — grid to the bottom under a top band, rail on the left — and parks the three bars in a new
strip _above_ the top band, which moves both the y of the parked bars and the position of every
grid row. Every one of those three points is stale after T5a. The plan assigns the recompute to
T5b ("`FIRST_CUE_TEXT` is recomputed for the new grid position"), one task too late.
`pnpm e2e:close-gate` runs in CI (`ci.yml:196`), so T5a merges red, and §5 of the plan's own rule
— "every merged task leaves an app the owner can open and use" — is broken at the one task that
moves everything.

**Correction.** `e2e/scripts/close-gate-check.js` joins T5a's owned files. T5a gains a criterion:
_"`pnpm e2e:close-gate` passes with its twelve checks and its assertions unchanged; only the three
coordinate constants move, and each one is re-derived from the new layout at 1024x700."_ T5b then
keeps only the part that is genuinely its own: replacing the field-and-button pair with the
toolbar control plus the T1 helper.

### B4 — T5b deletes the subject of a frozen regression check

**Task.** T5b.

**Defect.** `editor.spec.js:543`, _"leaves ctrl+z to the destination box and undoes exactly one step
from the toolbar"_, is the regression fixture for a real M2.3 defect (global Ctrl+Z stealing native
undo from text inputs). It works by `typeInto(toplevel, ".subbar__dest", …)` then `ctrl+z`, and
proves the keystroke did **not** reach the document. §3 of the plan lists `.subbar__dest` under
"Gone, gesture replaced". After T5b there is no text field in the workspace except the inline cue
editor, so this check cannot survive a selector re-point: its subject stops existing. T5b's
criterion "Ctrl+Z inside a text field still belongs to the field" names no field, and the plan's
standing rule ("assertions are frozen … any test weakened must be named in the delivery
description") is about to be broken silently by the one task that says it will not be.

**Correction.** Name the re-point in the plan, now, and name it as a change of subject rather than
of selector: _"The check re-points onto the inline cue editor. Open the editor on row 300, select
all, type the replacement, then press Ctrl+Z **without committing**: the editor's own text reverts
and the document does not undo — row 100's earlier edit is still there and row 300 still holds the
text it was opened with. Then commit, click toolbar undo, and the assertions that follow are the
literals they are today."_ Add to T5b's delivery description: this check changed subject because
its subject was removed, per §6.

### B5 — T4's shift-extension criterion is arithmetically wrong against the plan's own table

**Task.** T4.

**Defect.** _"From the keyboard alone: Down four times, then Shift+Down three times. Seven
consecutive rows are marked as selected."_ Under the gesture table in `shell-layout.md` (plain
arrow: active moves, selection collapses to `{active}`, **anchor set to active**; shift-arrow:
active moves, selection replaced by anchor→active, anchor unchanged) and with `selected`
initialised to `0` today (`CueList.tsx:85`), the sequence produces: Down x4 → active row 5, anchor
row 5; Shift+Down x3 → active row 8, selection rows 5-8. **Four rows, not seven.** The criterion
also never states which row is the cursor when the file opens, so the count cannot be derived
without reading the source. An implementer will meet a failing check and the cheapest way out will
look like adjusting the expected number, which is exactly the §5.4 failure the plan is written to
prevent.

**Correction.** _"Open the 2,000-cue fixture. Row 1 carries the cursor and is the only row marked
selected. Press Down four times: row 5 carries the cursor and is the only row marked. Press
Shift+Down three times: rows 5, 6, 7 and 8 are marked selected, row 8 and only row 8 carries the
cursor, and rows 4 and 9 are not marked."_ If the owner wanted seven rows the gesture is Down four
times then Shift+Down six times; either is fine, but the number and the gesture have to agree.

### B6 — T3 contradicts itself on the no-video case

**Task.** T3.

**Defect.** First criterion: _"A `Transcribe…` control is visible and disabled with no video
open."_ Fifth criterion: _"With no video open, `Transcribe…` cannot be started, and the dialog says
why."_ A disabled control opens no dialog, so there is no dialog to say anything. The implementer
has to guess: disabled control with a tooltip, or enabled control opening a dialog whose start
button is disabled with a reason line. Two different shells, and the fifth criterion is not
observable under the first.

**Correction.** Pick one and delete the other. Recommended, because it keeps the reason visible
where the user looks: _"With no video open, the `Transcribe…` control is present and disabled, and
its accessible name carries the reason (`en.ts`). Clicking it opens nothing. Open a video: the
control enables."_ Then the reason string is observable through the accessibility name, which is
readable from the DOM without a tooltip-hover dance.

### B7 — T2's last criterion is about source code, not behaviour

**Task.** T2.

**Defect.** _"Each chooser's title says what is being chosen, and the titles come from `en.ts` and
the Rust `strings.rs`, never from an inline literal."_ The second half is a code-review rule
wearing an acceptance criterion's clothes: no observation of the running app can distinguish a
title read from `strings.rs` from the same title inlined. CLAUDE.md §5.1 rules it out, and the rule
already exists in the right place — §6's standing rules, first bullet.

**Correction.** _"The four choosers carry these exact titles: `<project folder title>`,
`<media file title>`, `<subtitle file title>`, `<subtitle save title>`, read from the chooser
toplevel's `WM_NAME` by the T1 helper. No two are the same string."_ Delete the provenance clause;
it is already §6's job, and §6 already calls a JSX literal a review rejection.

---

## Serious

### S1 — T7's menu criterion is an unbounded equivalence over an unspecified keyboard model

_"Every menu item does exactly what its toolbar twin does, and both are reachable from the
keyboard."_ Nobody can run that. "Exactly what its twin does" spans nine items with no statement of
what each does, and "reachable from the keyboard" names no gesture — `shell-layout.md` says
"everything reachable from menu and keyboard" and never defines a key model for the menu bar (no
Alt-mnemonics, no arrow navigation, no Escape rule). This is design work the plan was written to
finish, and it is unfinished.

**Correction.** Replace with a table, one row per item, each row an observation: _"File → Open
video opens the same chooser as the toolbar's open-video control (same title), and picking
`fixtures/video/sample.mkv` plays it. Edit → Undo reverts the last committed cue edit, same as the
toolbar's undo. …"_ And add a fourth question to §7 for the owner: **what is the menu keyboard
model?** Recommendation to put in front of him: Alt opens the first title, Left/Right move between
titles, Up/Down move within a dropdown, Enter activates, Escape closes and returns focus where it
was — and one E2E check drives File → Open subtitle by keyboard alone.

### S2 — "The video does not flash back on screen in between" is a sampled negative sold as a fact

T7's second criterion asserts something did not happen inside a window of a few milliseconds. The
only instrument is `mapState`, polled; a poll can always miss a flash. The design guarantee behind
it is real (the intermediate never reaches the backend, `shell-layout.md`), but it is a claim about
internal state, and its external shadow is weaker than the sentence suggests.

**Correction.** _"With a video playing, open the File dropdown, then activate Transcribe. Sample
the surface's map state every 50 ms from the click until the dialog is on screen: every sample
reads `IsUnMapped`. Close the dialog: the surface is `IsViewable` over the panel rectangle."_ And
add to the M2.0 status, per §9: _this is a sampled negative at 50 ms, not a proof that no frame was
mapped._ Same treatment for T7's third criterion, which has the same shape.

### S3 — "Readable, not covered by the video" is not a measurement

T3's second criterion and T7's first both use it. Nothing in the suite reads text off the screen,
and after B2 that stays true.

**Correction.** _"While the dialog is open the surface is `IsUnMapped`, and for each of the
dialog's controls, `document.elementFromPoint` at the centre of its DOM rect returns that control
or a descendant of it."_ Hit-testing is the observable form of "not covered by something we
painted"; the map state is the observable form of "not covered by the video".

### S4 — The milestone's headline arrangement criterion has no automated check

T5a's first criterion — video left of the top band, current-line band to its right, grid full width
underneath, rail on the left of all of it — is the reason M2.0 exists, and T5a adds two checks,
neither of which is about the arrangement. As written it is eyeball-only: true of the owner, false
of CI, and it is precisely the "the layout is correct" class.

**Correction.** Add rect assertions, cheap and exact: _"With everything open, read the DOM rects of
the rail, the video panel, the top-right column, the grid and the status line. `rail.right <=
video.left`; `video.right <= side.left`; `grid.top >= video.bottom` and `grid.top >= side.bottom`;
`grid.left` equals `video.left` and `grid.right` equals `side.right`, within 1 px; `status.top >=
grid.bottom`. None of the five is zero-sized."_ That is the arrangement, stated so it can fail.

### S5 — The plan's own regression fixture is deleted one task after it is used

T5a's clipping criterion names the `Save copy to` label as "the regression fixture for this", which
is what BACKLOG.md says too. T5b deletes `SubtitleBar.tsx`, and the label with it. From T5b on, the
1024x700 clipping criterion points at nothing.

**Correction.** T5a's criterion keeps the label (it still exists there) but adds the durable rule,
and T5b's criteria inherit the rule alone: _"At 1024x700, every control carrying a visible label
satisfies `scrollWidth <= clientWidth + 1`, and every control's rect lies inside the viewport. The
toolbar's save-copy-as control is the one this milestone regressed on, so it is named in the
failure message."_ The rule survives the deletion; the fixture does not have to.

### S6 — "No box anywhere to type a path into" is not falsifiable as worded

T5b's first criterion and T6's first. A person can look; a check cannot, because "box for typing a
path" is not a DOM property, and after T5b there is still one text input in the workspace — the
inline cue editor.

**Correction.** _"With no cue editor open, the workspace contains no `input` or `textarea` at all:
`document.querySelectorAll('.shell input, .shell textarea')` is empty. Opening the editor on a row
adds exactly one, and closing it removes it again."_ For T6 the same sentence with the rail
included, which is the whole point of T6's version.

### S7 — Unbounded negatives and vibes in the safety criteria

Three of them, and they are the criteria that guard CLAUDE.md §3:

- T1: _"Nothing is opened, nothing is written."_ Where? Over what scope? An unbounded negative
  cannot be checked and will be reported as passed on the strength of nobody having looked.
- T2: _"the box is unchanged and nothing is written."_ Same.
- T6: _"the rail says so, stays usable, and no database file is created there."_ "Stays usable" is
  a vibe; the last clause is good and should be the model for the rest.

**Correction.** Bound each one to a named observation: _"After dismissing the chooser: the box
holds the same string it held before, the scratch folder's directory listing is byte-for-byte the
listing taken before the click, the project status line is unchanged, and no new file exists under
the project folder."_ And for T6: _"the rail lists no project, the message names the folder, and
clicking new-project still opens a chooser"_ in place of "stays usable".

### S8 — T2 asks for an observation T1's helper does not deliver

T2's third criterion requires the save-mode chooser to _"already propose a file name based on the
open file"_. Reading a proposed name means reading the GTK save dialog's name entry. T1's delivery
is "finds the chooser by title, enters a path and confirms or cancels" — no readback.

**Correction.** T1's delivery grows one function: read the chooser's current name-entry text and
its title, and T1's report states whether both are readable through `xdotool`/AT-SPI on this GTK
build. If the name entry cannot be read, T2's criterion changes to the observable consequence
instead: _"confirm the chooser without typing a name at all; the file that appears is named
`<episode>.<ext>`"_, which tests the proposal through its effect.

### S9 — T1's report demands an observation about code that does not exist yet

T1 is "no production code changes at all", and its report must state _"whether the save-mode chooser
behaves the same as the open-mode one"_. There is no save-mode chooser until T2 adds one
(`project/mod.rs:244` has exactly two kinds, `folder` and `file`, both open-mode). T1 cannot answer
its own report question without breaking its own constraint.

**Correction.** Move that question to T2's delivery, where the save chooser is built, and make it
T2's own stop condition: _"if the save-mode chooser cannot be driven by the T1 helper, T2 stops and
reports BLOCKED before any path box is removed"_ — the same gate T1 has, at the task that can
actually answer it.

### S10 — Moving every `before` hook to `.shell` weakens a precondition

T5b: _"Every `before` hook waits on `.shell` instead of a path box."_ Today each hook waits on the
control that spec drives: `.bar__input` (`video.spec.js:44`), `.subbar__input`
(`subtitle.spec.js:119`), `.asrbar__model` (`asr.spec.js:117`). `.shell` is the root element,
present "from first paint" by the plan's own §3 — so the new gate says the app rendered _something_
and no longer says the spec's subject exists. A toolbar that fails to mount would surface as a
confusing click failure several lines later instead of a clean timeout with a message.

**Correction.** _"Each `before` hook waits on `.shell` **and** on the control that file drives:
`video.spec.js` on `.toolbar__open-video`, `subtitle.spec.js` on `.toolbar__open-subtitle`,
`editor.spec.js` on `.toolbar__open-subtitle`, `asr.spec.js` on `.toolbar__transcribe`."_ Same
strength as today, and the `.shell` wait keeps the readiness idea the plan wants.

### S11 — T8's cold-start half two has no signal, and the plan says it has two

T8: _"Half two is measured inside the session in `title.spec.js`, from the page's `timeOrigin` to
`.shell` and the grid header being in the DOM … No production code is added to serve this; both
signals already exist."_ The second half is not established. `title.spec.js` runs long after
`.shell` appeared, so `performance.now()` at spec time measures the spec's own clock, not the
element's arrival; recovering "when `.shell` entered the DOM" after the fact needs either a
`MutationObserver` installed before it (impossible from a spec that starts later), an
`elementtiming` attribute (production markup serving a test, which §3 of the plan refuses), or a
`PerformancePaintTiming` entry whose availability in this WebKitGTK build nobody has checked.

**Correction.** Treat it the way T1 treats the file chooser: probe first. Either T8 opens with a
one-session probe that prints `performance.getEntriesByType('paint')` and
`getEntriesByType('navigation')` from the real app and the plan then names whichever retroactive
entry exists, or half two is redefined as first-contentful-paint (if present) and the criterion
says so with the substitution written down. If neither exists, cold start is measured externally
only — spawn to the 1024x700 toplevel being mapped — and the M2.0 status records that the
paint-to-interactive half is **not** measured. Any of the three is fine; claiming a signal that has
not been looked for is not (§9).

### S12 — "Waits for the process group to go quiet" is an undefined settle condition

T8's idle-memory measurement. Undefined settling is how a budget check becomes a coin flip, and a
flaky budget check gets its number raised rather than its cause found.

**Correction.** _"After pausing playback, sample the summed RSS across the process group every
500 ms. The measurement is the first sample whose value is within 2% of the previous two; if no
such sample arrives within 30 s the check fails as *did not settle*, and never reports the last
value it happened to see."_

### S13 — Decision 5's mouse half has no criteria

`shell-layout.md`: "Both states are drivable from the keyboard, and every mouse gesture mirrors a
key", and the gesture table names shift-click and ctrl-click. T4's eight criteria cover plain click
and the keyboard; shift-click and ctrl-click appear nowhere, so half of decision 5 would ship
unverified.

**Correction.** Add: _"Click row 10, then shift-click row 14: rows 10-14 are marked, row 14 carries
the cursor. Ctrl-click row 12: rows 10, 11, 13 and 14 stay marked, row 12 is not, and row 12
carries the cursor."_

### S14 — Nothing states the two states' initial values

§2.2 freezes the contract for `active` and `selection` but not their value when a document opens,
and no criterion names it. B5's rewrite depends on it, `.cuelist__row--active` on first paint
depends on it, and today's behaviour (`useState(0)`, `CueList.tsx:85`) is a source fact, not a
specified one.

**Correction.** Add to §2.2: _"On open, and after any patch that leaves rows, `active` is row 1 and
`selection` is `{1}`; on an empty document `active` is null and `selection` is empty."_ And a
criterion in T4: _"Open the fixture and touch nothing: row 1 carries the cursor and is the only row
marked."_

### S15 — The scroll criterion tests the panel that cannot scroll, not the ones that can

T5a checks the wheel over the video panel. The M0.2 hazard is a scroll event moving the surface,
and the regions that actually scroll after T5a are the grid and the rail — including the case
where a scroll inside the grid bubbles to an ancestor that should not have been scrollable.

**Correction.** Add: _"With a video playing and the 2,000-cue fixture open, scroll the grid to row
1500 and scroll the rail to its last episode. The surface's X11 geometry is identical before and
after, byte for byte, and the frame is still advancing."_

### S16 — Forty-nine criteria, seventeen new checks, and the gap is not where it should be

Counted from §4: T1 3/2, T2 5/3, T3 5/4, T4 8/3, T5a 6/2, T5b 8/0, T6 6/0, T7 6/3, T8 2/3.
Re-pointed existing checks cover some of the shortfall honestly (T5b's eight criteria are largely
the 27 doing their job). What is left uncovered is the wrong half: the cancel-the-chooser safety
criteria (T1, T2), the byte-and-mtime criterion for the attached file (T6 — this one is covered by
an existing project check and should say so), the arrangement (S4), both no-typed-path criteria
(S6), and every "says why / says so" string.

**Correction.** Each criterion in §4 gets a one-word tag naming its instrument: `[new check]`,
`[existing check: editor.spec.js "…"]`, `[close gate]`, `[owner checklist]`. Anything tagged
`[owner checklist]` is stated as such in the M2.0 status per §9. This costs an afternoon and makes
the coverage gap visible instead of arithmetical.

### S17 — The rail's fixed width is a constraint with no observable

T6 carries _"The rail keeps a fixed width"_ as a constraint because `close-gate-check.js` clicks
absolute coordinates. A constraint that nothing checks is a comment. When it breaks, it breaks the
close gate, and the failure will look like the gate, not like the rail.

**Correction.** A criterion in T6: _"At 1024x700 and at 1920x1080 the rail's DOM rect has the same
width, and that width is the number `close-gate-check.js`'s coordinates were derived from."_

---

## Minor

- **M1 — Unspecified copy in six criteria.** "the status line names the project" (T1), "the rail
  names the project" / "the rail says so" (T6), "the band is empty and says so" (T5a), "the dialog
  says why" (T3), "the error is readable on the status line" (T5b). The suite asserts literals
  (`STATUS_PREFIX = "SRT · 2000 cues · LF"`, `editor.spec.js:44`), so an unnamed string is a
  re-worded assertion waiting to happen. Every criterion that asserts text names the literal or
  points at the spec constant that holds it.
- **M2 — Referential criteria pointing at code being deleted.** "plays it exactly as typing the
  path did" (T2), "shows the same format and cue count line as before" (T2), "reports the same
  format, cue count and line endings as before" (T5b), "Undo and redo on the toolbar do what the
  old buttons did" (T5b). After the deletion there is nothing to compare against. Name the
  observation: the transport time advances past 0:01; the status line reads the `STATUS_PREFIX`
  literal; toolbar undo restores row 300 to the text it was opened with.
- **M3 — "Edit a row near position 1500" (T4).** Name the row, the way `editor.spec.js` names 43,
  100 and 300. "Near" is not a fixture.
- **M4 — "the same number of rows is in the DOM" (T4).** The DOM count is the mechanism, not the
  behaviour. State the behaviour and keep the mechanism as the note: _"Ctrl+A over 2,000 rows, then
  scroll to the bottom: scrolling stays inside the existing scroll budget, and the existing
  virtualization check — at most 60 rendered rows — still passes unchanged."_
- **M5 — T1's helper needs a finder it does not own.** `findToplevel` is hardcoded to the app title
  and geometry (`x11.js:74`), so `dialog.js` needs its own by-title lookup. Say so in T1's delivery,
  or give T1 `x11.js`; either is fine, silence is not.
- **M6 — T3's "still advancing, not a frozen or blank rectangle"** should carry its measure in the
  criterion rather than only in the E2E paragraph, since the criterion is what the owner reads.

---

## What this critique did not do

No code was run and no check was executed; every claim above is read from the sources listed at the
top and cites the line it came from. Whether the rewritten criteria pass once implemented is not
knowable from here. The design itself — layer registry, task ordering, the selector map, the
open-core boundary — was read for context and is out of this lens's scope; a defect there would not
have been reported unless it made a criterion unobservable.
