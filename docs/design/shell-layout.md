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

Both panels hide independently when their provider is absent (`SetDisplayMode`, `frame_main.cpp:218-244`), so no video open means no video panel at all. The audio panel is a `wxSashWindow` with a draggable bottom edge and a persisted height (`audio_box.cpp:47,101-103`).

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

## Effect on the E2E suite

The DOM changes, so the 27 checks need their selectors re-pointed. Selectors are updated; assertions are not. No assertion is weakened, skipped, or retargeted to make a check pass (CLAUDE.md §5.4).
