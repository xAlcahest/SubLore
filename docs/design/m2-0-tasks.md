# M2.0 — task breakdown

Preparation for the shell rebuild. Nothing here is implementation: this file exists so that the
first implementer session opens it, picks task 1, and has no design left to do.

Sources this decomposes, all read before writing: `CLAUDE.md` §1, §3, §5, §6, §7, §9;
`docs/design/shell-layout.md`; `docs/design/shell-mockup.html`; `docs/design/decisions.md`;
`docs/reports/n2-probe.md`; `BACKLOG.md` M2.0 (read only, not edited); `WORKFLOW.md`; the current
frontend (`src/App.tsx`, `src/components/*.tsx`, `src/hooks/*.ts`, `src/App.css`, `src/i18n/en.ts`)
with the commands in `src-tauri/src/lib.rs` and `src-tauri/src/project/mod.rs`; and the whole
harness — all six specs in `e2e/specs/`, `e2e/lib/*.js`, `e2e/scripts/close-gate-check.js`,
`e2e/wdio.conf.js`, `package.json`, `.github/workflows/ci.yml`.

Revision 2, 2026-08-29. The first draft was read by two adversarial passes,
`docs/reports/m2-0-critique-osservabilita.md` (7 blocking, 17 serious, 6 minor) and
`docs/reports/m2-0-critique-incrementalita.md` (5 blocking, 6 serious, 4 minor). Every blocking and
serious finding is applied below. Section 9 records the minors, the ones resolved differently from
what the critique proposed, and the one that cannot be closed at all.

Every claim below about the current code was checked against the code, not remembered. Nothing was
executed: this is a reading, and the numbers derived from CSS are marked as derived.

---

## 0. What the current shell actually is

Five bands stacked in one column inside `App.tsx`, plus a sidebar:

| band          | file                                   | what it holds                                                                                 |
| ------------- | -------------------------------------- | --------------------------------------------------------------------------------------------- |
| video open    | `VideoOpenBar.tsx`                     | `.bar__input` path box, `.bar__button`                                                        |
| subtitle      | `SubtitleBar.tsx`                      | `.subbar__input` path box, open, save, undo, redo, discard, `.subbar__dest` path box, save-as |
| transcription | `TranscribeBar.tsx`                    | model select, GPU tick, start, cancel, progress, cue preview                                  |
| video         | `VideoStage.tsx` + `VideoControls.tsx` | `.stage__surface`, seek slider, transport                                                     |
| grid          | `CueList.tsx`                          | virtualized cue list, inline editor                                                           |
| sidebar       | `ProjectPanel.tsx`                     | `.project__path` and `.project__file-path` path boxes, choose buttons, episodes, files        |

**Five hand-typed path boxes, not three.** `docs/design/shell-layout.md` names "the three fields
where a file path is pasted by hand", which are the three in the workspace column
(`.bar__input`, `.subbar__input`, `.subbar__dest`). The project rail has two more
(`.project__path`, `.project__file-path`), each already flanked by a working `Choose` button.
The BACKLOG AC is the wider one: "no field for typing a path is left anywhere in the interface".
So the workspace three go in **T5b** and the rail two go in **T6**.

### The three facts that shape the whole order

1. **Every E2E spec opens its file by typing a path into one of those boxes.** All 27 checks reach
   their subject that way (`typeInto(...)` then a click on the adjacent open button). Delete the
   boxes and the suite cannot reach anything, so the harness needs a way to drive the native file
   dialog _before_ the first box is removed. That is T1, and it is first for that reason.
2. **The native picker already exists and already works**: `project_choose_path`
   (`src-tauri/src/project/mod.rs:145,244`) opens a real rfd dialog from the rail's `Choose`
   buttons, with its title from `strings.rs`, and nothing in the suite exercises it. `Cargo.lock`
   has `rfd 0.16` with `gtk-sys` and no `ashpd`, so on Linux this is a GTK3 file chooser: a real
   X11 toplevel with a title we set, which `xdotool` can focus and type into exactly the way
   `close-gate-check.js` already answers the N1 dialog. T1 proves that against code that ships
   today, with zero production change.
3. **`close-gate-check.js` is the most shell-sensitive check in the battery, and it has no DOM.**
   It reaches the document through three absolute points, and the file says so itself:

   ```js
   /** Points in the current shell, relative to the toplevel origin. M2.0 must revisit these. */
   const SUBTITLE_PATH_FIELD = { x: 506, y: 73 };
   const SUBTITLE_OPEN_BUTTON = { x: 676, y: 73 };
   const FIRST_CUE_TEXT = { x: 750, y: 540 };
   ```

   Its twelve checks run in CI on every push (`ci.yml:196`) and they cover the only data-loss
   defect this project has found and closed (decision 9). Its fixture,
   `fixtures/subtitles/srt/clean/basic-lf.srt`, holds **three** cues, so at `ROW_HEIGHT = 28`
   (`CueList.tsx:17`) the clickable rows are 84 px tall inside a panel of roughly 217 px. Below
   those 84 px is empty space, where a click opens nothing.

   Any task that changes the vertical stack moves those points. Deriving from the CSS
   (`.stage` is `flex: 1 1 45%`, `.cuelist__panel` is `flex: 1 1 55%`, the ASR band is `flex: none`
   plus its status line, roughly 75 px), T3 alone lifts the grid by something like 40 px: the
   direction is certain, the magnitude is an estimate, and nobody has measured it. So **T2, T3,
   T5a and T5b each own `close-gate-check.js` and each re-derive its points**, instead of one task
   at the end inheriting four tasks' worth of drift.

   T3 also removes the height sensitivity for good, per §2.4: the point becomes a click into the
   grid's empty area followed by Enter, a target of roughly 130 px instead of 28.

---

## 1. Scope fence

M2.0 rebuilds the shell. It does not grow the product. Explicitly **not** in M2.0, and an
implementer who finds themselves writing one of these has drifted:

- The waveform panel. No audio provider exists, so per Aegisub's own rule (`SetDisplayMode`,
  quoted in the layout doc) the panel is simply absent until M2.4 brings its provider.
- The CPS column and the translation column in the grid. M2.5 and M2.6 own those.
- Editable time fields in the current-line band. M2.0 shows the active line's times; M2.5 makes
  them edit the document.
- Any menu title with no working item behind it. Menus arrive with their milestone.
- **`File > Close`.** It was listed in the first draft with no behaviour and no criterion, and the
  command it would call, `subtitle_close(state, discard: bool)`
  (`src-tauri/src/subtitle/mod.rs:133-139`), has exactly one frontend caller, which passes
  `discard: true` after the user has already chosen to discard (`useSubtitleFile.ts:184`). A menu
  item written by analogy with that caller throws unsaved work away in silence, which is the
  defect decision 9 exists to have closed. N1's gate watches `CloseRequested` on the window and
  never sees this route. `Close` arrives with the task that brings its own gate.
- Bulk operations. M2.0 owes the selection **state**; the operations belong to M2.5 and M7.
- Row markers for QA. M5.

Carried in from the ACs and kept: opening goes through the system dialog from menu and toolbar;
panels sit in Aegisub's arrangement; the transcription band is off the screen until asked for;
1024x700 and 1920x1080 are both clean; the video panel never scrolls; occlusion (decision 1);
active line separate from selection (decision 5); the 27 checks pass with assertions unchanged.

---

## 2. Contracts frozen now, so no task re-decides them

### 2.1 The layer registry

One owner, in the shell, exactly as `shell-layout.md` specifies. T3 builds it; T7 consumes it and
adds nothing to it.

- `useLayers()` exposes `openLayer(id: string)`, `closeLayer(id: string)`, and the derived count.
  State is a **set of ids**, never a counter and never a boolean.
- `surfaceVisible = videoLoaded && videoPanelMounted && layerCount === 0`. Three separate reasons
  for the rectangle to be absent; they are combined in one derived value so they cannot fight.
  `videoPanelMounted` is always true at M2.0 — the video panel keeps its `.stage__empty`
  placeholder rather than unmounting, and `shell-layout.md` says why: two specs poll that
  placeholder as their readiness gate, and a panel that vanishes makes both waits vacuous.
- The effect that reacts to that boolean is the **only** caller of the visibility command. A
  component that opens a layer registers an id and never touches the video.
- Going hidden: send visibility false. Coming back: send the last measured region **first**, then
  visibility true, in that order.
- While the set is non-empty, `VideoStage` keeps measuring on resize and stores the rectangle, and
  sends no region. Measuring stays on the ResizeObserver and the window resize listener, never on
  scroll (M0.2 constraint, `VideoStage.tsx:41-43`).
- A failed visibility command surfaces on the existing video error line and is not retried. The
  shell keeps its own state; the next transition re-asserts it.
- Backend command: **N2's**. T3 does not invent a second hide path. `video_set_visible { visible:
bool }` is a guess, not a fact: no such command exists on `main` today. **T3 opens by reading
  N2's delivered signature and writing it into this line**, and if N2 landed a different shape,
  N2 wins.

### 2.2 Active line and selection

T4 builds it, in one hook beside the document state, not inside the grid.

- `active`: one row index or null. `selection`: a sorted set of indices plus an anchor.
- **Starting values.** On open, and after any patch that leaves rows standing, `active` is row 1
  and `selection` is `{row 1}`; on an empty document `active` is null and `selection` is empty.
  This matches `useState(0)` today (`CueList.tsx:85`) and is now a contract, so a criterion can
  state what is marked at first paint without an implementer deriving it from the source.
- Gestures exactly as the table in `shell-layout.md`, plus the two rules that table now carries: a
  plain click on a row moves the cursor, collapses the selection onto it **and opens the inline
  editor**, which is today's behaviour and what three checks in `editor.spec.js` depend on
  (lines 327, 511, 549); a click inside the grid that lands on no row focuses the grid and changes
  neither state.
- The grid draws the two states differently: cursor is an outline (`.cuelist__row--active`),
  membership is the filled row (`.cuelist__row--selected`). `aria-selected` per row is membership,
  `aria-activedescendant` on the list is the cursor, and the list is `aria-multiselectable`.
  Today's `.cuelist__row--selected` means the cursor and no check asserts on it, so the split is
  free; it is written down because reusing the name for the new meaning is how the two states get
  confused again.
- Index remapping lives in `applyPatch` in `useSubtitleFile.ts`, in the same function that splices
  the rows, with the arithmetic written out in `shell-layout.md`. One place.
- After undo or redo: active moves to the first row of the patch, selection collapses onto it, the
  grid scrolls it into view.
- Editing is about active, never about selection. Tab commits, moves active down, collapses the
  selection onto it.

### 2.3 What the transcription dialog holds, and what it does not

Decided here rather than asked, because three constraints together leave one answer. Decision 1
hides the surface for any open layer; §7 of CLAUDE.md requires progress to be visible and
cancellable throughout the run; the milestone AC says transcription controls are off the screen
until asked for. A dialog that stayed up for the whole run would hold the picture off the screen
for minutes, and a dialog that closed while owning the run's output would swallow every error it
produced.

So the seam is between **inputs** and **outputs**, not between dialog and screen:

- Inside the dialog, and only reachable with it open: the model catalog, the download control, the
  GPU tick, the start button. Selectors `.transcribe__model`, `.transcribe__download`,
  `.transcribe__download-cancel`, `.transcribe__gpu`, `.transcribe__start`, `.transcribe__backend`.
- On the status line, always visible whether the dialog is open or shut: progress, cancel, the
  cue preview, and **every error a run produces**, the pre-flight checksum refusal included.
  Selectors `.status__transcribe`, `.status__transcribe-progress`, `.status__transcribe-cancel`,
  `.status__transcribe-error`, `.status__transcribe-cue` (with `-time` and `-text` leaves),
  `.status__download`, `.status__download-progress`.
- Clicking start closes the dialog, always, before the run is attempted. There is one error
  surface, on the status line, and no run outcome can land in a closed container. This is what
  makes `asr.spec.js`'s damaged-model check re-pointable without weakening it: the banner it waits
  for is on the status line, and the three assertions that follow reopen the dialog to read the
  model label, the download button and the disabled start.
- `Transcribe…` on the toolbar is **always enabled** and always opens the dialog. With no video
  open it is `.transcribe__start` inside that is disabled, and its accessible name carries the
  reason. This is what today's `.asrbar__start` does (`asr.spec.js:144`), so the first ASR check
  keeps its seven assertions and gains one step to reach them.

### 2.4 The close gate's route through the shell

`close-gate-check.js` has no WebDriver session and no DOM. Its route is frozen here so that four
tasks re-derive the same two numbers instead of inventing three each:

- `TOOLBAR_OPEN_SUBTITLE`: the centre of the toolbar's open-subtitle control, at 1024x700, with no
  project and no video open. Before T5b there is no toolbar, so until then the two existing points
  `SUBTITLE_PATH_FIELD` and `SUBTITLE_OPEN_BUTTON` stay and are re-derived.
- `CUE_LIST_EMPTY`: a point inside `.cuelist` **below the last row** — with the three-cue fixture
  that is roughly 130 px of empty space — followed by `Enter`, which opens the editor on the
  active row. §2.2 makes that click a no-op on both states, so the editor opens on row 1 whatever
  the click landed near.
- Every point is derived from `getBoundingClientRect()` in the layout the task leaves behind, at
  1024x700, **with no video open**, which is the state the close gate actually runs in. The number
  goes in the file with a one-line comment saying which task measured it.
- From T5a the points live in `e2e/lib/shell-points.js`, exported once, so `close-gate-check.js`
  and T8's `budget-check.js` read one definition and the next shell change has one place to edit.
- Every task that owns the file carries the criterion `pnpm e2e:close-gate` passes with all of its
  checks and none of its assertions changed.

### 2.5 The harness instruments, and who builds them

Three criteria in the first draft had no instrument in this repo. They are built in T1b, before
anything depends on them:

| instrument                                                   | why nothing today can do it                                                                          | home                             |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- | -------------------------------- |
| find the app at a size other than 1024x700, and resize it    | `findToplevel` matches exact geometry (`x11.js:74`); CI's Xvfb screen is 1280x1024 (`ci.yml:190`)    | T1b                              |
| read pixels over a rectangle, and the probe's spread measure | `e2e/lib/` reads the window tree and map states, never a pixel; the probe script was never committed | N2 if it delivered one, else T1b |
| "not covered by something we painted"                        | nothing reads text off the screen; the observable form is `elementFromPoint` hit-testing             | T1b helper, one function         |

---

## 3. Selector map

Assertions do not change. Where an element moves or is renamed, the selector is re-pointed and
nothing else. This table is the whole re-point job; an implementer who needs a selector not listed
here has changed something that was not asked for.

**Kept, byte for byte:** `.stage`, `.stage__surface`, `.stage__empty`, `.controls__button`,
`.controls__slider`, `.controls__time`, `.cuelist`, `.cuelist__panel`, `.cuelist__head`,
`.cuelist__row`, `.cuelist__pos`, `.cuelist__number`, `.cuelist__start`, `.cuelist__end`,
`.cuelist__text`, `.cuelist__editor`, `.cuelist__sizer`, `.cuelist__empty`, `.project__status`,
`.project__error`, `.project__episode`, `.project__file`, `.project__episodes`, `.project__files`,
`.project__new-episode`, `.project__add-episode`, `.project__role`, `.project__delete`.

Keeping `.stage__surface` is deliberate: it is what `video.spec.js` compares against X11 geometry,
and renaming it would put the one irreplaceable assertion in the suite through a needless edit.

**`.project__new-episode` is frozen for a second reason.** After T5b it is the only text input in
the shell that is not the inline cue editor, and `editor.spec.js`'s ctrl+z regression check moves
into it (see T5b). T6 may restyle the rail; it may not rename or remove that field.

**Renamed or moved:**

| today                                                                                                                                   | after                                                                                                                                              | task |
| --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| `.subbar__savefile`                                                                                                                     | `.toolbar__save`                                                                                                                                   | T5b  |
| `.subbar__undo`                                                                                                                         | `.toolbar__undo`                                                                                                                                   | T5b  |
| `.subbar__redo`                                                                                                                         | `.toolbar__redo`                                                                                                                                   | T5b  |
| `.subbar__discard`                                                                                                                      | `.status__discard`                                                                                                                                 | T5b  |
| `.subbar__status`                                                                                                                       | `.status__document`                                                                                                                                | T5b  |
| `.subbar__dirty`                                                                                                                        | `.status__dirty`                                                                                                                                   | T5b  |
| `.subbar__truncated`                                                                                                                    | `.status__truncated`                                                                                                                               | T5b  |
| `.subbar__error`                                                                                                                        | `.status__document-error`                                                                                                                          | T5b  |
| `.app__error`                                                                                                                           | `.status__video-error`                                                                                                                             | T5b  |
| `.asrbar__model`, `__download`, `__download-cancel`, `__gpu`, `__start`, `__backend`                                                    | `.transcribe__…` (same leaf), inside the dialog                                                                                                    | T3   |
| `.asrbar__status`, `__progress`, `__cancel`, `__error`, `__cue`, `__cue-time`, `__cue-text`, `__download-progress`, `__download-status` | `.status__transcribe`, `…-progress`, `…-cancel`, `…-error`, `…-cue`, `…-cue-time`, `…-cue-text`, `.status__download-progress`, `.status__download` | T3   |

The split of the ASR selectors across those two rows is §2.3's inputs-and-outputs seam, not a
cosmetic grouping: everything a run _produces_ stays on screen after the dialog closes.

**Gone, gesture replaced:** `.bar__input`, `.bar__brand`, `.bar__button` (T5b);
`.subbar__input`, `.subbar__open` (T5b); `.subbar__dest`, `.subbar__save` (T5b);
`.project__path`, `.project__choose-folder`, `.project__create`, `.project__open` (T6);
`.project__file-path`, `.project__choose-file`, `.project__attach` (T6).

**New:** `.shell` (root, present from first paint), `.menubar`, `.menubar__title`, `.menu`,
`.menu__item`, `.toolbar`, `.toolbar__open-video`, `.toolbar__open-subtitle`, `.toolbar__save`,
`.toolbar__save-as`, `.toolbar__undo`, `.toolbar__redo`, `.toolbar__transcribe`, `.rail`,
`.rail__new-project`, `.rail__open-project`, `.rail__attach-file`, `.video` (the panel that holds
the stage), `.side` (the top-right column), `.editbox`, `.editbox__start`, `.editbox__end`,
`.editbox__duration`, `.editbox__source`, `.status`, `.dialog`, `.dialog__title`, `.dialog__close`,
`.cuelist__row--active`.

Class names stay the selector vocabulary. No `data-testid` is introduced: the suite is
class-based throughout, and a test-only attribute would be production markup that exists to serve
the test, which this repo has refused elsewhere (`editor.spec.js` header).

---

## 4. The tasks

Order is the delivery order. Every task is a branch, a delivery and a merge, and **every merged
task leaves an app the owner can open and use and a battery that is green** — the mocha suite, the
close gate and the shutdown check, all three.

Running total of `EXPECTED_TESTS` (mocha specs only; the standalone node scripts carry their own
counters): 27 → T1 29 → T1b 30 → T2 33 → T3 37 → T4 41 → T5a 44 → T5b 44 → T6 44 → T7 48 →
T8 49. `close-gate-check.js` goes from 12 checks to 13 in T7. `budget-check.js` arrives in T8 with
2 of its own. `shutdown-check.js` keeps its 5 throughout and is touched by nobody.

**Every acceptance criterion below carries the instrument that observes it**, in brackets at the
end: `[new]` for a check this task writes, `[existing: file "name"]` for a check that already
exists and keeps its assertions, `[close gate]`, `[probe]` for a measurement recorded in a report,
and `[owner checklist]` for anything only a person can see. Anything tagged `[owner checklist]` is
stated as unautomated in the M2.0 status (CLAUDE.md §9).

**Every criterion that asserts a user-facing string names the literal**, either the spec constant
that already holds it (`STATUS_PREFIX`, `NO_PROJECT`, `NO_SUCH_FILE`, `IDLE_STATUS`) or a new
constant defined once at the head of the spec, whose value is the only value of its `en.ts` key.
A criterion that says "it says why" without naming what it says is a re-worded assertion waiting
to happen.

---

### T1 — The harness can drive a native file dialog

**Delivery.** A helper in `e2e/lib/dialog.js` that finds the GTK file chooser by title, enters a
path, reads back what it holds, and confirms or cancels, plus a probe report. **No production code
changes at all.**

Prove it against the picker that ships today: the rail's `Choose` buttons already call
`project_choose_path`, and nothing in the suite has ever exercised them.

**Files owned.** `e2e/lib/dialog.js` (new), `e2e/specs/project.spec.js`, `e2e/wdio.conf.js`
(count), `docs/reports/m2-0-dialog-probe.md` (new).

**Depends on.** Nothing. This is the first task in the milestone.

**Where the two new checks go, and why it matters.** They are appended **after** the five existing
ones, and each opens with `await browser.reloadSession()` then `attachToApp()` on a fresh scratch
folder, exactly as the third existing check already does (`project.spec.js:207`). Neither end of
the file is otherwise usable: the five checks share one sequential state, so creating a project
ahead of check 1 breaks `expect(await textOf(".project__status")).toBe(NO_PROJECT)`
(`project.spec.js:162`) without a line of it being edited, and after check 5 the project has been
deleted while the status line still names its folder, so "no project open" is false there too. The
second new check creates its own episode rather than reusing check 2's.

**Acceptance criteria.**

- After a fresh session on an empty scratch folder, click `Choose` beside the project folder box:
  a chooser toplevel appears whose `WM_NAME` is the `CHOOSE_PROJECT_FOLDER` literal from
  `strings.rs`. Give it the folder and confirm. The chooser is gone from the window tree, and
  `.project__path` holds exactly that path. Click `Create`: `.project__status` contains the folder
  and `project.sublore` exists in it. `[new: project.spec.js]`
- With that project open and an episode of its own added, click `Choose` beside the file box, give
  it a subtitle file, confirm: `.project__file-path` holds that path, and `Attach` lists it under
  the episode. `[new: project.spec.js]`
- Click `Choose` again and dismiss the chooser with Escape. The chooser is gone from the window
  tree; `.project__file-path` holds the same string it held before, character for character; the
  scratch folder's sorted directory listing is identical to the listing taken before the click;
  `.project__status` is unchanged; and no file under the project folder has a newer mtime than the
  one recorded before the click. `[new: project.spec.js]`

**E2E.** Two new checks in `project.spec.js`, using selectors that exist today. The five existing
project checks are untouched: not re-pointed, not re-ordered, not re-worded. Guard 27 → 29.

**Report.** `docs/reports/m2-0-dialog-probe.md` states, with the platform on it:

1. how the chooser toplevel is identified, and whether its title is readable as `WM_NAME`;
2. how a path is entered, and whether the name entry's current text can be **read back** through
   `xdotool` or AT-SPI on this GTK build — T2's proposed-name criterion depends on the answer;
3. how confirm and cancel are driven;
4. how long teardown takes, measured, because T5b's open budget will include it;
5. **whether an open-mode chooser can confirm a path that does not exist.** T6 depends on this:
   `project.spec.js:247-249` types `no-such-file.srt` into a path box and asserts `NO_SUCH_FILE`,
   and if the chooser refuses a non-existent path then that assertion has no route through the
   chooser and T6 must use the substitute route written into it.
6. `findToplevel` is hardcoded to the app's title and geometry (`x11.js:74`), so `dialog.js` needs
   its own by-title lookup. The report says which one it used.

**If it does not work, stop.** A BLOCKED report to the owner, before a single path box has been
removed. That is the reason this task is first and alone.

---

### T1b — The harness can see what the criteria describe

**Delivery.** The three instruments of §2.5. **No production code changes at all.**

**Files owned.** `e2e/lib/x11.js`, `e2e/lib/paths.js`, `e2e/lib/pixels.js` (new),
`e2e/lib/dom.js` (new, the hit-test helper), `e2e/specs/title.spec.js`,
`.github/workflows/ci.yml`, `e2e/wdio.conf.js` (count), `e2e/README.md`,
`docs/reports/m2-0-harness-probe.md` (new).

**Depends on.** Nothing. Runs immediately after T1, or beside it if two implementers are free:
their owned files are disjoint.

**What changes.**

- The Xvfb screen goes to at least 1920x1080 on all three E2E jobs (`e2e`, `e2e:shutdown`,
  `e2e:close-gate`), and the comment at `ci.yml:187` is rewritten to say why the number moved.
- `findToplevel` matches the title plus "the geometry is one of the sizes the suite drives",
  instead of one exact size. **The duplicate-toplevel guard at `x11.js:81` stays exactly as it
  is**: it is the leftover-instance guard, and widening the match makes it more necessary, not
  less. `paths.js` exports the set of driven sizes; the 1024x700 default is unchanged and remains
  the frozen contract `e2e/README.md` describes.
- `resizeWindow(id, w, h)` wraps `xdotool windowsize`. Every check that resizes restores 1024x700
  before it returns, and asserts the restore, so nothing downstream inherits a window it was not
  written for.
- `pixels.js` captures a rectangle and computes the probe's spread with the probe's formula, and
  refuses to measure unless mpv's child window is present — the precondition
  `docs/reports/n2-probe.md` earned by producing two wrong verdicts without it. If N2 already
  delivered this, T1b reuses it and says so instead of writing a second one. Whatever binary the
  capture needs (`xwd`, ImageMagick's `import`, ffmpeg) is declared in the delivery with its
  licence, its GPL compatibility and why `xwininfo` cannot do it (CLAUDE.md §8), and added to the
  CI package list.
- `dom.js` exports one function: given a selector, return whether `document.elementFromPoint` at
  the centre of its rect resolves to that element or a descendant of it.

**Acceptance criteria.**

- The app toplevel is found at 1024x700, resized to 1920x1080, found again there, resized back,
  and found again at 1024x700. The two-toplevel guard still throws when a second instance is
  present. `[new: title.spec.js]`
- Over a playing video, the spread measure returns a value in the range the probe recorded for a
  live frame, and returns its refusal — not a number — when mpv's child window is absent.
  `[probe: m2-0-harness-probe.md]`
- CI's three E2E jobs run on a screen of at least 1920x1080 and the existing suite is green on it,
  unchanged. `[existing: all 29]`

**E2E.** One new check in `title.spec.js`, the resize round trip. Guard 29 → 30.

**If `xdotool windowsize` does not really resize this app.** Under a WM-less Xvfb there is nothing
to negotiate the resize with the client, and whether GTK and the webview follow a bare
`XResizeWindow` is not established here — no code was run. If they do not, **T1b stops and reports
BLOCKED** with two options: drive the size some other way, or demote every 1920x1080 criterion in
T3, T5a and T7 to `[owner checklist]` and record in the M2.0 status that the second window size is
verified by a person and not by CI. What is not acceptable is a criterion that names a size the
harness cannot produce.

---

### T2 — One picker command for every kind of open

**Delivery.** The existing `project_choose_path` becomes a general `choose_path` command covering
four kinds: project folder, media file, subtitle file, subtitle save destination (save mode, with
a proposed file name). A `Choose…` button appears beside each of the three workspace path boxes.
Nothing is removed. Both routes work.

The buttons are not throwaway: their handlers are what T5b moves onto the toolbar and T7 onto the
menu.

**Files owned.** `src-tauri/src/dialog.rs` (new), `src-tauri/src/project/mod.rs` (the picker moves
out), `src-tauri/src/lib.rs`, `src-tauri/src/strings.rs`, `src/hooks/useProject.ts`,
`src/components/VideoOpenBar.tsx`, `src/components/SubtitleBar.tsx`, `src/i18n/en.ts`,
`src/App.css`, `e2e/specs/video.spec.js`, `e2e/specs/subtitle.spec.js`,
`e2e/scripts/close-gate-check.js`, `e2e/wdio.conf.js`.

**Depends on.** T1 (its checks use the helper). T1b is not a blocker for T2 but lands before it in
the queue.

**Public interface change, called out per CLAUDE.md §6:** the IPC command `project_choose_path` is
replaced by `choose_path`, and its single consumer (`useProject.ts`) is updated in the same
delivery. No other caller exists; verified by grep across `src/` and `src-tauri/src/`.

**Stop condition inherited from T1.** If the save-mode chooser cannot be found, typed into,
confirmed or cancelled by the T1 helper, **T2 stops and reports BLOCKED before any path box is
removed.** T1 could not answer this — there is no save-mode chooser until this task builds one
(`project/mod.rs:244` has exactly two kinds, both open-mode) — so the gate lives here, where the
answer exists.

**Acceptance criteria.**

- Click `Choose…` beside the video path box, pick `fixtures/video/sample.mkv`: `.bar__input` holds
  that path, and `Open` reaches the same ready state the existing check asserts — `.stage__empty`
  gone and `.controls__button` enabled — with the transport time advancing past `0:01`.
  `[new: video.spec.js]`
- Click `Choose…` beside the subtitle path box, pick `basic-lf.srt`: the box holds it, and `Open`
  makes `.subbar__status` read the `STATUS_PREFIX` literal the existing check asserts.
  `[new: subtitle.spec.js]`
- Click `Choose…` beside `Save copy to`. The chooser opens in save mode. Confirm a destination in
  the scratch folder, then `Save as`: the file appears there and `Buffer.compare` against the
  source is 0. `[new: subtitle.spec.js]`
- The save chooser proposes a name derived from the open file. If T1's report says the name entry
  is readable, the criterion is that it reads `<episode>.<ext>` before anything is typed; if it is
  not readable, the criterion is the same proposal observed through its effect — confirm without
  typing a name at all, and the file that appears is named `<episode>.<ext>`.
  `[new: subtitle.spec.js]`
- Dismiss any of those three choosers with Escape: the box holds the same string it held before,
  the destination folder's sorted listing is identical to the listing taken before the click, and
  no file in it has a newer mtime. `[new, folded into the three above]`
- The four choosers carry four distinct titles, read from the chooser toplevel's `WM_NAME` by the
  T1 helper, one per kind, none equal to another. `[new: subtitle.spec.js]`
- `pnpm e2e:close-gate` passes 12/12 with its assertions unchanged; the two points it clicks are
  re-derived if this task moved them. `[close gate]`

Where the titles are read from is a review rule, not a criterion: §6's first bullet already makes
a JSX or Rust literal a rejection, and no observation of the running app can tell an inlined
string from the same string in `strings.rs`.

**E2E.** New checks in `video.spec.js` and `subtitle.spec.js`, three in total. Existing assertions
in both files unchanged. Guard 30 → 33.

**Parallel.** May run alongside T4: T2 touches `VideoOpenBar.tsx`, `SubtitleBar.tsx`,
`useProject.ts` and the backend; T4 touches `CueList.tsx`, `useSubtitleFile.ts`, a new hook and
`App.tsx`. Two files are genuinely shared and both are one-liners: `en.ts` (T4 adds no user-facing
copy; if it turns out to need a string the pair is serialized) and the `EXPECTED_TESTS` integer in
`e2e/wdio.conf.js`, which both tasks bump. The orchestrator merges T2 first, sets the merged
number, and the second delivery re-runs the guard against it (WORKFLOW §5).

---

### T3 — The layer registry, the surface that hides for it, and transcription as a dialog

**Delivery.** The layer registry from §2.1, the video surface hiding while any layer is open, and
the first real layer: the transcription band becomes a dialog opened from a button, with the
inputs-and-outputs seam of §2.3. The always-on band leaves the screen. All of this happens
**inside the current layout**, so the riskiest piece of the milestone is proven while nothing else
is moving.

**Files owned.** `src/hooks/useLayers.ts` (new), `src/components/TranscribeDialog.tsx` (replaces
`TranscribeBar.tsx`), `src/components/StatusLine.tsx` (new, arrives here because the run's output
needs a home before T5b builds the rest of it), `src/components/VideoStage.tsx`,
`src/hooks/useVideoPlayer.ts`, `src/App.tsx`, `src/App.css`, `src/i18n/en.ts`,
`e2e/specs/asr.spec.js`, `e2e/specs/video.spec.js`, `e2e/scripts/close-gate-check.js`,
`e2e/wdio.conf.js`.

**Depends on.** N2 (the visibility command, decision 2) — a hard dependency, and N2 is not
started. T1 and T1b (the pixel instrument). It merges after T2 because both touch the app
composition. See §5 for the order to run if N2 slips.

**Opens by reading, not by assuming.** The first thing this task does is read N2's delivered
command signature and write it into §2.1. `video_set_visible` is a guess.

**Acceptance criteria.**

- At first paint there is no transcription band: `document.querySelector(".transcribe__model")` is
  null, and `.status__transcribe` reads the `IDLE_STATUS` literal. `[new: asr.spec.js]`
- `Transcribe…` is enabled with no video open and opens the dialog. Inside it,
  `.transcribe__start` is `disabled === true` and its accessible name contains the
  `NO_VIDEO_REASON` literal. Open a video: it enables. `[existing: asr.spec.js "offers the models
it knows and a compute choice", plus new]`
- With a video playing and the dialog open: the surface is `IsUnMapped`, and every control in the
  dialog passes the hit test — `elementFromPoint` at the centre of its rect returns it or a
  descendant. Press Escape: the surface is `IsViewable` again, its X11 geometry matches
  `.stage__surface`'s DOM rect within the existing 2 px tolerance, and two spread samples 1.5 s
  apart both read in the live-frame range and differ, so the picture is advancing and not a frozen
  or empty rectangle. `[new: video.spec.js]`
- Start a run from the dialog. The dialog closes; the surface is `IsViewable` again; progress and
  `.status__transcribe-cancel` are on the status line. Cancel stops the run, the sidecar pid is
  gone, and the scratch folder is empty. `[existing: asr.spec.js "shows progress, stays usable,
and leaves nothing running when cancelled", re-pointed]`
- Resize the window from 1024x700 to 1920x1080 with the dialog open: every sample of the surface
  across the resize reads `IsUnMapped`; on close, the surface lands on the new `.stage__surface`
  rect, not the old one. `[new: video.spec.js]`
- A run that fails its pre-flight checksum shows its banner on `.status__transcribe-error`
  containing `"checksum"`, with the dialog already closed; reopening the dialog shows the model
  row labelled damaged, `.transcribe__download` present and `.transcribe__start` disabled.
  `[existing: asr.spec.js "refuses a damaged model and never hands it to the sidecar",
re-pointed]`
- `pnpm e2e:close-gate` passes 12/12 with its assertions unchanged. This task removes the ASR band
  from the column, so both remaining points are re-derived, and `FIRST_CUE_TEXT` is replaced by
  §2.4's `CUE_LIST_EMPTY` plus `Enter`. `[close gate]`

**E2E re-point, spelled out because this is where a re-point could quietly become a rewrite.**
`asr.spec.js`'s `before` waits on `.status__transcribe` — a positive signal present at first paint
— and every check that reaches the model list, the GPU tick or the start button opens the dialog
first. **Adding a step to reach a control is not weakening an assertion**; every expectation in
those five checks stays exactly as written: the `>= 10` model options, `tiny.en` present and
selected, the "ready" label, the absent download button, the ticked GPU box, the disabled start,
the `IDLE_STATUS` line, the byte-level media-untouched comparisons, the `-ng` command line, the
`"checksum"` banner and the leaves-nothing-running reap. Three of the five now open or reopen the
dialog to reach an input; none of them asserts anything new and none asserts anything less.

Four new checks in `video.spec.js` and `asr.spec.js`. Guard 33 → 37.

---

### T4 — Active line and selection become two states

**Delivery.** The single `selected` index in `CueList.tsx:85` becomes the two states of §2.2, owned
by a hook beside the document state, drawn by the grid, driven by keyboard and mouse, and remapped
by `applyPatch`.

**Files owned.** `src/hooks/useSelection.ts` (new), `src/hooks/useSubtitleFile.ts`,
`src/components/CueList.tsx`, `src/App.tsx`, `src/App.css`, `e2e/specs/editor.spec.js`,
`e2e/wdio.conf.js`.

**Depends on.** Nothing structural. Merges after T3 because both touch `src/App.tsx`; may be
implemented in parallel with T2, and may be run **before** T3 if N2 has not landed (§5).

**Acceptance criteria.**

- Open the 2,000-cue fixture and touch nothing: row 1 carries `.cuelist__row--active` and is the
  only row carrying `.cuelist__row--selected`. `[new: editor.spec.js]`
- Press Down four times: row 5 carries the cursor and is the only row marked. Press Shift+Down
  three times: rows 5, 6, 7 and 8 are marked, row 8 and only row 8 carries the cursor, and rows 4
  and 9 are not marked. `[new: editor.spec.js]`
- Ctrl+Down twice then Ctrl+Space, twice over from row 8: the marked rows are exactly 8, 10 and
  12, rows 9 and 11 are not marked, and exactly one row carries the cursor. `[new:
editor.spec.js]`
- Click row 10, then shift-click row 14: rows 10 to 14 are marked and row 14 carries the cursor.
  Ctrl-click row 12: rows 10, 11, 13 and 14 stay marked, row 12 is not, and row 12 carries the
  cursor. `[new: editor.spec.js]`
- Escape with more than one row marked leaves the cursor where it is and collapses the marks onto
  it. Ctrl+A marks all 2,000 rows and leaves the cursor where it is. `[new: editor.spec.js]`
- A click inside the grid below the last row changes neither state and leaves the grid focused, so
  Enter opens the editor on the active row. `[new: editor.spec.js, and depended on by the close
gate from T3]`
- With rows 10, 11, 13 and 14 marked, edit row 13's text and press Enter: the same four rows are
  still marked, identified by their text and not by their index, and the cursor has not moved.
  `[new: editor.spec.js]`
- Press Tab from an open editor: the edit is committed, the cursor moves down one row, and that
  row is the only one marked. `[new: editor.spec.js]`
- Edit row 1500, scroll back to the top, then undo: the grid scrolls row 1500 into view, it
  carries the cursor and is the only row marked, and its text is the literal it was opened with.
  `[new: editor.spec.js]`
- Ctrl+A over 2,000 rows, then scroll to the bottom: scrolling stays inside the budget the
  existing scroll check measures, and the existing virtualization check still passes unchanged —
  at most the rows in view plus the overscan are in the DOM. A selection that rendered the whole
  file would fail it. `[existing: editor.spec.js "renders only the rows in view, over a sizer as
tall as the whole file" and "scrolls a viewport at a time without falling behind"]`

**E2E.** Four new checks in `editor.spec.js`, grouping the criteria above: the keyboard building
both states; the mouse building both states; a scattered selection surviving an edit; undo far
from the viewport bringing the row on screen. The ten existing checks keep their assertions; three
of them click once on a row's text and then wait for `.cuelist__editor` (lines 327, 511, 549), and
§2.2 keeps that gesture working, so they are not touched at all. Guard 37 → 41.

**Known gap, stated rather than papered over.** The remap branch for a row that a patch _removes_
cannot be driven from the UI at M2.0: there is no delete-line or insert-line gesture in the grid,
and `package.json` has no TypeScript test runner, so a unit test of the remap function would mean
a new dependency and an owner decision (CLAUDE.md §8). The branch is written and reviewed against
the arithmetic in `shell-layout.md`; the behavioural check for it belongs to the M2.5 task that
adds a delete gesture, and that task must bring it. Recorded as untested, not as done.

---

### T5a — The frame

**Delivery.** The Aegisub arrangement, with everything that exists today re-parented into it and
nothing removed yet: project rail on the left; a top band with the video panel on the left and the
top-right column beside it; the cue grid across the whole width underneath; a status line at the
bottom. The colour tokens and the dark-first palette land here, in one place, because this task
rewrites `App.css` anyway.

The **two** remaining bars — `VideoOpenBar` and `SubtitleBar`; the transcription band went in T3 —
are parked unchanged, with their class names intact, in one strip above the top band. They look
temporary because they are; every open route and every existing selector keeps working, so this
delivery re-points nothing.

The top-right column holds the current-line band: the active row's start, end and duration, and
its text, read-only. That is the place M2.5 fills with editable times and M2.6 with the
two-document editor. The waveform panel is not created: its provider does not exist yet.

**Files owned.** `src/App.tsx`, `src/App.css`, `src/components/Shell.tsx` (new),
`src/components/EditBox.tsx` (new), `src/components/VideoPanel.tsx` (new, wrapping the existing
stage and controls), `src/i18n/en.ts`, `e2e/specs/video.spec.js`,
`e2e/scripts/close-gate-check.js`, `e2e/lib/shell-points.js` (new), `e2e/wdio.conf.js`.

**Depends on.** T4 (the current-line band follows the active line), T1b (both window sizes).

**Acceptance criteria.**

- With a project, a video and the 2,000-cue fixture open, read the DOM rects of `.rail`,
  `.video`, `.side`, `.cuelist__panel` and `.status`. None is zero-sized;
  `rail.right <= video.left`; `video.right <= side.left`; `grid.top >= video.bottom` and
  `grid.top >= side.bottom`; `grid.left` equals `video.left` and `grid.right` equals `side.right`
  within 1 px; `status.top >= grid.bottom`. That is the arrangement, stated so it can fail.
  `[new: video.spec.js]`
- Click a row in the grid: `.editbox__start`, `.editbox__end`, `.editbox__duration` and
  `.editbox__source` hold that row's values. Arrow down: they follow. With no file open the band
  holds the `EMPTY_LINE` literal. `[new: video.spec.js]`
- At 1024x700 and again at 1920x1080, with everything open and in its tallest state — a video
  error line showing, a document error line showing, the discard control present:
  `document.scrollingElement.scrollWidth === clientWidth` and
  `document.scrollingElement.scrollHeight === clientHeight`, so the document scrolls in neither
  direction; every element carrying a visible label satisfies `scrollWidth <= clientWidth + 1`;
  and every control's rect lies inside the viewport. The toolbar's save-copy-as control is the one
  this milestone regressed on, so it is named in the failure message. `[new: video.spec.js]`
- With a video playing, scroll the wheel at four points: over the video panel, over the toolbar
  strip, over the status line, and inside the grid scrolled to row 1500; then scroll the rail to
  its last episode. The surface's X11 geometry is byte-identical before and after all five, and
  the frame is still advancing. The only regions that scroll are the grid and the rail. `[new:
video.spec.js]`
- Resize from 1024x700 to 1920x1080 with a video playing and back again: the surface tracks
  `.stage__surface`'s rect at both sizes, within the existing tolerance. `[new, folded into the
clipping check]`
- `pnpm e2e:close-gate` passes 12/12 with its assertions unchanged. This task moves every point it
  clicks: both are re-derived at 1024x700 with no video open, and they move into
  `e2e/lib/shell-points.js` as the single definition T8 will also read. `[close gate]`

The vertical-overflow half of the clipping criterion is not decoration. `VideoStage` recomputes
the region on the ResizeObserver and on `window resize` (`VideoStage.tsx:41-43`); a vertical
document scroll changes `getBoundingClientRect().top` without changing any element's size, so
neither callback fires and the native surface stays where it was while the panel moves under it.
That is precisely the M0.2 failure §6 forbids, and no criterion in the first draft measured it.

**E2E.** Three new checks in `video.spec.js`: the arrangement rects; no clipping and no document
scroll at both window sizes; the wheel and scroll safety at five points. The existing
surface-geometry check already proves the frame tracks the panel and is not modified.
Guard 41 → 44.

**Why the bars are parked rather than removed here.** Removing them means re-pointing four spec
files in the same delivery as a full layout rewrite, which is past one sitting (WORKFLOW §4). One
deliberately ugly intermediate buys two reviewable deliveries instead of one unreviewable one.

---

### T5b — The toolbar, the open routes, and the three path boxes go

**Delivery.** A toolbar row: open video, open subtitle, save, save copy as, undo, redo,
transcribe. Each open goes through the native dialog from T2. The two parked workspace bars are
deleted; their status, dirty, truncated, error and discard controls move onto the status line T3
started.

**This is the task that takes away the three boxes where a path is pasted by hand.**

**Files owned.** `src/components/Toolbar.tsx` (new), `src/components/StatusLine.tsx`,
`src/components/VideoOpenBar.tsx` and `src/components/SubtitleBar.tsx` (both deleted),
`src/App.tsx`, `src/components/Shell.tsx`, `src/App.css`, `src/i18n/en.ts`,
`e2e/specs/video.spec.js`, `e2e/specs/subtitle.spec.js`, `e2e/specs/editor.spec.js`,
`e2e/specs/asr.spec.js`, `e2e/scripts/close-gate-check.js`, `e2e/lib/shell-points.js`,
`e2e/wdio.conf.js`.

**Depends on.** T1, T2, T5a.

**Acceptance criteria.**

- With no cue editor open, `document.querySelectorAll(".shell input, .shell textarea")` returns
  nothing outside `.rail`. Opening the editor on a row adds exactly one and closing it removes it
  again. The rail still holds one text input, the episode title, which is not a path field; T6
  keeps it. `[new assertion inside an existing check: editor.spec.js]`
- Click `.toolbar__open-video`, pick `fixtures/video/sample.mkv`: `.stage__empty` is gone,
  `.controls__button` is enabled, and the transport time advances past `0:01`.
  `[existing: video.spec.js "opens the sample fixture", re-pointed]`
- Click `.toolbar__open-subtitle`, pick `basic-lf.srt`: `.status__document` reads the
  `STATUS_PREFIX` literal. `[existing: subtitle.spec.js "opens an SRT fixture and shows its format
and cue count", re-pointed]`
- Edit a cue: `.status__dirty` is present. Click `.toolbar__save`: the file is written and
  `.status__dirty` is gone. `[existing: editor.spec.js "commits the edit on Enter and marks the
file unsaved" and "saves the edit…", re-pointed]`
- Click `.toolbar__save-as`, confirm a destination: `Buffer.compare` between copy and source is 0.
  `[existing: subtitle.spec.js "saves a byte-identical copy", re-pointed]`
- Open a malformed fixture: `.status__document-error` holds the literal the existing check
  asserts, the app is still usable, and a clean fixture opens straight afterwards with the error
  line gone. `[existing: subtitle.spec.js "reports a malformed file readably and stays usable",
re-pointed]`
- With unsaved edits, opening another file is refused on `.status__document-error` and
  `.status__discard` appears; discard then opens the new file. `[existing: editor.spec.js,
re-pointed]`
- `.toolbar__undo` restores row 300 to the literal it was opened with and `.toolbar__redo` puts
  the edit back; ctrl+z typed inside `.project__new-episode` reaches that field and not the
  document. `[existing: editor.spec.js "leaves ctrl+z to the destination box and undoes exactly
one step from the toolbar", re-pointed — see below]`
- `pnpm e2e:close-gate` passes 12/12 with its assertions unchanged; the open route becomes
  `TOOLBAR_OPEN_SUBTITLE` plus the T1 helper, and both points are re-derived in
  `e2e/lib/shell-points.js`. `[close gate]`

**The ctrl+z regression check keeps its subject, and the change is named.** `editor.spec.js:543`
is the regression fixture for a real M2.3 defect: the shortcuts were bound on the window whatever
had focus, so ctrl+z typed into a path box undid a cue edit. It works by typing into
`.subbar__dest`, pressing ctrl+z there, and then proving the toolbar undo takes the **first** step
off the stack rather than the second. T5b deletes `.subbar__dest`, so the field moves to
`.project__new-episode`, the one text input in the shell that is neither a path box nor the cue
editor, and the one T6 is forbidden to remove. Everything else in that check — the two edits, the
literals, the order, the final two assertions — is unchanged. This is a selector re-point with the
same meaning, not a change of subject: an unrelated text field is exactly what the regression is
about. It is named in the delivery description anyway, per §6.

**E2E.** The open gesture in `video.spec.js`, `subtitle.spec.js`, `editor.spec.js` and
`asr.spec.js` becomes one call to the T1 helper instead of `typeInto` plus a click. Each `before`
hook waits on `.shell` **and** on the control that file drives — `video.spec.js` on
`.toolbar__open-video`, `subtitle.spec.js` and `editor.spec.js` on `.toolbar__open-subtitle`,
`asr.spec.js` on `.status__transcribe` — so the gate stays as strong as today's, where each hook
waits on the box it is about to type into. `.shell` alone would only say the app rendered
something, and a toolbar that failed to mount would surface as a confusing click failure ten lines
later instead of a clean timeout. **No assertion in the 44 is weakened, skipped, retargeted or
re-worded**; the expected strings, the byte comparisons, the budget numbers and the process checks
are the same literals they are today. Guard stays at 44.

`close-gate-check.js`: part of the N1 debt is paid here. The 1,500 ms fixed wait after opening the
file becomes a wait on the chooser toplevel disappearing, which is observable at the X level with
no DOM. The 600 ms wait for the inline editor and the 2,500 ms wait after commit stay fixed and
stay named as fixed, because a script with no WebDriver session still has nothing to observe
there.

`shutdown-check.js` is not touched: it drives no DOM, and its five checks are about the window and
the exit status.

**Open budget, re-pointed not weakened.** `editor.spec.js` stamps `__subloreClickAt` immediately
before the click on `.subbar__open` today. It moves to immediately before the confirm keystroke in
the file chooser, so the measured window becomes chooser teardown plus IPC plus parse plus paint.
That is strictly wider than today's window and `expect(elapsed).toBeLessThan(1000)` is unchanged.
The teardown cost was measured in T1's report; if it turns out to eat the budget, that is a §7
regression to put in front of the owner (WORKFLOW §3), never a reason to move the number.

---

### T6 — The project rail

**Delivery.** The sidebar becomes the rail from the mockup: the project caption, the episode rows,
the files under the selected episode. New project, open project and attach file each go through
the chooser. The rail's two path boxes go.

**Files owned.** `src/components/ProjectPanel.tsx` (becomes `Rail.tsx`), `src/hooks/useProject.ts`,
`src/App.css`, `src/i18n/en.ts`, `e2e/specs/project.spec.js`, `e2e/wdio.conf.js`.

**Depends on.** T2, T5a, T5b.

**Two constraints, both with a criterion.** `.project__new-episode` is not renamed and not removed
(§3). And the rail keeps a width that does not depend on its content, because the grid's left edge
is derived from it and `e2e/lib/shell-points.js` holds numbers measured against it.

**Acceptance criteria.**

- There is no box anywhere for typing a path: `document.querySelectorAll(".shell input,
.shell textarea")` returns exactly one element with no cue editor open, and it is
  `.project__new-episode`, the episode title. `[new assertion inside an existing check:
project.spec.js]`
- At 1024x700 and at 1920x1080, and with a project whose name is 80 characters long, `.rail`'s DOM
  rect has the same width in all three states. `[new assertion inside an existing check:
project.spec.js]`
- Click `.rail__new-project`, pick an empty folder: `.project__status` contains that folder and
  `project.sublore` exists in it. `[existing: project.spec.js "creates a project in an empty
folder", re-pointed]`
- Add an episode, click `.rail__attach-file` and pick a subtitle file outside the project folder:
  it is listed under the episode, and that file on disk has the same size, the same `mtimeMs` and
  the same bytes it had before. `[existing: project.spec.js "adds an episode and attaches a
subtitle file to it"]`
- Restart the app, click `.rail__open-project`, pick the same folder: the episode and its file are
  listed again, and a fresh app before that shows the `NO_PROJECT` literal. `[existing:
project.spec.js "still lists the episode and its file after the app is restarted"]`
- Pick a folder with no project in it: `.project__error` contains the `NO_PROJECT_HERE` literal,
  `.project__status` reads `NO_PROJECT`, no database file exists in that folder, and the real
  project opens straight afterwards with the error gone. `[existing: project.spec.js "reports a
folder that holds no project and stays usable"]`
- An attach of a path that points at nothing fails with `NO_SUCH_FILE`, and the project stays on
  screen beside the message. `[existing: project.spec.js, same check, substitute route below]`
- Delete the project: the database is gone, the attached user file is byte-identical, and the
  folder the user chose still exists. `[existing: project.spec.js "deletes the project without
touching the files it points at"]`

**The `NO_SUCH_FILE` route, decided here so no implementer has to.** That assertion is driven
today by typing `no-such-file.srt` into `.project__file-path` (`project.spec.js:247-249`). An
open-mode `GtkFileChooser` will not confirm a path that does not exist — T1's report says whether
that is so on this build — so after T6 there is no way to hand the app a missing path through the
chooser, and the cheapest way out would be to delete the assertion, which is §5.4. The substitute:
pick a **real** file in a scratch folder through the chooser, delete that file from the filesystem
inside the test, then click attach. The backend produces the error at attach time rather than the
chooser at pick time, and `NO_SUCH_FILE` stays the same literal meaning the same thing — the app
refuses to record a path that points at nothing.

**E2E.** `project.spec.js` re-pointed: `typeInto(".project__path")` plus `.project__create`
becomes the chooser helper on `.rail__new-project`, and the same for open and attach. The five
existing checks keep every assertion, including the stat and byte comparisons that are the whole
point of the milestone they came from. The two checks T1 added move onto the new controls with
their assertions intact. Guard stays at 44.

---

### T7 — The menu bar

**Delivery.** The menu bar strip above the toolbar, with dropdowns that register as layers through
T3's registry, driven by the keyboard model in `shell-layout.md`. Only titles with working items
ship: File (open video, open subtitle, save, save copy as), Edit (undo, redo), Video (play/pause,
transcribe), Subtitles (select all). Timing, Audio, Terms and View arrive with M2.5, M2.4, M5 and
later. `Close` is not here; §1 says why.

**Files owned.** `src/components/MenuBar.tsx` (new), `src/components/Shell.tsx`, `src/App.css`,
`src/i18n/en.ts`, `e2e/specs/video.spec.js`, `e2e/scripts/close-gate-check.js`,
`e2e/wdio.conf.js`.

**Depends on.** T3 (registry), T5b (the commands the items call), T5a (the frame).

**Acceptance criteria.**

- With a video playing, click File. The surface is `IsUnMapped`, and every `.menu__item` in the
  open dropdown passes the hit test. Close it: the surface is `IsViewable` over the same
  `.stage__surface` rect, and two spread samples 1.5 s apart both read live and differ. `[new:
video.spec.js]`
- File, then Transcribe. Sample the surface's map state every 50 ms from the click until the
  dialog is on screen: every sample reads `IsUnMapped`. Close the dialog: the surface is
  `IsViewable` over the panel rectangle. This is a sampled negative at 50 ms, not a proof that no
  frame was ever mapped, and the M2.0 status says so. `[new: video.spec.js]`
- Open the File dropdown, open a dialog from it, close the dialog: the surface is still
  `IsUnMapped`. Close the dropdown: it is `IsViewable`. Two layers, one transition each way.
  `[new: video.spec.js]`
- Each of the nine items does what the table below says, and File → Open subtitle is reachable
  with the keyboard alone: Alt, Right or Left to File, Down to the item, Enter, and the chooser
  appears. `[new: video.spec.js, table-driven]`
- Resize the window with a dropdown open: every sample reads `IsUnMapped`; on close the surface
  lands on the new rect. `[new, folded into the second check]`
- No menu title opens an empty dropdown: every `.menubar__title` opens a `.menu` holding at least
  one `.menu__item`. `[new, folded into the fourth check]`

**The item table, which replaces "every item does exactly what its twin does".** One row is one
observation, and the check walks the table.

| item                   | observation                                                                                                                                       |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| File → Open video      | opens a chooser whose title equals the toolbar's open-video chooser title; picking `sample.mkv` clears `.stage__empty` and the transport advances |
| File → Open subtitle   | opens a chooser with the subtitle title; picking `basic-lf.srt` makes `.status__document` read `STATUS_PREFIX`                                    |
| File → Save            | with `.status__dirty` present, writes the file and clears `.status__dirty`                                                                        |
| File → Save copy as    | opens the save-mode chooser; confirming produces a byte-identical copy                                                                            |
| Edit → Undo            | restores the last committed cue edit to the literal it was opened with                                                                            |
| Edit → Redo            | puts it back                                                                                                                                      |
| Video → Play/pause     | toggles the transport state `.controls__button` reports                                                                                           |
| Video → Transcribe     | opens the same dialog `.toolbar__transcribe` opens: `.transcribe__model` present                                                                  |
| Subtitles → Select all | marks all 2,000 rows, same as Ctrl+A, cursor unmoved                                                                                              |

**E2E.** Four new checks in `video.spec.js`. Guard 44 → 48.

Also here: the check `shell-layout.md` reserves for N1's close gate raised over a playing video.
N1 was verified with no video loaded, so "the native dialog stacks above our surface" is an
argument about window managers and not a fact. It goes in `close-gate-check.js`, which already
owns the gate: with a video playing, request the close; the gate is found in the window tree and
answerable, and the answer lands. `EXPECTED_CHECKS` goes 12 → 13; the mocha guard is untouched by
it. Native windows sit outside the layer rule by construction, so nothing about the registry
changes here.

---

### T8 — Where the budgets are measured after the restructure

**Delivery.** The two §7 budgets that have no home get one, and the two that have one keep it.

**Files owned.** `e2e/scripts/budget-check.js` (new), `package.json` (one script entry),
`.github/workflows/ci.yml`, `e2e/specs/title.spec.js`, `e2e/lib/shell-points.js`,
`e2e/wdio.conf.js`.

**Depends on.** T5b, T6, T7 (there is no point measuring a shell that is still moving).

**The four budgets, after M2.0:**

| §7 budget                          | measured where                                          | status today                                                        |
| ---------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------- |
| Opening a 2,000-line file < 1 s    | `editor.spec.js`, `OPEN_BUDGET_MS`, on `large-2000.srt` | already measured; T5b widens the window and keeps the number        |
| UI responsive during transcription | `asr.spec.js`, the progress and cancel checks           | already measured; T3 keeps it with the dialog closed during the run |
| Cold start to interactive < 2 s    | new, `budget-check.js` plus `title.spec.js`             | **never measured**                                                  |
| Idle memory < 400 MB               | new, `budget-check.js`                                  | **never measured**                                                  |
| QA pass < 5 s                      | M5                                                      | not M2.0's                                                          |

**Cold start, and the half that has to be probed first.** `budget-check.js` owns the spawn, so
half one is exact: from spawn to the 1024x700 "Sublore" toplevel existing. Half two is the paint,
and **the plan does not yet know it has a signal**. `title.spec.js` runs long after `.shell`
appeared, so `performance.now()` there measures the spec's own clock; recovering "when `.shell`
entered the DOM" after the fact needs a `MutationObserver` installed before it (impossible from a
spec that starts later), an `elementtiming` attribute (production markup serving a test, which §3
refuses), or a `PerformancePaintTiming` entry whose availability in this WebKitGTK build nobody
has looked for.

So T8 opens with a one-session probe, exactly the way T1 opens the milestone: print
`performance.getEntriesByType("paint")` and `getEntriesByType("navigation")` from the real app and
write what came back into `docs/reports/m2-0-budget-probe.md`. Then one of three, chosen by the
answer and written down: half two is the retroactive entry that exists; or it is
first-contentful-paint with the substitution stated; or **cold start is measured externally only**
— spawn to the toplevel being mapped — and the M2.0 status records that the paint-to-interactive
half is not measured. Any of the three is fine. Claiming a signal nobody looked for is not (§9).

**Idle memory, with a settle condition that is not a vibe.** `budget-check.js` spawns the app and
opens the 2,000-cue fixture and the video fixture through the chooser, reusing the T1 helper and
`e2e/lib/shell-points.js` rather than duplicating a third set of coordinates. It pauses playback,
then samples the summed RSS across `processGroupMembers(pgid)` every 500 ms. The measurement is
the first sample within 2% of the previous two. If no such sample arrives within 30 s the check
fails as _did not settle_ and never reports the last value it happened to see. Whisper is excluded
by construction: nothing is transcribed, so no model is resident. Asserted under 400 MB.

**Acceptance criteria.**

- `pnpm e2e:budgets` prints the cold-start figure and the idle-memory figure, each with the halves
  or the settle sample it came from, and exits non-zero if either is over budget or if the memory
  measurement did not settle. `[new: budget-check.js, 2 checks]`
- The CI Linux job runs it beside the other checks and a regression turns CI red. `[new: ci.yml]`
- Whichever cold-start shape the probe settles on is the one implemented, and the M2.0 status
  names it. `[probe: m2-0-budget-probe.md]`

**Honesty clause, non-negotiable (CLAUDE.md §9).** These are debug-build numbers under Xvfb with
software rendering, on Linux. They are a necessary condition, not the §7 verdict; the verdict is
the release build on the owner's machine, and the report says so with the platform on it, the same
way `editor.spec.js` already qualifies its own numbers. The mocha guard goes 48 → 49 for the
cold-start half measured in `title.spec.js`, if the probe leaves one there; if it does not, the
guard stays at 48 and the plan's running total is corrected in the delivery rather than the check
being invented to match the number.

---

## 5. Dependency graph, parallelism, and the order if N2 slips

```
N2 ─────────────────────────┐
T1 ─┬─ T1b ─┬─ T2 ─┬────────┴─ T3 ─┬─ T5a ── T5b ─┬─ T6 ─┬─ T8
    │       │      │               │              │      │
    └───────┴──────┴───── T4 ──────┘              └─ T7 ─┘
```

- **N2 is a hard predecessor of T3**, and `BACKLOG.md` marks it `[ ]`: not started. Six of the ten
  tasks sit behind it. That is a schedule fact the first draft hid by drawing T1 → T2 → T3 as if
  it were runnable end to end.
- **The fallback order.** T4's dependency on T3 is convenience, not substance — they merge in
  sequence only because both touch `src/App.tsx`. So if N2 has not landed when T2 finishes, run
  **T4 before T3**, and T3 rejoins as soon as N2 is delivered. Nothing else in the order changes.
- T2 and T4 are the one sanctioned parallel pair: file ownership is otherwise disjoint, and the
  two shared one-liners (`en.ts` if T4 needs a string, and the `EXPECTED_TESTS` integer) are
  handled the way T2 describes.
- T1 and T1b may run in parallel: `dialog.js` against `x11.js`, `paths.js`, `pixels.js`,
  `dom.js`. Both bump the counter, so the same one-liner rule applies.
- T6 and T7 are disjoint in components (`Rail.tsx` against `MenuBar.tsx`) but both edit
  `Shell.tsx`, `App.css` and `en.ts`. Run them in sequence unless the orchestrator freezes those
  three files first (WORKFLOW §5).
- Everything else is sequential, because `src/App.tsx` and later `src/components/Shell.tsx` are the
  composition root and almost every task passes through them.

**Every merged task leaves a working app and a green battery — all three runners.** The nearest
thing to a compromise is T5a's two parked bars, which are ugly for exactly one delivery and fully
functional throughout; the alternative was a single delivery containing a layout rewrite, a
toolbar, four deletions and four re-pointed spec files, which is not reviewable in one sitting and
is exactly the big-bang this ordering exists to avoid.

---

## 6. Standing rules for every task in this milestone

- **No user-facing string in a component.** Menu titles, menu items, dialog titles, dialog buttons,
  toolbar labels and tooltips all live in `src/i18n/en.ts`; native dialog titles live in
  `src-tauri/src/strings.rs`. A literal in JSX is a review rejection (CLAUDE.md §9).
- **Every criterion that asserts text names the literal**, as a spec constant. No criterion says
  "it says why" and leaves the string to the implementer and the assertion to a later argument.
- **The video panel and every ancestor of it never scroll, and the document never scrolls in
  either direction.** The region is computed on resize and on the ResizeObserver, never on scroll
  (M0.2). The rail and the grid scroll; nothing else does.
- **Assertions are frozen.** Selectors move, expectations do not. Any test weakened, skipped or
  deleted must be named in the delivery description with its reason, and doing it silently is
  grounds for rejection (CLAUDE.md §5.4, WORKFLOW §4).
- **`close-gate-check.js` belongs to whoever moves the shell.** T2, T3, T5a and T5b each own it,
  each re-derive its points against the layout they leave behind, and each carry the 12/12
  criterion (13/13 from T7). A task that finds the gate red and the file outside its ownership
  stops with a BLOCKED report; that situation is a defect in this plan, not in the app.
- **`EXPECTED_TESTS` moves with the checks.** Adding a check without bumping it, or bumping it
  without adding one, both defeat the guard. When the count in §4 and the count in the delivery
  disagree, the delivery corrects this file; it never invents a check to reach the number.
- **The engine is not touched.** M2.0 is the frontend shell plus the dialog commands. A task that
  finds itself editing `sublore-formats`, `sublore-edit`, `sublore-io` or the player has drifted.
- **Reviews are delegated and start from `docs/reviews/review-prompt.md`**, and a review's own
  fixes get reviewed (WORKFLOW §4b).
- **Every verdict carries its platform.** "Verified on Linux", never a bare "verified".

---

## 7. What is still the owner's to decide

One thing, and it is an acceptance criterion the shape cannot fully meet.

**"Video and waveform panels sit side by side" cannot be shown at M2.0.** No audio provider exists
before M2.4, and the layout doc adopts Aegisub's rule that a panel with no provider is absent
rather than empty. _Recommendation:_ M2.0 delivers the top-right column with the current-line band
in it, and that half of the AC closes at M2.4 when the waveform panel arrives above the band. The
alternative, an empty placeholder panel, is dead UI that CLAUDE.md §6 rules out. Affects T5a's
first criterion, which asserts the arrangement as three panels and not four.

Two questions the first draft put to the owner are now decided in the plan, because the
constraints already answered them and asking would have cost a round trip:

- **The transcription dialog closes when a run starts**, and everything a run produces lives on
  the status line. §2.3 has the reasoning, and it is not a free choice: leaving the dialog open
  holds the picture off the screen for the whole run, and closing it while it owned the output
  would have swallowed the checksum error and broken two ASR checks that assert on it.
- **The remove branch of the selection remap stays uncovered at M2.0**, recorded as uncovered in
  the M2.0 status, and the M2.5 task that adds a delete gesture brings the check. Covering it now
  means either a delete gesture the owner did not ask for or a TypeScript test runner, which is an
  §8 dependency decision. Doing nothing and writing it down is the in-scope answer; the owner
  should know it, but nothing waits on him.

And one thing that could become a question during T1b: if `xdotool windowsize` cannot really
resize this app under a WM-less Xvfb, the 1920x1080 criteria become owner-checklist items and T1b
reports BLOCKED with that recommendation. Nothing is assumed about it here.

---

## 8. Task count and shape, for the orchestrator

Ten tasks. Two are harness-only with no production code (T1, T1b), one is backend plus two buttons
(T2), one is the risky one (T3), one is state (T4), three are the shell itself (T5a, T5b, T6), one
is the menu (T7), one is measurement (T8).

Three pairs may run in parallel with the file rules above: T1 with T1b, T2 with T4, and T6 with T7
if the orchestrator freezes `Shell.tsx`, `App.css` and `en.ts` first. Everything else is
sequential through the composition root.

Forty-nine mocha checks at the end, from twenty-seven. Thirteen in the close gate, from twelve.
Five in the shutdown check, unchanged. Two in the new budget script.

---

## 9. Rilievi lasciati aperti

Nothing from either critique disappears in silence. All twelve blocking and all twenty-three
serious findings are applied above. The ten minor findings are applied too, so this section is
short: it holds the five findings resolved differently from what the critique proposed, and the
two things that stay open with their reason.

### Resolved, but not the way the critique proposed

- **Observability B4 (the ctrl+z regression loses its subject).** The critique proposed re-pointing
  it onto the inline cue editor as a declared change of subject. Taken instead in the form the
  incrementality critique proposed (S5): the field moves to `.project__new-episode`, which T6 is
  now forbidden to remove. The regression is about a shortcut bound on the window stealing a
  keystroke from an _unrelated_ text field; the editor version would test editor-local undo, which
  is a different property. This keeps the check's meaning and makes it a selector re-point.
- **Observability B2 (no instrument for "the picture is alive").** The critique offered building
  the instrument or dropping the criterion. Building it, in T1b, because N2's own acceptance
  criterion already demands an assertion on the visible frame — the instrument is owed regardless,
  and dropping M2.0's criterion would not have removed the need for it.
- **Observability S1's fourth owner question (the menu keyboard model).** Answered in
  `shell-layout.md` rather than put to the owner. It is design work, and this preparation exists
  to finish design work rather than forward it.
- **Owner question 2 (does the dialog stay open during the run).** Decided in §2.3 rather than
  asked, for the reason the incrementality critique gave: the question was presented as neutral to
  the rest of the plan and it is not — two ASR checks depend on the answer.
- **Observability S6 ("no box to type a path into" is not falsifiable).** Taken, with the wording
  qualified: the workspace holds no input at all outside the cue editor, and the rail holds
  exactly one, the episode title, which is not a path field. The unqualified version would have
  been false the moment T5b's ctrl+z re-point landed in that field.

### Left open

- **A frame that never appeared cannot be proved absent (observability S2).** The menu-into-dialog
  check samples the map state every 50 ms and reports "every sample read `IsUnMapped`". A faster
  poll narrows the window and never closes it. The design guarantee is real and the check is worth
  having; what stays open is the gap between them, and the fix is the honesty clause in the M2.0
  status, not a better instrument.
- **The remove branch of the selection remap (T4's known gap).** No UI gesture reaches it and no
  TypeScript test runner exists. Written, reviewed against the arithmetic, recorded as untested,
  and owed by the M2.5 task that adds a delete gesture. Closing it inside M2.0 costs either scope
  the owner did not ask for or a dependency decision that is his.
