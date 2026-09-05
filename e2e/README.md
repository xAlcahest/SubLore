# E2E harness (M0.5, extended by M1.5, M2.3, M3.4 and M4.4)

Behavioral tests that launch the real Sublore binary on a real X server and assert what a person
would see. Nothing here reads Rust or TypeScript source: the harness only drives the app and looks
at the window.

Tools the harness needs on PATH: `xdotool`, `xwininfo`, `python3` with python-xlib, and `ffmpeg`.
ffmpeg is there for the app, not for the harness: `asr.spec.js` transcribes audio the app really
extracts from `sample.mkv` with it, which is why `wdio.conf.js` requires it at load and says so in
the same line. No spec measures pixels — `lib/pixels.js`'s `saturation()` has no caller left, and
`video-surface.spec.js`'s own header says why the picture is not asserted under Xvfb, and
`preview.spec.js` asserts mpv's own read-back of the overlay it holds for the same reason. xdotool,
xwininfo and ffmpeg are each checked before any spec starts, so a missing one is a sentence naming it
rather than a timeout inside whichever spec needed it first.

All of that is Linux, and `lib/platform.js` is where the harness says so. Every library function that
drives X11 or a POSIX process group calls `requireLinuxBackend(seam, owes)` first, so on any other
platform it refuses by name and says what a Windows counterpart would have to do, instead of failing
later as a broken assertion or quietly doing nothing. BACKLOG MW.1a lists which file is which kind
and MW.1b writes the Windows side, on a machine that can run it.

The two things that do read pixels run only on the owner's machine: `webview-paint-check.js` and the
`real-session-check.mjs` probe. Both capture with ImageMagick's `import` and measure with ffmpeg
`signalstats`. `import` exists in ImageMagick 6; `magick` ships only with version 7 and the CI runner
has 6, which is one reason nothing in CI touches ImageMagick at all. Under rootless XWayland an X
root grab reads black whatever the app draws, and `import -window <id>` reads the window itself. The
check names `import` as a prerequisite before it launches anything; the probe reports the failed
capture's exit status instead.

## What each spec proves

| File                                    | Test                                                                                 | Acceptance criterion it binds                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| --------------------------------------- | ------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `specs/title.spec.js`                   | `native window title is Sublore`                                                     | The X11 toplevel is named `Sublore`. This is the AC test.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `specs/title.spec.js`                   | `document title is Sublore`                                                          | The webview loaded the app document, not a blank page. Different thing from the X11 name; both are asserted.                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `specs/video.spec.js`                   | `opens the sample fixture`                                                           | Answering the system chooser with `fixtures/video/sample.mkv` reaches the ready state with no error banner.                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `specs/video.spec.js`                   | `sizes the native video surface over the stage`                                      | The native video child window is mapped and covers the `.stage__surface` rectangle within 2 px.                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `specs/video.spec.js`                   | `seeks the video to where the slider is dragged`                                     | A real press-move-release across the seek slider lands mpv at the middle of the clip, proved by playing on from there.                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `specs/subtitle.spec.js`                | `opens an SRT fixture and shows its format and cue count`                            | Opening `fixtures/subtitles/srt/clean/basic-lf.srt` puts `SRT · 3 cues · LF` on the status line with no error.                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `specs/subtitle.spec.js`                | `saves a byte-identical copy`                                                        | Save-as of `basic-crlf.srt` writes a file the spec then compares byte for byte with `Buffer.compare`.                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `specs/subtitle.spec.js`                | `opens an ASS fixture and saves a byte-identical copy`                               | `ass/clean/basic.ass` puts `ASS · 3 cues · CRLF` on the status line, and the copy matches byte for byte.                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `specs/subtitle.spec.js`                | `opens a VTT fixture and saves a byte-identical copy`                                | `vtt/clean/basic.vtt` puts `VTT · 3 cues · LF` on the status line, and the copy matches byte for byte.                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `specs/subtitle.spec.js`                | `reports a malformed file readably and stays usable`                                 | `srt/malformed/missing-arrow.srt` shows an error naming line 6, and the clean fixture opens straight after.                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `specs/subtitle.spec.js`                | `throws an unsaved edit away and writes nothing when the edit is discarded`          | Discard puts the cue back to the text it was opened with, clears the dirty marker, and leaves the file's bytes alone. The button itself is drawn throughout, greyed until an open the unsaved edit refused and greyed again once the edit is gone (owner ruling 2026-09-03).                                                                                                                                                                                                                                                                    |
| `specs/editor.spec.js`                  | `opens the 2000-cue fixture inside the open budget`                                  | A copy of `srt/clean/large-2000.srt` opens and the first row appears in under 1 s (CONTRIBUTING.md §7).                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `specs/editor.spec.js`                  | `renders only the rows in view, over a sizer as tall as the whole file`              | At three scroll positions at most 60 rows exist in the DOM, over a spacer of `2000 × row height`.                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `specs/editor.spec.js`                  | `scrolls a viewport at a time without falling behind`                                | Twenty scroll steps, each timed until the list shows different rows: mean under 32 ms, max under 150 ms.                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `specs/editor.spec.js`                  | `types into a cue without the list re-rendering behind every keystroke`              | Twenty keystrokes into the inline editor: p95 keydown to input under 50 ms, max under 150 ms.                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `specs/editor.spec.js`                  | `commits the edit on Enter and marks the file unsaved`                               | Enter puts the typed text on the row in under 200 ms, the dirty marker appears, and nothing is written yet.                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `specs/editor.spec.js`                  | `undoes the edit back to the original text and redoes it`                            | Ctrl+Z restores the original text in under 200 ms and clears the dirty marker; the Redo button brings it back.                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `specs/editor.spec.js`                  | `saves the edit, and every other byte of the file is the byte that was there`        | Node compares the saved file block by block against the copy that was opened.                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `specs/editor.spec.js`                  | `reopens the saved file with the edit in it`                                         | Opening the saved file again shows the edited row and an unedited neighbour.                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `specs/editor.spec.js`                  | `saves the text still sitting in an open editor`                                     | Clicking Save with the inline editor still open writes what was typed: the blur's commit and the save both land.                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `specs/editor.spec.js`                  | `leaves ctrl+z to the text box it was typed in and undoes one step from the toolbar` | Ctrl+Z typed into the project panel's episode box is that box's own undo, and the toolbar's Undo then steps the document exactly once.                                                                                                                                                                                                                                                                                                                                                                                                          |
| `specs/asr.spec.js`                     | `offers the models it knows and a compute choice`                                    | The model list holds the whole catalog, the one on disk is preselected and reads `ready`, the GPU box is there, and Transcribe is off until a video is open.                                                                                                                                                                                                                                                                                                                                                                                    |
| `specs/asr.spec.js`                     | `transcribes the open video and shows the cues`                                      | A run over `sample.mkv` lists cues carrying words from a real whisper transcript, the sidecar was handed audio from the app's scratch directory and never the media, the scratch directory is gone afterwards, and the fixture's own bytes and mtime are untouched.                                                                                                                                                                                                                                                                             |
| `specs/asr.spec.js`                     | `leaves the cues it produced as the open document, unsaved and nowhere on disk`      | The finished run's cues are the document: the grid holds them, the subtitle status line counts them, the document is marked unsaved and offers only a copy to save because it has no file yet, and nothing was written beside the media or anywhere Sublore saves (BACKLOG.md M3.5).                                                                                                                                                                                                                                                            |
| `specs/asr.spec.js`                     | `edits a cue of the result, saves it, and reopens the file with the edit in it`      | Typing over a cue of the transcription, saving a copy of it and opening that file again shows the edit. This is M3.5's own end-to-end sentence.                                                                                                                                                                                                                                                                                                                                                                                                 |
| `specs/asr.spec.js`                     | `asks before a transcription replaces unsaved work, and cancel keeps both`           | With an edited file open, a finished run raises the same save/discard/cancel dialog the close gate raises. Cancel leaves the document, its edits, the file on disk and the run's cues all exactly as they were.                                                                                                                                                                                                                                                                                                                                 |
| `specs/asr.spec.js`                     | `takes the cues once the same question is answered with Discard`                     | The result is offered again, and Discard replaces the document with it: the unsaved edits are dropped and nothing is written to the file they came from.                                                                                                                                                                                                                                                                                                                                                                                        |
| `specs/asr.spec.js`                     | `shows progress, stays usable, and leaves nothing running when cancelled`            | The bar advances past zero, the playback button still answers mid-run, and after Cancel the sidecar's pid is gone from `ps` (a killed-but-unreaped child would still be there as a zombie), no scratch directory is left, and Transcribe is available again.                                                                                                                                                                                                                                                                                    |
| `specs/asr.spec.js`                     | `runs on the CPU when the GPU box is unticked`                                       | The status names the CPU and the sidecar's real command line carries `-ng`. The cues on screen are unsaved work by then, so this run's own replacement question is answered with Discard.                                                                                                                                                                                                                                                                                                                                                       |
| `specs/asr.spec.js`                     | `refuses a damaged model and never hands it to the sidecar`                          | One bit is flipped in the model file, which keeps its catalogued length. The run is refused on its checksum, the sidecar is never spawned, no scratch directory is left behind, and the row offers Download instead of Transcribe.                                                                                                                                                                                                                                                                                                              |
| `specs/project.spec.js`                 | `creates a project in an empty folder`                                               | Choosing a fresh temp folder in the chooser and clicking Create puts that folder on the status line and writes `project.sublore`.                                                                                                                                                                                                                                                                                                                                                                                                               |
| `specs/project.spec.js`                 | `adds an episode and attaches a subtitle file to it`                                 | The episode is listed, the attached path is listed under it, and the user's file is byte-identical afterwards.                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `specs/project.spec.js`                 | `still lists the episode and its file after the app is restarted`                    | The AC's restart: the session is deleted and a new one launches the binary again, and reopening the folder lists both.                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `specs/project.spec.js`                 | `reports a folder that holds no project and stays usable`                            | Opening an empty folder shows the `noProjectHere` sentence, writes nothing there, and the real project reopens after it.                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `specs/project.spec.js`                 | `deletes the project without touching the files it points at`                        | `project.sublore` is gone, the attached subtitle outside the folder is byte-identical, and the folder itself still exists.                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `specs/chrome.spec.js`                  | `draws every title with nothing open, greying the one with nothing behind it`        | With no document and no media open: File, Edit, View, Audio and Help are all on the bar, Audio greyed because it has no track to list, and File draws Save, Save a copy and Discard greyed instead of leaving them out. Owner ruling 2026-09-03, which reverses decision 24 A2.                                                                                                                                                                                                                                                                 |
| `specs/chrome.spec.js`                  | `draws the same titles, greying and all, once a document is open`                    | The subtitle is opened through the toolbar and the bar is read again: the same five titles, in the same order, Audio still greyed because a subtitle brings no audio with it. No title arrives when a document does.                                                                                                                                                                                                                                                                                                                            |
| `specs/chrome.spec.js`                  | `offers every command the removed bars offered, from the menu and from the toolbar`  | The command ids behind the three dropdowns hold everything the toolbar draws, and the six commands the deleted bars carried are on both routes.                                                                                                                                                                                                                                                                                                                                                                                                 |
| `specs/chrome.spec.js`                  | five keyboard checks                                                                 | Alt opens the first dropdown with the cursor on its first enabled item, the arrows walk the items and step over the disabled ones, left and right change dropdown, Enter activates the item under the cursor, and Escape closes and hands the keyboard back to where it was. Driven as keys through XTEST, never as a claim about a handler.                                                                                                                                                                                                    |
| `specs/chrome.spec.js`                  | five accelerator and route checks                                                    | Ctrl+O, Ctrl+Shift+O and Ctrl+Shift+S raise their choosers and leave the app as it was when dismissed, File > Open subtitle opens a copy of the fixture end to end, and Ctrl+S then writes an edit into it and moves nothing else. Ctrl+Q is `scripts/quit-gate-check.js`, which owns every route that ends the process.                                                                                                                                                                                                                        |
| `specs/command-registry.spec.js`        | `draws one item per registry entry, and both routes draw the same record`            | Every item the bar draws is one registry entry and every entry is drawn: the two sets agree by id. Nothing the toolbar draws is missing from the menus, and for each command on both routes the label and the greying match, which is what one record behind two routes means (T3 C1).                                                                                                                                                                                                                                                          |
| `specs/command-registry.spec.js`        | `draws every command with nothing open, greyed rather than absent`                   | With no subtitle and no video: every title is on the bar, Audio greyed because no media has tracks; File draws Save, Save a copy and Discard, all greyed; Edit draws Undo and Redo, greyed; and the toolbar draws all seven of its buttons. Owner ruling 2026-09-03, which reverses decision 24 A2 (T3 C2).                                                                                                                                                                                                                                     |
| `specs/command-registry.spec.js`        | `refuses a greyed command from the menu, the toolbar and the keyboard`               | Greyed Undo, Redo and Save a copy are fired from the menu, from the toolbar, and Save a copy also from Ctrl+Shift+S. Nothing crosses the IPC boundary, counted with the `lib/ipc.js` probe, and no chooser is raised. The barrier: opening a subtitle through the toolbar straight after sends exactly one `subtitle_open`, so the zero is a refusal and not a probe that never took (T3 C2 and C3).                                                                                                                                            |
| `specs/command-registry.spec.js`        | `greys and ungreys in place, with nothing appearing or disappearing`                 | Opening a subtitle leaves every title, every menu item and every toolbar button where it was, in the same order. The one greying that moves is Save a copy, and it moves on both routes at once (T3 C2).                                                                                                                                                                                                                                                                                                                                        |
| `scripts/shutdown-check.js`             | 5 checks                                                                             | Closing the window exits 0, unsignalled, with nothing left alive in the app's process group, and with no close gate raised over a document nobody edited.                                                                                                                                                                                                                                                                                                                                                                                       |
| `scripts/close-gate-check.js`           | 12 checks                                                                            | Closing with unsaved edits asks save/discard/cancel; each answer is proved by the dialog going away, cancel keeps the app and the file, discard exits 0 leaving the file untouched, save writes the edit, moves nothing else and keeps a backup (BACKLOG N1).                                                                                                                                                                                                                                                                                   |
| `scripts/close-gate-late-edit-check.js` | 8 checks                                                                             | An edit made after the gate was answered and before the close it asked for is asked about a second time instead of being carried away in silence, and that late edit is the one that ends up on disk (gate 2; the session is read on every close and `CloseAction::AskAgain` is the branch, `lib.rs:178-199`).                                                                                                                                                                                                                                  |
| `scripts/quit-gate-check.js`            | 17 checks                                                                            | A quit that is not a window close — `AppHandle::exit`, what a menu's Quit item will call — asks what the X button asks: the unsaved-changes dialog, cancel keeping the app and the file, a second quit asking again instead of riding the cancelled answer out, discard exiting 0 with the file untouched, save writing the edit, and a clean quit still exiting (BACKLOG N6). Driven through the File menu's own Quit item from the keyboard, and through Ctrl+Q in the save branch, and red unless the app's log says the quit went that way. |
| `scripts/startup-args-check.js`         | 7 checks                                                                             | A name on the command line that is not valid Unicode costs that one name and never the launch: the window comes up, the subtitle beside it is the one opened, a real file whose name starts with a dash is opened rather than filtered away, and every argument the app refuses is named in the log (gate 2; `lib.rs:55-57` for the name that is not Unicode, `:62-69` for the dash, `:154-155` for the log).                                                                                                                                   |
| `scripts/no-display-check.js`           | 5 checks                                                                             | A launch with no display exits non-zero and not with the panic status, having printed one line naming `DISPLAY` and what to do about it, with no panic trace and no crash report (BACKLOG N4). It is the one check that runs without an X server.                                                                                                                                                                                                                                                                                               |
| `scripts/picker-thread-check.js`        | 20 checks                                                                            | Choosing a project folder and a project file leaves no thread but the main one running `gtk_main_iteration`, read with `eu-stack`, and a cancelled choice still returns as a cancellation (BACKLOG N1c). Then a second run of the app over the same data home: the chooser opens at the folder chosen before the app closed and Open project is what ran over it, a remembered folder that has been deleted is dropped and its chooser still answers, and the cancellation before the restart left the memory alone (BACKLOG N7).               |
| `scripts/mpv-context-check.js`          | 5 checks                                                                             | A `gpu-context` mpv refuses costs the request and not the window: the app still starts, the refusal is in the log, mpv falls back to the pinned `x11egl`, and the video still attaches (BACKLOG N2b).                                                                                                                                                                                                                                                                                                                                           |
| `scripts/scaled-surface-check.js`       | 5 checks                                                                             | At an integer display scale the video surface doubles with the window instead of quadrupling or standing still. It does not prove N2c's fractional case, and its header says why.                                                                                                                                                                                                                                                                                                                                                               |
| `scripts/webview-paint-check.js`        | 5 checks                                                                             | In the configuration users actually get — the NVIDIA WebKit workarounds armed by the app's own detection — the window paints instead of coming up blank, and the app's recorded decision agrees with the machine's driver state.                                                                                                                                                                                                                                                                                                                |
| `scripts/wayland-attach-check.js`       | 4 checks                                                                             | Inside a real Wayland session, with `WAYLAND_DISPLAY` left alone, mpv's own window exists inside the native surface and the surface is viewable (BACKLOG N2b).                                                                                                                                                                                                                                                                                                                                                                                  |
| `scripts/n1b-load-probe.js`             | probe, asserts nothing                                                               | One close-gate run on one branch, recorded as a line of output, so batteries of runs can answer N1b. It is not a check and must never be quoted as one.                                                                                                                                                                                                                                                                                                                                                                                         |
| `scripts/real-session-check.mjs`        | probe, asserts nothing                                                               | A saturation reading of the app's own window with and without a video loaded, on the owner's real display, so a human can judge whether the picture painted.                                                                                                                                                                                                                                                                                                                                                                                    |
| `specs/video-surface.spec.js`           | `brings the picture back after hide and show, with the video playing`                | Collapsing the stage unmaps the native surface; restoring it brings it back mapped with mpv's own window still inside it, and mpv's clock keeps advancing (BACKLOG N2). The pixels are deliberately not asserted; the spec's header says why.                                                                                                                                                                                                                                                                                                   |
| `specs/video-surface.spec.js`           | `brings the picture back with the video paused, without restarting playback`         | Same, with the video paused: the surface comes back mapped and attached with no seek, play or redraw, and the clock never moves.                                                                                                                                                                                                                                                                                                                                                                                                                |
| `specs/video-surface.spec.js`           | `survives ten hide and show cycles without leaking a surface`                        | Ten cycles leave exactly one large child window, mapped, with mpv still attached inside it.                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `specs/video-empty.spec.js`             | `leaves the stage empty and the surface unmapped before anything is opened`          | At first paint the placeholder is there and the surface is `IsUnMapped`: no opaque slab over an empty stage (BACKLOG N2, gate 1).                                                                                                                                                                                                                                                                                                                                                                                                               |
| `specs/video-empty.spec.js`             | `keeps the surface unmapped when the layout changes with no video`                   | Collapsing and restoring the stage with no video sends a real rectangle again and the surface stays unmapped: visibility follows the video, not the rectangle.                                                                                                                                                                                                                                                                                                                                                                                  |
| `specs/video-empty.spec.js`             | `keeps the surface unmapped after an open that failed`                               | A file mpv refuses leaves an error on screen, the surface unmapped, and a later layout change does not show it.                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `specs/video-occlusion.spec.js`         | `hides the surface for a File menu over the video and brings it back on close`       | Decision 1's own criterion (T8): with the video playing, the File dropdown lands over the stage rectangle — read off both elements, never assumed — the surface goes `IsUnMapped` while it is down, and closing it brings the frame back with mpv attached and the clock still advancing.                                                                                                                                                                                                                                                       |
| `specs/video-occlusion.spec.js`         | `keeps the surface hidden across the swap from the menu to the dialog it opens`      | Enter on Help > About closes one layer and opens another in one update. Twelve samples at 100 ms all read `IsUnMapped`; a poll cannot prove a negative over an interval, so the claim is worded as what was sampled. Closing the dialog brings the frame back.                                                                                                                                                                                                                                                                                  |
| `specs/video-occlusion.spec.js`         | `comes back on the rectangle measured while the menu was open`                       | While a layer is open the rectangle keeps being measured and stops being sent, so nothing can raise the surface over the layer. Shrinking the stage with the menu down and then closing it lands the frame on the new rectangle within 2 px, which is what proves the held one was sent again.                                                                                                                                                                                                                                                  |
| `specs/chooser.spec.js`                 | `leaves no field in the interface that a path can be typed into`                     | T1's promise: no text input anywhere takes a path, the rail's own question excepted (and the cue editor, which exists only while a cue is open).                                                                                                                                                                                                                                                                                                                                                                                                |
| `specs/chooser.spec.js`                 | five `... when the chooser is dismissed` checks                                      | Video, subtitle, save-a-copy, project folder and episode file: cancelling each chooser leaves the app exactly as it was, and writes nothing.                                                                                                                                                                                                                                                                                                                                                                                                    |
| `specs/current-line.spec.js`            | `shows the line the cursor is on, and follows the cursor when it moves`              | The tools column draws the cue the grid's cursor is on, and moving the cursor moves what it draws.                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `specs/current-line.spec.js`            | `commits through the command the grid commits with, and the grid row shows it`       | The only subtitle command the commit issues is `subtitle_set_text`, the grid row shows the text, and nothing is written to disk.                                                                                                                                                                                                                                                                                                                                                                                                                |
| `specs/current-line.spec.js`            | `is undone in one step, which is what a grid edit costs`                             | One Undo puts the document back where it opened and clears the unsaved marker.                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `specs/cue-structure.spec.js`           | `inserts a cue under the cursor, from the menu`                                      | Subtitles > Insert cue puts an empty cue below the cursor's row, starting where that row's cue ends and running two seconds. One `subtitle_insert` crosses the boundary and the bytes on disk do not move (M2.7 E2).                                                                                                                                                                                                                                                                                                                            |
| `specs/cue-structure.spec.js`           | `deletes the cue the cursor is on, from the menu`                                    | Subtitles > Delete cue takes the cursor's row out of the document, one `subtitle_delete` crosses, and nothing is written (M2.7 E2).                                                                                                                                                                                                                                                                                                                                                                                                             |
| `specs/cue-structure.spec.js`           | `splits a cue at the caret in the current line, from the menu`                       | The caret is walked to a named offset in the current-line box, and Split divides the text there and the times at the cue's midpoint, which is what the shell chooses when no video is open (M2.7 E3).                                                                                                                                                                                                                                                                                                                                           |
| `specs/cue-structure.spec.js`           | `merges the cursor's cue with the one after it, from the menu`                       | Merge with next joins the two halves back into one cue running from the first one's start to the second one's end (M2.7 E3).                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `specs/cue-structure.spec.js`           | `takes the four back one undo each, and puts them back one redo each`                | Four clicks on the toolbar's Undo walk the document back through each intermediate state and clear the unsaved marker; four Redos walk it forward again. One step per edit, on the stack the text edits use (M2.7).                                                                                                                                                                                                                                                                                                                             |
| `specs/cue-structure.spec.js`           | `writes nothing until a save, and the saved file reopens as what the grid showed`    | Every edit above leaves the file byte-identical to the one that was opened. The save writes them, and reopening that file through the chooser draws the same rows with the same timings (M2.7).                                                                                                                                                                                                                                                                                                                                                 |
| `specs/cue-structure.spec.js`           | `keeps the cursor and the selection on the lines they were on when rows move`        | With a three-row range selected, an insert below the cursor moves neither state off its line, and a delete pulls both up with the rows rather than letting the row that filled the gap into the selection (M2.7 E2).                                                                                                                                                                                                                                                                                                                            |
| `specs/cue-structure.spec.js`           | `never leaves the cursor past the end when the last cue is the one deleted`          | Deleting the last row puts the cursor on the new last row instead of leaving it past the end, where no row is drawn as the cursor at all and the current line says it has none (M2.7 E2).                                                                                                                                                                                                                                                                                                                                                       |
| `specs/preview.spec.js`                 | `puts a document that was open first onto a video opened after it`                   | Decision 7, one order of the pair: with the document already open, opening the video leaves mpv holding one external subtitle track, selected and visible, with the first cue's own character count at the playhead. Read back off mpv, not off our own bookkeeping.                                                                                                                                                                                                                                                                            |
| `specs/preview.spec.js`                 | `puts a document opened while the video is already loaded onto the frame`            | The other order, which is the half of the reported bug that had no test: opening a second document over a playing video changes the character count mpv reports at the playhead.                                                                                                                                                                                                                                                                                                                                                                |
| `specs/preview.spec.js`                 | `puts an edit on the frame without stacking a second subtitle track`                 | Typing over the first cue changes the count mpv reports, and the external track count stays at one: the shadow copy is re-read in place rather than added again.                                                                                                                                                                                                                                                                                                                                                                                |
| `specs/preview.spec.js`                 | `takes the document off the frame from View, and puts it back`                       | View's `Subtitles on video` turns mpv's overlay off, mpv still holding the same document underneath, and turning it back on restores it with the same line under it.                                                                                                                                                                                                                                                                                                                                                                            |
| `specs/preview.spec.js`                 | `never writes the subtitle file it is drawing from, and keeps no backup of it`       | After opening, drawing, editing and toggling, the user's file is byte-identical with the same mtime, its folder gained nothing, the backup store gained nothing, and the document is still marked unsaved.                                                                                                                                                                                                                                                                                                                                      |

Everything above runs in the `e2e` CI job except four rows, named rather than counted:
`webview-paint-check.js`, `wayland-attach-check.js`, and the two probes. `webview-paint-check.js`
needs an NVIDIA module for the branch it tests to be taken, and `wayland-attach-check.js` needs a
real Wayland socket; on a GitHub runner neither prerequisite exists, so both would prove nothing
there and `.github/workflows/ci.yml` records that omission as a decision. Both fail loudly when their
prerequisite is missing rather than skipping, so they cannot go green for the wrong reason. The two
probes are run by hand and assert nothing at all; a probe's output is evidence for a report, never a
pass.

The window title AC is covered by the **native** assertion. The document title is a second, weaker
signal kept because a blank webview is otherwise invisible to X11 assertions.

The video surface is an X11 child window with no DOM presence, so the expected rectangle comes from
the DOM (`getBoundingClientRect` times `devicePixelRatio`) and the actual one from `xwininfo`. The
surface exists and is already sized **before** any video is opened; it is only _mapped_ once a video
is ready, so the `IsViewable` check is what makes this test meaningful.

## Running it locally

```sh
sh fixtures/video/make-sample.sh     # the fixture is generated, never committed
sh scripts/fetch-model.sh            # ggml-tiny.en.bin, fetched once, never committed
pnpm e2e:build                       # tauri build --debug --no-bundle
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e           # the WebDriver spec files
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:shutdown  # the clean-close check
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:close-gate  # the unsaved-edits gate
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:close-gate-late-edit  # an edit made while the answer is in flight
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:quit-gate  # the quit that is not a window close
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:startup-args  # names the command line cannot carry
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:scale       # an integer display scale
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:picker-thread  # no second GTK thread, and the picker opens where it last landed
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:mpv-context  # a refused gpu-context costs the request, not the window
xvfb-run -a -s "-screen 0 1024x700x24" pnpm e2e:waveform-budget  # the waveform's two numbers, CONTRIBUTING.md section 7
pnpm e2e:no-display                          # no xvfb-run: this one proves what happens without a display
```

Two more have prerequisites no headless runner has, so they are run by hand and are not CI steps:

```sh
pnpm e2e:webview   # needs /sys/module/nvidia for the branch it tests to be taken
pnpm e2e:wayland   # needs a real Wayland session, so no Xvfb wrapper
```

The waveform budget has a second half that is the owner's machine's and not CI's, because a runner
is not the machine CONTRIBUTING.md section 7 names and a 24-minute fixture is not a thing to
generate on every push:

```sh
sh fixtures/video/make-waveform-fixtures.sh --with-24min
xvfb-run -a -s "-screen 0 1024x700x24" pnpm e2e:waveform-budget --with-24min
```

The screen has to hold the whole window under test. Fedora's `xvfb-run` defaults to 640x480, and on
a root window that small the fixture never reaches the ready state, so the size is passed explicitly
here and in CI. The app starts at 1024x700 and `lib/input.js`'s `resizeWindow` can grow it, so the
screen is 1920x1080: the largest window a check can ask for and still measure all of.

A check can no longer assume the window took the size it asked for. The shell measures the narrowest
width it can be drawn at and the window is held there, so at 150 per cent a request for 1024 comes
back wider under the runner's fonts. `askForWindowSize` sends the request and `waitForWindowSize`
waits for the width the caller says the window will settle at; `resizeWindow` is still the two
together for a width that will be granted.

Prerequisites, all of them dev tools rather than repo dependencies:

- `tauri-driver` — `cargo install tauri-driver --version 2.0.6 --locked`
- `WebKitWebDriver` — Fedora: `webkit2gtk4.1`. Debian/Ubuntu: `webkit2gtk-driver`.
- `xwininfo` (`x11-utils`), `xdotool`, `Xvfb` (`xvfb`), python-xlib (`python3-xlib`)
- `eu-stack` (`elfutils`), for `e2e:picker-thread` only. It ptraces the app, so that check also
  needs `kernel.yama.ptrace_scope=0`: it is a sibling of the app, not an ancestor. The check says so
  and refuses to run rather than report a process it could not read.

Environment knobs:

- `DISPLAY` is required. A missing display is a failure with a clear message, never a skip.
- `E2E_PORT` (default 4444) moves both driver ports so two runs can share a machine.
- `CARGO_TARGET_DIR` is honoured, the same way cargo honours it.
- `TAURI_DRIVER_PATH`, `WEBKIT_WEBDRIVER_PATH` override the two binaries.
- `XDG_DATA_HOME` is pointed at a fresh temp dir by the harness, so a run never touches the real one.
- `SUBLORE_WHISPER_BIN` and `SUBLORE_E2E_ASR_DIR` are **set** by `wdio.conf.js`, not read from the
  environment: the transcription spec always runs against the stand-in sidecar below.
- `SUBLORE_TEST_MODEL_DIR` points at a directory holding your own `ggml-tiny.en.bin`; the gated Rust
  suite reads the same variable. Without it the harness reads the cache `scripts/fetch-model.sh`
  writes to. The app checks the model's sha256 before every run, so this file has to be the real one;
  the harness copies it into the run's own data directory and names the command to run when it is
  missing.

Neither entry point builds anything. A missing binary or fixture fails immediately with the command
to run, because a silent four-minute rebuild inside a test hook is worse than a red line.

## Four ways to make a run tell you nothing

Each of these produced a failure that meant nothing, and cost a re-run to find out.

**Do not edit a spec file while a run is in progress.** Workers read each file as they reach it, so
a run started before an edit and finished after it mixes an old binary with new expectations. The
failure looks like a defect and is not one. Wait for the run, then edit.

**Do not pipe a run through `tail`.** The spec reporter prints a failing spec's assertion where it
happened, and a failure in the eighth of twenty-seven specs is thousands of lines above the summary.
Write the whole log to a file and grep it:

```sh
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e > /tmp/run.log 2>&1
grep -nE "✖|failing|Spec Files" /tmp/run.log
```

**`cp` is aliased to `cp -i` in this shell and blocks on a prompt.** Overwriting a file that is
already there stops and waits for a keypress nobody is going to press, so a step that looks like a
copy is a step that has not happened, and the command after it runs against the old bytes. It cost
ten minutes of a wall clock on 2026-09-05 and it has cost a whole mutation run before that, where
the mutation was never applied and the green that followed meant nothing. Copy with
`python3 -c "import shutil; shutil.copyfile(a, b)"` in anything that is not typed by hand.

**`cargo test -p <crate>` stops at the first failing test binary.** A mutation that reddens
`tests/mutation.rs` leaves `tests/session.rs` unrun, so the report undercounts what the mutation
actually broke. Use `--no-fail-fast` whenever the point of the run is to see the full blast radius.

## Four ways the instrument lied, all found by running it

**`xdotool` cannot press a function key by name on this X server.** `xdotool key F3` presses Alt
before the key, with or without `--clearmodifiers`, and the webview is told `altKey` is true, so the
shell drops it: `commandFor` refuses every press carrying Alt. F5 behaves the same way. The keymap
has `keycode 69 = F3 F3 F3 F3 F3 F3 XF86Switch_VT_3`, and xdotool resolving the keysym picks a level
that carries Alt. `pressKey` therefore presses a function key by keycode, looked up through
python-xlib, and every other key by name exactly as before. A real keyboard sends the keycode, so
this is the instrument being made to match the hardware rather than the app being bent to the
instrument. Measured 2026-09-04, on this machine under Xvfb.

**A wait measured in milliseconds is a wait that passes alone and fails in the battery.**
`waveform-follow.spec.js` paused a fixed 200 ms after a seek before reading the drawn playhead. It
passed every time the spec ran alone and failed twice in a full run, both times reading a playhead
three seconds from where the seek had put it. It waits on the transport now, which is drawn from the
position the seek set, so the slider reading the target is the render the canvas was painted in.
This is the same class as `docs/environment-anchored-assertions.md`: a number calibrated on an idle
machine is a number that is wrong on a busy one.

**A side effect more than one command produces cannot say which command ran.**
`picker-thread-check.js` proved "Add episode reached the project" by watching the size and mtime of
`project.sublore-wal`. Its keyboard walk landed on Close project instead, a clean close of a WAL
database checkpoints and removes that file, and a file that has gone is a file that changed: the
check printed `ok` on a project it had just closed, and then failed four steps later at a chooser
the closed rail could no longer reach. Two things came out of it. The commands in
`src-tauri/src/project/mod.rs` each write one line naming what they did, so a check driving the rail
without a DOM can say which command ran instead of inferring it from a side effect. And a menu walk
now states two positions, the item it wants and the item the menu's cursor starts on, because
`RailMenu` opens on the first item that can _run_ while its arrow walk steps over everything: a Down
count is a distance and never an index, and a constant that reads like an index is how the same bug
was written twice. See BACKLOG N26.

**A status line that reads the same before and after cannot say the change happened.**
`asr.spec.js`'s Discard check waited for `SRT · N cues · LF` and then read the grid. The document
being replaced was itself a transcription of the same stub run, so it had the same cue count and the
same format and that line was already what the check was waiting for: the wait was satisfied without
anything having happened, and the assertion read the old grid. It passed for weeks by winning a
race, and it lost that race the day an unrelated wait a few lines earlier changed the timing. It now
waits for the offer to take the result to go away, which is true only after the result has been
taken. The check three tests further down had the right shape already, with a comment saying so, so
this was a lesson the file had learned once and not applied twice. Same class as N26 above, and the
same rule underneath both: a guard that cannot tell "not there" from "not there yet" is a race.

## What is retried in CI, and what is never

One thing is retried and it is named here so the list does not grow by habit: `apt-get update` in
`.github/scripts/package-smoke.sh` and in the `package smoke (ubuntu)` container's own checkout
step, three attempts fifteen seconds apart. An Ubuntu mirror caught halfway through a sync answers a
`Packages.gz` one byte off the size it advertised, which took that job red on 2026-09-05 with
nothing wrong in the tree. Nothing else in that script is retried, because everything below the
index is what the check exists to find: a package whose dependencies do not resolve, a binary that
will not start, a library the bundler never declared. A retry there would turn a real failure into a
slow one.

## Running a Windows binary here

Wine is not Windows and nothing run under it is a Windows behavioural verdict. What it is good for
is the loader: a Windows build can be linked and started on this machine, and `WINEDEBUG=+loaddll`
prints every library the loader looks for and every one it does not find. That answered N29 in one
run, where the CI diagnostic had named the wrong symbol twice and each attempt cost fourteen minutes.

```sh
# Once: the pinned libmpv's own MinGW import library, the same archive install-libmpv-windows.ps1
# fetches and the same checksum.
7z x mpv-dev-x86_64-<tag>-git-<rev>.7z -o/tmp/mpv
RUSTFLAGS="-L native=/tmp/mpv" cargo test -p sublore --lib --target x86_64-pc-windows-gnu --no-run
# The DLLs the loader will look for beside the binary, which is where Windows looks first.
cp target/x86_64-pc-windows-gnu/debug/WebView2Loader.dll target/x86_64-pc-windows-gnu/debug/deps/
WINEDEBUG=+loaddll wine64 target/x86_64-pc-windows-gnu/debug/deps/sublore_lib-*.exe --list
```

The gnu linker keeps the whole graph and the MSVC linker CI uses keeps only what is referenced, so
an import present here may be absent there and the reverse. It proves loading and linking, and the
runner is still what says whether a job is green.

## Reading a CI run that looks stopped

Each check streams its output while it runs. `.github/scripts/e2e-check.sh` runs the check into
`ci-logs/<name>.log` and follows that file rather than piping the check into `tee`: node's stdout is
asynchronous to a pipe and synchronous to a file, so a runner that calls `process.exit()` after a
burst drops whatever is still buffered, which is the end of a failing check's output. Measured on a
stand-in that writes 300,022 bytes and exits: all of it reaches a file, 65,558 bytes reach a pipe.

A check that has written nothing for 20 seconds says so, then again every 15, and each step ends
with what the check returned:

```
smoke: 3m20s elapsed, no output for 21s
smoke: exit 0 after 7m37s
```

Both lines come from the wrapper, not from the check. A quiet stretch is normal: an app launch under
Xvfb is silent for about half a minute, and there is one per spec file.

Each check step also carries a `timeout-minutes` of roughly three times what it takes today, so a
check that hangs costs minutes rather than the job's whole 45. A step killed that way writes no
`ci-logs/<name>.exit`, so the verdict reports it as a check that never reported, which is what it is.

GitHub hides per-line timestamps behind `Shift+T`, per viewer, so a workflow cannot turn them on.
`gh run view <id> --log` always has them, and is the fastest way to find where a run spent its time.

## The anti-zero-test guard

A harness that runs nothing must not report success. WebdriverIO does not reliably fail a run with no
specs, so `wdio.conf.js` asserts the count itself, in `EXPECTED_TESTS` at the top of that file.

The number is not repeated here. A second copy in a document nobody runs goes stale, and this one
had: it said 71 while the suite had grown to 77.

`onComplete` throws if fewer than that many tests passed, which covers a deleted spec file, an
`it.skip`, and a spec filter that matches nothing. **Update the number when you add or remove a
test.** `scripts/shutdown-check.js` guards itself the same way with `EXPECTED_CHECKS`, and because CI
invokes it by path, deleting the file turns the step red on its own.

## Why input goes through X11, not WebDriver

WebKitWebDriver answers Element Click, Element Send Keys **and** the W3C Actions endpoint with
`unsupported operation` against a wry webview; only reads and `Execute Script` work. So the harness
clicks and types with `xdotool`, which sends real XTEST key and button events to the focused window.
That is closer to a user than synthesizing DOM events would be, and it is the only option that
exists here. WebDriver is still what reads the DOM, which is what the surface test needs.

Element coordinates come from `getBoundingClientRect` plus the toplevel's absolute origin. There is
no window manager under Xvfb, so the toplevel origin is also the viewport origin.

`clickAt` asks where the pointer is before moving it. `xdotool mousemove --sync` waits for the
pointer to leave the position it was at, so a move to the position it already holds never returns
while a window sits under it: verified here, it blocks until it is killed. Clicking the same element
twice in a row — which `asr.spec.js` does with Transcribe — is exactly that case, so the move is
skipped when the pointer is already there.

## Closing the window

`tools/close-window.py` sends an ICCCM `WM_DELETE_WINDOW` ClientMessage, which is the app's real
close path. Two things that do not work here and must not be reintroduced:

- `xdotool windowclose` is `XDestroyWindow`. It bypasses the close path entirely and currently
  segfaults the app.
- `xdotool windowquit` is a no-op without a window manager, which is what Xvfb gives us.

The toplevel is also never selected by name alone: GTK creates a 10x10 group-leader window that
answers to the same name and is listed first. `lib/x11.js` selects on the 1024x700 geometry from
`src-tauri/tauri.conf.json` and then asserts the name.

## Selectors this harness depends on

There are no `data-testid` attributes; these class names from `src/App.tsx` and `src/components/`
are the contract. Renaming one breaks the harness. T1 took three of them away with the fields they
belonged to — `.bar__input`, `.subbar__input` and `.subbar__dest` — and nothing here uses them any
more.

`.bar__button`, `.stage__surface`, `.stage__empty`, `.controls`, `.controls__button`,
`.controls__slider`, `.subbar__open`, `.subbar__save-copy`

S1's window floor adds one attribute and freezes three rows. `.shell` carries `data-minimum-width`,
the narrowest the shell says the window may be, which `interface-scale.spec.js` reads because there
is no number it could be compared against. The rows that floor is the widest of are `.menubar`,
`.toolbar` and `.cuelist__head`, listed in `src/hooks/useWindowFloor.ts`. Rename one in the markup
without renaming it there and the shell says so in the status bar, in the same line it uses when the
window refuses the floor: a row it was told to measure and could not find is a bar about to be cut
off, not a row of width zero.

`.controls` and `.controls__slider` carry more than their names here. The video panel's floor is not
a number any more: it is what that row asks for on one line, so `dividers.spec.js` and
`interface-scale.spec.js` read the row's height at both ends of its edge, and read the seek bar
against the `min-width` its own rule gives it. Take that rule away and those checks stop, loudly.

T7 replaced the project panel's buttons and fields with the rail tree and its context menu, so every
`.project__*` name is gone. What stands in their place: `.rail`, `.rail__cap`, `.rail__empty`,
`.rail__project`, `.rail__episode`, `.rail__episode--selected`, `.rail__file`,
`.rail__file--missing`, `.rail__file-name`, `.rail__missing`, `.rail__none`,
`.railmenu`, `.railmenu__item--<command>`, `.raildialog`, `.raildialog__message`,
`.raildialog__field`, `.raildialog__confirm`, `.raildialog__cancel`.

The `<command>` half of a menu item's class is the command: `create-project`, `open-project`,
`close-project`, `delete-project`, `add-episode`, `attach-media`, `attach-source`, `attach-target`,
`rename-episode`, `delete-episode`, `open-file`, `locate-file`, `detach-file`. A `.rail__file` row
carries the whole path as its `title`, because the row itself is only as wide as the rail.

The project's own messages moved into the status bar with the rest (decision 24, A1):
`.statusbar__project-message` and `.statusbar__project-error`.

Added by M2.3: `.subbar__save`, `.subbar__undo`, `.subbar__redo`, `.subbar__discard`, `.cuelist`, `.cuelist__sizer`, `.cuelist__row`, `.cuelist__row--selected`,
`.cuelist__row--comment`, `.cuelist__pos`, `.cuelist__number`, `.cuelist__start`, `.cuelist__end`,
`.cuelist__text`, `.cuelist__editor`, `.cuelist__empty`

Added by the Style and Actor columns (grid-columns-tasks.md G3): `.cuelist__style`,
`.cuelist__actor`, `.cuelist__headcell--style`, `.cuelist__headcell--actor` and
`.cuelist__headcell--text`. The first four are absent from the head and from every row whenever no
cue in the list fills that column, which is what `grid-columns.spec.js` reads a missing cell for;
the fifth is on the head's text cell always, and carries the rule that makes the head and a row
divide the row's slack the same way.

Added by T6, the cursor and the selection as two states (decision 5): `.cuelist__row--active` is the
cursor and `.cuelist__row--selected` is now membership in the selection, not the one row that was
both. `aria-selected` on a row is membership, `aria-activedescendant` on `.cuelist` names the cursor
row by its `cuelist-row-<0-based index>` id, and `.cuelist` is `aria-multiselectable`. The CPS column
is `.cuelist__cps`, carrying `.cuelist__cps--over` above the 21 cps of decision 24 A8.

Added by M3.4: `.asrbar__model`, `.asrbar__download`, `.asrbar__gpu`, `.asrbar__start`, `.asrbar__cancel`,
`.asrbar__progress`, `.asrbar__status`, `.asrbar__backend`, `.asrbar__error`, `.asrbar__cue`

`.asrbar__cue` carries `data-start` and `data-end` in milliseconds, which is how the spec checks cue
times without parsing the timecodes it renders.

Added by T2, the five regions and the status bar outside them: `.shell__chrome`, `.shell__rail`,
`.shell__video`, `.shell__tools`, `.shell__grid`, `.statusbar__document`, `.statusbar__dirty`,
`.statusbar__truncated`, `.statusbar__message`, `.statusbar__error`, `.statusbar__video-error`.
The last six carry copy that used to live in the subtitle bar and in the loose video error band:
`.subbar__status` and `.subbar__dirty` are now `.statusbar__document` and `.statusbar__dirty`, the
saved line is `.statusbar__message` on its own, `.subbar__error` is `.statusbar__error` and
`.app__error` is `.statusbar__video-error`.

Added by T3, the menu bar and the toolbar that replaced the command bars: `.menubar__title`,
`.menubar__title--file`, `.menubar__menu`, `.menubar__item`, `.menubar__item--cursor`,
`.toolbar__button`, `.toolbar__file-open-subtitle`, `.toolbar__video-open`, `.toolbar__file-save`,
`.toolbar__file-save-copy`, `.toolbar__edit-undo`, `.toolbar__edit-redo`, `.toolbar__file-discard`, `.about`,
`.about__title`, `.statusbar__chrome-error`. The toolbar carries the six commands the bars carried,
under new names: `.bar__button` is `.toolbar__video-open`, `.subbar__open` is
`.toolbar__file-open-subtitle`, and `.subbar__save`, `.subbar__save-copy`, `.subbar__undo`,
`.subbar__redo` and `.subbar__discard` are the same names under `.toolbar__`. A menu item's id is
`menuitem-<command>`, which is how a check names the item the cursor is on without reading copy.

Added by T4, which took the transcription band off the screen: `.asrpanel`, `.asrpanel__close` and
the `transcribe` menu item, last in Edit. The `.asrbar__*` names above are unchanged, but none of
them is in the DOM until the panel is open, so `asr.spec.js` opens it first. The route is keys, not
clicks: a dropdown hangs over the video rectangle, and a click there lands on the native surface
instead of the webview, measured on Linux — Alt opens File, Right moves to Edit, Up wraps to
Transcribe, Return activates. That is also why the panel takes space under the grid instead of
covering anything: until decision 1's occlusion lands (T8), an HTML layer over the video is neither
visible nor clickable.

Two other keyboard routes decide where that item may sit, and both are in File: `chrome.spec.js`
walks Open video down to Quit over the disabled pair, and `quit-gate-check.js` reaches Quit as the
last enabled item in File. An item added anywhere in File breaks one of them, which is why
Transcribe waits in Edit for the Audio title (decision 24 A2).

Added by T5, the current line in the tools column: `.currentline`, `.currentline__times`,
`.currentline__start`, `.currentline__end`, `.currentline__duration`, `.currentline__cps`,
`.currentline__cps--over`, `.currentline__text`, `.currentline__empty`. The box is a second text
field in the DOM, so it is in `chooser.spec.js`'s `ALLOWED_TEXT_FIELDS` beside the rail's question;
what that check asserts — that no field ever holds a path — is unchanged. It carries
`data-document-editor`, as `.cuelist__editor` does: the two are the document's own editors, so
Ctrl+Z and Ctrl+S inside either belong to the document rather than to the webview, which is the
distinction `editor.spec.js`'s ctrl+z check turns on.

Added by the audio panel pass of 2026-09-05: `.waveform__ruler` and `.wavebar`, `.wavebar__button`,
`.wavebar__divider` and `.wavebar__<command>`, where `<command>` is the command token the way the
toolbar's own classes carry it. The strip is deliberately not `.toolbar__button`: `command-registry`
and `chrome.spec.js` both walk every `.toolbar__button` in the document as one ordered strip, and a
second bar under that name would join it.

Both live inside `.waveform`, which is why `shell.spec.js`'s list of the tools column's children did
not change: the ruler above the wave, the strip below it, and the whole panel still exactly the
height the layout stores. That is also the reason the strip is one row that scrolls sideways rather
than a row that wraps. A wrapping strip takes the height it needs out of the panel, and the panel's
height is pinned at 128 and at a floor of 64 by `waveform-sash.spec.js`, `dividers.spec.js` and
`interface-scale.spec.js`, so every extra line would come off the wave or off the current line's own
floor.

The ruler is a canvas of its own rather than a band inside `.waveform__canvas`, and that is a
contract, not an implementation detail. `waveform.spec.js` and `waveform-view.spec.js` read the wave
as a distance from `canvas.height / 2` and `waveform-follow.spec.js` finds the playhead in row 0, so
anything else drawn inside that backing store changes what all three measure.

`waveform-timing.spec.js` and `waveform-panel.spec.js` read the markers at the middle row of the
backing store, not at row 0: every drawn boundary, the neighbouring lines' included, carries a
triangular foot at each end pointing into its own line's span, so row 0 of the end marker's column
is the foot's tip six CSS pixels to its left.

`waveform-panel.spec.js` reads the ruler's ticks one row above the band's own bottom rule, and works
out how thick that rule is from `window.devicePixelRatio`. Every size the two canvases draw is a CSS
pixel multiplied by that ratio, because both backing stores are sized in device pixels and neither
context is scaled; the rule itself is the full width at every zoom, so it is the one row that can
never answer "did the ruler change".

Three tokens the pixel checks read by name: `--marker-other` for the neighbouring lines' boundaries,
and `--wave-other`, `--wave-other-selected` and `--wave-current` for the three background tints a
line's own span can carry. Each is far enough from `--accent`, `--marker-start` and `--marker-end`
that the colour searches above cannot confuse them.

`shell.spec.js` asserts the waveform's absence as "nothing else takes space in `.shell__tools`",
never as a missing selector, and shows the reading catching a panel it inserts itself before it
reads the real column. A waveform shipped under another name walks past the second and not past the
first.

Readiness has no dedicated signal: a video is loaded when `.stage__empty` is gone **and**
`.controls__button` is enabled. A subtitle file is open when `.statusbar__document` stops saying
"No subtitle file open."; `.statusbar__error` is absent from the DOM when there is nothing wrong.

## The project spec

The restart in `project.spec.js` is `browser.reloadSession()`: WebdriverIO deletes the session, which
ends the app process, then asks `tauri-driver` for a new one, which launches the binary again. The
test proves the relaunch happened rather than assuming it, by asserting that the fresh app has no
project open before it reopens the folder. `lib/x11.js`'s `findToplevel` throws when two windows
match, so a leftover instance from the old session fails the run instead of poisoning it.

**Every path a spec supplies goes through the native chooser**, because T1 left no field to type one
into. The chooser is a separate X toplevel that WebDriver cannot see, so `lib/chooser.js` answers it
at the X level: find the toplevel by title, then Alt+Home to leave GTK's Recent list, where the
accept button is insensitive and the location entry's Return therefore reaches nothing, then Ctrl+L,
the path, and Return. One copy of that sequence, shared by the specs and by
`scripts/picker-thread-check.js` (`pnpm e2e:picker-thread`, BACKLOG N1c), which grew it. Every step is
proved by what it caused, never by the dialog closing.

One helper there answers a chooser without naming anything: `acceptChooser` presses the accept
button's mnemonic and takes what the chooser is already showing. It is how N7's check reads where a
chooser opened from outside the app, and it also carries its own discrimination — on GTK's Recent
list the accept button is insensitive, so a chooser that ignored the folder it was given cannot be
accepted at all and the check fails there.

`project.spec.js` writes only under `$SUBLORE_E2E_DATA_HOME/project`, and the subtitle it attaches is
a **copy** of `fixtures/subtitles/srt/clean/basic-lf.srt` placed in a separate user directory. That
is deliberate: the deletion test asserts on a real user file, and no committed fixture is ever within
reach of a delete path.

`editor.spec.js` replaces what a text box holds in one helper, `typeInto`, which clicks, waits for
the box to take focus and clears it with a ctrl+a of its own. `lib/input.js` is deliberately not
extended for that: it is shared with every spec, and a sequence shaped by one spec's page does not
belong in it. What does belong there is an input gesture: `dragAt` joined `clickAt` and
`doubleClickAt` because a range input reads the motion between press and release, so a drag cannot
be spelled with clicks. It walks the pointer across in steps for that reason, and releases the
button in a `finally`, since a button left down lands on whatever the next check clicks.

A row is found by the 1-based list position in its `.cuelist__pos` cell, never by DOM order or a
test-only attribute. `editor.spec.js` reads the row height from a rendered row rather than repeating
the `ROW_HEIGHT` constant, so the virtualization assertions cannot drift from the component.

Subtitle fixtures are committed, so nothing generates them. The save-as test writes into
`$SUBLORE_E2E_DATA_HOME/save-as`, and `editor.spec.js` copies `large-2000.srt` into
`$SUBLORE_E2E_DATA_HOME/editor` and edits the copy: never into the repo, never beside a fixture.

## The M2.3 performance numbers

`editor.spec.js` measures inside the page with `performance.now()`, from probes it installs through
`browser.execute` before it acts, so no production code exists to serve the test and the 250 ms poll
interval of `waitFor` is not the measurement resolution. The four budgets are open under 1 s,
scroll step mean under 32 ms, keystroke to input p95 under 50 ms, and an IPC round trip under 200 ms.
Each test logs the number it measured.

Read them honestly: this is a **debug** build under Xvfb with software rendering, so a number under
budget here is a necessary condition for the release budget, not a measurement of it. The owner's
checklist measures the release build. That checklist lives in the owner's planning archive, outside
this repository.

## The stand-in sidecar, and why the transcription spec does not run whisper

`asr.spec.js` drives the whole app: the real ffmpeg extracts real audio from `sample.mkv`, and the
real JSON parser, segmentation rule and IPC layer all run. What it does not run is whisper itself,
because that would need a 77 MB model download in every CI job and a run too short to cancel
deterministically.

In its place, `wdio.conf.js` points `SUBLORE_WHISPER_BIN` at `tools/whisper-stub.mjs`, copied into
the run's temp directory so the repository is never written to. The stub:

- prints the exact progress literal whisper prints, at a pace a control file chooses (`fast`
  finishes at once, `slow` keeps going until it is killed, which is what makes the cancel test
  deterministic instead of a race);
- writes `fixtures/asr/whisper-tiny-en.json`, a byte-exact capture of a real whisper run, where
  whisper would have written its own, so everything downstream parses genuine whisper output;
- records its pid and its command line, which is what the cancellation and CPU checks read;
- spawns nothing of its own, exactly like `whisper-cli`, so the orphan check asks about the same
  shape of process tree.

The model beside it is a real copy of `ggml-tiny.en.bin`: the app hashes a model against its
catalogue row before every run, so a stand-in of the right length would be refused, which is what the
damaged-model test asserts. The stub sidecar never opens the file.
`src-tauri/tests/asr_commands.rs` asserts against the same capture in Rust, so the two layers cannot
drift apart silently.

What this spec therefore does **not** prove: that whisper transcribes correctly, that a model
download works, or that a real Vulkan build falls back to the CPU binary. The first two live in
`crates/sublore-asr/tests/real_sidecar.rs` behind `--features sublore-asr/real-asr`, which is not
compiled by default and fails loudly rather than skipping when its prerequisites are missing: it
downloads `tiny.en` through the app's own code and transcribes a real speech fixture with a real
whisper build. **The Vulkan-to-CPU fallback is in neither.** That suite builds its `Tools` with
`whisper_gpu: None` and asks for `Compute::Cpu`, then asserts `!transcript.fell_back_to_cpu`
(`real_sidecar.rs:169-178`, `:214-215`), so it runs the CPU path deliberately and never exercises a
GPU run that fails. Nothing automated covers that fallback on any platform today.

## Why `pnpm e2e:build`, never `cargo build`

`tauri build` runs `cargo build --bins --features tauri/custom-protocol`, and that feature is what
makes the `tauri` crate serve the bundled `dist/` instead of `build.devUrl`. A plain `cargo build`
binary has the `dev` cfg instead and loads `http://localhost:1420`, which without a Vite server is a
connection error on a blank page. That is why `lib/paths.js` names `pnpm e2e:build` in its failure
message, and why running `cargo build` or `cargo test` after it invalidates the binary: both rewrite
`target/debug/sublore` without the feature.

All checks were run against a `pnpm e2e:build` binary with nothing listening on port 1420.
