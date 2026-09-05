//! Where the panels were left, between one session and the next.
//!
//! D1: three draggable edges, and where each was left outlives the session. This is UI state and
//! not derived data, so decision 20 keeps it out of the peaks cache; it lives in the app's own
//! store beside the chooser's remembered folders, and it is read the same way: a missing or
//! unreadable file is the default and a warning, never a failure.
//!
//! The video edge is stored as a share of the top row and the other two as pixels. A window that
//! grows sideways has no panel that is supposed to swallow the extra columns, so the video keeps
//! its share of them; a window that grows downwards has one, the cue grid, so the block above it
//! keeps the height it was given.
//!
//! S1 adds a fourth number beside the three edges: the interface scale, a multiplier on the root
//! font size rather than a panel bound, stored and read the same way.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::log;

/// Beside `chooser-folders.json` in the app data directory, for the same reason: derived
/// convenience rather than the user's own work, and losing it costs one drag.
const LAYOUT_FILE: &str = "layout.json";

/// The height the waveform panel opens at, in CSS pixels. The same number `tools.css` gives it
/// before anything has been dragged.
const DEFAULT_WAVEFORM_HEIGHT: f64 = 128.0;

/// Below this the wave has no room to be read on either side of its middle line, and the sash may
/// never reach zero (W6).
const MIN_WAVEFORM_HEIGHT: f64 = 64.0;

/// A ceiling on what may be stored, which is not the same as what may be dragged to: only the
/// window knows how much room there is, so the drag clamps against the window and this clamps
/// against the file. Every ceiling below is that kind of number, and it exists so a hand-edited or
/// half-written value cannot open the app with a panel bigger than any screen.
const MAX_WAVEFORM_HEIGHT: f64 = 512.0;

/// The share of the top row the video panel opens at. The same 38% `shell.css` gives it.
const DEFAULT_VIDEO_FRACTION: f64 = 0.38;

/// A share, so both ends are the file guard and neither is the drag bound: what the panel may
/// actually be dragged to is measured off the rendered row, in `App.tsx`.
const MIN_VIDEO_FRACTION: f64 = 0.1;
const MAX_VIDEO_FRACTION: f64 = 0.9;

/// The height the top block opens at, in CSS pixels: the 13.5rem `shell.css` gives it at the
/// default root size.
const DEFAULT_TOP_HEIGHT: f64 = 216.0;

/// The transport under the video measures 46 px and the stage is never smaller than its own
/// transport, so this is the block's floor whatever else is in it (D1).
const MIN_TOP_HEIGHT: f64 = 92.0;

const MAX_TOP_HEIGHT: f64 = 1200.0;

/// A ceiling on the width the shell may ask the window to be held at, for the same reason every
/// other ceiling here exists: a number arriving from outside cannot be allowed to hold the window
/// at a width no screen has. It is a guard and not a measurement: the widest row the shell can ask
/// for is a fraction of it at every interface size the View menu offers.
const MAX_MINIMUM_WIDTH: f64 = 4096.0;

/// The root font-size multiplier the interface opens at. S1 moves this off 1.0, today's size, to
/// 1.1 (110%): the controls it scales read small for a tool used for hours.
const DEFAULT_INTERFACE_SCALE: f64 = 1.1;

/// The View submenu's own ends, 90 and 150 per cent (S1): nothing hand-written in the file can open
/// the interface outside what the menu offers.
const MIN_INTERFACE_SCALE: f64 = 0.9;
const MAX_INTERFACE_SCALE: f64 = 1.5;

/// What the panels were left at. Every field carries a default so a file written by an older
/// version, or one a hand has been in, reads as far as it goes and defaults the rest.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layout {
    pub waveform_height: f64,
    pub video_fraction: f64,
    pub top_height: f64,
    pub interface_scale: f64,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            waveform_height: DEFAULT_WAVEFORM_HEIGHT,
            video_fraction: DEFAULT_VIDEO_FRACTION,
            top_height: DEFAULT_TOP_HEIGHT,
            interface_scale: DEFAULT_INTERFACE_SCALE,
        }
    }
}

/// A number that is a number: NaN, infinity and a value off either end all become one, because the
/// alternative is a panel the layout cannot lay out and nothing on screen saying why.
fn sane_number(value: f64, min: f64, max: f64, fallback: f64, what: &str) -> f64 {
    let kept = if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    };
    if kept != value {
        log::warn!("layout: {what} of {value} is not one the window can use, opening at {kept}");
    }
    kept
}

impl Layout {
    fn sane(self) -> Self {
        Self {
            waveform_height: sane_number(
                self.waveform_height,
                MIN_WAVEFORM_HEIGHT,
                MAX_WAVEFORM_HEIGHT,
                DEFAULT_WAVEFORM_HEIGHT,
                "a waveform height",
            ),
            video_fraction: sane_number(
                self.video_fraction,
                MIN_VIDEO_FRACTION,
                MAX_VIDEO_FRACTION,
                DEFAULT_VIDEO_FRACTION,
                "a video share",
            ),
            top_height: sane_number(
                self.top_height,
                MIN_TOP_HEIGHT,
                MAX_TOP_HEIGHT,
                DEFAULT_TOP_HEIGHT,
                "a top block height",
            ),
            interface_scale: sane_number(
                self.interface_scale,
                MIN_INTERFACE_SCALE,
                MAX_INTERFACE_SCALE,
                DEFAULT_INTERFACE_SCALE,
                "an interface scale",
            ),
        }
    }
}

/// The layout on disk, or the default. Nothing here is worth refusing to open the app over.
fn read_from(path: &Path) -> Layout {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            // A first launch has no file, which is not something to say anything about.
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("layout: the stored layout could not be read: {error}");
            }
            return Layout::default();
        }
    };
    match serde_json::from_str::<Layout>(&text) {
        Ok(layout) => layout.sane(),
        Err(error) => {
            log::warn!("layout: the stored layout is not readable JSON: {error}");
            Layout::default()
        }
    }
}

/// Written whole and renamed over the old one, so a crash mid-write leaves the previous layout
/// rather than a file that reads as none.
fn write_to(path: &Path, layout: Layout) -> std::io::Result<()> {
    let text = serde_json::to_string(&layout).map_err(std::io::Error::other)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)
}

fn layout_path(app: &AppHandle) -> Option<PathBuf> {
    use tauri::Manager;

    match app.path().app_data_dir() {
        Ok(dir) => Some(dir.join(LAYOUT_FILE)),
        Err(error) => {
            log::warn!("layout: no app data directory, so no layout is remembered: {error}");
            None
        }
    }
}

/// What the panels open at. Called once, when the shell mounts.
#[tauri::command]
pub fn layout_read(app: AppHandle) -> Layout {
    layout_path(&app).map_or_else(Layout::default, |path| read_from(&path))
}

/// Store where the sashes were left. The caller writes when a drag ends and not while it runs: a
/// write per pointer move would put the disk in the middle of the frame budget.
///
/// The whole layout arrives every time rather than the one edge that moved, so a write can never
/// reset the two edges it was not told about.
#[tauri::command]
pub fn layout_write(app: AppHandle, layout: Layout) {
    let Some(path) = layout_path(&app) else {
        return;
    };
    if let Err(error) = write_to(&path, layout.sane()) {
        log::warn!("layout: where the sashes were left could not be stored: {error}");
    }
}

/// The window's height floor, which is the one declared in `tauri.conf.json` and is not measured:
/// nothing in the shell's vertical chain is a row that cannot shrink, and the grid gives up its
/// height before anything is pushed out. Zero when the file declares none, which is what the window
/// already had.
fn configured_min_height(window: &tauri::WebviewWindow) -> f64 {
    use tauri::Manager;

    window
        .config()
        .app
        .windows
        .iter()
        .find(|declared| declared.label == window.label())
        .and_then(|declared| declared.min_height)
        .unwrap_or(0.0)
}

/// How narrow the window may be made, in CSS pixels, measured off the rows the shell cannot draw
/// narrower than their contents. Sent when that measurement changes, which is when the interface
/// size changes and when the fonts settle.
///
/// Nothing here is stored: the number is a fact about the type this machine has, and one written
/// into a file would outlive the fonts it was read against. The height stays what the configuration
/// declared, for the reason on `configured_min_height`.
#[tauri::command]
pub fn layout_set_minimum_width(window: tauri::WebviewWindow, width: f64) -> Result<(), String> {
    hold_at_least(&window, width).inspect_err(|reason| {
        log::warn!("layout: the window could not be held at {width} css pixels wide: {reason}");
    })
}

fn hold_at_least(window: &tauri::WebviewWindow, width: f64) -> Result<(), String> {
    if !width.is_finite() || width <= 0.0 || width > MAX_MINIMUM_WIDTH {
        return Err(format!("{width} is not a width a window can be held at"));
    }
    let height = configured_min_height(window);
    window
        .set_min_size(Some(tauri::LogicalSize::new(width, height)))
        .map_err(|error| format!("the smallest size was refused: {error}"))?;

    // A floor the window is already under is not a floor: a minimum does not resize a window that
    // is smaller than it, so the difference is asked for once, here.
    let scale = window
        .scale_factor()
        .map_err(|error| format!("no scale factor to read a width against: {error}"))?;
    let inner: tauri::LogicalSize<f64> = window
        .inner_size()
        .map_err(|error| format!("the window would not say how wide it is: {error}"))?
        .to_logical(scale);
    if inner.width < width {
        window
            .set_size(tauri::LogicalSize::new(width, inner.height))
            .map_err(|error| format!("coming up to it was refused: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of this test's own, removed when it returns. Same reason as `chooser`: no
    /// `tempfile` dependency for a handful of tests that need a path that exists and one that does
    /// not.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("sublore-layout-{}-{name}", std::process::id()));
            std::fs::remove_dir_all(&path).ok();
            std::fs::create_dir_all(&path).expect("a directory under the temp dir");
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn every_size_written_is_the_size_read_back() {
        let dir = TempDir::new("round-trip");
        let path = dir.join(LAYOUT_FILE);
        let left = Layout {
            waveform_height: 211.0,
            video_fraction: 0.62,
            top_height: 305.0,
            interface_scale: 1.25,
        };
        write_to(&path, left).expect("a layout written under the temp dir");
        assert_eq!(read_from(&path), left);
    }

    #[test]
    fn a_layout_that_was_never_written_opens_at_the_default() {
        let dir = TempDir::new("never-written");
        assert_eq!(read_from(&dir.join(LAYOUT_FILE)), Layout::default());
    }

    #[test]
    fn a_layout_file_that_is_not_json_opens_at_the_default_rather_than_failing() {
        let dir = TempDir::new("not-json");
        let path = dir.join(LAYOUT_FILE);
        std::fs::write(&path, "{\"waveformHeight\": ").expect("a half-written file");
        assert_eq!(read_from(&path), Layout::default());
    }

    #[test]
    fn a_layout_file_with_no_sizes_in_it_reads_rather_than_failing_to_parse() {
        // Read directly and not through `read_from`, which defaults a broken file too: the thing
        // being asserted is that a file an older version wrote parses, rather than that a file
        // nothing can parse ends up at the same place.
        assert_eq!(
            serde_json::from_str::<Layout>("{}").expect("a layout with nothing in it"),
            Layout::default()
        );

        let dir = TempDir::new("no-sizes");
        let path = dir.join(LAYOUT_FILE);
        std::fs::write(&path, "{}").expect("a layout with nothing in it");
        assert_eq!(read_from(&path), Layout::default());
    }

    /// The file W6 wrote had one edge in it. D1 added two, and a translator upgrading has that
    /// file: the edge it names is kept and the two it never heard of open where they open.
    #[test]
    fn a_layout_written_before_the_other_two_edges_existed_keeps_the_one_it_names() {
        let dir = TempDir::new("one-edge");
        let path = dir.join(LAYOUT_FILE);
        std::fs::write(&path, "{\"waveformHeight\": 200}").expect("an older layout");
        assert_eq!(
            read_from(&path),
            Layout {
                waveform_height: 200.0,
                ..Layout::default()
            }
        );
    }

    /// The same upgrade as above, one field later: a file D1 wrote has the three edges but no
    /// interface scale, and S1 must default the field it never heard of rather than fail to parse.
    #[test]
    fn a_layout_written_before_the_interface_scale_existed_keeps_the_edges_it_names() {
        let dir = TempDir::new("no-scale");
        let path = dir.join(LAYOUT_FILE);
        std::fs::write(
            &path,
            "{\"waveformHeight\": 200, \"videoFraction\": 0.5, \"topHeight\": 260}",
        )
        .expect("a layout written before S1");
        assert_eq!(
            read_from(&path),
            Layout {
                waveform_height: 200.0,
                video_fraction: 0.5,
                top_height: 260.0,
                ..Layout::default()
            }
        );
    }

    #[test]
    fn a_size_off_either_end_is_brought_back_into_range_rather_than_used() {
        let dir = TempDir::new("out-of-range");
        let path = dir.join(LAYOUT_FILE);
        for (field, stored, expected) in [
            ("waveformHeight", "0", MIN_WAVEFORM_HEIGHT),
            ("waveformHeight", "-4000", MIN_WAVEFORM_HEIGHT),
            ("waveformHeight", "99999", MAX_WAVEFORM_HEIGHT),
            ("videoFraction", "0", MIN_VIDEO_FRACTION),
            ("videoFraction", "-1", MIN_VIDEO_FRACTION),
            ("videoFraction", "40", MAX_VIDEO_FRACTION),
            ("topHeight", "1", MIN_TOP_HEIGHT),
            ("topHeight", "-9", MIN_TOP_HEIGHT),
            ("topHeight", "99999", MAX_TOP_HEIGHT),
            ("interfaceScale", "0", MIN_INTERFACE_SCALE),
            ("interfaceScale", "-1", MIN_INTERFACE_SCALE),
            ("interfaceScale", "40", MAX_INTERFACE_SCALE),
        ] {
            std::fs::write(&path, format!("{{\"{field}\": {stored}}}"))
                .expect("a layout with a size off the end");
            let read = read_from(&path);
            let got = match field {
                "waveformHeight" => read.waveform_height,
                "videoFraction" => read.video_fraction,
                "topHeight" => read.top_height,
                _ => read.interface_scale,
            };
            assert_eq!(got, expected, "a stored {field} of {stored}");
        }
    }

    #[test]
    fn a_size_that_is_not_a_number_opens_at_the_default() {
        // JSON has no NaN, so this is what a `f64::NAN` on the way in becomes: `null`, which serde
        // defaults, and the value the frontend sent, which `sane` catches on the way out.
        let dir = TempDir::new("not-a-number");
        let path = dir.join(LAYOUT_FILE);
        std::fs::write(
            &path,
            "{\"waveformHeight\": null, \"videoFraction\": null, \"interfaceScale\": null}",
        )
        .expect("a layout with null sizes");
        assert_eq!(read_from(&path), Layout::default());
        for broken in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                Layout {
                    waveform_height: broken,
                    video_fraction: broken,
                    top_height: broken,
                    interface_scale: broken,
                }
                .sane(),
                Layout::default(),
                "a layout of {broken}"
            );
        }
    }

    /// The rename is the whole of the crash safety, so it is asserted rather than read: a file
    /// replaced by a rename is a different inode, a file written in place is the same one.
    #[cfg(unix)]
    #[test]
    fn a_layout_is_renamed_over_the_old_one_rather_than_written_through_it() {
        use std::os::unix::fs::MetadataExt;

        let dir = TempDir::new("renamed");
        let path = dir.join(LAYOUT_FILE);
        write_to(&path, Layout::default()).expect("the first layout");
        let first = std::fs::metadata(&path).expect("the first file").ino();
        write_to(
            &path,
            Layout {
                waveform_height: 300.0,
                ..Layout::default()
            },
        )
        .expect("the second layout");
        let second = std::fs::metadata(&path).expect("the second file").ino();
        assert_ne!(
            first, second,
            "a layout written through the old file would leave a half-written one behind a crash"
        );
    }

    #[test]
    fn a_write_that_lands_on_top_of_an_older_one_leaves_only_the_newer_sizes() {
        let dir = TempDir::new("overwrite");
        let path = dir.join(LAYOUT_FILE);
        write_to(&path, Layout::default()).expect("the first layout");
        let second = Layout {
            waveform_height: 300.0,
            video_fraction: 0.5,
            top_height: 260.0,
            interface_scale: 1.25,
        };
        write_to(&path, second).expect("the second layout");
        assert_eq!(read_from(&path), second);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "the temp file is renamed over the old one, never left beside it"
        );
    }
}
