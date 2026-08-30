# Sublore

> **Pre-alpha, under construction, not usable yet.** The repository is public so the work can be read, not because there is something to run. There are no releases and no builds to download, and the parts that exist are being reshaped as the editor's layout is rewritten.

**Translation memory for subtitles.** A local-first desktop app for translating subtitles across a whole series: your terminology, enforced everywhere it appears, instead of remembered episode by episode.

Whisper transcription is a commodity Sublore wraps. The product is the memory — a persistent termbase and translation memory that follows the translator through every episode, and a QA pass that flags every line where an approved term was not used.

`CLAUDE.md` is the honest description of how this is built and what the rules are, including the ones about what is verified and what is merely assumed. `WORKFLOW.md` is how the work moves and `BACKLOG.md` is what is left.

Sublore works offline. It does not phone home, has no accounts, and collects no telemetry.

Local transcription runs whisper.cpp as a separate process, on your machine. Transcription accuracy is Whisper's, and we say so; what Sublore adds is consistency across episodes.

## Prerequisites

- Rust, stable toolchain, installed with [rustup](https://rustup.rs/).
- Node.js 22 or newer.
- pnpm 10. The exact version is pinned in `package.json` (`packageManager`), so `corepack enable pnpm` picks it up; otherwise install pnpm from [pnpm.io/installation](https://pnpm.io/installation).

### Linux system dependencies (Tauri 2)

Debian and Ubuntu:

```sh
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Fedora:

```sh
sudo dnf install webkit2gtk4.1-devel \
  openssl-devel \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  libxdo-devel
sudo dnf group install "c-development"
```

### libmpv

Video playback is libmpv, embedded. It must be present at build time and at run time.

- Debian and Ubuntu: `sudo apt install libmpv-dev`
- Fedora: `sudo dnf install mpv-libs-devel`
- Windows: there is no package. Download a `mpv-dev-x86_64-*.7z` build from [mpv-winbuild-cmake releases](https://github.com/shinchiro/mpv-winbuild-cmake/releases), generate `mpv.lib` from `libmpv-2.dll` with `dumpbin`/`lib`, and point `LIBMPV_LIB_DIR` at the folder holding it. `.github/workflows/ci.yml` does exactly this and is the reference. `libmpv-2.dll` must also be on `PATH` when the app or the tests run; packaging it next to the executable comes with M0.3.

Sublore runs on X11. On a Wayland desktop it uses XWayland, because libmpv embeds into an X11 window id.

Video tests need `fixtures/video/sample.mkv`, which is generated, not committed:

```sh
sh fixtures/video/make-sample.sh   # needs ffmpeg
```

### Windows

Install the Microsoft C++ Build Tools with the "Desktop development with C++" workload. WebView2 ships with Windows 10 1803 and later; on older systems install the WebView2 Evergreen Runtime.

macOS is not a target for v1.

## Development

```sh
pnpm install
pnpm tauri dev
```

## Subtitle files

Sublore opens SRT, VTT and ASS files, shows the format, the cue count and the line endings, and saves a copy elsewhere. There is no editor yet.

The file you open is never written to. "Save as" writes the copy atomically: a temporary file first, then a rename, so the destination is always either the old file or the new one and never something in between. If the destination already existed, its previous contents are kept as a timestamped backup, inside Sublore's own folder rather than next to your file:

- Linux: `~/.local/share/com.sublore.app/backups/`
- Windows: `%APPDATA%\com.sublore.app\backups\`

Ten backups are kept per file. Nothing else deletes them; removing them is your call.

Sublore reads UTF-8 only. A file it cannot decode, or cannot parse, is refused with the line number and the reason, and is never rewritten.

## Transcription

Sublore transcribes the audio of the video you have open, using whisper.cpp as a separate process.
Choose a model, press Transcribe, watch the progress, and stop it whenever you like. The cues it
produces are listed under the bar; editing them is not built yet.

Two things have to be on the machine before it will run:

- **ffmpeg**, at run time. Sublore uses it to read the audio out of your video.
- **The whisper.cpp sidecar.** It is not committed, and nothing downloads it for you:

  ```sh
  sh scripts/build-whisper.sh              # both binaries: Vulkan and CPU-only
  sh scripts/build-whisper.sh --cpu-only   # skip the Vulkan build
  ```

  Everything lands in `.whisper/`, which is git-ignored. The Vulkan build additionally needs
  `glslc` and the Vulkan headers (Fedora: `glslc`, `vulkan-headers`, `spirv-headers-devel`;
  Debian and Ubuntu: `glslang-tools`, `libvulkan-dev`, `spirv-headers`). CUDA is never used.

"Use GPU when available" runs the Vulkan binary. Without one, or when a GPU run fails, Sublore runs
the CPU binary instead and says on screen that it did. The CPU path always works.

Models are yours to fetch, and nothing is fetched until you press Download: Sublore opens no network
connection of any other kind. A download is checked against a known size and SHA-256 when it
finishes, and is only then given its real name, so a file that fails its checksum is never handed to
whisper; an interrupted one resumes where it stopped. Before each run the model is checked again,
its size and its SHA-256 both, which is what catches a file damaged after it arrived: whisper loads a
corrupted model without complaining and transcribes nonsense from it, so Sublore refuses the run and
says so instead. Downloading that model again replaces the damaged file. Models live beside the rest of Sublore's
data:

- Linux: `~/.local/share/com.sublore.app/models/`
- Windows: `%APPDATA%\com.sublore.app\models\`

Your video and audio files are only ever read. The audio Sublore extracts goes into a scratch folder
inside its own data directory and is deleted when the run ends, whether it finished or you stopped
it. Cancelling kills the transcription process; closing Sublore mid-run does too.

## Logs and crash reports

Sublore writes a log file, and a crash report if it ever crashes. Both stay on your machine: nothing is sent anywhere.

- Linux: `~/.local/share/com.sublore.app/logs/`
- Windows: `%LOCALAPPDATA%\com.sublore.app\logs\`

The log is `sublore.log`, capped at 2 MB with two older files kept beside it. A crash appends to `crash.log` in the same folder, so earlier crashes are not lost, and moves it to `crash.log.1` once it passes 256 KB. If Sublore crashes before it has resolved that folder, the report goes to `sublore-crash.log` in the system temp directory instead.

Release builds write to the log file only. Debug builds also print to the console.

Development builds can be made to crash on purpose, to check that path: set `SUBLORE_FORCE_PANIC` to `startup`, `open` or `main-thread` before launching. The variable is read only in debug builds (`cargo build`, `cargo test`, `tauri build --debug`); release binaries contain no trigger.

On Linux, WebKitGTK cannot allocate a DMABUF buffer on the NVIDIA proprietary driver and the window opens blank, so Sublore looks for `/sys/module/nvidia` when it starts and, if it is there, sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` and `__NV_DISABLE_EXPLICIT_SYNC=1` for itself before the webview exists. `SUBLORE_WEBKIT_WORKAROUNDS` overrides that check in either direction. `0`, `false`, `no` or `off` turns the workarounds off: worth trying if your window paints without them, because they cost some input latency. `1`, `true`, `yes` or `on` turns them on where the check finds nothing, which is what a hybrid laptop rendering through NVIDIA needs. Case and surrounding spaces are ignored. An empty value means the same as not setting the variable at all, and so does any other word: the `/sys/module/nvidia` check decides. Either way the app prints which path it took to stderr as it starts, and unlike `SUBLORE_FORCE_PANIC` this variable is read in release builds too.

Also on Linux, mpv draws into a child window of Sublore's own, and with a Wayland display in the environment mpv's `gpu-context=auto` picks Wayland and draws past that window, so Sublore asks for `x11egl` instead. It is a request, not a requirement: an mpv built without that context refuses the name, Sublore says so in the log and lets mpv choose, and the app still starts. `SUBLORE_MPV_GPU_CONTEXT` overrides the request with any context name your mpv accepts — `x11`, `x11vk`, `wayland`, `auto` — and is worth reaching for only if the video area stays black while the rest of the window is fine. Surrounding spaces are ignored; an empty value means the same as not setting it, as with `SUBLORE_WEBKIT_WORKAROUNDS`. A name mpv rejects costs you the request and not the video: Sublore falls back to `x11egl` and writes both names to the log.

## Known limitations

- A panic on the main thread (startup, or the window event loop) writes the crash report and exits, but cannot show the crash dialog: the thread that would have to draw it is the one that failed.
- An external client that destroys the window with `XDestroyWindow` (`xkill`, `xdotool windowclose`) crashes the app inside GTK's teardown. Closing normally, including the window manager's close button, is clean. This is accepted rather than handled: catching it would need a SIGSEGV handler, and Sublore writes no user data at this stage, so nothing is lost. See BACKLOG.md M0.4.

## Checks

The same commands CI runs:

```sh
pnpm format:check
pnpm lint
pnpm build
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build
cargo test --workspace
```

## Git hooks

Enable the repo's hooks once per clone:

```sh
git config core.hooksPath .githooks
```

The pre-commit hook runs `pnpm format:check`, `pnpm lint` and `cargo fmt --check`, and refuses the commit if any of them fails.

## License

GNU General Public License v3.0. See [LICENSE](LICENSE).
