//! Write failures: a stable kind, the path, and the OS message. BACKLOG.md M1.4.
//!
//! The kind is the whole vocabulary this crate speaks. `detail` carries the operating system's
//! message for logs; it is technical and never becomes UI copy.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Exhaustive on purpose: adding a variant must break the mapping in the app, not slip past a
/// wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoErrorKind {
    /// Empty, or no directory to write into.
    InvalidPath,
    /// The destination exists and is not a regular file.
    NotAFile,
    ReadFailed,
    TempCreateFailed,
    WriteFailed,
    SyncFailed,
    RenameFailed,
    PermissionDenied,
    BackupFailed,
}

#[derive(Clone, Debug)]
pub struct IoError {
    pub kind: IoErrorKind,
    pub path: PathBuf,
    /// The OS message, for logs. Never rendered as UI copy.
    pub detail: String,
}

impl IoError {
    pub(crate) fn new(kind: IoErrorKind, path: &Path, detail: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }

    /// An OS failure at one step. A permission problem keeps its own kind at every step: it is the
    /// one the user can act on. See BACKLOG.md M1.4.
    pub(crate) fn from_io(error: &io::Error, path: &Path, kind: IoErrorKind) -> Self {
        let kind = match error.kind() {
            io::ErrorKind::PermissionDenied => IoErrorKind::PermissionDenied,
            _ => kind,
        };
        Self::new(kind, path, error.to_string())
    }

    /// The same failure, seen from the backup step, so a caller can tell where a save stopped.
    pub(crate) fn into_backup_failed(self) -> Self {
        match self.kind {
            IoErrorKind::PermissionDenied => self,
            _ => Self {
                kind: IoErrorKind::BackupFailed,
                ..self
            },
        }
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} at {}: {}",
            self.kind,
            self.path.display(),
            self.detail
        )
    }
}

impl std::error::Error for IoError {}

#[cfg(test)]
mod tests {
    use super::{IoError, IoErrorKind};
    use std::io;
    use std::path::Path;

    #[test]
    fn displays_the_kind_the_path_and_the_os_message() {
        let error = IoError::new(
            IoErrorKind::RenameFailed,
            Path::new("/tmp/ep01.srt"),
            "busy",
        );
        assert_eq!(error.to_string(), "RenameFailed at /tmp/ep01.srt: busy");
    }

    #[test]
    fn a_permission_problem_keeps_its_kind_at_every_step() {
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        let error = IoError::from_io(
            &denied,
            Path::new("/tmp/ep01.srt"),
            IoErrorKind::WriteFailed,
        );
        assert_eq!(error.kind, IoErrorKind::PermissionDenied);
        assert_eq!(
            error.into_backup_failed().kind,
            IoErrorKind::PermissionDenied
        );
    }

    #[test]
    fn other_failures_keep_the_step_that_failed() {
        let full = io::Error::from(io::ErrorKind::WriteZero);
        let error = IoError::from_io(&full, Path::new("/tmp/ep01.srt"), IoErrorKind::WriteFailed);
        assert_eq!(error.kind, IoErrorKind::WriteFailed);
        assert!(!error.detail.is_empty(), "the OS message is kept for logs");
        assert_eq!(error.into_backup_failed().kind, IoErrorKind::BackupFailed);
    }
}
