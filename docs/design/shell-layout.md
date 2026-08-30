# Shell layout — decided 2026-08-29

Owner decision after running the app and rejecting the interface. M0–M4 each bolted a horizontal band with a path field and a button onto one column, because that is the cheapest way to give an E2E spec a stable selector; the result is the union of five test harnesses. This document is the target shape; `shell-mockup.html` is the picture.

**Reference: Aegisub.** Its bones, not its finish. Read from source (`arch1t3cht/Aegisub`, BSD-3-Clause, GPL-compatible) — structure only, no code taken. Line references below are to that tree so any claim here can be checked.

## The real Aegisub arrangement

From `FrameMain::InitContents`, `src/frame_main.cpp:183-216`:

```
TopSizer   (horizontal): [ videoBox ][ ToolsSizer ]
ToolsSizer (vertical):   audioBox (proportion 0, natural height)
                         editBox  (proportion 1, expands)
MainSizer  (vertical):   TopSizer (proportion 0)
                         subsGrid (proportion 1, takes everything left)
```

So:

```
menu bar
toolbar
──────────────────────────────────────
┌─────────┬──────────────────────────┐
│         │ audio (waveform)         │
│  video  ├──────────────────────────┤
│         │ edit box (current line)  │
└─────────┴──────────────────────────┘
  grid — all remaining vertical space
```

The video sits on the left and spans the whole top block. Audio and the edit box are stacked to its right. **The edit box is not full width under both** — that was the error in the first draft of this document.

Both panels hide independently when their provider is absent (`SetDisplayMode`, `frame_main.cpp:218-244`), so no video open means no video panel at all. Sublore takes that rule for the waveform and not, yet, for the video; "Panels with no provider, and the one exception" below says why. The audio panel is a `wxSashWindow` with a draggable bottom edge and a persisted height (`audio_box.cpp:47,101-103`).

## Edit box, row by row

From `SubsEditBox::SubsEditBox`, `src/subs_edit_box.cpp:106-231`:

1. comment checkbox · style combo · edit-style button · actor · effect · character count
2. layer · start · end · duration · left/right/vert margins
3. B I U S · font · 4 colour buttons · new line · time/frame radio · **`Show Original` checkbox**
4. `secondary_editor` (read-only, hidden by default) stacked **above** `edit_ctrl` (the text)
5. revert · clear · clear text · **`insert original`** (hidden by default)

Rows 1–3 are ASS typesetting, which CLAUDE.md §1 rules out for v1. Rows 4–5 are the interesting part: Aegisub already solves source/target by stacking the original above the translation inside the edit box, toggled by `Show Original` and persisted in `Subtitle/Show Original`. Stacked, not side by side, because subtitle lines are wide and short and two narrow columns wrap badly.

## Grid columns

From `GetGridColumns`, `src/grid_column.cpp:466-481`, in order: `#` · folds · layer · start · end · **CPS** · style · actor · effect · left · right · vert · text.

CPS (characters per second, `grid_column.cpp:342-351`) is a reading-speed check and is worth having. The ASS columns are not.

## Video and audio panel furniture

Video (`video_box.cpp:47-95`): visual-tools toolbar vertical along the left edge, then under the display a seek slider, then a row with the playback toolbar, current frame time and number, **time relative to the current line's start and end** (`VideoSubsPos`), and a zoom combo.

Audio (`audio_box.cpp:79-106`): the display with horizontal-zoom, vertical-zoom and volume sliders down its right edge, and its own toolbar underneath. That toolbar (`default_toolbar.json`) is the real timing vocabulary: prev/next line, play selection, play line, play before/after/begin/end of selection, play to end, **lead in, lead out**, commit, go to, and the autocommit/autonext/autoscroll toggles.

That list is the specification for M2.5. Dragging boundaries is only part of it; the keyboard timing commands are what makes the workflow fast.

## Sublore's shape

Same skeleton, with the typesetting rows dropped and our own things added:

```
menu bar     File · Edit · Subtitles · Timing · Video · Audio · Terms · View
toolbar      open · save | undo · redo | transcribe · termbase · QA
─────────────┬─────────┬────────────────────────────────────────
project rail │         │ waveform            (M2.4, sash-resizable)
series,      │  video  ├────────────────────────────────────────
episodes     │         │ start · end · duration · CPS   (M2.5)
             │         │ original   (read-only)         (M2.6)
             │         │ translation                    (M2.6)
─────────────┴─────────┴────────────────────────────────────────
 grid: # · start · end · CPS · original · translation
```

| Task               | Home                                                                                      |
| ------------------ | ----------------------------------------------------------------------------------------- |
| M2.4 waveform      | top-right panel, resizable, hides with no audio                                           |
| M2.5 timing        | start/end/duration fields, drag on the waveform, and the keyboard command set above       |
| M2.6 source/target | original stacked above translation in the edit box, plus a second text column in the grid |
| M5 termbase + QA   | `Terms` menu, QA flag as a grid row marker                                                |

Each lane owns a panel, so parallel work cannot collide in the layout. This replaces the pre-allocated-CSS-seams scheme drafted earlier.

### Panels with no provider, and the one exception

Aegisub removes a panel whose provider is absent (`SetDisplayMode`, `frame_main.cpp:218-244`). The waveform follows that rule here: no audio provider exists before M2.4, so there is no empty waveform panel to build.

The video panel is the exception, and it is deliberate. `.stage__empty` — the "No video open." placeholder inside the panel — is what two specs poll to decide a video is ready: both wait for `document.querySelector(".stage__empty") === null` (`video.spec.js:73`, `asr.spec.js:160`). A panel that does not exist until a video loads makes that condition true from first paint, so both waits return immediately and stop asserting anything. That is an assertion weakened as a side effect of a layout rule, which is a §5.4 failure whether or not anyone notices it.

So the video panel is always mounted and shows `.stage__empty` when there is no video, exactly as today. Adopting Aegisub's rule for it is a later task, and that task owes a positive readiness signal in exchange — mpv's child window present plus the transport enabled, which is the honest predicate `docs/reports/n2-probe.md` arrived at — before it takes the placeholder away.

The visibility rule below is unaffected: `videoPanelMounted` stays in the derived boolean, because unmounting still has to hide the surface and a later shape may unmount. It is simply always true for now.

## What is ours, not Aegisub's

- The project rail. Aegisub works one file at a time; following the translator across a series is the product.
- The translation column in the grid. Aegisub's translation assistant is a separate modal tool; for Sublore source-plus-target is the normal state, so it belongs in the grid where the whole episode is visible.
- Row markers for QA: a term used against its approved rendering flags the row.
- `Show Original` is not a toggle for us. It is always on.

## Finish

Aegisub's density, not its wxWidgets chrome. Small type, tight rows, nothing wasted, everything reachable from menu and keyboard. Flat borders, no bevels, no system grey.

Dark first: subtitlers work against a lit video. Every colour is a token in one place, so a light theme is a value swap and not a rewrite. Because the chrome is CSS and not native widgets, it renders identically on Windows and Linux.

## What this removes

- The three fields where a file path is pasted by hand. Opening goes through the system file dialog, from the menu and the toolbar. The picker already exists for the project folder (`project_choose_path`) and gets extended.
- The transcription band standing in the way at all times. It becomes a menu item that opens a dialog.
- The clipped `Save copy to` label at 1024x700.

## Constraint carried over from M0.2

`VideoStage` positions a native X11 surface from DOM coordinates and recomputes it on resize, never on scroll. The panel holding the video must never scroll: it resizes, or the video surface detaches from its frame. Note that Aegisub's sash-dragged audio panel resizes its siblings, which is a resize and therefore safe.

## Layers over the video (decision 1)

The surface is an X11 child of the toplevel, so it stacks above the webview by construction, and `set_region` raises it again on every update (`surface/linux.rs:62-67`). Any HTML painted where the video is would be behind it. Decision 1: the surface hides while an HTML layer is open and comes back when the last one closes.

The probe settled feasibility, so this section only has to say how it applies to the shape above. mpv remaps its output with the video playing and with it paused, no seek, no play, no forced redraw, and the paused case comes back with the same pixel spread it had before (`docs/reports/n2-probe.md`). Nothing is restarted to get the frame back, so the cost of an open menu is a blank rectangle for exactly as long as the menu is open.

### What counts as a layer

A layer is anything the shell paints outside the panel flow: it appears on top of what was already there, it is transient, and it is dismissed rather than resized away. In the shape above that is:

- a dropdown open under a menu bar title (File, Edit, Subtitles, Timing, Video, Audio, Terms, View). The menu bar strip is not a layer; the panel it opens is.
- a modal dialog: transcription (the band M2.0 takes off the screen), model download, and every dialog added after them.
- a context menu, on the grid, on the project rail or on the waveform.
- a popover or picker hanging off a field in the edit box.

Not layers: the toolbar, the rail, the sash, the status and error lines, the inline cue editor, and everything else that takes space in the layout instead of covering it.

Native windows are outside the rule by construction, not by exception. The system file dialog and N1's close gate are separate toplevels; the window manager stacks them above everything we own, the surface included, and they never enter the stack below. Whether the close gate really does draw over an open video has not been checked, because N1 was verified without a video loaded, so the test list at the end of this document carries that check.

Every layer hides the surface, whether or not it overlaps the video rectangle. No geometry test. A rule that compares rectangles has to be re-evaluated on every layout change, and it generates exactly the flicker class of bug this shape is meant to avoid; worse, any region update while a layer is open raises the surface again (`surface/linux.rs:66`), so a layer that opens clear of the video does not stay clear of it.

### Who owns the state

One owner. The workspace shell holds the set of open layers, and surface visibility is derived from it. The component that opens a layer registers an id and removes it on close; it never calls a video command itself.

Visible = a video is loaded, and the video panel is mounted, and the layer set is empty. Those are three separate reasons for the rectangle to be absent and they must not fight each other. The middle one is always true for now — the video panel keeps its `.stage__empty` placeholder rather than unmounting, for the reason given above — but it stays in the derived value rather than being folded away, because an unmounted panel does already hide the surface today by reporting an empty region on cleanup (`VideoStage.tsx:52` into `video/mod.rs:196-197`), and the day the panel starts unmounting the two reasons must not have been merged into one.

Because visibility is derived and re-asserted after every video command, `video_open`'s own unconditional `show()` (`video/mod.rs:106`) cannot leave the surface visible underneath an open layer. That is a belt, not the design: the menu item that opens a video closes its own layer before it dispatches.

M2.0 does not invent a second hide path. It consumes the one N2 built, and that one derives visibility rather than setting it. The backend keeps a single state — a video is open, the last reported rectangle has area — and one function turns it into show or hide; nothing else in the module touches the window. So a rectangle that goes empty hides the surface and a rectangle that comes back shows it again, but only while a video is loaded, which is why the empty stage is never covered. M2.0 adds a third input to that state, the open-layer set below, and the derivation stays in the one place.

### Two layers at once

The state is a set of ids, not a depth counter and not a single flag. The surface is hidden while the set is non-empty and shown when it empties. Opening a dialog from an open menu therefore changes nothing: the surface is already hidden and stays hidden until both are gone. Dismissal need not be last-in-first-out, since a dialog can outlive the menu that opened it, which is why removal is by id.

One consequence to hold on to: a layer closing in the same commit as another one opening, which is what a menu item that opens a dialog does, must not put a frame on screen in between. The set is state and the effect reads the derived boolean, so the intermediate never reaches the backend. An implementation that showed and hid the surface inside each open and close handler would flash, and is out.

### Resize while a layer is open

The region keeps being measured. It stops being sent.

While the layer set is non-empty the shell holds the last measured rectangle and sends nothing, so no `set_region` raise can restack the surface over an open layer. When the set empties the shell sends that rectangle again, and the backend's derivation brings the frame back by itself: there is no separate show to call, because visibility is not something the frontend sets.

This does not weaken the M0.2 constraint above. The region is still computed on resize and never on scroll, by the same ResizeObserver and window resize listener in `VideoStage.tsx`; occlusion changes when the value is delivered, not when it is computed. The panel holding the video still must never scroll.

If the visibility command fails, and it can, since the main thread hop has a timeout (`video/mod.rs`, `MAIN_THREAD_TIMEOUT`), the failure surfaces through the video error path already on screen, the shell keeps its own state, and the next transition re-asserts it. No silent retry loop and no swallowed error (CLAUDE.md §6).

### Driving a menu from the keyboard

"Everything reachable from menu and keyboard" is not a specification until the keys are named, and a dropdown is a layer, so its key model decides when the surface hides and when it comes back.

| key         | effect                                                                           |
| ----------- | -------------------------------------------------------------------------------- |
| Alt         | opens the first title's dropdown, cursor on its first enabled item               |
| Left, Right | move between titles; with a dropdown open, the neighbouring dropdown replaces it |
| Up, Down    | move between items inside the open dropdown, skipping disabled ones              |
| Enter       | activates the item under the cursor and closes the dropdown                      |
| Escape      | closes the dropdown and returns focus where it was before Alt                    |

Moving between titles with a dropdown open has the same shape as a menu item that opens a dialog: one layer id replaces another inside a single state update, the derived boolean never passes through true, and no frame reaches the screen in between.

No Alt-mnemonic letters. They need a mnemonic assigned per title in every locale, which is i18n work out of proportion to a menu bar with four working titles.

Menu titles, menu items, dialog titles and dialog buttons are user-facing copy and live in `src/i18n/en.ts` with everything else. None of them is written inline (CLAUDE.md §9).

## Active line and selection (decision 5)

One number does both jobs today (`CueList.tsx:85`): the row the arrows move, the row the editor opens on, and the only row anything could act on. M2.5 asks for "play selection" and M5 will want every flagged row selected at once, so the number becomes two states.

- **active**: one row index, or null when the document has no rows. The cursor: where the keyboard is, what the edit box shows, what the editor opens on.
- **selection**: a set of row indices plus an anchor for range extension. What bulk operations act on.

While the document has rows the selection is never empty, and active is a member of it, except while a ctrl-move is walking the cursor to build a scattered set (below). A plain click or a plain arrow leaves the selection at exactly `{active}`, which is today's behaviour and is what keeps the existing checks meaningful.

The selection is a sorted set of indices, not a start and an end. Ctrl-click makes it sparse, and sparse is the case the product needs: a QA pass flags scattered rows across an episode. Decision 4's composite history entry exists so that one bulk edit over such a set is one undo.

### Where the two states live

Not in the grid. The grid draws them and dispatches intents; the workspace shell owns them, in one hook beside the document state.

There are four consumers, which is the reason. The grid draws both. The edit box under the video follows active (M2.6). The waveform plays and times the selection (M2.5). Menu and toolbar items enable on the selection. Left inside the grid, M2.5 would read the cursor and call it a selection, and M5 would grow a second flagged-set beside it. Preventing exactly that is why decision 5 lands here and not later.

The grid keeps its virtualization untouched: the selection is a set consulted per rendered row, so selecting 2,000 rows renders the same rows in view as selecting one. Selection state that forced a full render would spend the M2.3 open budget the existing spec measures.

### Keyboard

Both states are drivable from the keyboard, and every mouse gesture mirrors a key.

| gesture                                            | active           | selection                            | anchor        |
| -------------------------------------------------- | ---------------- | ------------------------------------ | ------------- |
| Up, Down, PageUp, PageDown, Home, End; plain click | moves            | collapses to `{active}`              | set to active |
| Shift with the same keys; shift-click              | moves            | replaced by the run anchor to active | unchanged     |
| Ctrl with the same keys                            | moves            | unchanged                            | unchanged     |
| Ctrl+Space; ctrl-click                             | moves to the row | toggles that row's membership        | set to active |
| Ctrl+A                                             | unchanged        | every row                            | unchanged     |
| Escape, with more than one row selected            | unchanged        | collapses to `{active}`              | unchanged     |

Ctrl with an arrow is the one gesture that takes active out of the selection, and it has to exist: without it the keyboard can never build a scattered set, since Ctrl+Space only toggles the row under the cursor. So the cursor is drawn as an outline on the row and membership as the filled row, and the two are visibly different states rather than one style used twice.

The two states carry two class names and two ARIA concepts, so a check can name either one on its own: `aria-selected` on each row is membership, `aria-activedescendant` on the list is the cursor, and the list is `aria-multiselectable`.

### Starting values, and the gesture that changes nothing

On open, and after any patch that leaves rows standing, `active` is the first row and `selection` is the set holding it alone. On an empty document `active` is null and `selection` is empty. That is what `useState(0)` does today (`CueList.tsx:85`); writing it down turns a source fact into a contract, so a check can state how many rows are marked at first paint without anyone having to read the component to derive it.

A click inside the grid that lands on no row — the area below the last row, which is most of the panel on a short file — focuses the grid and changes neither state. It has to be nameable, because it is the only gesture that hands the grid the keyboard without moving the cursor, and it is the one `close-gate-check.js` needs: that script has no DOM and reaches the document by absolute pixels, so clicking a large empty region and pressing Enter to open the editor on the active row is far steadier than hitting a 28 px row.

Today's `.cuelist__row--selected` marks the single row that is cursor and selection at once. From here it means membership only, and the cursor is `.cuelist__row--active`. No check asserts on either class today, so the split costs nothing now; it is written down because reusing the old name for the new meaning is exactly how the two states get confused again.

### With the inline editor

Editing is about active, never about the selection. Enter or a double-click opens the editor on the active row and leaves the selection alone, so a bulk operation issued after the edit still means what it meant before.

A single click on a row's text opens the editor too, and keeps doing so. That is today's behaviour (`onClick={() => beginEdit(index)}`, `CueList.tsx:353`), and three checks in `editor.spec.js` click once and then wait for `.cuelist__editor` (lines 327, 511, 549). Making double-click the only route would mean editing those three to add a second click: a gesture change nobody asked for, paid for in checks. So a plain click does three things at once — moves the cursor, collapses the selection onto that row, and opens the editor on it — and the click that lands on no row does none of them.

While an editor is open the navigation keys belong to the editor. That is already the rule (`onListKeyDown` returns while `editingRef.current` is set, `CueList.tsx`) and it stays. Escape cancels. Enter commits and returns focus to the grid. Tab commits, moves active to the next row, and collapses the selection onto it, since otherwise tabbing down a file would silently grow a range.

Committing an edit moves nothing and selects nothing.

Every bulk operation flushes an open editor first, exactly as saving does today through `flushRef` (`App.tsx`, `saveWithPendingEdit`), because text sitting in an open editor is unsaved work whether or not it has reached the document. A bulk operation that ran before the flush would act on a document the user cannot see.

The edit box under the video (M2.6) and the grid's inline editor are two views of the active row, not two states. Whichever has focus holds the text being typed; the other shows what the document holds.

### When the document changes underneath

A cue row has no identity of its own: `CueRow` carries no id, and its index is its position in the array (`src/types/subtitle.ts`). Every mutation comes back as one contiguous splice, undo and redo included, since both go through the same command shape: `CuePatch` is `{ from, removed, cues }` and the list applies it as slice, insert, slice (`applyPatch`, `useSubtitleFile.ts`). So both states are indices, and they are remapped by the same arithmetic in the same function that splices the rows. One place, so the rows and the selection cannot drift apart.

With `delta = patch.cues.length - patch.removed`, every index held by either state maps as:

- `i < from`: unchanged.
- `i >= from + removed`: `i + delta`.
- inside the replaced run, with `i - from < patch.cues.length`: stays at `i`. The row was rewritten where it stood, which is what a text edit is (one removed, one back), and it stays selected.
- inside the replaced run, past the end of the new rows: the row is gone. Drop it from the selection; if it was active, active becomes `min(from + patch.cues.length, newCount - 1)`, the first row after the replaced run, or the last row when the run took the tail.

The anchor is remapped by the same rule and falls back to active when its row is gone. If the selection empties while rows remain it collapses to `{active}`. If the document empties, active is null and the selection is empty.

Undo and redo need one thing beyond the remap. They arrive as patches like everything else, so the states survive on their own, but an undo the user cannot see is indistinguishable from nothing happening. After undo or redo, active moves to the first row of the patch, the selection collapses onto it, and the grid scrolls it into view. This is the only case where the document moves the cursor instead of the user.

Decision 4 will put N child edits under one history entry. When it lands, the transaction arrives as more than one patch: the remap runs once per patch in arrival order, unchanged, and active goes to the first row of the first patch of the transaction. That is the whole interaction between decision 4 and this state, and it asks for no shape change here.

## Effect on the E2E suite

The DOM changes, so the 27 checks need their selectors re-pointed. Selectors are updated; assertions are not. No assertion is weakened, skipped, or retargeted to make a check pass (CLAUDE.md §5.4).

### The instruments those checks need, three of which do not exist

Every check below was written against `e2e/lib/` as it stands, and three of them have nothing to run on. A criterion whose instrument is missing reads as covered and is not, which is a §5.4 failure by omission, so the instruments are named here and built before the criteria that use them.

- **A screen, and a window that can be another size.** `findToplevel` selects the app by exact geometry (`x11.js:74`, 1024x700 from `paths.js:21`), so a resized window stops being found at all and every later click in that file misses. CI runs Xvfb at 1280x1024 (`ci.yml:190`), on which a 1920x1080 window cannot exist. Both are harness facts and both move before any criterion names a second size.
- **A way to read pixels.** "The picture is alive" is the probe's pixel-spread measure, and the probe script was deliberately not committed. `e2e/lib/` can read the window tree and a map state; it cannot read a pixel. N2's own criterion asks for an assertion on the visible frame, so this instrument belongs to N2 or to the harness work ahead of the first occlusion check — never to the check that assumes it.
- **A way to say "not covered by something we painted".** Nothing in the suite reads text off the screen. The observable form is `document.elementFromPoint` at the centre of each control's rect returning that control or a descendant of it; the map state is the observable form of "not covered by the video".

### Checks M2.0 adds

Occlusion, on top of what N2 proves about the surface itself:

- with a video playing, open a File dropdown over the video rectangle: the dropdown is readable in the DOM and the surface is `IsUnMapped` (`mapState`, `e2e/lib/x11.js`). Close it: the surface is `IsViewable` again, over the rectangle the stage reports, which is the geometry comparison `video.spec.js` already makes.
- open a dialog from the open dropdown and close the dialog: the surface is still hidden. Close the dropdown: it comes back. Two layers, one transition each way.
- resize the window with a layer open: the surface stays `IsUnMapped` while it is open, and on close it lands on the new stage rectangle, not the old one.
- the picture that comes back is alive, not a frozen or empty rectangle. The probe's measure, pixel spread over the surface rectangle across two samples, tells those apart, and the probe's precondition comes with it: refuse to measure unless mpv's child window is present, or the check measures the webview underneath and passes vacuously (`docs/reports/n2-probe.md`).
- N1's close gate raised over a playing video is readable and answerable. Native dialogs sit outside the layer rule, and that they stack above the surface is currently an argument about window managers, not a verified fact.

Selection:

- shift extends and ctrl scatters, from the keyboard alone: the rows carrying the membership marker are exactly the expected set, and exactly one row carries the cursor.
- delete a row above a scattered selection: the same cues stay selected, identified by their text, not by their index.
- undo of an edit made several hundred rows away brings that row on screen and makes it active.
- a bulk operation acts on the selection and one undo puts every touched row back. The operation itself belongs to the milestone that needs it; what M2.0 owes is the state it acts on.

One class of claim carries a caveat rather than a proof. "The video does not flash back on screen between the menu closing and the dialog opening" is a negative sampled over a few milliseconds, and a poll can always miss a flash. The design guarantee is real — the two layer ids swap inside one state update and the effect reads the derived boolean, so the intermediate never reaches the backend — but what a check can honestly say is "every sample across that window read `IsUnMapped`". The milestone status says it that way, with the sampling interval on it (CLAUDE.md §9).

Where the budgets are measured. §7's open budget stays where it is measured today, `editor.spec.js` on the 2,000-cue fixture, and the two selection states must not move that number; the existing check that only the rows in view are rendered is the guard against a selection that renders the whole file. Occlusion adds one main-thread round trip per layer transition, so the menu check records the time from the click to the dropdown being painted and the frame-return check records the time from close to `IsViewable`. Neither has a §7 budget of its own; both are reported with the M2.0 status, with the platform on them.
