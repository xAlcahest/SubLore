//! The only place in Sublore that removes a file from disk. See BACKLOG.md M4.3.
//!
//! It never opens the project database, so no path the user gave us can reach it: the four names
//! it may remove are literals from [`layout::OWNED_FILES`], and it recognises a project by reading
//! 72 bytes of the SQLite file header with `std::fs` alone.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::error::{ProjectError, ProjectErrorKind};
use crate::layout::{self, APPLICATION_ID, OWNED_FILES};

/// The first bytes of every SQLite file, and where the application id sits after them. Both are
/// frozen parts of the SQLite file format.
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";
const APPLICATION_ID_AT: usize = 68;
/// Read exactly as far as the application id: the slice below is then the tail of the array.
const HEADER_LEN: usize = APPLICATION_ID_AT + 4;

/// What [`delete_project`] did. Both lists hold absolute paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteOutcome {
    pub removed: Vec<PathBuf>,
    /// Owned names that were there and were not ours to remove. An entry here means: we did not
    /// touch it.
    pub left_in_place: Vec<PathBuf>,
}

/// What an owned name in the folder turned out to be.
enum Entry {
    Absent,
    Regular,
    /// A link, a directory, a device: something the user made, whatever it is named.
    Other,
}

/// Remove Sublore's own files from `folder`. Never removes the folder, never reads the database,
/// never touches a name that is not in [`layout::OWNED_FILES`]. See BACKLOG.md M4.3.
pub fn delete_project(folder: &Path) -> Result<DeleteOutcome, ProjectError> {
    if folder.as_os_str().is_empty() {
        return Err(ProjectError::new(
            ProjectErrorKind::InvalidPath,
            folder,
            "empty folder path",
        ));
    }

    let database = layout::database_path(folder);
    match classify(&database)? {
        Entry::Absent => {
            return Err(ProjectError::new(
                ProjectErrorKind::NoProjectHere,
                &database,
                "this folder holds no project.sublore",
            ))
        }
        // A link or a directory under our name is the user's. Nothing is removed at all, because
        // without a database we can read we cannot say this folder is a project.
        Entry::Other => {
            return Ok(DeleteOutcome {
                removed: Vec::new(),
                left_in_place: present(folder)?,
            })
        }
        Entry::Regular => {}
    }
    confirm_sublore_project(&database)?;

    let mut removed = Vec::new();
    let mut left_in_place = Vec::new();
    // The database is taken last: if a journal cannot go, the folder is still a project and the
    // whole deletion can simply be run again.
    for name in OWNED_FILES.iter().rev() {
        let path = folder.join(name);
        match classify(&path)? {
            Entry::Absent => continue,
            Entry::Other => left_in_place.push(path),
            Entry::Regular => {
                fs::remove_file(&path).map_err(|error| {
                    ProjectError::from_io(&error, &path, ProjectErrorKind::DeleteFailed)
                })?;
                removed.push(path);
            }
        }
    }
    removed.sort();
    left_in_place.sort();
    Ok(DeleteOutcome {
        removed,
        left_in_place,
    })
}

/// What is at `path`, without following a link: a link is never the file it points at.
fn classify(path: &Path) -> Result<Entry, ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(Entry::Regular),
        Ok(_) => Ok(Entry::Other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Entry::Absent),
        Err(error) => Err(ProjectError::from_io(
            &error,
            path,
            ProjectErrorKind::DeleteFailed,
        )),
    }
}

/// Every owned name that exists in `folder`, in the order of the frozen list.
fn present(folder: &Path) -> Result<Vec<PathBuf>, ProjectError> {
    let mut found = Vec::new();
    for name in OWNED_FILES {
        let path = folder.join(name);
        if !matches!(classify(&path)?, Entry::Absent) {
            found.push(path);
        }
    }
    Ok(found)
}

/// The header says whether this is a SQLite database and whether it is one of ours. Nothing else
/// in the file is read, and nothing in it is trusted.
fn confirm_sublore_project(database: &Path) -> Result<(), ProjectError> {
    let mut header = [0u8; HEADER_LEN];
    File::open(database)
        .map_err(|error| ProjectError::from_io(&error, database, ProjectErrorKind::DeleteFailed))?
        .read_exact(&mut header)
        .map_err(|error| match error.kind() {
            io::ErrorKind::UnexpectedEof => ProjectError::new(
                ProjectErrorKind::NotASubloreProject,
                database,
                "shorter than a SQLite header",
            ),
            _ => ProjectError::from_io(&error, database, ProjectErrorKind::DeleteFailed),
        })?;

    if &header[..SQLITE_MAGIC.len()] != SQLITE_MAGIC {
        return Err(ProjectError::new(
            ProjectErrorKind::NotASubloreProject,
            database,
            "not a SQLite database",
        ));
    }
    let mut id = [0u8; 4];
    id.copy_from_slice(&header[APPLICATION_ID_AT..HEADER_LEN]);
    if i32::from_be_bytes(id) != APPLICATION_ID {
        return Err(ProjectError::new(
            ProjectErrorKind::NotASubloreProject,
            database,
            "another application's database",
        ));
    }
    Ok(())
}
