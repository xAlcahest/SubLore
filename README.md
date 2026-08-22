# Sublore

Translation memory for subtitles. Sublore is a local-first desktop app for subtitle translation: your terminology, enforced across the whole series.

Early development. Nothing is released yet, and the app does not do much so far.

Sublore works offline. It does not phone home, has no accounts, and collects no telemetry.

Local transcription is planned through whisper.cpp. Transcription accuracy is Whisper's, and we say so; what Sublore adds is consistency across episodes.

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
