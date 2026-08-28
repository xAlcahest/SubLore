# E2E harness (M0.5, extended by M1.5 and M4.4)

Behavioral tests that launch the real Sublore binary on a real X server and assert what a person
would see. Nothing here reads Rust or TypeScript source: the harness only drives the app and looks
at the window.

## What each spec proves

| File                        | Test                                                              | Acceptance criterion it binds                                                                                              |
| --------------------------- | ----------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `specs/title.spec.js`       | `native window title is Sublore`                                  | The X11 toplevel is named `Sublore`. This is the AC test.                                                                  |
| `specs/title.spec.js`       | `document title is Sublore`                                       | The webview loaded the app document, not a blank page. Different thing from the X11 name; both are asserted.               |
| `specs/video.spec.js`       | `opens the sample fixture`                                        | Typing `fixtures/video/sample.mkv` into the open bar and clicking Open reaches the ready state with no error banner.       |
| `specs/video.spec.js`       | `sizes the native video surface over the stage`                   | The native video child window is mapped and covers the `.stage__surface` rectangle within 2 px.                            |
| `specs/subtitle.spec.js`    | `opens an SRT fixture and shows its format and cue count`         | Opening `fixtures/subtitles/srt/clean/basic-lf.srt` puts `SRT · 3 cues · LF` on the status line with no error.             |
| `specs/subtitle.spec.js`    | `saves a byte-identical copy`                                     | Save-as of `basic-crlf.srt` writes a file the spec then compares byte for byte with `Buffer.compare`.                      |
| `specs/subtitle.spec.js`    | `reports a malformed file readably and stays usable`              | `srt/malformed/missing-arrow.srt` shows an error naming line 6, and the clean fixture opens straight after.                |
| `specs/project.spec.js`     | `creates a project in an empty folder`                            | Typing a fresh temp folder and clicking Create puts that folder on the status line and writes `project.sublore`.           |
| `specs/project.spec.js`     | `adds an episode and attaches a subtitle file to it`              | The episode is listed, the attached path is listed under it, and the user's file is byte-identical afterwards.             |
| `specs/project.spec.js`     | `still lists the episode and its file after the app is restarted` | The AC's restart: the session is deleted and a new one launches the binary again, and reopening the folder lists both.     |
| `specs/project.spec.js`     | `reports a folder that holds no project and stays usable`         | Opening an empty folder shows the `noProjectHere` sentence, writes nothing there, and the real project reopens after it.   |
| `specs/project.spec.js`     | `deletes the project without touching the files it points at`     | `project.sublore` is gone, the attached subtitle outside the folder is byte-identical, and the folder itself still exists. |
| `scripts/shutdown-check.js` | 4 checks                                                          | Closing the window exits 0, unsignalled, with nothing left alive in the app's process group.                               |

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
const EXPECTED_TESTS = 12;
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
`.subbar__input`, `.subbar__open`, `.subbar__dest`, `.subbar__save`, `.subbar__status`, `.subbar__error`,
`.project__path`, `.project__choose-folder`, `.project__create`, `.project__open`, `.project__delete`,
`.project__status`, `.project__error`, `.project__episodes`, `.project__episode`,
`.project__episode--selected`, `.project__episode-title`, `.project__files`, `.project__file`,
`.project__new-episode`, `.project__add-episode`, `.project__file-path`, `.project__choose-file`,
`.project__role-media`, `.project__role-source`, `.project__role-target`, `.project__attach`

Readiness has no dedicated signal: a video is loaded when `.stage__empty` is gone **and**
`.controls__button` is enabled. A subtitle file is open when `.subbar__status` stops saying
"No subtitle file open."; `.subbar__error` is absent from the DOM when there is nothing wrong.

## The project spec

The restart in `project.spec.js` is `browser.reloadSession()`: WebdriverIO deletes the session, which
ends the app process, then asks `tauri-driver` for a new one, which launches the binary again. The
test proves the relaunch happened rather than assuming it, by asserting that the fresh app has no
project open before it reopens the folder. `lib/x11.js`'s `findToplevel` throws when two windows
match, so a leftover instance from the old session fails the run instead of poisoning it.

**The harness must never click `.project__choose-folder` or `.project__choose-file`.** Those open a
native dialog, and under Xvfb there is nobody to answer it: the run would hang until the suite timed
out. Every path the spec supplies goes through the text field beside the button, which is how
`VideoOpenBar` and `SubtitleBar` are driven too.

`project.spec.js` writes only under `$SUBLORE_E2E_DATA_HOME/project`, and the subtitle it attaches is
a **copy** of `fixtures/subtitles/srt/clean/basic-lf.srt` placed in a separate user directory. That
is deliberate: the deletion test asserts on a real user file, and no committed fixture is ever within
reach of a delete path.

`subtitle.spec.js` types four different paths through one field, so it clears the field with a
ctrl+a of its own before typing. `lib/input.js` is deliberately not extended for that: it is shared
with the specs above, and this is the only place that needs it so far.

Subtitle fixtures are committed, so nothing generates them. The save-as test writes into
`$SUBLORE_E2E_DATA_HOME/save-as`, never into the repo and never beside a fixture.

## Why `pnpm e2e:build`, never `cargo build`

`tauri build` runs `cargo build --bins --features tauri/custom-protocol`, and that feature is what
makes the `tauri` crate serve the bundled `dist/` instead of `build.devUrl`. A plain `cargo build`
binary has the `dev` cfg instead and loads `http://localhost:1420`, which without a Vite server is a
connection error on a blank page. That is why `lib/paths.js` names `pnpm e2e:build` in its failure
message, and why running `cargo build` or `cargo test` after it invalidates the binary: both rewrite
`target/debug/sublore` without the feature.

All sixteen checks were run against a `pnpm e2e:build` binary with nothing listening on port 1420,
and all sixteen pass.
