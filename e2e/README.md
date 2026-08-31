# E2E harness (M0.5, extended by M1.5, M2.3, M3.4 and M4.4)

Behavioral tests that launch the real Sublore binary on a real X server and assert what a person
would see. Nothing here reads Rust or TypeScript source: the harness only drives the app and looks
at the window.

Tools the harness needs on PATH: `xdotool`, `xwininfo`, `python3` with python-xlib, and `ffmpeg`.
ffmpeg measures whether the video surface is showing a picture, and it is already a build
dependency; nothing that runs in CI uses ImageMagick, because `magick` ships only with version 7 and
the CI runner has 6. xdotool, xwininfo and ffmpeg are each checked before any spec starts, so a missing one is a
sentence naming it rather than a timeout inside whichever spec needed it first.

Two things that run only on the owner's machine do use ImageMagick's `import`, which exists in
version 6 too: `webview-paint-check.js` and the `real-session-check.mjs` probe. Under rootless
XWayland an X root grab reads black whatever the app draws, and `import -window <id>` reads the
window itself. The check names `import` as a prerequisite before it launches
anything; the probe reports the failed capture's exit status instead.

## What each spec proves

| File                                    | Test                                                                                | Acceptance criterion it binds                                                                                                                                                                                                                                                                                                            |
| --------------------------------------- | ----------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `specs/title.spec.js`                   | `native window title is Sublore`                                                    | The X11 toplevel is named `Sublore`. This is the AC test.                                                                                                                                                                                                                                                                                |
| `specs/title.spec.js`                   | `document title is Sublore`                                                         | The webview loaded the app document, not a blank page. Different thing from the X11 name; both are asserted.                                                                                                                                                                                                                             |
| `specs/video.spec.js`                   | `opens the sample fixture`                                                          | Typing `fixtures/video/sample.mkv` into the open bar and clicking Open reaches the ready state with no error banner.                                                                                                                                                                                                                     |
| `specs/video.spec.js`                   | `sizes the native video surface over the stage`                                     | The native video child window is mapped and covers the `.stage__surface` rectangle within 2 px.                                                                                                                                                                                                                                          |
| `specs/subtitle.spec.js`                | `opens an SRT fixture and shows its format and cue count`                           | Opening `fixtures/subtitles/srt/clean/basic-lf.srt` puts `SRT · 3 cues · LF` on the status line with no error.                                                                                                                                                                                                                           |
| `specs/subtitle.spec.js`                | `saves a byte-identical copy`                                                       | Save-as of `basic-crlf.srt` writes a file the spec then compares byte for byte with `Buffer.compare`.                                                                                                                                                                                                                                    |
| `specs/subtitle.spec.js`                | `reports a malformed file readably and stays usable`                                | `srt/malformed/missing-arrow.srt` shows an error naming line 6, and the clean fixture opens straight after.                                                                                                                                                                                                                              |
| `specs/editor.spec.js`                  | `opens the 2000-cue fixture inside the open budget`                                 | A copy of `srt/clean/large-2000.srt` opens and the first row appears in under 1 s (CONTRIBUTING.md §7).                                                                                                                                                                                                                                  |
| `specs/editor.spec.js`                  | `renders only the rows in view, over a sizer as tall as the whole file`             | At three scroll positions at most 60 rows exist in the DOM, over a spacer of `2000 × row height`.                                                                                                                                                                                                                                        |
| `specs/editor.spec.js`                  | `scrolls a viewport at a time without falling behind`                               | Twenty scroll steps, each timed until the list shows different rows: mean under 32 ms, max under 150 ms.                                                                                                                                                                                                                                 |
| `specs/editor.spec.js`                  | `types into a cue without the list re-rendering behind every keystroke`             | Twenty keystrokes into the inline editor: p95 keydown to input under 50 ms, max under 150 ms.                                                                                                                                                                                                                                            |
| `specs/editor.spec.js`                  | `commits the edit on Enter and marks the file unsaved`                              | Enter puts the typed text on the row in under 200 ms, the dirty marker appears, and nothing is written yet.                                                                                                                                                                                                                              |
| `specs/editor.spec.js`                  | `undoes the edit back to the original text and redoes it`                           | Ctrl+Z restores the original text in under 200 ms and clears the dirty marker; the Redo button brings it back.                                                                                                                                                                                                                           |
| `specs/editor.spec.js`                  | `saves the edit, and every other byte of the file is the byte that was there`       | Node compares the saved file block by block against the copy that was opened.                                                                                                                                                                                                                                                            |
| `specs/editor.spec.js`                  | `reopens the saved file with the edit in it`                                        | Opening the saved file again shows the edited row and an unedited neighbour.                                                                                                                                                                                                                                                             |
| `specs/editor.spec.js`                  | `saves the text still sitting in an open editor`                                    | Clicking Save with the inline editor still open writes what was typed: the blur's commit and the save both land.                                                                                                                                                                                                                         |
| `specs/editor.spec.js`                  | `leaves ctrl+z to the destination box and undoes exactly one step from the toolbar` | Ctrl+Z inside the save-as field is the field's own undo, and the toolbar's Undo steps the document once.                                                                                                                                                                                                                                 |
| `specs/asr.spec.js`                     | `offers the models it knows and a compute choice`                                   | The model list holds the whole catalog, the one on disk is preselected and reads `ready`, the GPU box is there, and Transcribe is off until a video is open.                                                                                                                                                                             |
| `specs/asr.spec.js`                     | `transcribes the open video and shows the cues`                                     | A run over `sample.mkv` lists cues carrying words from a real whisper transcript, the sidecar was handed audio from the app's scratch directory and never the media, the scratch directory is gone afterwards, and the fixture's own bytes and mtime are untouched.                                                                      |
| `specs/asr.spec.js`                     | `shows progress, stays usable, and leaves nothing running when cancelled`           | The bar advances past zero, the playback button still answers mid-run, and after Cancel the sidecar's pid is gone from `ps` (a killed-but-unreaped child would still be there as a zombie), no scratch directory is left, and Transcribe is available again.                                                                             |
| `specs/asr.spec.js`                     | `runs on the CPU when the GPU box is unticked`                                      | The status names the CPU and the sidecar's real command line carries `-ng`.                                                                                                                                                                                                                                                              |
| `specs/asr.spec.js`                     | `refuses a damaged model and never hands it to the sidecar`                         | One bit is flipped in the model file, which keeps its catalogued length. The run is refused on its checksum, the sidecar is never spawned, no scratch directory is left behind, and the row offers Download instead of Transcribe.                                                                                                       |
| `specs/project.spec.js`                 | `creates a project in an empty folder`                                              | Typing a fresh temp folder and clicking Create puts that folder on the status line and writes `project.sublore`.                                                                                                                                                                                                                         |
| `specs/project.spec.js`                 | `adds an episode and attaches a subtitle file to it`                                | The episode is listed, the attached path is listed under it, and the user's file is byte-identical afterwards.                                                                                                                                                                                                                           |
| `specs/project.spec.js`                 | `still lists the episode and its file after the app is restarted`                   | The AC's restart: the session is deleted and a new one launches the binary again, and reopening the folder lists both.                                                                                                                                                                                                                   |
| `specs/project.spec.js`                 | `reports a folder that holds no project and stays usable`                           | Opening an empty folder shows the `noProjectHere` sentence, writes nothing there, and the real project reopens after it.                                                                                                                                                                                                                 |
| `specs/project.spec.js`                 | `deletes the project without touching the files it points at`                       | `project.sublore` is gone, the attached subtitle outside the folder is byte-identical, and the folder itself still exists.                                                                                                                                                                                                               |
| `scripts/shutdown-check.js`             | 5 checks                                                                            | Closing the window exits 0, unsignalled, with nothing left alive in the app's process group, and with no close gate raised over a document nobody edited.                                                                                                                                                                                |
| `scripts/close-gate-check.js`           | 12 checks                                                                           | Closing with unsaved edits asks save/discard/cancel; each answer is proved by the dialog going away, cancel keeps the app and the file, discard exits 0 leaving the file untouched, save writes the edit, moves nothing else and keeps a backup (BACKLOG N1).                                                                            |
| `scripts/close-gate-late-edit-check.js` | 8 checks                                                                            | An edit made after the gate was answered and before the close it asked for is asked about a second time instead of being carried away in silence, and that late edit is the one that ends up on disk (gate 2, `lib.rs:138`).                                                                                                             |
| `scripts/startup-args-check.js`         | 6 checks                                                                            | A name on the command line that is not valid Unicode costs that one name and never the launch: the window comes up, the subtitle beside it is the one opened, a real file whose name starts with a dash is opened rather than filtered away, and every argument the app refuses is named in the log (gate 2, `lib.rs:75`, `:43`, `:45`). |
| `scripts/scaled-surface-check.js`       | 5 checks                                                                            | At an integer display scale the video surface doubles with the window instead of quadrupling or standing still. It does not prove N2c's fractional case, and its header says why.                                                                                                                                                        |
| `scripts/webview-paint-check.js`        | 5 checks                                                                            | In the configuration users actually get — the NVIDIA WebKit workarounds armed by the app's own detection — the window paints instead of coming up blank, and the app's recorded decision agrees with the machine's driver state.                                                                                                         |
| `scripts/wayland-attach-check.js`       | 4 checks                                                                            | Inside a real Wayland session, with `WAYLAND_DISPLAY` left alone, mpv's own window exists inside the native surface and the surface is viewable (BACKLOG N2b).                                                                                                                                                                           |
| `scripts/n1b-load-probe.js`             | probe, asserts nothing                                                              | One close-gate run on one branch, recorded as a line of output, so batteries of runs can answer N1b. It is not a check and must never be quoted as one.                                                                                                                                                                                  |
| `scripts/real-session-check.mjs`        | probe, asserts nothing                                                              | A saturation reading of the app's own window with and without a video loaded, on the owner's real display, so a human can judge whether the picture painted.                                                                                                                                                                             |
| `specs/video-surface.spec.js`           | `brings the picture back after hide and show, with the video playing`               | Collapsing the stage unmaps the native surface; restoring it brings the picture back and mpv's clock keeps advancing (BACKLOG N2).                                                                                                                                                                                                       |
| `specs/video-surface.spec.js`           | `brings the picture back with the video paused, without restarting playback`        | Same, with the video paused: the frame returns with no seek, play or redraw, and the clock never moves.                                                                                                                                                                                                                                  |
| `specs/video-surface.spec.js`           | `survives ten hide and show cycles without leaking a surface`                       | Ten cycles leave exactly one surface, still showing a picture.                                                                                                                                                                                                                                                                           |

Everything above runs in the `e2e` CI job except the last four rows. `webview-paint-check.js` needs
an NVIDIA module for the branch it tests to be taken, and `wayland-attach-check.js` needs a real
Wayland socket; on a GitHub runner neither prerequisite exists, so both would prove nothing there and
`.github/workflows/ci.yml` records that omission as a decision. Both fail loudly when their
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
xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e           # the four WebDriver specs
xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e:shutdown  # the clean-close check
xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e:close-gate  # the unsaved-edits gate
xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e:close-gate-late-edit  # an edit made while the answer is in flight
xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e:startup-args  # names the command line cannot carry
xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e:scale       # an integer display scale
xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e:picker-thread  # the picker starts no second GTK thread
```

Two more have prerequisites no headless runner has, so they are run by hand and are not CI steps:

```sh
pnpm e2e:webview   # needs /sys/module/nvidia for the branch it tests to be taken
pnpm e2e:wayland   # needs a real Wayland session, so no Xvfb wrapper
```

The screen has to be bigger than the 1024x700 window. Fedora's `xvfb-run` defaults to 640x480, and
on a root window that small the fixture never reaches the ready state, so the size is passed
explicitly here and in CI.

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
const EXPECTED_TESTS = 30;
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
are the contract. Renaming one breaks the harness.

`.bar__input`, `.bar__button`, `.app__error`, `.stage__surface`, `.stage__empty`, `.controls__button`,
`.subbar__input`, `.subbar__open`, `.subbar__dest`, `.subbar__save`, `.subbar__status`, `.subbar__error`,
`.project__path`, `.project__choose-folder`, `.project__create`, `.project__open`, `.project__delete`,
`.project__status`, `.project__error`, `.project__episodes`, `.project__episode`,
`.project__episode--selected`, `.project__episode-title`, `.project__files`, `.project__file`,
`.project__new-episode`, `.project__add-episode`, `.project__file-path`, `.project__choose-file`,
`.project__role-media`, `.project__role-source`, `.project__role-target`, `.project__attach`

Added by M2.3: `.subbar__savefile`, `.subbar__undo`, `.subbar__redo`, `.subbar__discard`, `.subbar__dirty`,
`.subbar__truncated`, `.cuelist`, `.cuelist__sizer`, `.cuelist__row`, `.cuelist__row--selected`,
`.cuelist__row--comment`, `.cuelist__pos`, `.cuelist__number`, `.cuelist__start`, `.cuelist__end`,
`.cuelist__text`, `.cuelist__editor`, `.cuelist__empty`

Added by M3.4: `.asrbar__model`, `.asrbar__download`, `.asrbar__gpu`, `.asrbar__start`, `.asrbar__cancel`,
`.asrbar__progress`, `.asrbar__status`, `.asrbar__backend`, `.asrbar__error`, `.asrbar__cue`

`.asrbar__cue` carries `data-start` and `data-end` in milliseconds, which is how the spec checks cue
times without parsing the timecodes it renders.

Readiness has no dedicated signal: a video is loaded when `.stage__empty` is gone **and**
`.controls__button` is enabled. A subtitle file is open when `.subbar__status` stops saying
"No subtitle file open."; `.subbar__error` is absent from the DOM when there is nothing wrong.

## The project spec

The restart in `project.spec.js` is `browser.reloadSession()`: WebdriverIO deletes the session, which
ends the app process, then asks `tauri-driver` for a new one, which launches the binary again. The
test proves the relaunch happened rather than assuming it, by asserting that the fresh app has no
project open before it reopens the folder. `lib/x11.js`'s `findToplevel` throws when two windows
match, so a leftover instance from the old session fails the run instead of poisoning it.

**No WebdriverIO spec may click `.project__choose-folder` or `.project__choose-file`.** Those open a
native dialog, and the suite has nobody to answer it: the run would hang until it timed out. Every
path the spec supplies goes through the text field beside the button, which is how `VideoOpenBar`
and `SubtitleBar` are driven too.

One script does click them, and it is the only one that may: `scripts/picker-thread-check.js`
(`pnpm e2e:picker-thread`, BACKLOG N1c). It answers the GTK chooser from the keyboard — Alt+Home to
leave GTK's Recent list, where the accept button is insensitive and the location entry's Return
therefore reaches nothing, then Ctrl+L, the path, and Return — and proves the answer by what it
caused, never by the dialog closing.

`project.spec.js` writes only under `$SUBLORE_E2E_DATA_HOME/project`, and the subtitle it attaches is
a **copy** of `fixtures/subtitles/srt/clean/basic-lf.srt` placed in a separate user directory. That
is deliberate: the deletion test asserts on a real user file, and no committed fixture is ever within
reach of a delete path.

`subtitle.spec.js` and `editor.spec.js` type several different paths through one field, so each
clears the field with a ctrl+a of its own before typing. `lib/input.js` is deliberately not extended
for that: it is shared with every spec, and each spec that needs it keeps its own three lines.

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
checklist measures the release build (WORKFLOW §6).

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

What this spec therefore does **not** prove: that whisper transcribes correctly, that a real Vulkan
build falls back to the CPU binary, and that a model download works. Those live in
`crates/sublore-asr/tests/real_sidecar.rs` behind `--features sublore-asr/real-asr`, which is not
compiled by default and fails loudly rather than skipping when its prerequisites are missing.

## Why `pnpm e2e:build`, never `cargo build`

`tauri build` runs `cargo build --bins --features tauri/custom-protocol`, and that feature is what
makes the `tauri` crate serve the bundled `dist/` instead of `build.devUrl`. A plain `cargo build`
binary has the `dev` cfg instead and loads `http://localhost:1420`, which without a Vite server is a
connection error on a blank page. That is why `lib/paths.js` names `pnpm e2e:build` in its failure
message, and why running `cargo build` or `cargo test` after it invalidates the binary: both rewrite
`target/debug/sublore` without the feature.

All checks were run against a `pnpm e2e:build` binary with nothing listening on port 1420.
