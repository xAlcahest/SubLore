# M2.0 — task breakdown

Preparation for the shell rebuild. Nothing here is implementation: this file exists so that the
first implementer session opens it, picks task 1, and has no design left to do.

Sources this decomposes, all read before writing: `CLAUDE.md` §1, §3, §5, §6, §7, §9;
`docs/design/shell-layout.md`; `docs/design/shell-mockup.html`; `docs/design/decisions.md`;
`docs/reports/n2-probe.md`; `BACKLOG.md` M2.0 (read only, not edited); `WORKFLOW.md`; the current
frontend (`src/App.tsx`, `src/components/*.tsx`, `src/hooks/*.ts`, `src/App.css`, `src/i18n/en.ts`)
with the commands in `src-tauri/src/lib.rs` and `src-tauri/src/project/mod.rs`; and the whole
harness — all eight specs in `e2e/specs/`, `e2e/lib/*.js`, `e2e/scripts/close-gate-check.js`,
`e2e/wdio.conf.js`, `package.json`, `.github/workflows/ci.yml`.

Revision 2, 2026-08-29. The first draft was read by two adversarial passes,
`docs/reports/m2-0-critique-osservabilita.md` (7 blocking, 17 serious, 6 minor) and
`docs/reports/m2-0-critique-incrementalita.md` (5 blocking, 6 serious, 4 minor). Every blocking and
serious finding is applied below. Section 9 records the minors, the ones resolved differently from
what the critique proposed, and the one that cannot be closed at all.

**Revision 3, 2026-08-30.** Revision 2 was written one minute after N2 merged and before N2b, N1b
and N2c landed, so it described a tree that stopped existing the same night. Six lenses read it
against the tree and wrote `docs/reports/m2-0-prontezza.md`: seven blockers, nineteen serious, nine
minor. This revision applies them. **Every sentence changed by that report carries its origin in
brackets** — `(prontezza B1, :33)` names the finding and its line in the report — so any correction
can be traced back to the finding that caused it. Sentences with no such mark are revision 2's and
were found sound. What the report could not check is listed in its §6 and is not re-asserted here.

Every claim below about the current code was checked against the code, not remembered. Nothing was
executed: this is a reading, and the numbers derived from CSS are marked as derived. Revision 3's
claims were re-checked against `main` at `c7261a5`, which is N2c merged.

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

1. **Every route into a loaded file in the mocha suite goes through one of those boxes.** Seven of
   the eight spec files reach their subject that way, each of them by putting the path in the box
   and then clicking the open button beside it: `project.spec.js`, `subtitle.spec.js` and
   `editor.spec.js` through the `typeInto` helper; `video.spec.js` and `asr.spec.js` by focusing
   `.bar__input` and typing into it (`:53-66`, `:149-155`); `video-surface.spec.js` with a click and
   `xdotool type` (`:122-136`); `video-empty.spec.js` through the input's own value setter
   (`:99-111`). The eighth, `title.spec.js`, opens no file at all — its two checks are about the
   window. So the harness needs a way to drive the native file dialog _before_ the first box is
   removed. That is T1, and it is first for that reason. What is **not** true any more is the
   stronger claim revision 2 made, that deleting the boxes leaves the suite unable to reach
   anything: `startup_files` (`src-tauri/src/lib.rs:38-75,105`) opens whatever is named on the
   command line, `useStartupFiles` (`src/App.tsx:23`) is the frontend half of that route, and the
   close gate already takes it. T1 is first because the chooser is the route the product will have
   and the milestone's first AC asserts it, not because nothing else can reach a file. (prontezza
   minor "§0 fact 1", :482; B2, :80; S14, :389)
2. **The native picker already exists and already works**: `project_choose_path`
   (`src-tauri/src/project/mod.rs:145,244`) opens a real rfd dialog from the rail's `Choose`
   buttons, with its title from `strings.rs`, and nothing in the suite exercises it. `Cargo.lock`
   has `rfd 0.16` with `gtk-sys` and no `ashpd`, so on Linux this is a GTK3 file chooser: a real
   X11 toplevel with a title we set. **What is proven about such a toplevel is focus, an estimated
   click and Escape** — that is all `close-gate-check.js` does to the N1 dialog (`:117-134` and `:136-139`), and
   nothing in this repo has ever typed into a `GtkFileChooser`. Entering a path into the chooser is
   the unproven gesture, and answering it is exactly why T1 exists and why its stop condition is
   written the way it is. T1 proves it against code that ships today, with zero production change.
   (prontezza S19, :447)
3. **`close-gate-check.js` is the most shell-sensitive check in the battery, and it has no DOM.**
   `fee26f8` changed how it reaches the document. It no longer opens through the shell at all: the
   fixture is passed in argv and `startup_files` loads it, under the file's own comment "The
   subtitle is passed as an argument, never typed: see WORKFLOW.md 4c and `startup_files`"
   (`:140-147`). Two of the three absolute points went with the typed open. **One point is left**
   (`:42-43`):

   ```js
   /** Point in the current shell, relative to the toplevel origin. M2.0 must revisit this. */
   const FIRST_CUE_TEXT = { x: 750, y: 540 };
   ```

   Its twelve checks run in CI on every push (`ci.yml:196`) and they cover the only data-loss
   defect this project has found and closed (decision 9). Its fixture,
   `fixtures/subtitles/srt/clean/basic-lf.srt`, holds **three** cues, so at `ROW_HEIGHT = 28`
   (`CueList.tsx:17`) the clickable rows are 84 px tall inside a panel of roughly 217 px. Below
   those 84 px is empty space, where a click opens nothing.

   Any task that changes the vertical stack moves that point. Deriving from the CSS
   (`.stage` is `flex: 1 1 45%`, `.cuelist__panel` is `flex: 1 1 55%`, the ASR band is `flex: none`
   plus its status line, roughly 75 px), T3 alone lifts the grid by something like 40 px: the
   direction is certain, the magnitude is an estimate, and nobody has measured it. So **T2, T3,
   T5a and T5b each own `close-gate-check.js` and each re-derive its one point**, instead of one
   task at the end inheriting four tasks' worth of drift. (prontezza B3, :96)

   T3 also removes the height sensitivity for good, per §2.4: the point becomes a click into the
   grid's empty area followed by Enter, a target of roughly 130 px instead of 28.

---

## 1. Scope fence

M2.0 rebuilds the shell. It does not grow the product. Explicitly **not** in M2.0, and an
implementer who finds themselves writing one of these has drifted:

- The waveform panel. No audio provider exists, and the layout doc's rule is that a panel with no
  provider is absent rather than empty, so nothing is built here until M2.4 brings the provider.
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
- **N1's open-editor gap stays deferred, and T3 owns saying so.** `BACKLOG.md`'s N1 entry files a
  data-loss path that is still open: an inline cue editor holding uncommitted text leaves the
  backend session clean, so the window closes without asking and that text is lost. N1 parked it
  against "the HTML-dialog shape decision 1 will settle", and decision 1 is now T3, so the parking
  place has arrived. It does **not** close in M2.0: T3 builds a layer registry over a native close
  gate, and making the gate consult the frontend is a change to the close path, which WORKFLOW
  §4a puts behind gate 4. T3 carries one criterion that names it as still open, and the M2.0
  status repeats it, so a live data-loss path is never silent (CLAUDE.md §3, §9). The task that
  closes it is the one that moves the gate onto an HTML dialog, and that task is not in this
  milestone. (prontezza S17, :424)

Carried in from the ACs and kept: opening goes through the system dialog from menu and toolbar;
panels sit in the arrangement `shell-layout.md` specifies, with rail, video and top-right column
across the top and the grid underneath; the transcription band is off the screen until asked for;
1024x700 and 1920x1080 are both clean; the video panel never scrolls; occlusion (decision 1);
active line separate from selection (decision 5); the 33 checks pass with assertions unchanged.
(prontezza B2, :80. `BACKLOG.md`'s M2.0 AC still says 27 at `:136`; correcting it is outside this
document's edit and is owed by whoever next touches that entry, before T1 is written against it.)

---

## 2. Contracts frozen now, so no task re-decides them

### 2.1 The layer registry

One owner, in the shell, exactly as `shell-layout.md` specifies. T3 builds it; T7 consumes it and
adds nothing to it.

- `useLayers()` exposes `openLayer(id: string)`, `closeLayer(id: string)`, and the derived count.
  State is a **set of ids**, never a counter and never a boolean.
- `surfaceWanted = videoLoaded && videoPanelMounted && layerCount === 0`. Three separate reasons
  for the rectangle to be absent; they are combined in one derived value so they cannot fight. Of
  the three, only `layerCount` changes what the shell puts on the wire: `videoLoaded` is the
  backend's own `video_open`, which already derives visibility from it (`video/mod.rs:57-59`). An
  implementer who gives either of the other two a transport of its own has built the second hide
  path the bullets below forbid. (prontezza B1, :33)
  `videoPanelMounted` is always true at M2.0 — the video panel keeps its `.stage__empty`
  placeholder rather than unmounting. `shell-layout.md`'s reason needs correcting but its
  conclusion does not: three specs reference `.stage__empty` (`video.spec.js:73`,
  `video-empty.spec.js:70`, `asr.spec.js:160`), and the readiness gate all of them actually poll
  is `.bar__input`, which T5b deletes. So the placeholder is not today's readiness gate; it is an
  asserted-on element in three files, and a panel that vanishes takes it out from under them.
  (prontezza minor "§2.1 line 126", :472)
- **There is no visibility command, and T3 must not add one.** N2 delivered derived visibility:
  `src-tauri/src/lib.rs:100-104` registers exactly five video commands — `video_open`,
  `video_play`, `video_pause`, `video_seek`, `video_set_region` — and `video/mod.rs:30-31` states
  the rule in its own words, "Visibility is derived, never set". `wants_shown()` (`:57-59`) is
  `video_open && !region_empty`, and `settle()` (`:64-88`) is the only caller of show and hide.
  `video_set_visible` was revision 2's guess and it was never built. Adding it now would be the
  second hide path this bullet forbids **and** a public-interface change, which WORKFLOW §3 makes
  a STOP condition. (prontezza B1, :33)
- **The transport, decided here so T3 does not decide it.** Hide is one `video_set_region`
  carrying a zero-area rectangle. Show is one `video_set_region` carrying the last measured
  rectangle. Geometry-before-visibility is already inside `apply_region` (`video/mod.rs:263-276`),
  which moves the surface only when the rectangle has area and then calls `settle`, so the
  two-message ordering rule revision 2 wrote is dead: there is one message and the backend orders
  it. N2's own spec says this is the path decision 1 will take
  (`e2e/specs/video-surface.spec.js:13-16`). (prontezza B1, :33)
- **The panel keeps its DOM size; the hide is a _reported_ rectangle, not a collapsed element.**
  `video-surface.spec.js:40-50` drives hide by setting `.stage__surface`'s `style.height` to 0
  because that is the cheapest lever for a test. A layer must not do that: collapsing the element
  makes the layout jump under the dialog. `VideoStage` reports `HIDDEN` while a layer is open and
  leaves the element alone. (prontezza B1, :33)
- **The layer count is an input to `VideoStage`'s `report()`, not a second caller of the command.**
  `VideoStage` already drives the region channel from four places — the ResizeObserver, the window
  resize listener and the mount, all three coalesced into one `report()` per frame
  (`VideoStage.tsx:44-53`), and the unmount, which hands `HIDDEN` straight to the same prop
  (`:61`) — so there is one `onRegionChange`, which `useVideoPlayer.ts:112-117` turns into the
  single `invoke("video_set_region")`. After the correction above, hide, show and
  measure all travel on that one channel. A layer effect reaching the command by a second route
  would race the ResizeObserver:
  any resize while a menu is open re-sends a real rectangle and the surface reappears over the
  menu. So `report()` sends `HIDDEN` when the layer set is non-empty and the measured rectangle
  when it is empty, and stays the single owner of the channel. Measuring stays on the
  ResizeObserver and the window resize listener, never on scroll (M0.2 constraint,
  `VideoStage.tsx:50-52`). (prontezza B1, :33)
- **The rectangle's unit is native device pixels.** N2c (`c7261a5`) moved the resolution into
  `report()`: it multiplies `getBoundingClientRect()` by `window.devicePixelRatio` and rounds the
  edges before invoking (`VideoStage.tsx:30-41`), and `src/types/video.ts:4-9` states the unit on
  the contract. Every criterion in this plan that compares the surface's X11 geometry with a DOM
  rect compares against the rect **resolved to native pixels**, never the CSS rect. Under Xvfb the
  ratio is 1, so the Linux suite cannot tell the two wordings apart; the discrimination lives on a
  scaled display. (prontezza S3, :234)
- A failed `video_set_region` surfaces on the existing video error line and is not retried. The
  shell keeps its own state; the next transition re-asserts it.

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
  `.transcribe__download-cancel`, `.transcribe__gpu`, `.transcribe__start`.
- On the status line, always visible whether the dialog is open or shut: progress, cancel, the
  cue preview, the backend a finished run used, and **every error a run produces**, the pre-flight
  checksum refusal included. Selectors `.status__transcribe`, `.status__transcribe-progress`,
  `.status__transcribe-cancel`, `.status__transcribe-error`, `.status__transcribe-cue` (with
  `-time` and `-text` leaves), `.status__transcribe-backend`, `.status__download`,
  `.status__download-progress`.
- **The backend label is an output, so it is on the status line and not in the dialog.** Revision 2
  put `.transcribe__backend` inside the dialog; `TranscribeBar.tsx:135-139` renders
  `.asrbar__backend` inside the status paragraph, after the run, and only when a result exists.
  Hiding a run's result in a container that closes when the run starts is precisely what this
  section's seam forbids. (prontezza minor "§2.3 line 182", :476)
- Clicking start closes the dialog, always, before the run is attempted. There is one error
  surface, on the status line, and no run outcome can land in a closed container. This is what
  makes `asr.spec.js`'s damaged-model check re-pointable without weakening it: the banner it waits
  for is on the status line, and the three assertions that follow reopen the dialog to read the
  model label, the download button and the disabled start.
- `Transcribe…` on the toolbar is **always enabled** and always opens the dialog. With no video
  open it is `.transcribe__start` inside that is disabled, and its accessible name carries the
  reason. The disabled half is what today's `.asrbar__start` does (`asr.spec.js:144` asserts
  `propertyOf(".asrbar__start", "disabled") === true`, and nothing else); **the reason is new
  production behaviour, not an existing one re-pointed.** `TranscribeBar.tsx:117-124` gives the
  button no `title` and no `aria-label`, so its accessible name today is its own label text, and
  `src/i18n/en.ts` has no key for a reason. So the string is decided here rather than by whoever
  implements it, otherwise the criterion asserts whatever the delivery invents and cannot fail:
  **`en.asr.noVideoReason` = `"Open a video first."`**, held by the spec constant
  `NO_VIDEO_REASON`. It is a new key, and T3 adds it. (prontezza S5, :267)

### 2.4 The close gate's route through the shell

`close-gate-check.js` has no WebDriver session and no DOM. Its route is frozen here so that four
tasks re-derive the same number instead of inventing four:

- **The gate keeps opening its file from argv, and no task gives it a chooser route.** Since
  `fee26f8` the fixture is a command-line argument that `startup_files` loads (`:140-147`), which
  is cheaper than any click, keeps the gate independent of the chooser T1 and T2 build, and is
  what WORKFLOW §4c asks for. Revision 2 froze a `TOOLBAR_OPEN_SUBTITLE` point and had T5b give
  the gate a toolbar open route; that is deleted. It would have been **new behaviour in the most
  data-safety-sensitive script in the battery**, bought for nothing. (prontezza B3, :96)
- `CUE_LIST_EMPTY`: **the one point**, replacing `FIRST_CUE_TEXT`. A point inside `.cuelist`
  **below the last row** — with the three-cue fixture that is roughly 130 px of empty space —
  followed by `Enter`, which opens the editor on the active row. §2.2 makes that click a no-op on
  both states, so the editor opens on row 1 whatever the click landed near. Until T3 replaces it,
  `FIRST_CUE_TEXT` stays and is re-derived by every task that moves the stack. (prontezza B3, :96)
- **T3 does not wait on T4 for this.** §2.2 is T4's, and T3 merges first, so the route has to stand
  on the grid as it is: `.cuelist` already carries `tabIndex={0}` and the keydown handler
  (`CueList.tsx:304-312`), the only pointer handlers are on a row and on a row's text (`:334`,
  `:351`), and `Enter` opens the editor on the active row (`:239-242`). What T4 adds is the
  contract, not the behaviour.
  (prontezza §5 "§2.4's replacement route", :539)
- The point is derived from `getBoundingClientRect()` in the layout the task leaves behind, at
  1024x700, **with no video open**, which is the state the close gate actually runs in, and
  resolved to native device pixels the same way §2.1 resolves the video rectangle — the gate
  clicks X11 coordinates, and CSS pixels are only the same thing where the ratio is 1. The number
  goes in the file with a one-line comment saying which task measured it. (prontezza S3, :234)
- From T5a the point lives in `e2e/lib/shell-points.js`, exported once, so `close-gate-check.js`,
  `n1b-load-probe.js` and `real-session-check.mjs` read one definition and the next shell change
  has one place to edit. Those last two are named because they carry shell coordinates of their own
  today; see T5a's and T5b's owned files. T8's `budget-check.js` reads it only if it ends up
  clicking at all: it loads its fixtures from argv, and the file is in its owned list for the case
  where the cold-start half still needs a point. (prontezza S13, :371; minor "T8's fixture
  loading", :486)
- Every task that owns the file carries the criterion `pnpm e2e:close-gate` passes with all of its
  checks and none of its assertions changed.

### 2.5 The harness instruments, and who builds them

Three criteria in the first draft had no instrument in this repo. One of the three now exists.

| instrument                                                | why nothing today can do it                                                                          | home                     |
| --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------ |
| find the app at a size other than 1024x700, and resize it | `findToplevel` matches exact geometry (`x11.js:74-88`); CI's Xvfb screen is 1280x1024 (`ci.yml:190`) | T1b                      |
| read the surface's colour over a rectangle                | **built.** `e2e/lib/pixels.js` exists and `wdio.conf.js:8,34` calls `requireFfmpeg()` every run      | N2, delivered            |
| "not covered by something we painted"                     | nothing reads text off the screen; the observable form is `elementFromPoint` hit-testing             | T1b helper, one function |

**The pixel instrument exists, and its unit is not what revision 2 called it.** N2 delivered
`e2e/lib/pixels.js`, whose measure is ffmpeg's **average saturation**, `SATAVG` (`:18,36`), chosen
because the empty stage is grey chrome spanning black to white and a luma range cannot tell it from
a picture. That is a different scale from `docs/reports/n2-probe.md`'s 0.3833/0.3850 pair, which was
a spread and is not comparable; the only recorded numbers on the delivered scale are **5.86 live
against 2.1 empty**, taken once, on real hardware (`BACKLOG.md` N2b). T1b does not build a second
one. (prontezza B5, :142)

**Saturation does not go into the mocha suite, and N2 already measured why.**
`video-surface.spec.js:9-12` records the refusal: "under Xvfb with llvmpipe the frame is presented
unreliably, measured at 2 appearances in 10 with mpv attached every time, which made this suite
intermittent for a reason unrelated to the code." So in CI "the picture is alive" is asserted the
way N2 settled it — **map state plus the presence of mpv's child window inside the surface** — and
the saturation measurement lives in the real-session check and the owner checklist, with its
platform written on it (CLAUDE.md §9). A pixel assertion inside CI is a new investigation, not a
criterion to hand an implementer. (prontezza B5, :142)

**The hit test answers HTML occlusion only.** `elementFromPoint` cannot see the native surface: it
is an X11 child window and not a DOM node, so no hit-test clause can fail for the occlusion T3
exists to prevent. The clause is worth keeping — it catches a dialog control painted under
something else — but the native surface is answered by map state and by nothing else. (prontezza
minor "T3's hit-test clause", :469)

---

## 3. Selector map

Assertions do not change. Where an element moves or is renamed, the selector is re-pointed and
nothing else. This table is the whole re-point job **for every name an assertion depends on**; an
implementer who needs a selector not listed here has changed something that was not asked for.

Three names T3 necessarily touches are deliberately absent because nothing asserts on them:
`.asrbar__phase`, `.asrbar__gpu-label` and `.asrbar__cues` (`TranscribeBar.tsx:107,136,160`). They
move with their elements under the §2.3 seam and need no re-point. Revision 2 called the table "the
whole re-point job" without that qualifier, which made their absence look like an omission.
(prontezza minor "§3's table", :479)

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

| today                                                                                                                                                | after                                                                                                                                                           | task |
| ---------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| `.subbar__savefile`                                                                                                                                  | `.toolbar__save`                                                                                                                                                | T5b  |
| `.subbar__undo`                                                                                                                                      | `.toolbar__undo`                                                                                                                                                | T5b  |
| `.subbar__redo`                                                                                                                                      | `.toolbar__redo`                                                                                                                                                | T5b  |
| `.subbar__discard`                                                                                                                                   | `.status__discard`                                                                                                                                              | T5b  |
| `.subbar__status`                                                                                                                                    | `.status__document`                                                                                                                                             | T5b  |
| `.subbar__dirty`                                                                                                                                     | `.status__dirty`                                                                                                                                                | T5b  |
| `.subbar__truncated`                                                                                                                                 | `.status__truncated`                                                                                                                                            | T5b  |
| `.subbar__error`                                                                                                                                     | `.status__document-error`                                                                                                                                       | T5b  |
| `.app__error`                                                                                                                                        | `.status__video-error`                                                                                                                                          | T5b  |
| `.asrbar__model`, `__download`, `__download-cancel`, `__gpu`, `__start`                                                                              | `.transcribe__…` (same leaf), inside the dialog                                                                                                                 | T3   |
| `.asrbar__status`, `__progress`, `__cancel`, `__error`, `__cue`, `__cue-time`, `__cue-text`, `__backend`, `__download-progress`, `__download-status` | `.status__transcribe`, `…-progress`, `…-cancel`, `…-error`, `…-cue`, `…-cue-time`, `…-cue-text`, `…-backend`, `.status__download-progress`, `.status__download` | T3   |

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

**The baseline is 33, not 27.** `e2e/wdio.conf.js:17` reads `const EXPECTED_TESTS = 33`, and
counting `it(` across the eight spec files gives exactly 33: asr 5, editor 10, project 5, subtitle
3, title 2, video 2, video-empty 3, video-surface 3. Revision 2's chain started at 27 and every
guard line in it was six low, three of them at or below the number already on `main` — a T1
implementer following its own line would have written 29 into the config and **silently disarmed
the guard for the six checks N2 added**. (prontezza B2, :80)

Running total of `EXPECTED_TESTS` (mocha specs only; the standalone node scripts carry their own
counters): 33 → T1 35 → T1b 36 → T2 39 → T3 43 → T4 47 → T5a 50 → T5b 50 → T6 50 → T7 54 →
T8 55. `close-gate-check.js` goes from 12 checks to 13 in T7. `budget-check.js` arrives in T8 with
2 of its own. `shutdown-check.js` keeps its 5 throughout and is touched by nobody, and so do
`wayland-attach-check.js` and `scaled-surface-check.js`, which launch from argv and click nothing.

**Every acceptance criterion below carries the instrument that observes it**, in brackets at the
end: `[new]` for a check this task writes, `[existing: file "name"]` for a check that already
exists and keeps its assertions, `[close gate]`, `[probe]` for a measurement recorded in a report,
and `[owner checklist]` for anything only a person can see. Anything tagged `[owner checklist]` is
stated as unautomated in the M2.0 status (CLAUDE.md §9).

**The `[owner checklist]` tag is used, not merely defined.** Revision 2 defined it here and put it
on no criterion, which would have let the M2.0 status present the whole shell as machine-verified
when its appearance had been checked by nothing: no automation can say whether the arrangement
reads as one tool rather than merely satisfying five rect inequalities, whether the dark palette
is right, or whether the rail matches the mockup. T5a and T6 each carry an appearance criterion
tagged `[owner checklist]`, and both are listed as unautomated in the M2.0 status. (prontezza S8,
:307)

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

**Depends on.** No other task, but **gate 2 is its merge predecessor**. The owner's 2026-08-30
ruling is N2c, then gate 2, then M2.0 starting at T1 (`BACKLOG.md:74`), and WORKFLOW §4a freezes
merges of new code while a gate is open. T1 is new harness code: it may be written during the
freeze, it merges when the gate opens. (prontezza S2, :227)

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
project checks are untouched: not re-pointed, not re-ordered, not re-worded. Guard 33 → 35.
(prontezza B2, :80)

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

**Delivery.** The two instruments of §2.5 that are still owed — the resize round trip and the
hit-test helper. The third, the pixel measurement, was delivered by N2 and is not rebuilt.
**No production code changes at all.** (prontezza B5, :142)

**Files owned.** `e2e/lib/x11.js`, `e2e/lib/paths.js`, `e2e/lib/dom.js` (new, the hit-test
helper), `e2e/specs/title.spec.js`, `.github/workflows/ci.yml`, `e2e/wdio.conf.js` (count),
`e2e/README.md`, `docs/reports/m2-0-harness-probe.md` (new). `e2e/lib/pixels.js` is **not** new and
is not owned here. (prontezza B5, :142)

**Depends on.** No other task, and **gate 2 is its merge predecessor**, for the same reason as T1:
new harness code, written during the freeze if useful, merged when the gate opens (WORKFLOW §4a,
`BACKLOG.md:74`). Runs immediately after T1, or beside it if two implementers are free: their owned
files are disjoint. (prontezza S2, :227)

**What changes.**

- The Xvfb screen goes to at least 1920x1080 on all three E2E jobs (`e2e`, `e2e:shutdown`,
  `e2e:close-gate`), and the comment at `ci.yml:187` is rewritten to say why the number moved.
- `findToplevel` matches the title plus "the geometry is one of the sizes the suite drives",
  instead of one exact size. **The duplicate-toplevel guard at `x11.js:80-86` stays exactly as it
  is**: it is the leftover-instance guard, and widening the match makes it more necessary, not
  less. `paths.js` exports the set of driven sizes; the 1024x700 default (`paths.js:22-23`) is
  unchanged and remains the frozen contract `e2e/README.md` describes.
- `resizeWindow(id, w, h)` wraps `xdotool windowsize`. Every check that resizes restores 1024x700
  before it returns, and asserts the restore, so nothing downstream inherits a window it was not
  written for.
- `pixels.js` is left alone. It exists, it measures `SATAVG` over a rectangle with ffmpeg, and
  `wdio.conf.js:34` already requires the binary on every run. The mpv-child precondition it needs
  before a number means anything lives **in the callers**, where N2 put it, and stays there:
  `pixels.js:36-87` has no such precondition and T1b does not add one, because the caller is the
  only party that knows which surface it is asking about. (prontezza B5, :142)
- `dom.js` exports one function: given a selector, return whether `document.elementFromPoint` at
  the centre of its rect resolves to that element or a descendant of it. It answers HTML occlusion
  only; §2.5 says why it cannot answer for the native surface.

**Acceptance criteria.**

- The app toplevel is found at 1024x700, resized to 1920x1080, found again there, resized back,
  and found again at 1024x700. `[new: title.spec.js]`
- The `SATAVG` measurement of `pixels.js` is exercised over a playing video and the number is
  recorded with its platform, beside the empty-shell number, so the plan carries a second sample of
  the 5.86-against-2.1 pair instead of one. **This is a probe, not a check**: no saturation
  assertion enters the mocha suite, for the reason N2 measured and `video-surface.spec.js:9-12`
  records. `[probe: m2-0-harness-probe.md]` (prontezza B5, :142)
- CI's three E2E jobs run on a screen of at least 1920x1080 and the existing suite is green on it,
  unchanged. `[existing: all 35]`

**The two-toplevel clause is dropped, deliberately.** Revision 2's first criterion ended "the
two-toplevel guard still throws when a second instance is present", and named nothing that produces
a second toplevel: the harness owns exactly one app instance under the driver, and no check was
counted against `EXPECTED_TESTS` for it. The cheapest way to satisfy such a clause is to test the
parser instead of the situation, and nothing would catch its quiet removal. The guard itself stays,
for the reason given above; what is dropped is the clause that claimed to exercise it. T1b's
instrument is the resize round trip and that is what it owes. (prontezza S18, :436)

**E2E.** One new check in `title.spec.js`, the resize round trip. Guard 35 → 36. (prontezza B2,
:80)

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

**Files owned.** `src-tauri/src/dialog.rs` (**existing**, 156 lines, added by `fee26f8`),
`src-tauri/src/project/mod.rs` (the picker moves out), `src-tauri/src/lib.rs`,
`src-tauri/src/strings.rs`, `src/hooks/useProject.ts`, `src/components/VideoOpenBar.tsx`,
`src/components/SubtitleBar.tsx`, `src/App.tsx` (the four chooser handlers are threaded as props
from here), `src/i18n/en.ts`, `src/App.css` (the `Choose…` button styling only; T4 owns the row
classes), `e2e/specs/video.spec.js`, `e2e/specs/subtitle.spec.js`,
`e2e/scripts/close-gate-check.js`, `e2e/wdio.conf.js`.

**`dialog.rs` exists, and it is gate 2's named lens.** Revision 2 declared it new. It holds the
close gate's three GTK message dialogs on the main thread, and its own module doc (`:1-11`) says
Linux moved **off** rfd because "the plugin uses rfd, which starts a second thread the first time
any dialog is shown and iterates GTK on it for the rest of the process's life, which GTK3 is not
built for". `WORKFLOW.md:55` names the close path in that file as gate 2's one pre-declared review
lens and CLAUDE.md §3 puts it under data safety. T2 **adds the four choosers beside `ask_close`
and changes nothing that `ask_close` touches**, or it takes its own module; either is acceptable,
dropping an rfd-driven chooser into that file without saying so is not. (prontezza B6, :169)

**N1c is the ordering question this task cannot leave open.** `BACKLOG.md:114` files N1c against
exactly the code T2 generalises: `project/mod.rs:244-257` still calls `blocking_pick_folder` and
`blocking_pick_file` through the plugin, so T2 would turn one plugin call site into four. Whichever
of the two lands second pays for the first. **The order goes to the owner before T2 starts**, and
it is one of two: N1c runs first and T2 inherits a GTK-direct picker, in which case T1's by-title
lookup is re-validated against the new picker before T2 begins; or T2 builds the four choosers on
GTK directly and closes N1c in the same delivery. Leaving it undecided silently doubles one of the
two tasks. T1's whole identification strategy assumes an rfd/GTK3 chooser toplevel, and T2's "zero
production change" framing for T1 carries the N1c caveat either way. (prontezza B6, :169)

**Depends on.** T1 (its checks use the helper). T1b is not a blocker for T2 but lands before it in
the queue. The N1c ordering above is a precondition, not a dependency.

**Public interface change, called out per CLAUDE.md §6:** the IPC command `project_choose_path` is
replaced by `choose_path`, and its single consumer (`useProject.ts`) is updated in the same
delivery. No other caller exists; verified by grep across `src/` and `src-tauri/src/`.

**Gate 4 applies to this task's merge.** WORKFLOW §4a requires a gate before **any** merge that
touches saving, subtitle formats or the open-core boundary, "whatever the regime, whatever the
schedule". T2 adds the save-mode chooser and the save destination route, so it merges behind a
gate-4 review and not on a green battery alone. (prontezza S16, :412)

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
  makes `.subbar__status` read the `LF_STATUS` literal the existing check asserts —
  `"SRT · 3 cues · LF"` (`subtitle.spec.js:16`, asserted with `toBe` at `:129`), **not**
  `STATUS_PREFIX`, which is `editor.spec.js:44`'s `"SRT · 2000 cues · LF"` and belongs to the
  2,000-cue fixture. `[new: subtitle.spec.js]` (prontezza S4, :254)
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
- `pnpm e2e:close-gate` passes 12/12 with its assertions unchanged; its one point,
  `FIRST_CUE_TEXT` (`close-gate-check.js:42-43`), is re-derived if this task moved it. There is no
  second point: `fee26f8` took the other two away with the typed open. `[close gate]`
  (prontezza B3, :96)

Where the titles are read from is a review rule, not a criterion: §6's first bullet already makes
a JSX or Rust literal a rejection, and no observation of the running app can tell an inlined
string from the same string in `strings.rs`.

**E2E.** New checks in `video.spec.js` and `subtitle.spec.js`, three in total. Existing assertions
in both files unchanged. Guard 36 → 39. (prontezza B2, :80)

**Parallel, and what the pair actually shares.** T2 may run alongside T4, but revision 2's "two
shared one-liners" was wrong twice. First, **both lists contain `src/App.css` outright** — T2 for
the `Choose…` button styling, T4 for splitting the row classes — so the file is split by hand:
T2 owns the chooser button rules, T4 owns `.cuelist__row--active` and `.cuelist__row--selected`,
and neither touches the other's block. Second, **T2 cannot be built without `src/App.tsx`**: no
component in this repo calls `invoke` (every `@tauri-apps/api` import is in `src/hooks/*.ts`) and
the existing chooser reaches its button as a prop, `onChoosePath={project.choosePath}` at
`App.tsx:47`. Adding `Choose…` to `VideoOpenBar` and `SubtitleBar` the repo's own way means new
props threaded through `App.tsx`, which is also T4's biggest structural edit. So **T2 does the
`App.tsx` wiring for the four choosers first and the orchestrator freezes it before T4 starts**;
if that freeze cannot hold, the pair is serialized and the parallel claim is dropped. The genuine
one-liners are `en.ts` (T4 adds no user-facing copy; if it turns out to need a string the pair is
serialized) and the `EXPECTED_TESTS` integer in `e2e/wdio.conf.js`, which both tasks bump. The
orchestrator merges T2 first, sets the merged number, and the second delivery re-runs the guard
against it (WORKFLOW §5). (prontezza S6, :281)

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
`src/types/video.ts` (the region contract, so the unit §2.1 freezes has an owner),
`src/hooks/useVideoPlayer.ts`, `src/App.tsx`, `src/App.css`, `src/i18n/en.ts`,
`e2e/specs/asr.spec.js`, `e2e/specs/video.spec.js`, **`e2e/specs/video-surface.spec.js`**,
**`e2e/specs/video-empty.spec.js`**, `e2e/scripts/close-gate-check.js`, `e2e/README.md` (the names
this task renames), `e2e/wdio.conf.js`.

**The two N2 specs get a written owner, and it is this task.** `video-surface.spec.js` (3 checks)
and `video-empty.spec.js` (3 checks) were added by N2 and appeared in no Files-owned list in
revision 2, which put T3 in the §6 BLOCKED situation by construction: they are the only behavioural
coverage the native surface has, and `video-surface.spec.js:40-50` drives hide and show through the
same lever T3 rewires. **Updating them is part of this delivery, not a discovery inside it.**
(prontezza B4, :122; owner ruling 2026-08-30 naming T3 and T5b as the owners.)

**`setStageCollapsed` survives, and the two levers cannot mask each other.** Under §2.1's corrected
contract `report()` sends `HIDDEN` when the layer set is non-empty and the measured rectangle when
it is empty, so collapsing `.stage__surface` to `height: 0` still produces a zero-area rectangle and
still hides the surface. The two levers meet at exactly one place — the reported rectangle has no
area — and the spec's own assertion that the DOM really collapsed (`:52-60`) keeps a failed collapse
from reading as a successful hide. (prontezza B4, :122)

**Depends on.** N2 (`d224f3c`, merged and `[x]` in `BACKLOG.md:84`) and N2c (`c7261a5`, merged),
which together settle the transport and the unit §2.1 freezes. T1 and T1b. It merges after T2
because both touch the app composition. There is no order-if-N2-slips: N2 landed. (prontezza S1,
:214; S3, :234)

**Opens by building §2.1's contract, not by reading a signature.** Revision 2 told this task to
begin by reading N2's delivered visibility command and writing it into §2.1. There is no such
command and there must not be one: §2.1 now carries the delivered mechanism, decided in the plan.
An implementer who finds themselves adding `video_set_visible` has built the second hide path both
§2.1 and N2 forbid, and has made a public-interface change WORKFLOW §3 makes a STOP condition.
(prontezza B1, :33)

**Acceptance criteria.**

- At first paint there is no transcription band: `document.querySelector(".transcribe__model")` is
  null, and `.status__transcribe` reads the `IDLE_STATUS` literal. `[new: asr.spec.js]`
- `Transcribe…` is enabled with no video open and opens the dialog. Inside it,
  `.transcribe__start` is `disabled === true` and its accessible name contains the
  `NO_VIDEO_REASON` literal, whose value §2.3 fixes as `"Open a video first."` under the new key
  `en.asr.noVideoReason`. The disabled half is a re-point of `asr.spec.js:144`; the accessible name
  is new production behaviour this task adds, and the string predates the implementation so the
  criterion can fail. Open a video: it enables. `[existing: asr.spec.js "offers the models it knows
and a compute choice", plus new]` (prontezza S5, :267)
- With a video playing and the dialog open: the surface is `IsUnMapped`, and every control in the
  dialog passes the hit test — `elementFromPoint` at the centre of its rect returns it or a
  descendant, which answers HTML occlusion and, per §2.5, cannot answer for the native surface.
  Press Escape: the surface is `IsViewable` again; **mpv's child window is present inside it**; and
  its X11 geometry matches `.stage__surface`'s DOM rect **resolved to native device pixels**
  (`rect × window.devicePixelRatio`, the resolution N2c moved into `VideoStage.report()`) within
  the existing 2-native-pixel tolerance `video.spec.js:10,108-115` already uses. Map state plus the
  mpv child is the whole signal here; no saturation sample enters this check, for the reason §2.5
  gives. Under Xvfb the ratio is 1, so this run cannot discriminate the native-pixel wording from
  the CSS-pixel one — the discrimination is the owner's scaled display. `[new: video.spec.js]`
  (prontezza B5, :142; S3, :234)
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
  from the column, so its one remaining point, `FIRST_CUE_TEXT`, is replaced by §2.4's
  `CUE_LIST_EMPTY` plus `Enter`. The gate keeps opening its fixture from argv; it gains no chooser
  route here or anywhere in this milestone. `[close gate]` (prontezza B3, :96)
- **`n1b-load-probe.js` holds the second copy of that point and this task does not own it.** It
  carries its own `FIRST_CUE_TEXT` (`:33-34`) and T5a is where both copies fold into
  `e2e/lib/shell-points.js`. Nothing turns red in between — the probe is in no package script, no
  CI job and none of the three runners — but the N1b battery run between T3 and T5a is aiming at
  the old layout, and the delivery says so rather than leaving the next person to find it.
  (prontezza S13, :371)
- The six checks in `video-surface.spec.js` and `video-empty.spec.js` are green with their
  assertions unchanged, re-pointed only where §3's map moves a name. `[existing: video-surface.spec.js,
video-empty.spec.js]` (prontezza B4, :122)
- N1's open-editor gap is **still open** after this task, and the delivery says so: an inline editor
  holding uncommitted text still leaves the backend session clean and still closes without asking.
  §1 explains why closing it is not M2.0's, and the M2.0 status repeats it. `[owner checklist]`
  (prontezza S17, :424)

**E2E re-point, spelled out because this is where a re-point could quietly become a rewrite.**
`asr.spec.js`'s `before` waits on `.status__transcribe` — a positive signal present at first paint
— and every check that reaches the model list, the GPU tick or the start button opens the dialog
first. **Adding a step to reach a control is not weakening an assertion**; every expectation in
those five checks stays exactly as written: the `>= 10` model options, `tiny.en` present and
selected, the "ready" label, the absent download button, the ticked GPU box, the disabled start,
the `IDLE_STATUS` line, the byte-level media-untouched comparisons, the `-ng` command line, the
`"checksum"` banner and the leaves-nothing-running reap. Three of the five now open or reopen the
dialog to reach an input; none of them asserts anything new and none asserts anything less.

Four new checks in `video.spec.js` and `asr.spec.js`. Guard 39 → 43. (prontezza B2, :80)

---

### T4 — Active line and selection become two states

**Delivery.** The single `selected` index in `CueList.tsx:85` becomes the two states of §2.2, owned
by a hook beside the document state, drawn by the grid, driven by keyboard and mouse, and remapped
by `applyPatch`.

**Files owned.** `src/hooks/useSelection.ts` (new), `src/hooks/useSubtitleFile.ts`,
`src/components/CueList.tsx`, `src/App.tsx`, `src/App.css`, `e2e/specs/editor.spec.js`,
`e2e/wdio.conf.js`.

**Depends on.** Nothing structural. Merges after T3 because both touch `src/App.tsx`; may be
implemented in parallel with T2 under the file split T2 describes. Revision 2's "may be run before
T3 if N2 has not landed" is deleted: N2 landed as `d224f3c` and `BACKLOG.md:84` marks it `[x]`.
(prontezza S1, :214)

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
  it. **Ctrl+A**: with the cursor on row 14 beforehand, at the top of the file, at mid-file and at
  the bottom, every rendered `.cuelist__row` carries the membership class, the rendered row count
  stays at or under 60 at each of those three positions, and row 14 still carries the cursor.
  "Marks all 2,000 rows" is not observable: `CueList.tsx:17-19` sets `ROW_HEIGHT = 28` and
  `OVERSCAN = 8`, `:313-320` renders only `indices`, so at most about 60 rows exist in the DOM at
  any scroll position and no assertion can count 2,000 of them. `[new: editor.spec.js]` (prontezza
  B7, :192)
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
- With rows 10 to 14 marked and the cursor on row 14, the computed style of row 14 differs from
  that of rows 10 to 13 in at least one named property — outline or border against background —
  and neither equals an unmarked row's. Every other criterion here is about which class a row
  carries, and an implementation that gives `.cuelist__row--active` and `.cuelist__row--selected`
  identical CSS satisfies all of them while leaving the user unable to tell the cursor from the
  selection, which is the whole of decision 5 from the user's side.
  `docs/design/shell-layout.md:219` states the requirement: "the cursor is drawn as an outline on
  the row and membership as the filled row, and the two are visibly different states rather than
  one style used twice". Today one class does both jobs (`CueList.tsx:318-321`). `[new, folded into
the mouse check]` (prontezza S9, :320)
- Ctrl+A over 2,000 rows, then scroll to the bottom: scrolling stays inside the budget the
  existing scroll check measures. The existing virtualization check
  (`editor.spec.js:231-263`) is **not** leaned on for this: it performs three scrolls and asserts
  `sample.rows <= 60`, presses no key at all, and so cannot fail because of a selection; whether it
  even runs with a select-all in effect depends on the order mocha happens to run the file's `it`
  blocks, which this plan does not fix. The row-count assertion that guards against a selection
  rendering the whole file lives in the Ctrl+A criterion above, inside the new check.
  `[existing: editor.spec.js "scrolls a viewport at a time without falling behind"]` (prontezza B7,
  :192)

**E2E.** Four new checks in `editor.spec.js`, grouping the criteria above: the keyboard building
both states; the mouse building both states; a scattered selection surviving an edit; undo far
from the viewport bringing the row on screen. The ten existing checks keep their assertions; three
of them click once on a row's text and then wait for `.cuelist__editor` (lines 327, 511, 549), and
§2.2 keeps that gesture working, so they are not touched at all. Guard 43 → 47. (prontezza B2, :80)

**Known gap, stated rather than papered over.** The remap branch for a row that a patch _removes_
cannot be driven from the UI at M2.0: there is no delete-line or insert-line gesture in the grid,
and `package.json` has no TypeScript test runner, so a unit test of the remap function would mean
a new dependency and an owner decision (CLAUDE.md §8). The branch is written and reviewed against
the arithmetic in `shell-layout.md`; the behavioural check for it belongs to the M2.5 task that
adds a delete gesture, and that task must bring it. Recorded as untested, not as done.

---

### T5a — The frame

**Delivery.** The arrangement `shell-layout.md` specifies, with everything that exists today
re-parented into it and nothing removed yet: project rail on the left; a top band with the video
panel on the left and the top-right column beside it; the cue grid across the whole width
underneath; a status line at the bottom. The colour tokens and the dark-first palette land here,
in one place, because this task rewrites `App.css` anyway.

The **two** remaining bars — `VideoOpenBar` and `SubtitleBar`; the transcription band went in T3 —
are parked unchanged, with their class names intact, in one strip above the top band. They look
temporary because they are; every open route and every existing selector keeps working, so this
delivery re-points nothing.

The top-right column holds the current-line band: the active row's start, end and duration, and
its text, read-only. That is the place M2.5 fills with editable times and M2.6 with the
two-document editor. The waveform panel is not created: its provider does not exist yet.

**Files owned.** `src/App.tsx`, `src/App.css`, `src/components/Shell.tsx` (new),
`src/components/EditBox.tsx` (new), `src/components/VideoPanel.tsx` (new, wrapping the existing
stage and controls), `src/hooks/useStartupFiles.ts`, `src/i18n/en.ts`, `e2e/specs/video.spec.js`,
`e2e/scripts/close-gate-check.js`, `e2e/scripts/real-session-check.mjs`,
`e2e/scripts/n1b-load-probe.js`, `e2e/lib/shell-points.js` (new), `e2e/wdio.conf.js`.

**Two shell-coordinate scripts join the owned list, because they hold points nobody owned.**
`real-session-check.mjs:52-56` clicks the video path field and open button as fractions of the
1024x700 layout (`videoField: 683/1024`, `videoOpen: 978/1024`), and it is the one script
WORKFLOW §4c blesses for checks on the owner's real display. `n1b-load-probe.js:33-34` carries its
own copy of `FIRST_CUE_TEXT = {750, 540}` under the comment "M2.0 must revisit this", and it is the
script N1b's closing criterion is written in. Both are re-pointed here and both read
`e2e/lib/shell-points.js` afterwards, so the next shell change has one place to edit.
`wayland-attach-check.js` and `scaled-surface-check.js` are **not** affected and are stated as such:
they launch from argv and click nothing. (prontezza S13, :371)

**`useStartupFiles` is named so the rewrite does not drop it.** It appears nowhere in revision 2,
and it is the only route by which the close gate (`close-gate-check.js:140-147`) and the real-session
check reach a loaded document; WORKFLOW §4c makes it the rule for the owner's display. It is wired
at `src/App.tsx:23` as `useStartupFiles(open, subtitle.open)`. If a composition-root rewrite drops
it, the close gate fails as a 3,500 ms wait landing on an empty grid, which reads as a timing flake
rather than a deleted feature. (prontezza S14, :389)

**Depends on.** T4 (the current-line band follows the active line), T1b (both window sizes), and
N2c (`c7261a5`) for the unit the geometry criteria compare in. (prontezza S3, :234)

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
  direction; **an enumerated element list** satisfies `scrollWidth <= clientWidth + 1` — at T5a
  that list is `.bar__input`, `.bar__button`, `.subbar__input`, `.subbar__open`, `.subbar__dest`,
  `.subbar__save`, `.subbar__savefile`, `.subbar__undo`, `.subbar__redo`, `.subbar__discard`,
  `.subbar__status`, `.editbox__start`, `.editbox__end`, `.editbox__duration`, `.editbox__source`
  and `.status__transcribe`, which is the only status-line element mounted this early: the document
  half of that line is `.subbar__status` until T5b renames it to `.status__document` (§3), and
  naming a selector that does not exist yet is a criterion that cannot fail. Every one of those
  rects also lies inside the viewport. Revision 2 said
  "every element carrying a visible label", which left the element set to the implementer, so the
  narrowest defensible choice passed trivially and the criterion's strength was decided after the
  fact by the person it constrains. The save-copy-as control this milestone regressed on is
  **T5b's**, not T5a's: at T5a there is no toolbar — the two bars are parked unchanged and the
  toolbar arrives in T5b — so that named control moves to T5b's copy of this criterion.
  `[new: video.spec.js]` (prontezza S12, :358)
- **The shell matches the mockup**, setting aside everything `docs/design/shell-mockup.html`
  marks for a later milestone: the rail, video, top-right column, grid and status line sit where
  and in the proportions it draws them, with the top-right column given wholly to the current-line
  band, because the waveform the mockup stacks above that band arrives at M2.4. The dark-first
  palette reads as intended too, in the built app at both window sizes. No automation can answer
  either: the rect inequalities above can all hold over a layout that looks nothing like the
  picture. `[owner checklist]` (prontezza S8, :307)
- With a video playing, scroll the wheel at four points: over the video panel, over the toolbar
  strip, over the status line, and inside the grid scrolled to row 1500; then scroll the rail to
  its last episode. The surface's X11 geometry is byte-identical before and after all five, and
  the frame is still advancing. The only regions that scroll are the grid and the rail. `[new:
video.spec.js]`
- Resize from 1024x700 to 1920x1080 with a video playing and back again: the surface tracks
  `.stage__surface`'s rect **resolved to native device pixels** at both sizes, within the existing
  2-native-pixel tolerance. Under Xvfb the ratio is 1 and this run cannot tell that wording from
  the CSS-pixel one; the discrimination is the owner's scaled display. `[new, folded into the
clipping check]` (prontezza S3, :234)
- The six checks in `video-surface.spec.js` and `video-empty.spec.js` stay green with their
  assertions unchanged. T5a does not own them — T3 and T5b do — and it is expected to leave them
  alone: `.stage__surface` and `.stage__empty` keep their names (§3), `.bar__input` and
  `.bar__button` are still mounted because the two bars are parked, and `VideoStage.tsx` is not in
  this task's file list. If either spec goes red here, that is the §6 BLOCKED situation and a
  defect in this plan. `[existing: video-surface.spec.js, video-empty.spec.js]` (prontezza B4,
  :122)
- `pnpm e2e:close-gate` passes 12/12 with its assertions unchanged. This task moves the one point
  it clicks: it is re-derived at 1024x700 with no video open, in native device pixels, and moves
  into `e2e/lib/shell-points.js` as the single definition `n1b-load-probe.js`,
  `real-session-check.mjs` and T8's `budget-check.js` all read. `[close gate]` (prontezza B3, :96;
  S13, :371)

The vertical-overflow half of the clipping criterion is not decoration. `VideoStage` recomputes
the region on the ResizeObserver and on `window resize` (`VideoStage.tsx:50-52`, moved by N2c from
the `:41-43` revision 2 cited); a vertical document scroll changes `getBoundingClientRect().top`
without changing any element's size, so
neither callback fires and the native surface stays where it was while the panel moves under it.
That is precisely the M0.2 failure §6 forbids, and no criterion in the first draft measured it.

**E2E.** Three new checks in `video.spec.js`: the arrangement rects; no clipping and no document
scroll at both window sizes; the wheel and scroll safety at five points. The existing
surface-geometry check already proves the frame tracks the panel and is not modified.
Guard 47 → 50. (prontezza B2, :80)

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
`src/App.tsx`, `src/components/Shell.tsx`, `src/hooks/useStartupFiles.ts`, `src/App.css`,
`src/i18n/en.ts`, `e2e/specs/video.spec.js`, `e2e/specs/subtitle.spec.js`,
`e2e/specs/editor.spec.js`, `e2e/specs/asr.spec.js`, **`e2e/specs/video-surface.spec.js`**,
**`e2e/specs/video-empty.spec.js`**, `e2e/scripts/close-gate-check.js`,
`e2e/scripts/real-session-check.mjs`, `e2e/lib/shell-points.js`, `e2e/README.md` (the names this
task deletes and renames), `e2e/wdio.conf.js`.

**The two N2 specs are this task's second written owner.** Both reach the app through the controls
T5b deletes: `video-empty.spec.js:60,99,102,110` and `video-surface.spec.js:116,122,130` use
`.bar__input` and `.bar__button`, which §3 lists under "Gone, gesture replaced". Revision 2 owned
neither file, so T5b blocked by construction under §6. **Re-pointing them onto the toolbar is part
of this delivery**, and their readiness gate becomes `.toolbar__open-video` — the control this file
drives — in place of `.bar__input`. (prontezza B4, :122; owner ruling 2026-08-30.)

**`useStartupFiles` and `real-session-check.mjs` come with the deletions.** The three workspace path
boxes are the only thing `real-session-check.mjs:52-56` clicks, so this task re-points it onto the
toolbar through `e2e/lib/shell-points.js` or it is left driving controls that no longer exist. And
`useStartupFiles` stays wired through the rewritten composition root, because it is how the close
gate and the real-session check reach a loaded document (WORKFLOW §4c). (prontezza S13, :371; S14,
:389)

**Depends on.** T1, T2, T5a.

**Gate 4 applies to this task's merge.** T5b re-points save, save-as, dirty, truncated and discard
onto the toolbar and status line, including the byte-comparison save checks, which puts it squarely
inside WORKFLOW §4a's "any merge that touches saving". It merges behind a gate-4 review, not on a
green battery alone. (prontezza S16, :412)

**Acceptance criteria.**

- With no cue editor open,
  `document.querySelectorAll('.shell input, .shell textarea, .shell [contenteditable], .shell [role="textbox"]')`
  returns nothing outside `.rail`. Opening the editor on a row adds exactly one and closing it
  removes it again. The rail still holds one text input, the episode title, which is not a path
  field; T6 keeps it. The selector list is widened from revision 2's `input, textarea` because the
  BACKLOG AC is the wider claim, "no field for typing a path is left anywhere in the interface",
  and a `contenteditable` div would satisfy the narrow one while breaking the AC.
  `[new assertion inside an existing check: editor.spec.js]` (prontezza minor "T5b criterion 1 and
  T6 criterion 1", :466)
- Click `.toolbar__open-video`, pick `fixtures/video/sample.mkv`: `.stage__empty` is gone,
  `.controls__button` is enabled, and the transport time advances past `0:01`.
  `[existing: video.spec.js "opens the sample fixture", re-pointed]`
- Click `.toolbar__open-subtitle`, pick `basic-lf.srt`: `.status__document` reads the `LF_STATUS`
  literal, `"SRT · 3 cues · LF"` (`subtitle.spec.js:16`), not `STATUS_PREFIX`, which belongs to the
  2,000-cue fixture. `[existing: subtitle.spec.js "opens an SRT fixture and shows its format and cue
count", re-pointed]` (prontezza S4, :254)
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
- At 1024x700 and again at 1920x1080, with everything open and in its tallest state, the toolbar's
  controls — `.toolbar__open-video`, `.toolbar__open-subtitle`, `.toolbar__save`,
  `.toolbar__save-as`, `.toolbar__undo`, `.toolbar__redo`, `.toolbar__transcribe` — and the status
  line's `.status__document`, `.status__dirty`, `.status__truncated`, `.status__discard` each
  satisfy `scrollWidth <= clientWidth + 1` and lie inside the viewport, and the document scrolls in
  neither direction. **`.toolbar__save-as` is the control this milestone regressed on, so it is
  named in the failure message.** T5a carries the same criterion over the parked bars; the named
  control lives here because the toolbar does, and this is that check's element list re-pointed,
  not a new check. `[existing: video.spec.js, T5a's clipping check, re-pointed]` (prontezza S12,
  :358)
- `pnpm e2e:close-gate` passes 12/12 with its assertions unchanged. **The gate's open route does
  not change**: it keeps loading its fixture from argv through `startup_files`, so no
  `TOOLBAR_OPEN_SUBTITLE` point is introduced and the T1 helper is not wired into it. Only
  `CUE_LIST_EMPTY` is re-derived, in `e2e/lib/shell-points.js`, against the layout this task leaves
  behind. `[close gate]` (prontezza B3, :96)
- The six checks in `video-surface.spec.js` and `video-empty.spec.js` pass with their assertions
  unchanged, re-pointed off `.bar__input` and `.bar__button` onto the toolbar. `[existing:
video-surface.spec.js, video-empty.spec.js]` (prontezza B4, :122)
- `__subloreClickAt` moves to immediately before the confirm keystroke in the chooser and
  `expect(elapsed).toBeLessThan(1000)` still passes. This was a paragraph in revision 2 and no
  criterion, so nothing in the done-list held it. `[existing: editor.spec.js, the open budget]`
  (prontezza minor "T5b's Open budget", :489)

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
later instead of a clean timeout. **No assertion in the 50 is weakened, skipped, retargeted or
re-worded**; the expected strings, the byte comparisons, the budget numbers and the process checks
are the same literals they are today. Guard stays at 50. (prontezza B2, :80)

`close-gate-check.js`: **the N1 debt is smaller than revision 2 thought, and differently shaped.**
The 1,500 ms fixed wait after opening the file no longer exists — it went with the typed open in
`fee26f8`, and no chooser is involved in the gate any more, so there is no chooser toplevel to wait
on. The waits that are actually there are `sleep(3500)` at `:180` (webview paint plus the argv
parse), `sleep(600)` at `:186` and `sleep(2500)` at `:191`. **The 3,500 ms setup wait is the one
worth attacking**, and this task states plainly whether a script with no DOM has anything to wait on
in its place: the toplevel exists long before the document is parsed, so the honest answer may be
"nothing observable, the wait stays fixed and stays named as fixed". Either answer is acceptable;
inventing a signal is not (CLAUDE.md §9). The 600 ms and 2,500 ms waits stay fixed for the reason
they always did. (prontezza B3, :96)

`shutdown-check.js`, `wayland-attach-check.js` and `scaled-surface-check.js` are not touched: none
drives a DOM, and all three launch from argv and click nothing. (prontezza S13, :371)

**Open budget, re-pointed not weakened, and now a criterion.** `editor.spec.js` stamps
`__subloreClickAt` immediately before the click on `.subbar__open` today. It moves to immediately
before the confirm keystroke in the file chooser, so the measured window becomes chooser teardown
plus IPC plus parse plus paint, and `expect(elapsed).toBeLessThan(1000)` is unchanged. The teardown
cost was measured in T1's report; if it turns out to eat the budget, that is a §7 regression to put
in front of the owner (WORKFLOW §3), never a reason to move the number. Revision 2 also called the
new window "strictly wider than today's": that depends on executing script in the page while a
native chooser is up, which nothing in this repo has ever done, so **T1's report answers whether
the stamp can be placed there at all** and this claim stands or falls on that answer. It is a
criterion above, not a paragraph, so the done-list holds it. (prontezza minor "T5b's Open budget",
:489)

---

### T6 — The project rail

**Delivery.** The sidebar becomes the rail from the mockup: the project caption, the episode rows,
the files under the selected episode. New project, open project and attach file each go through
the chooser. The rail's two path boxes go.

**Files owned.** `src/components/ProjectPanel.tsx` (becomes `Rail.tsx`),
`src/components/Shell.tsx`, `src/hooks/useProject.ts`, `src/App.css`, `src/i18n/en.ts`,
`e2e/specs/project.spec.js`, `e2e/README.md` (the rail names this task deletes),
`e2e/wdio.conf.js`.

**`Shell.tsx` joins the list because T6 cannot avoid it.** Revision 2 left it out of T6's files and
then said in §5 that T6 and T7 "both edit `Shell.tsx`". Both cannot be true: renaming
`ProjectPanel.tsx` to `Rail.tsx` means editing whatever renders it, and that is the shell. §5 now
runs the pair sequentially for this reason. (prontezza S7, :294)

**Depends on.** T2, T5a, T5b.

**Two constraints, both with a criterion.** `.project__new-episode` is not renamed and not removed
(§3). And the rail keeps a width that does not depend on its content, because the grid's left edge
is derived from it and `e2e/lib/shell-points.js` holds numbers measured against it.

**Acceptance criteria.**

- There is no box anywhere for typing a path:
  `document.querySelectorAll('.shell input, .shell textarea, .shell [contenteditable], .shell [role="textbox"]')`
  returns exactly one element with no cue editor open, and it is `.project__new-episode`, the
  episode title. Widened from revision 2's `input, textarea` for the same reason as T5b's.
  `[new assertion inside an existing check: project.spec.js]` (prontezza minor "T5b criterion 1 and
  T6 criterion 1", :466)
- At 1024x700 and at 1920x1080, and with a project whose name is 80 characters long, `.rail`'s DOM
  rect has the same width in all three states. `[new assertion inside an existing check:
project.spec.js]`
- **The rail matches the mockup** in the built app: the project caption, the episode rows and the
  files under the selected episode read as `docs/design/shell-mockup.html` draws them. The width
  and rect criteria above can all hold over a rail nobody would recognise, so this half is a
  person's. `[owner checklist]` (prontezza S8, :307)
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
their assertions intact. Guard stays at 50. (prontezza B2, :80)

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
  `.stage__surface` rect **resolved to native device pixels**, with mpv's child window present
  inside it. No saturation sample enters this check: §2.5 records why the pixels stay out of the
  mocha suite, and the picture is verified on the owner's display instead, with its platform
  written on it. `[new: video.spec.js]` (prontezza B5, :142; S3, :234)
- File, then Transcribe. Sample the surface's map state every 50 ms from the click until the
  dialog is on screen: **at least 5 samples are taken**, the count is asserted alongside the
  values, and every sample reads `IsUnMapped`. A check that asserts only "every sample read
  `IsUnMapped`" passes vacuously when the dialog arrives in under 50 ms and the loop takes one
  sample or none, which is the same defect gate 1 found inside N2 (`BACKLOG.md:88`: "the 'clock is
  frozen' assertion could not fail because `waitFor` returns on its first evaluation"). If the
  transition is too fast to sample, the check fails as **did not sample**, never passes. Close the
  dialog: the surface is `IsViewable` over the panel rectangle. Even with the count asserted this
  is a sampled negative at 50 ms and not a proof that no frame was ever mapped, and the M2.0 status
  says so. `[new: video.spec.js]` (prontezza S10, :334)
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

| item                   | observation                                                                                                                                                                                                                                                                                                                       |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| File → Open video      | opens a chooser whose title equals the toolbar's open-video chooser title; picking `sample.mkv` clears `.stage__empty` and the transport advances                                                                                                                                                                                 |
| File → Open subtitle   | opens a chooser with the subtitle title; picking `basic-lf.srt` makes `.status__document` read `LF_STATUS`, `"SRT · 3 cues · LF"` (prontezza S4, :254)                                                                                                                                                                            |
| File → Save            | with `.status__dirty` present, writes the file and clears `.status__dirty`                                                                                                                                                                                                                                                        |
| File → Save copy as    | opens the save-mode chooser; confirming produces a byte-identical copy                                                                                                                                                                                                                                                            |
| Edit → Undo            | restores the last committed cue edit to the literal it was opened with                                                                                                                                                                                                                                                            |
| Edit → Redo            | puts it back                                                                                                                                                                                                                                                                                                                      |
| Video → Play/pause     | toggles the transport state `.controls__button` reports                                                                                                                                                                                                                                                                           |
| Video → Transcribe     | opens the same dialog `.toolbar__transcribe` opens: `.transcribe__model` present                                                                                                                                                                                                                                                  |
| Subtitles → Select all | same as Ctrl+A, observed the way T4's Ctrl+A criterion observes it: every rendered row carries the membership class at the top, at mid-file and at the bottom, the rendered count stays at or under 60 at each, and the cursor has not moved. "Marks all 2,000 rows" is not observable in a virtualized grid (prontezza B7, :192) |

**E2E.** Four new checks in `video.spec.js`. Guard 50 → 54. (prontezza B2, :80)

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

**The cold-start threshold is produced by the probe, never invented.** Idle memory has its number
(400 MB, stated in §7). Cold start does not: §7's 2 s is a release-build figure on a mid-range 2020
laptop, and T8 measures a debug build under Xvfb with software rendering. WORKFLOW §2.5 forbids a
threshold that was not actually measured, so an implementer handed "exits non-zero if either is
over budget" would either invent a number or block. Instead: **the probe session records the
measured debug-under-Xvfb cold start, and the delivery writes that measurement plus a stated
headroom into the script as the CI threshold**, with both the measurement and the headroom named in
the file. The M2.0 status says the §7 verdict is still owed on a release build. (prontezza S11,
:346)

**Idle memory, with a settle condition that is not a vibe.** `budget-check.js` spawns the app and
**loads the 2,000-cue fixture and the video fixture from argv through `startup_files`**, not
through the chooser. Revision 2 drove them with the T1 helper, which puts chooser teardown and
input latency inside numbers that exist to answer CLAUDE.md §7. `e2e/lib/shell-points.js` stays for
whatever still needs a click (prontezza minor "T8's fixture loading", :486). It pauses playback,
then samples the summed RSS across `processGroupMembers(pgid)` every 500 ms. The measurement is
the first sample within 2% of the previous two. If no such sample arrives within 30 s the check
fails as _did not settle_ and never reports the last value it happened to see. Whisper is excluded
by construction: nothing is transcribed, so no model is resident. Asserted under 400 MB.

**Acceptance criteria.**

- `pnpm e2e:budgets` prints the cold-start figure and the idle-memory figure, each with the halves
  or the settle sample it came from, and exits non-zero if either is over budget or if the memory
  measurement did not settle. Idle memory's budget is §7's 400 MB; cold start's is the number the
  probe measured plus the headroom the delivery states, both written in the script beside the
  measurement they came from. `[new: budget-check.js, 2 checks]` (prontezza S11, :346)
- The CI Linux job runs it beside the other checks and a regression turns CI red. `[new: ci.yml]`
- The M2.0 status names which cold-start shape the probe settled on. This is a **process note, not
  a pass/fail criterion**: all three listed outcomes satisfy "whichever shape the probe settles on
  is the one implemented", including the one where the paint half is never measured, so it cannot
  fail. The measured threshold above is what carries pass and fail.
  `[probe: m2-0-budget-probe.md]` (prontezza minor "T8 criterion 3", :463)

**Honesty clause, non-negotiable (CLAUDE.md §9).** These are debug-build numbers under Xvfb with
software rendering, on Linux. They are a necessary condition, not the §7 verdict; the verdict is
the release build on the owner's machine, and the report says so with the platform on it, the same
way `editor.spec.js` already qualifies its own numbers. The mocha guard goes 54 → 55 for the
cold-start half measured in `title.spec.js`, if the probe leaves one there; if it does not, the
guard stays at 54 and the plan's running total is corrected in the delivery rather than the check
being invented to match the number. (prontezza B2, :80)

---

## 5. Dependency graph and parallelism

Closed nodes are merged work. `[N2]`, `[N2b]`, `[N1b]` and `[N2c]` are all on `main`; gate 2 is the
freeze that stands between them and T1. (prontezza S1, :214; S2, :227; S3, :234)

```
[N2] ─ [N2b] ─ [N1b] ─ [N2c] ─ gate 2 ─┐
                                       │
      ┌────────────────────────────────┘
      T1 ─┬─ T1b ─┬─ T2 ─┬─ T3 ─┬─ T5a ── T5b ── T6 ── T7 ── T8
          │       │      │      │
          └───────┴──────┴── T4 ┘
```

- **N2 is merged, and so is everything else ahead of the milestone.** N2 landed as `d224f3c` and
  `BACKLOG.md:84` marks it `[x]`, verified-by-tests, with its gate-1 status; N2c landed as
  `c7261a5`. Revision 2's sentence — "N2 is a hard predecessor of T3, and `BACKLOG.md` marks it
  `[ ]`: not started" — was false in both halves by the time it was written, and every schedule
  claim built on it is deleted. What actually stands between today and T1 is **gate 2**.
  (prontezza S1, :214)
- **Gate 2 is the head of the graph, not a task.** WORKFLOW §4a freezes merges of new code while it
  is open and lets documentation and planning keep running, so T1 and T1b may be written during the
  freeze and merge when it opens. Nothing becomes startable earlier than revision 2 said: T3 is
  still behind T1, T1b and T2. What changed is that **T3's risk moved from schedule to contract**,
  which is §2.1. (prontezza S2, :227)
- **N2c is a closed predecessor of T3 and T5a**, because it moved the unit their geometry criteria
  compare in. It resolves the rectangle to native device pixels inside `report()`
  (`VideoStage.tsx:30-41`) and states the unit on `src/types/video.ts:4-9`. Both tasks' criteria are
  written in that unit; T3 owns `src/types/video.ts` so the contract has one owner. (prontezza S3,
  :234)
- **The fallback order is deleted.** Revision 2 said "if N2 has not landed when T2 finishes, run T4
  before T3". N2 landed; the bullet was dead text an implementer session would have read as live
  scheduling advice. T3 still merges before T4, on the composition-root argument in T4's own header
  and not on N2. (prontezza S1, :214)
- T2 and T4 remain the one sanctioned parallel pair, under the split T2 now spells out: T2 does the
  four choosers' `App.tsx` wiring first and the orchestrator freezes it before T4 starts, `App.css`
  is split by block, and the genuine one-liners are `en.ts` and the `EXPECTED_TESTS` integer. If the
  `App.tsx` freeze cannot hold, the pair is serialized. (prontezza S6, :281)
- T1 and T1b may run in parallel: `dialog.js` against `x11.js`, `paths.js`, `dom.js`. Both bump the
  counter, so the same one-liner rule applies.
- **T6 and T7 run in sequence.** They are disjoint in components (`Rail.tsx` against `MenuBar.tsx`)
  but `Shell.tsx` carries both deliveries — T7's entire delivery is mounting a menu bar strip in it
  and T6's is swapping the rail component in it — so revision 2's "freeze `Shell.tsx`, `App.css` and
  `en.ts` first" freezes the one file the pair cannot proceed without. Read as "nobody edits it" the
  pair cannot run; read as "the orchestrator edits it first" it is an eleventh, unwritten task.
  Sequential, T7 after T6, and the shared files are `Shell.tsx`, `App.css`, `en.ts` and
  `e2e/wdio.conf.js`, which revision 2's freeze list omitted. (prontezza S7, :294)
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
  each re-derive its point against the layout they leave behind, and each carry the 12/12
  criterion (13/13 from T7). A task that finds the gate red and the file outside its ownership
  stops with a BLOCKED report; that situation is a defect in this plan, not in the app.
- **Every spec file has a written owner before the task that breaks it starts.** The two N2 specs,
  `video-surface.spec.js` and `video-empty.spec.js`, are T3's and T5b's — the two tasks that break
  them. A spec with no owner turns the rule above into a guaranteed BLOCKED report. (prontezza B4,
  :122; owner ruling 2026-08-30.)
- **Whoever renames or deletes a selector updates `e2e/README.md` in the same delivery.**
  `:144` says of those class names "Renaming one breaks the harness", and `:146-155` lists roughly
  twenty that §3 renames or deletes, including `.bar__input`, the `.subbar__*` family,
  `.project__path` and `.cuelist__row--selected`. Revision 2 gave that file to T1b alone, which runs
  before every rename, so the milestone would have ended with a selector contract describing a shell
  that no longer exists. T3, T5b and T6 each own it for the names they move. (prontezza S15, :401)
- **`EXPECTED_TESTS` moves with the checks, and it never decreases.** Adding a check without
  bumping it, or bumping it without adding one, both defeat the guard; **writing a smaller number
  than the one already in the config silently disarms the guard for every check between the two**,
  which is exactly what revision 2's chain would have done. When the count in §4 and the count in
  the delivery disagree, the delivery corrects this file; it never invents a check to reach the
  number, and it never lowers the config to reach it either. (prontezza B2, :80)
- **Gate 4 outranks the between-gates regime.** WORKFLOW §4a requires a gate before any merge that
  touches saving, subtitle formats or the open-core boundary, whatever the schedule. In this
  milestone that is **T2 and T5b**, and both carry it as an explicit merge precondition. The
  per-delivery review this plan was originally written under no longer exists: it is now the §2.5
  self-check plus the gate regime of WORKFLOW §4a. (prontezza S16, :412)
- **The engine is not touched.** M2.0 is the frontend shell plus the dialog commands. A task that
  finds itself editing `sublore-formats`, `sublore-edit`, `sublore-io` or the player has drifted.
- **Gate reviews are delegated and start from `docs/reviews/review-prompt.md`**, and a review's own
  fixes get reviewed (WORKFLOW §4b). Between gates, each task closes on its own behavioural tests, a
  green full battery and the §2.5 self-check.
- **Every verdict carries its platform.** "Verified on Linux", never a bare "verified".

---

## 7. What is still the owner's to decide

One thing, and it is an acceptance criterion the shape cannot fully meet.

**"Video and waveform panels sit side by side" cannot be shown at M2.0.** No audio provider exists
before M2.4, and the layout doc's rule is that a panel with no provider is absent rather than
empty. _Recommendation:_ M2.0 delivers the top-right column with the current-line band in it, and
that half of the AC closes at M2.4 when the waveform panel arrives above the band. The alternative,
an empty placeholder panel, is dead UI that CLAUDE.md §6 rules out. Affects T5a's first criterion,
which asserts the arrangement as three panels and not four.

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

Two pairs may run in parallel with the file rules above: T1 with T1b, and T2 with T4 under the
`App.tsx` freeze T2 describes. **T6 and T7 are sequential**, because `Shell.tsx` carries both
deliveries. Everything else is sequential through the composition root. (prontezza S7, :294)

Fifty-five mocha checks at the end, from thirty-three. Thirteen in the close gate, from twelve.
Five in the shutdown check, unchanged. Two in the new budget script. `wayland-attach-check.js` and
`scaled-surface-check.js` are untouched throughout. (prontezza B2, :80)

The milestone's first merge waits on gate 2 (WORKFLOW §4a), and two of its merges — T2 and T5b —
wait on gate 4. (prontezza S2, :227; S16, :412)

---

## 9. Rilievi lasciati aperti

Nothing from either critique disappears in silence. All twelve blocking and all twenty-three
serious findings are applied above. The ten minor findings are applied too, so this section is
short: it holds the five findings resolved differently from what the critique proposed, and the
two things that stay open with their reason.

**Revision 3 adds its own.** `m2-0-prontezza.md`'s seven blockers, nineteen serious findings and
nine minor findings are all applied above, each cited where it lands. Two things it names are not
closed by revision 3 and are not dropped either: the paragraph above claims twelve blocking and
twenty-three serious findings from the two earlier critiques were applied, and **no lens has
re-walked that claim** — gate 1 already records it as a debt, and it stays one. And the readiness
report's own §6 lists what no lens could check, including every gesture whose feasibility T1 and
T1b exist to answer; nothing in revision 3 turns those questions into facts.

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
  status, not a better instrument. Revision 3 closed the separate defect that the check could take
  no samples at all (prontezza S10, :334); it does not close this.
- **The picture is not asserted in CI.** After revision 3, "the surface is showing a live frame" is
  asserted in the mocha suite as map state plus mpv's child window, and the pixels are measured on
  real hardware and on the owner's checklist only. N2 measured why — 2 frame appearances in 10 under
  Xvfb with llvmpipe — and a pixel assertion inside CI is a new investigation nobody has run.
  (prontezza B5, :142)
- **The geometry criteria cannot discriminate on the platform that runs them.** Every criterion
  comparing the surface's X11 geometry with a DOM rect now names native device pixels, and under
  Xvfb `devicePixelRatio` is 1, so the correct wording and the wrong one produce the same green run.
  The discrimination is the owner's 1.5-scaled display, which is a checklist item and not
  automation. (prontezza S3, :234)
- **N1's open-editor data-loss gap stays open through M2.0.** §1 and T3 both say so, and the M2.0
  status repeats it. It is not fixed here and it is not forgotten here. (prontezza S17, :424)
- **The remove branch of the selection remap (T4's known gap).** No UI gesture reaches it and no
  TypeScript test runner exists. Written, reviewed against the arithmetic, recorded as untested,
  and owed by the M2.5 task that adds a delete gesture. Closing it inside M2.0 costs either scope
  the owner did not ask for or a dependency decision that is his.
