# E2E harness (M0.5, extended by M1.5 and M2.3)

Behavioral tests that launch the real Sublore binary on a real X server and assert what a person
would see. Nothing here reads Rust or TypeScript source: the harness only drives the app and looks
at the window.

## What each spec proves

| File                        | Test                                                                          | Acceptance criterion it binds                                                                                        |
| --------------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `specs/title.spec.js`       | `native window title is Sublore`                                              | The X11 toplevel is named `Sublore`. This is the AC test.                                                            |
| `specs/title.spec.js`       | `document title is Sublore`                                                   | The webview loaded the app document, not a blank page. Different thing from the X11 name; both are asserted.         |
| `specs/video.spec.js`       | `opens the sample fixture`                                                    | Typing `fixtures/video/sample.mkv` into the open bar and clicking Open reaches the ready state with no error banner. |
| `specs/video.spec.js`       | `sizes the native video surface over the stage`                               | The native video child window is mapped and covers the `.stage__surface` rectangle within 2 px.                      |
| `specs/subtitle.spec.js`    | `opens an SRT fixture and shows its format and cue count`                     | Opening `fixtures/subtitles/srt/clean/basic-lf.srt` puts `SRT · 3 cues · LF` on the status line with no error.       |
| `specs/subtitle.spec.js`    | `saves a byte-identical copy`                                                 | Save-as of `basic-crlf.srt` writes a file the spec then compares byte for byte with `Buffer.compare`.                |
| `specs/subtitle.spec.js`    | `reports a malformed file readably and stays usable`                          | `srt/malformed/missing-arrow.srt` shows an error naming line 6, and the clean fixture opens straight after.          |
| `specs/editor.spec.js`      | `opens the 2000-cue fixture inside the open budget`                           | A copy of `srt/clean/large-2000.srt` opens and the first row appears in under 1 s (CLAUDE §7).                       |
| `specs/editor.spec.js`      | `renders only the rows in view, over a sizer as tall as the whole file`       | At three scroll positions at most 60 rows exist in the DOM, over a spacer of `2000 × row height`.                    |
| `specs/editor.spec.js`      | `scrolls a viewport at a time without falling behind`                         | Twenty scroll steps, each timed until the list shows different rows: mean under 32 ms, max under 150 ms.             |
| `specs/editor.spec.js`      | `types into a cue without the list re-rendering behind every keystroke`       | Twenty keystrokes into the inline editor: p95 keydown to input under 50 ms, max under 150 ms.                        |
| `specs/editor.spec.js`      | `commits the edit on Enter and marks the file unsaved`                        | Enter puts the typed text on the row in under 200 ms, the dirty marker appears, and nothing is written yet.          |
| `specs/editor.spec.js`      | `undoes the edit back to the original text and redoes it`                     | Ctrl+Z restores the original text in under 200 ms and clears the dirty marker; the Redo button brings it back.       |
| `specs/editor.spec.js`      | `saves the edit, and every other byte of the file is the byte that was there` | Node compares the saved file block by block against the copy that was opened.                                        |
| `specs/editor.spec.js`      | `reopens the saved file with the edit in it`                                  | Opening the saved file again shows the edited row and an unedited neighbour.                                         |
| `specs/editor.spec.js`      | `saves the text still sitting in an open editor`                              | Clicking Save with the inline editor still open writes what was typed: the blur's commit and the save both land.     |
| `scripts/shutdown-check.js` | 4 checks                                                                      | Closing the window exits 0, unsignalled, with nothing left alive in the app's process group.                         |

The window title AC is covered by the **native** assertion. The document title is a second, weaker
signal kept because a blank webview is otherwise invisible to X11 assertions.

The video surface is an X11 child window with no DOM presence, so the expected rectangle comes from
the DOM (`getBoundingClientRect` times `devicePixelRatio`) and the actual one from `xwininfo`. The
surface exists and is already sized **before** any video is opened; it is only _mapped_ once a video
is ready, so the `IsViewable` check is what makes this test meaningful.

## Running it locally

```sh
sh fixtures/video/make-sample.sh     # the fixture is generated, never committed
pnpm e2e:build                       # tauri build --debug --no-bundle
xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e           # the four WebDriver specs
xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e:shutdown  # the clean-close check
```

The screen has to be bigger than the 1024x700 window. Fedora's `xvfb-run` defaults to 640x480, and
on a root window that small the fixture never reaches the ready state, so the size is passed
explicitly here and in CI.

Prerequisites, all of them dev tools rather than repo dependencies:

- `tauri-driver` — `cargo install tauri-driver --version 2.0.6 --locked`
- `WebKitWebDriver` — Fedora: `webkit2gtk4.1`. Debian/Ubuntu: `webkit2gtk-driver`.
- `xwininfo` (`x11-utils`), `xdotool`, `Xvfb` (`xvfb`), python-xlib (`python3-xlib`)

Environment knobs:

- `DISPLAY` is required. A missing display is a failure with a clear message, never a skip.
- `E2E_PORT` (default 4444) moves both driver ports so two runs can share a machine.
- `CARGO_TARGET_DIR` is honoured, the same way cargo honours it.
- `TAURI_DRIVER_PATH`, `WEBKIT_WEBDRIVER_PATH` override the two binaries.
- `XDG_DATA_HOME` is pointed at a fresh temp dir by the harness, so a run never touches the real one.

Neither entry point builds anything. A missing binary or fixture fails immediately with the command
to run, because a silent four-minute rebuild inside a test hook is worse than a red line.

## The anti-zero-test guard

A harness that runs nothing must not report success. WebdriverIO does not reliably fail a run with no
specs, so `wdio.conf.js` asserts the count itself:

```js
const EXPECTED_TESTS = 16;
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
`.subbar__input`, `.subbar__open`, `.subbar__dest`, `.subbar__save`, `.subbar__status`, `.subbar__error`

Added by M2.3: `.subbar__savefile`, `.subbar__undo`, `.subbar__redo`, `.subbar__discard`, `.subbar__dirty`,
`.subbar__truncated`, `.cuelist`, `.cuelist__sizer`, `.cuelist__row`, `.cuelist__row--selected`,
`.cuelist__row--comment`, `.cuelist__pos`, `.cuelist__number`, `.cuelist__start`, `.cuelist__end`,
`.cuelist__text`, `.cuelist__editor`, `.cuelist__empty`

Readiness has no dedicated signal: a video is loaded when `.stage__empty` is gone **and**
`.controls__button` is enabled. A subtitle file is open when `.subbar__status` stops saying
"No subtitle file open."; `.subbar__error` is absent from the DOM when there is nothing wrong.

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

## Why `pnpm e2e:build`, never `cargo build`

`tauri build` runs `cargo build --bins --features tauri/custom-protocol`, and that feature is what
makes the `tauri` crate serve the bundled `dist/` instead of `build.devUrl`. A plain `cargo build`
binary has the `dev` cfg instead and loads `http://localhost:1420`, which without a Vite server is a
connection error on a blank page. That is why `lib/paths.js` names `pnpm e2e:build` in its failure
message, and why running `cargo build` or `cargo test` after it invalidates the binary: both rewrite
`target/debug/sublore` without the feature.

All checks were run against a `pnpm e2e:build` binary with nothing listening on port 1420.
