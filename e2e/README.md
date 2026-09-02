# E2E harness (M0.5, extended by M1.5, M2.3, M3.4 and M4.4)

Behavioral tests that launch the real Sublore binary on a real X server and assert what a person
would see. Nothing here reads Rust or TypeScript source: the harness only drives the app and looks
at the window.

Tools the harness needs on PATH: `xdotool`, `xwininfo`, `python3` with python-xlib, and `ffmpeg`.
ffmpeg is there for the app, not for the harness: `asr.spec.js` transcribes audio the app really
extracts from `sample.mkv` with it, which is why `wdio.conf.js` requires it at load and says so in
the same line. No spec measures pixels — `lib/pixels.js`'s `saturation()` has no caller left, and
`video-surface.spec.js`'s own header says why the picture is not asserted under Xvfb. xdotool,
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

| File                                    | Test                                                                                 | Acceptance criterion it binds                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| --------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `specs/title.spec.js`                   | `native window title is Sublore`                                                     | The X11 toplevel is named `Sublore`. This is the AC test.                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `specs/title.spec.js`                   | `document title is Sublore`                                                          | The webview loaded the app document, not a blank page. Different thing from the X11 name; both are asserted.                                                                                                                                                                                                                                                                                                                                                                                            |
| `specs/video.spec.js`                   | `opens the sample fixture`                                                           | Answering the system chooser with `fixtures/video/sample.mkv` reaches the ready state with no error banner.                                                                                                                                                                                                                                                                                                                                                                                             |
| `specs/video.spec.js`                   | `sizes the native video surface over the stage`                                      | The native video child window is mapped and covers the `.stage__surface` rectangle within 2 px.                                                                                                                                                                                                                                                                                                                                                                                                         |
| `specs/video.spec.js`                   | `seeks the video to where the slider is dragged`                                     | A real press-move-release across the seek slider lands mpv at the middle of the clip, proved by playing on from there.                                                                                                                                                                                                                                                                                                                                                                                  |
| `specs/subtitle.spec.js`                | `opens an SRT fixture and shows its format and cue count`                            | Opening `fixtures/subtitles/srt/clean/basic-lf.srt` puts `SRT · 3 cues · LF` on the status line with no error.                                                                                                                                                                                                                                                                                                                                                                                          |
| `specs/subtitle.spec.js`                | `saves a byte-identical copy`                                                        | Save-as of `basic-crlf.srt` writes a file the spec then compares byte for byte with `Buffer.compare`.                                                                                                                                                                                                                                                                                                                                                                                                   |
| `specs/subtitle.spec.js`                | `opens an ASS fixture and saves a byte-identical copy`                               | `ass/clean/basic.ass` puts `ASS · 3 cues · CRLF` on the status line, and the copy matches byte for byte.                                                                                                                                                                                                                                                                                                                                                                                                |
| `specs/subtitle.spec.js`                | `opens a VTT fixture and saves a byte-identical copy`                                | `vtt/clean/basic.vtt` puts `VTT · 3 cues · LF` on the status line, and the copy matches byte for byte.                                                                                                                                                                                                                                                                                                                                                                                                  |
| `specs/subtitle.spec.js`                | `reports a malformed file readably and stays usable`                                 | `srt/malformed/missing-arrow.srt` shows an error naming line 6, and the clean fixture opens straight after.                                                                                                                                                                                                                                                                                                                                                                                             |
| `specs/subtitle.spec.js`                | `throws an unsaved edit away and writes nothing when the edit is discarded`          | Discard puts the cue back to the text it was opened with, clears the dirty marker, and leaves the file's bytes alone.                                                                                                                                                                                                                                                                                                                                                                                   |
| `specs/editor.spec.js`                  | `opens the 2000-cue fixture inside the open budget`                                  | A copy of `srt/clean/large-2000.srt` opens and the first row appears in under 1 s (CONTRIBUTING.md §7).                                                                                                                                                                                                                                                                                                                                                                                                 |
| `specs/editor.spec.js`                  | `renders only the rows in view, over a sizer as tall as the whole file`              | At three scroll positions at most 60 rows exist in the DOM, over a spacer of `2000 × row height`.                                                                                                                                                                                                                                                                                                                                                                                                       |
| `specs/editor.spec.js`                  | `scrolls a viewport at a time without falling behind`                                | Twenty scroll steps, each timed until the list shows different rows: mean under 32 ms, max under 150 ms.                                                                                                                                                                                                                                                                                                                                                                                                |
| `specs/editor.spec.js`                  | `types into a cue without the list re-rendering behind every keystroke`              | Twenty keystrokes into the inline editor: p95 keydown to input under 50 ms, max under 150 ms.                                                                                                                                                                                                                                                                                                                                                                                                           |
| `specs/editor.spec.js`                  | `commits the edit on Enter and marks the file unsaved`                               | Enter puts the typed text on the row in under 200 ms, the dirty marker appears, and nothing is written yet.                                                                                                                                                                                                                                                                                                                                                                                             |
| `specs/editor.spec.js`                  | `undoes the edit back to the original text and redoes it`                            | Ctrl+Z restores the original text in under 200 ms and clears the dirty marker; the Redo button brings it back.                                                                                                                                                                                                                                                                                                                                                                                          |
| `specs/editor.spec.js`                  | `saves the edit, and every other byte of the file is the byte that was there`        | Node compares the saved file block by block against the copy that was opened.                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `specs/editor.spec.js`                  | `reopens the saved file with the edit in it`                                         | Opening the saved file again shows the edited row and an unedited neighbour.                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `specs/editor.spec.js`                  | `saves the text still sitting in an open editor`                                     | Clicking Save with the inline editor still open writes what was typed: the blur's commit and the save both land.                                                                                                                                                                                                                                                                                                                                                                                        |
| `specs/editor.spec.js`                  | `leaves ctrl+z to the text box it was typed in and undoes one step from the toolbar` | Ctrl+Z typed into the project panel's episode box is that box's own undo, and the toolbar's Undo then steps the document exactly once.                                                                                                                                                                                                                                                                                                                                                                  |
| `specs/asr.spec.js`                     | `offers the models it knows and a compute choice`                                    | The model list holds the whole catalog, the one on disk is preselected and reads `ready`, the GPU box is there, and Transcribe is off until a video is open.                                                                                                                                                                                                                                                                                                                                            |
| `specs/asr.spec.js`                     | `transcribes the open video and shows the cues`                                      | A run over `sample.mkv` lists cues carrying words from a real whisper transcript, the sidecar was handed audio from the app's scratch directory and never the media, the scratch directory is gone afterwards, and the fixture's own bytes and mtime are untouched.                                                                                                                                                                                                                                     |
| `specs/asr.spec.js`                     | `leaves the cues it produced as the open document, unsaved and nowhere on disk`      | The finished run's cues are the document: the grid holds them, the subtitle status line counts them, the document is marked unsaved and offers only a copy to save because it has no file yet, and nothing was written beside the media or anywhere Sublore saves (BACKLOG.md M3.5).                                                                                                                                                                                                                    |
| `specs/asr.spec.js`                     | `edits a cue of the result, saves it, and reopens the file with the edit in it`      | Typing over a cue of the transcription, saving a copy of it and opening that file again shows the edit. This is M3.5's own end-to-end sentence.                                                                                                                                                                                                                                                                                                                                                         |
| `specs/asr.spec.js`                     | `asks before a transcription replaces unsaved work, and cancel keeps both`           | With an edited file open, a finished run raises the same save/discard/cancel dialog the close gate raises. Cancel leaves the document, its edits, the file on disk and the run's cues all exactly as they were.                                                                                                                                                                                                                                                                                         |
| `specs/asr.spec.js`                     | `takes the cues once the same question is answered with Discard`                     | The result is offered again, and Discard replaces the document with it: the unsaved edits are dropped and nothing is written to the file they came from.                                                                                                                                                                                                                                                                                                                                                |
| `specs/asr.spec.js`                     | `shows progress, stays usable, and leaves nothing running when cancelled`            | The bar advances past zero, the playback button still answers mid-run, and after Cancel the sidecar's pid is gone from `ps` (a killed-but-unreaped child would still be there as a zombie), no scratch directory is left, and Transcribe is available again.                                                                                                                                                                                                                                            |
| `specs/asr.spec.js`                     | `runs on the CPU when the GPU box is unticked`                                       | The status names the CPU and the sidecar's real command line carries `-ng`. The cues on screen are unsaved work by then, so this run's own replacement question is answered with Discard.                                                                                                                                                                                                                                                                                                               |
| `specs/asr.spec.js`                     | `refuses a damaged model and never hands it to the sidecar`                          | One bit is flipped in the model file, which keeps its catalogued length. The run is refused on its checksum, the sidecar is never spawned, no scratch directory is left behind, and the row offers Download instead of Transcribe.                                                                                                                                                                                                                                                                      |
| `specs/project.spec.js`                 | `creates a project in an empty folder`                                               | Choosing a fresh temp folder in the chooser and clicking Create puts that folder on the status line and writes `project.sublore`.                                                                                                                                                                                                                                                                                                                                                                       |
| `specs/project.spec.js`                 | `adds an episode and attaches a subtitle file to it`                                 | The episode is listed, the attached path is listed under it, and the user's file is byte-identical afterwards.                                                                                                                                                                                                                                                                                                                                                                                          |
| `specs/project.spec.js`                 | `still lists the episode and its file after the app is restarted`                    | The AC's restart: the session is deleted and a new one launches the binary again, and reopening the folder lists both.                                                                                                                                                                                                                                                                                                                                                                                  |
| `specs/project.spec.js`                 | `reports a folder that holds no project and stays usable`                            | Opening an empty folder shows the `noProjectHere` sentence, writes nothing there, and the real project reopens after it.                                                                                                                                                                                                                                                                                                                                                                                |
| `specs/project.spec.js`                 | `deletes the project without touching the files it points at`                        | `project.sublore` is gone, the attached subtitle outside the folder is byte-identical, and the folder itself still exists.                                                                                                                                                                                                                                                                                                                                                                              |
| `scripts/shutdown-check.js`             | 5 checks                                                                             | Closing the window exits 0, unsignalled, with nothing left alive in the app's process group, and with no close gate raised over a document nobody edited.                                                                                                                                                                                                                                                                                                                                               |
| `scripts/close-gate-check.js`           | 12 checks                                                                            | Closing with unsaved edits asks save/discard/cancel; each answer is proved by the dialog going away, cancel keeps the app and the file, discard exits 0 leaving the file untouched, save writes the edit, moves nothing else and keeps a backup (BACKLOG N1).                                                                                                                                                                                                                                           |
| `scripts/close-gate-late-edit-check.js` | 8 checks                                                                             | An edit made after the gate was answered and before the close it asked for is asked about a second time instead of being carried away in silence, and that late edit is the one that ends up on disk (gate 2; the session is read on every close and `CloseAction::AskAgain` is the branch, `lib.rs:178-199`).                                                                                                                                                                                          |
| `scripts/quit-gate-check.js`            | 17 checks                                                                            | A quit that is not a window close — `AppHandle::exit`, what a menu's Quit item will call — asks what the X button asks: the unsaved-changes dialog, cancel keeping the app and the file, a second quit asking again instead of riding the cancelled answer out, discard exiting 0 with the file untouched, save writing the edit, and a clean quit still exiting (BACKLOG N6). Driven by the debug-only `SUBLORE_QUIT_ON_FILE` hook, and red unless the log says the quit went that way.                |
| `scripts/startup-args-check.js`         | 7 checks                                                                             | A name on the command line that is not valid Unicode costs that one name and never the launch: the window comes up, the subtitle beside it is the one opened, a real file whose name starts with a dash is opened rather than filtered away, and every argument the app refuses is named in the log (gate 2; `lib.rs:55-57` for the name that is not Unicode, `:62-69` for the dash, `:154-155` for the log).                                                                                           |
| `scripts/no-display-check.js`           | 5 checks                                                                             | A launch with no display exits non-zero and not with the panic status, having printed one line naming `DISPLAY` and what to do about it, with no panic trace and no crash report (BACKLOG N4). It is the one check that runs without an X server.                                                                                                                                                                                                                                                       |
| `scripts/picker-thread-check.js`        | 14 checks                                                                            | Choosing a project folder and a project file leaves no thread but the main one running `gtk_main_iteration`, read with `eu-stack`, and a cancelled choice still returns as a cancellation (BACKLOG N1c). Then a second run of the app over the same data home: the folder chooser opens at the folder chosen before the app was closed, a remembered folder that has been deleted is dropped and its chooser still answers, and the cancellation before the restart left the memory alone (BACKLOG N7). |
| `scripts/mpv-context-check.js`          | 5 checks                                                                             | A `gpu-context` mpv refuses costs the request and not the window: the app still starts, the refusal is in the log, mpv falls back to the pinned `x11egl`, and the video still attaches (BACKLOG N2b).                                                                                                                                                                                                                                                                                                   |
| `scripts/scaled-surface-check.js`       | 5 checks                                                                             | At an integer display scale the video surface doubles with the window instead of quadrupling or standing still. It does not prove N2c's fractional case, and its header says why.                                                                                                                                                                                                                                                                                                                       |
| `scripts/webview-paint-check.js`        | 5 checks                                                                             | In the configuration users actually get — the NVIDIA WebKit workarounds armed by the app's own detection — the window paints instead of coming up blank, and the app's recorded decision agrees with the machine's driver state.                                                                                                                                                                                                                                                                        |
| `scripts/wayland-attach-check.js`       | 4 checks                                                                             | Inside a real Wayland session, with `WAYLAND_DISPLAY` left alone, mpv's own window exists inside the native surface and the surface is viewable (BACKLOG N2b).                                                                                                                                                                                                                                                                                                                                          |
| `scripts/n1b-load-probe.js`             | probe, asserts nothing                                                               | One close-gate run on one branch, recorded as a line of output, so batteries of runs can answer N1b. It is not a check and must never be quoted as one.                                                                                                                                                                                                                                                                                                                                                 |
| `scripts/real-session-check.mjs`        | probe, asserts nothing                                                               | A saturation reading of the app's own window with and without a video loaded, on the owner's real display, so a human can judge whether the picture painted.                                                                                                                                                                                                                                                                                                                                            |
| `specs/video-surface.spec.js`           | `brings the picture back after hide and show, with the video playing`                | Collapsing the stage unmaps the native surface; restoring it brings it back mapped with mpv's own window still inside it, and mpv's clock keeps advancing (BACKLOG N2). The pixels are deliberately not asserted; the spec's header says why.                                                                                                                                                                                                                                                           |
| `specs/video-surface.spec.js`           | `brings the picture back with the video paused, without restarting playback`         | Same, with the video paused: the surface comes back mapped and attached with no seek, play or redraw, and the clock never moves.                                                                                                                                                                                                                                                                                                                                                                        |
| `specs/video-surface.spec.js`           | `survives ten hide and show cycles without leaking a surface`                        | Ten cycles leave exactly one large child window, mapped, with mpv still attached inside it.                                                                                                                                                                                                                                                                                                                                                                                                             |
| `specs/video-empty.spec.js`             | `leaves the stage empty and the surface unmapped before anything is opened`          | At first paint the placeholder is there and the surface is `IsUnMapped`: no opaque slab over an empty stage (BACKLOG N2, gate 1).                                                                                                                                                                                                                                                                                                                                                                       |
| `specs/video-empty.spec.js`             | `keeps the surface unmapped when the layout changes with no video`                   | Collapsing and restoring the stage with no video sends a real rectangle again and the surface stays unmapped: visibility follows the video, not the rectangle.                                                                                                                                                                                                                                                                                                                                          |
| `specs/video-empty.spec.js`             | `keeps the surface unmapped after an open that failed`                               | A file mpv refuses leaves an error on screen, the surface unmapped, and a later layout change does not show it.                                                                                                                                                                                                                                                                                                                                                                                         |
| `specs/chooser.spec.js`                 | `leaves no field in the interface that a path can be typed into`                     | T1's promise: no text input anywhere takes a path, the project panel's episode-name box excepted (and the cue editor, which exists only while a cue is open).                                                                                                                                                                                                                                                                                                                                           |
| `specs/chooser.spec.js`                 | five `... when the chooser is dismissed` checks                                      | Video, subtitle, save-a-copy, project folder and episode file: cancelling each chooser leaves the app exactly as it was, and writes nothing.                                                                                                                                                                                                                                                                                                                                                            |

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
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e           # the nine WebDriver spec files
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:shutdown  # the clean-close check
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:close-gate  # the unsaved-edits gate
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:close-gate-late-edit  # an edit made while the answer is in flight
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:quit-gate  # the quit that is not a window close
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:startup-args  # names the command line cannot carry
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:scale       # an integer display scale
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:picker-thread  # no second GTK thread, and the picker opens where it last landed
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm e2e:mpv-context  # a refused gpu-context costs the request, not the window
pnpm e2e:no-display                          # no xvfb-run: this one proves what happens without a display
```

Two more have prerequisites no headless runner has, so they are run by hand and are not CI steps:

```sh
pnpm e2e:webview   # needs /sys/module/nvidia for the branch it tests to be taken
pnpm e2e:wayland   # needs a real Wayland session, so no Xvfb wrapper
```

The screen has to hold the whole window under test. Fedora's `xvfb-run` defaults to 640x480, and on
a root window that small the fixture never reaches the ready state, so the size is passed explicitly
here and in CI. The app starts at 1024x700 and `lib/input.js`'s `resizeWindow` can grow it, so the
screen is 1920x1080: the largest window a check can ask for and still measure all of.

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

## The anti-zero-test guard

A harness that runs nothing must not report success. WebdriverIO does not reliably fail a run with no
specs, so `wdio.conf.js` asserts the count itself:

```js
const EXPECTED_TESTS = 53;
```

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

`.bar__button`, `.stage__surface`, `.stage__empty`, `.controls__button`,
`.controls__slider`, `.subbar__open`, `.subbar__save-copy`,
`.project__path`, `.project__choose-folder`, `.project__create`, `.project__open`, `.project__delete`,
`.project__status`, `.project__error`, `.project__episodes`, `.project__episode`,
`.project__episode--selected`, `.project__episode-title`, `.project__files`, `.project__file`,
`.project__new-episode`, `.project__add-episode`, `.project__file-path`, `.project__choose-file`,
`.project__role-media`, `.project__role-source`, `.project__role-target`, `.project__attach`

Added by M2.3: `.subbar__save`, `.subbar__undo`, `.subbar__redo`, `.subbar__discard`, `.cuelist`, `.cuelist__sizer`, `.cuelist__row`, `.cuelist__row--selected`,
`.cuelist__row--comment`, `.cuelist__pos`, `.cuelist__number`, `.cuelist__start`, `.cuelist__end`,
`.cuelist__text`, `.cuelist__editor`, `.cuelist__empty`

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
