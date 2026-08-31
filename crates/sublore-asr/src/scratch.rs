//! The per-run working directory: the extracted audio and whisper's JSON, and nothing else.
//! See BACKLOG.md M3.1.
//!
//! It lives under the app's own data directory, never beside the user's media and never in the
//! system temp directory, which on a shared Linux box is world-readable and would put the user's
//! speech in it (CONTRIBUTING.md §3.5). It is removed on every exit path by `Drop`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use crate::error::{AsrError, AsrErrorKind};

/// Every directory this module creates starts with it, and `sweep` removes nothing else.
pub const PREFIX: &str = "asr-";
/// How old an abandoned directory has to be before a sweep removes it. Age-based rather than
/// pid-based so a second Sublore running at the same time is never harmed.
pub const SWEEP_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// Names tried before giving up.
const ATTEMPTS: u32 = 8;

/// A directory that deletes itself. Holding one is what guarantees the cleanup: there is no exit
/// path from a run, including a panic, that skips it.
#[derive(Debug)]
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    pub fn create(root: &Path) -> Result<Self, AsrError> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        fs::create_dir_all(root).map_err(|error| {
            AsrError::new(
                AsrErrorKind::ScratchFailed,
                format!("cannot create {}: {error}", root.display()),
            )
        })?;
        for _ in 0..ATTEMPTS {
            let path = root.join(format!(
                "{PREFIX}{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            // create_dir, not create_dir_all: an existing name is never reused, so two runs can
            // never share a directory and delete each other's files.
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(AsrError::new(
                        AsrErrorKind::ScratchFailed,
                        format!("cannot create {}: {error}", path.display()),
                    ))
                }
            }
        }
        Err(AsrError::new(
            AsrErrorKind::ScratchFailed,
            format!(
                "{ATTEMPTS} names were already taken under {}",
                root.display()
            ),
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where ffmpeg writes the 16 kHz mono WAV.
    pub fn audio(&self) -> PathBuf {
        self.path.join("audio.wav")
    }

    /// What `-of` is given. whisper appends `.json` to it itself.
    pub fn output_stem(&self) -> PathBuf {
        self.path.join("out")
    }

    /// Where `-ojf` actually lands.
    pub fn json(&self) -> PathBuf {
        self.path.join("out.json")
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        // A leftover temp file is annoyance; the run's result is not. Never propagated.
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Remove run directories older than `max_age`. Called at startup, because a killed process
/// cannot run its own `Drop`. Returns how many went.
///
/// Touches only `PREFIX` directories directly under `root`, and never follows a symlink: a link
/// planted in the scratch root cannot make this delete anything outside it (CONTRIBUTING.md §3.5).
pub fn sweep(root: &Path, max_age: Duration) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(PREFIX) {
            continue;
        }
        let Ok(metadata) = entry.path().symlink_metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let old_enough = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= max_age);
        if old_enough && fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::{sweep, ScratchDir, PREFIX};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sublore-scratch-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the test root should be creatable");
        root
    }

    #[test]
    fn a_run_directory_is_created_and_removed_with_its_guard() {
        let root = temp_root("lifecycle");
        let path = {
            let scratch = ScratchDir::create(&root).expect("a fresh directory");
            let path = scratch.path().to_path_buf();
            assert!(path.is_dir());
            fs::write(scratch.audio(), b"noise").expect("writable");
            path
        };
        assert!(
            !path.exists(),
            "Drop removes the directory and its contents"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn two_runs_never_share_a_directory() {
        let root = temp_root("distinct");
        let first = ScratchDir::create(&root).expect("first");
        let second = ScratchDir::create(&root).expect("second");
        assert_ne!(first.path(), second.path());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_sweep_removes_old_run_directories_and_nothing_else() {
        let root = temp_root("sweep");
        let old = root.join(format!("{PREFIX}dead"));
        let recent = root.join(format!("{PREFIX}alive"));
        let foreign = root.join("models");
        for dir in [&old, &recent, &foreign] {
            fs::create_dir_all(dir).expect("creatable");
        }
        // Backdate the abandoned one instead of waiting a day for it.
        let long_ago = SystemTime::now() - Duration::from_secs(48 * 60 * 60);
        set_modified(&old, long_ago);

        let removed = sweep(&root, Duration::from_secs(24 * 60 * 60));
        assert_eq!(removed, 1);
        assert!(!old.exists());
        assert!(
            recent.exists(),
            "a directory a live run may own is left alone"
        );
        assert!(
            foreign.exists(),
            "nothing outside the asr- prefix is touched"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_sweep_of_a_missing_root_is_not_an_error() {
        assert_eq!(
            sweep(
                &PathBuf::from("/nonexistent-sublore-scratch"),
                Duration::ZERO
            ),
            0
        );
    }

    /// Backdate a directory so a sweep sees it as abandoned, instead of waiting a day.
    fn set_modified(path: &std::path::Path, when: SystemTime) {
        // A directory opens with `File::open` on Unix and not on Windows, where a handle to one
        // needs backup semantics and the right to write attributes. Same fixture, two doors.
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            fs::OpenOptions::new()
                .access_mode(FILE_WRITE_ATTRIBUTES)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)
                .expect("the fixture directory should be openable")
        };
        #[cfg(not(windows))]
        let handle = fs::File::open(path).expect("the fixture directory should be openable");
        handle
            .set_times(fs::FileTimes::new().set_modified(when))
            .expect("backdating the fixture directory should succeed");
    }
}
