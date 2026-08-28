//! Errors the project commands send to the UI: a stable code, and for a file from a newer Sublore
//! the two version numbers. The UI maps them through src/i18n/en.ts, so no English prose crosses
//! the IPC boundary. Same shape as `subtitle::error`. See BACKLOG.md M4.4.

use serde::Serialize;
use sublore_project::{ProjectError as CrateError, ProjectErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectErrorCode {
    /// Empty path where one was required.
    InvalidPath,
    FolderNotFound,
    /// The project folder is not a directory, or `project.sublore` is not a regular file.
    NotADirectory,
    /// Creating where a project already exists.
    AlreadyAProject,
    /// Opening or deleting where there is none.
    NoProjectHere,
    /// A `project.sublore` that some other program wrote.
    NotASubloreProject,
    DatabaseCorrupt,
    /// Written by a newer Sublore. `found` and `supported` say which versions.
    SchemaTooNew,
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
    /// The command needs a project and none is open. The app layer's own.
    NoProjectOpen,
    WriteFailed,
    DeleteFailed,
    PermissionDenied,
    QueryFailed,
    /// The command machinery itself failed. Never a situation the user created.
    CommandFailed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectError {
    pub code: ProjectErrorCode,
    /// Both present exactly when the code is `schemaTooNew`.
    pub found: Option<u32>,
    pub supported: Option<u32>,
    /// Technical, never shown to the user, may be empty.
    pub detail: String,
}

impl ProjectError {
    pub fn new(code: ProjectErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            found: None,
            supported: None,
            detail: detail.into(),
        }
    }

    /// Map a crate failure onto the IPC contract. Exhaustive on purpose: a new
    /// [`ProjectErrorKind`] must break this build rather than fall into a wildcard.
    pub fn from_crate(error: CrateError) -> Self {
        let code = match error.kind {
            ProjectErrorKind::InvalidPath => ProjectErrorCode::InvalidPath,
            ProjectErrorKind::FolderNotFound => ProjectErrorCode::FolderNotFound,
            ProjectErrorKind::NotADirectory => ProjectErrorCode::NotADirectory,
            ProjectErrorKind::AlreadyAProject => ProjectErrorCode::AlreadyAProject,
            ProjectErrorKind::NoProjectHere => ProjectErrorCode::NoProjectHere,
            ProjectErrorKind::NotASubloreProject => ProjectErrorCode::NotASubloreProject,
            ProjectErrorKind::DatabaseCorrupt => ProjectErrorCode::DatabaseCorrupt,
            ProjectErrorKind::MigrationFailed => ProjectErrorCode::MigrationFailed,
            ProjectErrorKind::PathNotAbsolute => ProjectErrorCode::PathNotAbsolute,
            ProjectErrorKind::PathNotUtf8 => ProjectErrorCode::PathNotUtf8,
            ProjectErrorKind::FileNotFound => ProjectErrorCode::FileNotFound,
            ProjectErrorKind::NotAFile => ProjectErrorCode::NotAFile,
            ProjectErrorKind::DuplicateFile => ProjectErrorCode::DuplicateFile,
            ProjectErrorKind::EpisodeNotFound => ProjectErrorCode::EpisodeNotFound,
            ProjectErrorKind::WriteFailed => ProjectErrorCode::WriteFailed,
            ProjectErrorKind::DeleteFailed => ProjectErrorCode::DeleteFailed,
            ProjectErrorKind::PermissionDenied => ProjectErrorCode::PermissionDenied,
            ProjectErrorKind::QueryFailed => ProjectErrorCode::QueryFailed,
            // The two numbers ride along, because the sentence needs them and prose cannot.
            ProjectErrorKind::SchemaTooNew { found, supported } => {
                return Self {
                    code: ProjectErrorCode::SchemaTooNew,
                    found: Some(found),
                    supported: Some(supported),
                    detail: error.to_string(),
                }
            }
        };
        Self::new(code, error.to_string())
    }
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

impl std::error::Error for ProjectError {}

#[cfg(test)]
mod tests {
    use super::{ProjectError, ProjectErrorCode};
    use std::path::Path;
    use sublore_project::{ProjectError as CrateError, ProjectErrorKind};

    /// `ProjectError::new` is `pub(crate)` in the crate, so a failure is built by asking the crate
    /// for one. Creating twice in the same folder is the cheapest kind to obtain.
    fn crate_error(kind: ProjectErrorKind) -> CrateError {
        CrateError {
            kind,
            path: Path::new("/tmp/series/project.sublore").to_path_buf(),
            detail: "boom".to_owned(),
        }
    }

    #[test]
    fn every_crate_kind_maps_to_a_code_the_ui_has_a_sentence_for() {
        for (kind, code) in [
            (ProjectErrorKind::InvalidPath, ProjectErrorCode::InvalidPath),
            (
                ProjectErrorKind::FolderNotFound,
                ProjectErrorCode::FolderNotFound,
            ),
            (
                ProjectErrorKind::NotADirectory,
                ProjectErrorCode::NotADirectory,
            ),
            (
                ProjectErrorKind::AlreadyAProject,
                ProjectErrorCode::AlreadyAProject,
            ),
            (
                ProjectErrorKind::NoProjectHere,
                ProjectErrorCode::NoProjectHere,
            ),
            (
                ProjectErrorKind::NotASubloreProject,
                ProjectErrorCode::NotASubloreProject,
            ),
            (
                ProjectErrorKind::DatabaseCorrupt,
                ProjectErrorCode::DatabaseCorrupt,
            ),
            (
                ProjectErrorKind::MigrationFailed,
                ProjectErrorCode::MigrationFailed,
            ),
            (
                ProjectErrorKind::PathNotAbsolute,
                ProjectErrorCode::PathNotAbsolute,
            ),
            (ProjectErrorKind::PathNotUtf8, ProjectErrorCode::PathNotUtf8),
            (
                ProjectErrorKind::FileNotFound,
                ProjectErrorCode::FileNotFound,
            ),
            (ProjectErrorKind::NotAFile, ProjectErrorCode::NotAFile),
            (
                ProjectErrorKind::DuplicateFile,
                ProjectErrorCode::DuplicateFile,
            ),
            (
                ProjectErrorKind::EpisodeNotFound,
                ProjectErrorCode::EpisodeNotFound,
            ),
            (ProjectErrorKind::WriteFailed, ProjectErrorCode::WriteFailed),
            (
                ProjectErrorKind::DeleteFailed,
                ProjectErrorCode::DeleteFailed,
            ),
            (
                ProjectErrorKind::PermissionDenied,
                ProjectErrorCode::PermissionDenied,
            ),
            (ProjectErrorKind::QueryFailed, ProjectErrorCode::QueryFailed),
        ] {
            let error = ProjectError::from_crate(crate_error(kind));
            assert_eq!(error.code, code, "{kind:?}");
            assert_eq!(error.found, None, "{kind:?}");
            assert_eq!(error.supported, None, "{kind:?}");
            assert!(error.detail.contains("boom"), "{kind:?}");
        }
    }

    #[test]
    fn a_project_from_the_future_carries_both_version_numbers() {
        let error = ProjectError::from_crate(crate_error(ProjectErrorKind::SchemaTooNew {
            found: 7,
            supported: 1,
        }));
        assert_eq!(error.code, ProjectErrorCode::SchemaTooNew);
        assert_eq!(error.found, Some(7));
        assert_eq!(error.supported, Some(1));
    }

    #[test]
    fn the_app_s_own_codes_carry_no_version_numbers() {
        for code in [
            ProjectErrorCode::NoProjectOpen,
            ProjectErrorCode::CommandFailed,
        ] {
            let error = ProjectError::new(code, "nothing open");
            assert_eq!(error.code, code);
            assert_eq!(error.found, None);
            assert_eq!(error.supported, None);
        }
    }
}
