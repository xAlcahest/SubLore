//! Project failures: a stable kind, the path it is about, and a technical detail. BACKLOG.md M4.1.
//!
//! Same shape as `sublore_io::error::IoError`. The kind is the whole vocabulary this crate speaks;
//! `detail` carries the SQLite or operating system message for logs and never becomes UI copy.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::ErrorCode;

/// Exhaustive on purpose: adding a variant must break the mapping in the app, not slip past a
/// wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectErrorKind {
    /// Empty path, or a relative one where an absolute is required.
    InvalidPath,
    FolderNotFound,
    /// A path component is not the kind of thing it must be: the project folder is not a
    /// directory, or `project.sublore` is not a regular file.
    NotADirectory,
    /// `create` where a `project.sublore` already exists.
    AlreadyAProject,
    /// `open` where the folder holds no `project.sublore`.
    NoProjectHere,
    /// Not a database, or a database that is not ours.
    NotASubloreProject,
    /// SQLITE_CORRUPT from a file that does carry our application id.
    DatabaseCorrupt,
    /// The file was written by a newer Sublore. Two numbers, never a sentence.
    SchemaTooNew {
        found: u32,
        supported: u32,
    },
    MigrationFailed,
    PathNotAbsolute,
    PathNotUtf8,
    /// The file being attached is not there.
    FileNotFound,
    /// The file being attached is a directory or a device.
    NotAFile,
    /// Already attached to this episode under this path.
    DuplicateFile,
    EpisodeNotFound,
    /// The attachment row being re-pointed is not in the project any more.
    FileNotAttached,
    /// SQLITE_FULL or an IO error from the database.
    WriteFailed,
    PermissionDenied,
    /// Any other SQLite failure. Never a situation the user created.
    QueryFailed,
    DeleteFailed,
}

#[derive(Clone, Debug)]
pub struct ProjectError {
    pub kind: ProjectErrorKind,
    pub path: PathBuf,
    /// Technical, for logs. Never rendered as UI copy.
    pub detail: String,
}

impl ProjectError {
    pub(crate) fn new(kind: ProjectErrorKind, path: &Path, detail: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }

    /// A SQLite failure. Codes the user can act on keep their own kind; `on_constraint` is what a
    /// UNIQUE or CHECK violation means at this call site, and everything unmapped is a query
    /// failure. See BACKLOG.md M4.1.
    pub(crate) fn from_sqlite(
        error: &rusqlite::Error,
        path: &Path,
        on_constraint: ProjectErrorKind,
    ) -> Self {
        let kind = match error.sqlite_error_code() {
            Some(ErrorCode::NotADatabase) => ProjectErrorKind::NotASubloreProject,
            Some(ErrorCode::DatabaseCorrupt) => ProjectErrorKind::DatabaseCorrupt,
            Some(ErrorCode::CannotOpen) => resolve_cannot_open(path),
            Some(ErrorCode::PermissionDenied | ErrorCode::ReadOnly) => {
                ProjectErrorKind::PermissionDenied
            }
            Some(ErrorCode::DiskFull | ErrorCode::SystemIoFailure) => ProjectErrorKind::WriteFailed,
            Some(ErrorCode::ConstraintViolation) => on_constraint,
            _ => ProjectErrorKind::QueryFailed,
        };
        Self::new(kind, path, error.to_string())
    }

    /// An operating system failure. A permission problem keeps its own kind, as in `sublore-io`:
    /// it is the one the user can act on.
    pub(crate) fn from_io(error: &io::Error, path: &Path, kind: ProjectErrorKind) -> Self {
        let kind = match error.kind() {
            io::ErrorKind::PermissionDenied => ProjectErrorKind::PermissionDenied,
            _ => kind,
        };
        Self::new(kind, path, error.to_string())
    }
}

/// SQLITE_CANTOPEN says nothing about why. Ask the filesystem, so the user gets the one fact that
/// tells them what to fix.
fn resolve_cannot_open(path: &Path) -> ProjectErrorKind {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        match fs::metadata(parent) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return ProjectErrorKind::FolderNotFound
            }
            Ok(metadata) if !metadata.is_dir() => return ProjectErrorKind::NotADirectory,
            _ => {}
        }
    }
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => ProjectErrorKind::NotADirectory,
        _ => ProjectErrorKind::PermissionDenied,
    }
}

impl fmt::Display for ProjectError {
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

impl std::error::Error for ProjectError {}

#[cfg(test)]
mod tests {
    use super::{resolve_cannot_open, ProjectError, ProjectErrorKind};
    use std::io;
    use std::path::Path;

    fn sqlite_failure(code: i32) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), Some("boom".into()))
    }

    #[test]
    fn displays_the_kind_the_path_and_the_message() {
        let error = ProjectError::new(
            ProjectErrorKind::NoProjectHere,
            Path::new("/tmp/series/project.sublore"),
            "nothing there",
        );
        assert_eq!(
            error.to_string(),
            "NoProjectHere at /tmp/series/project.sublore: nothing there"
        );
    }

    #[test]
    fn maps_the_sqlite_codes_the_user_can_act_on() {
        let path = Path::new("/tmp/series/project.sublore");
        let cases = [
            (26, ProjectErrorKind::NotASubloreProject),
            (11, ProjectErrorKind::DatabaseCorrupt),
            (3, ProjectErrorKind::PermissionDenied),
            (8, ProjectErrorKind::PermissionDenied),
            (13, ProjectErrorKind::WriteFailed),
            (10, ProjectErrorKind::WriteFailed),
            (1, ProjectErrorKind::QueryFailed),
        ];
        for (code, expected) in cases {
            let mapped = ProjectError::from_sqlite(
                &sqlite_failure(code),
                path,
                ProjectErrorKind::QueryFailed,
            );
            assert_eq!(mapped.kind, expected, "sqlite code {code}");
            assert!(!mapped.detail.is_empty(), "the message is kept for logs");
        }
    }

    #[test]
    fn a_constraint_violation_means_what_the_call_site_says_it_means() {
        let path = Path::new("/tmp/series/project.sublore");
        let duplicate =
            ProjectError::from_sqlite(&sqlite_failure(19), path, ProjectErrorKind::DuplicateFile);
        assert_eq!(duplicate.kind, ProjectErrorKind::DuplicateFile);
        let other =
            ProjectError::from_sqlite(&sqlite_failure(19), path, ProjectErrorKind::QueryFailed);
        assert_eq!(other.kind, ProjectErrorKind::QueryFailed);
    }

    #[test]
    fn cannot_open_is_resolved_from_the_path() {
        let temp = std::env::temp_dir();
        assert_eq!(
            resolve_cannot_open(&temp.join("no-such-sublore-folder").join("project.sublore")),
            ProjectErrorKind::FolderNotFound
        );
        assert_eq!(
            resolve_cannot_open(&temp),
            ProjectErrorKind::NotADirectory,
            "a directory where a database should be"
        );
    }

    #[test]
    fn a_permission_problem_keeps_its_kind() {
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        let error = ProjectError::from_io(
            &denied,
            Path::new("/tmp/series"),
            ProjectErrorKind::FolderNotFound,
        );
        assert_eq!(error.kind, ProjectErrorKind::PermissionDenied);
    }
}
