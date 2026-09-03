//! Where the panels were left, between one session and the next.
//!
//! M2.4 W6: the waveform's bottom edge is the one draggable thing in v1 (decision 24 A5) and how
//! much waveform a translator wants on screen depends on the task, so the height it was left at
//! outlives the session. This is UI state and not derived data, so decision 20 keeps it out of the
//! peaks cache; it lives in the app's own store beside the chooser's remembered folders, and it is
//! read the same way: a missing or unreadable file is the default and a warning, never a failure.

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
/// against the file. It exists so a hand-edited or half-written number cannot open the app with a
/// panel taller than any screen.
const MAX_WAVEFORM_HEIGHT: f64 = 512.0;

/// What the panels were left at. Every field carries a default so a file written by an older
/// version, or one a hand has been in, reads as far as it goes and defaults the rest.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layout {
    pub waveform_height: f64,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            waveform_height: DEFAULT_WAVEFORM_HEIGHT,
        }
    }
}

impl Layout {
    /// A height that is a height: NaN, infinity and a number off either end all become one, because
    /// the alternative is a panel the layout cannot lay out and nothing on screen saying why.
    fn sane(self) -> Self {
        let height = if self.waveform_height.is_finite() {
            self.waveform_height
                .clamp(MIN_WAVEFORM_HEIGHT, MAX_WAVEFORM_HEIGHT)
        } else {
            DEFAULT_WAVEFORM_HEIGHT
        };
        if height != self.waveform_height {
            log::warn!(
                "layout: a waveform height of {} is not one the window can use, opening at {height}",
                self.waveform_height
            );
        }
        Self {
            waveform_height: height,
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

/// Store where the sash was left. The caller writes when a drag ends and not while it runs: a
/// write per pointer move would put the disk in the middle of the frame budget.
#[tauri::command]
pub fn layout_write(app: AppHandle, waveform_height: f64) {
    let Some(path) = layout_path(&app) else {
        return;
    };
    let layout = Layout { waveform_height }.sane();
    if let Err(error) = write_to(&path, layout) {
        log::warn!("layout: where the sash was left could not be stored: {error}");
    }
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
    fn the_height_written_is_the_height_read_back() {
        let dir = TempDir::new("round-trip");
        let path = dir.join(LAYOUT_FILE);
        write_to(
            &path,
            Layout {
                waveform_height: 211.0,
            },
        )
        .expect("a layout written under the temp dir");
        assert_eq!(read_from(&path).waveform_height, 211.0);
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
    fn a_layout_file_with_no_height_in_it_reads_rather_than_failing_to_parse() {
        // Read directly and not through `read_from`, which defaults a broken file too: the thing
        // being asserted is that a file an older version wrote parses, rather than that a file
        // nothing can parse ends up at the same place.
        assert_eq!(
            serde_json::from_str::<Layout>("{}").expect("a layout with nothing in it"),
            Layout::default()
        );

        let dir = TempDir::new("no-height");
        let path = dir.join(LAYOUT_FILE);
        std::fs::write(&path, "{}").expect("a layout with nothing in it");
        assert_eq!(read_from(&path), Layout::default());
    }

    #[test]
    fn a_height_off_either_end_is_brought_back_into_range_rather_than_used() {
        let dir = TempDir::new("out-of-range");
        let path = dir.join(LAYOUT_FILE);
        for (stored, expected) in [
            ("0", MIN_WAVEFORM_HEIGHT),
            ("-4000", MIN_WAVEFORM_HEIGHT),
            ("99999", MAX_WAVEFORM_HEIGHT),
        ] {
            std::fs::write(&path, format!("{{\"waveformHeight\": {stored}}}"))
                .expect("a layout with a height off the end");
            assert_eq!(
                read_from(&path).waveform_height,
                expected,
                "a stored height of {stored}"
            );
        }
    }

    #[test]
    fn a_height_that_is_not_a_number_opens_at_the_default() {
        // JSON has no NaN, so this is what a `f64::NAN` on the way in becomes: `null`, which serde
        // defaults, and the value the frontend sent, which `sane` catches on the way out.
        let dir = TempDir::new("not-a-number");
        let path = dir.join(LAYOUT_FILE);
        std::fs::write(&path, "{\"waveformHeight\": null}").expect("a layout with a null height");
        assert_eq!(read_from(&path), Layout::default());
        assert_eq!(
            Layout {
                waveform_height: f64::NAN,
            }
            .sane(),
            Layout::default()
        );
        assert_eq!(
            Layout {
                waveform_height: f64::INFINITY,
            }
            .sane(),
            Layout::default()
        );
    }

    /// The rename is the whole of the crash safety, so it is asserted rather than read: a file
    /// replaced by a rename is a different inode, a file written in place is the same one.
    #[cfg(unix)]
    #[test]
    fn a_layout_is_renamed_over_the_old_one_rather_than_written_through_it() {
        use std::os::unix::fs::MetadataExt;

        let dir = TempDir::new("renamed");
        let path = dir.join(LAYOUT_FILE);
        write_to(
            &path,
            Layout {
                waveform_height: 100.0,
            },
        )
        .expect("the first layout");
        let first = std::fs::metadata(&path).expect("the first file").ino();
        write_to(
            &path,
            Layout {
                waveform_height: 300.0,
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
    fn a_write_that_lands_on_top_of_an_older_one_leaves_only_the_newer_height() {
        let dir = TempDir::new("overwrite");
        let path = dir.join(LAYOUT_FILE);
        write_to(
            &path,
            Layout {
                waveform_height: 100.0,
            },
        )
        .expect("the first layout");
        write_to(
            &path,
            Layout {
                waveform_height: 300.0,
            },
        )
        .expect("the second layout");
        assert_eq!(read_from(&path).waveform_height, 300.0);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "the temp file is renamed over the old one, never left beside it"
        );
    }
}
