//! The project a user has open, its episodes, and the files attached to them. See BACKLOG.md M4.2.
//!
//! Nothing here writes to a user's file. `attach_file` reads one metadata entry and stores a path;
//! `detach_file` and `delete_episode` run SQL and are incapable of anything else. CONTRIBUTING.md §3.1.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::types::Type;
use rusqlite::{params, OptionalExtension, Row};

use crate::db::Database;
use crate::error::{ProjectError, ProjectErrorKind};
use crate::model::{Episode, EpisodeFile, FileRole, ProjectSummary};

/// An open project: the connection, and the summary the UI shows beside it.
#[derive(Debug)]
pub struct Project {
    database: Database,
    summary: ProjectSummary,
}

impl Project {
    pub fn create(folder: &Path, title: &str, now: SystemTime) -> Result<Self, ProjectError> {
        Self::from_database(Database::create(folder, title, now)?)
    }

    pub fn open(folder: &Path) -> Result<Self, ProjectError> {
        Self::from_database(Database::open(folder)?)
    }

    pub fn close(self) -> Result<(), ProjectError> {
        self.database.close()
    }

    pub fn summary(&self) -> &ProjectSummary {
        &self.summary
    }

    /// Re-read the title and the episode count. Every operation here that changes either one calls
    /// it, so `summary` is never stale.
    pub fn refresh_summary(&mut self) -> Result<&ProjectSummary, ProjectError> {
        self.summary = read_summary(&self.database)?;
        Ok(&self.summary)
    }

    /// The first query a freshly opened database runs, and so the one that reports a file that is
    /// damaged rather than merely unfamiliar.
    fn from_database(database: Database) -> Result<Self, ProjectError> {
        let summary = read_summary(&database)?;
        Ok(Self { database, summary })
    }
}

/// Append an episode. The ordinal is `MAX(ordinal) + 1`, computed inside the insert itself.
pub fn add_episode(
    project: &mut Project,
    title: &str,
    now: SystemTime,
) -> Result<Episode, ProjectError> {
    let database = project.database.database_path().to_path_buf();
    let episode = project
        .database
        .conn()
        .query_row(
            "INSERT INTO episodes (series_id, ordinal, title, created_at)
             VALUES (1, (SELECT COALESCE(MAX(ordinal), 0) + 1 FROM episodes), ?1, ?2)
             RETURNING id, ordinal, title, created_at",
            params![title, unix_seconds(now)],
            to_episode,
        )
        .map_err(|error| query_failed(&error, &database))?;
    project.refresh_summary()?;
    Ok(episode)
}

/// Ordered by ordinal, which is the order the user added them in.
pub fn episodes(project: &Project) -> Result<Vec<Episode>, ProjectError> {
    let database = project.database.database_path();
    let mut statement = project
        .database
        .conn()
        .prepare("SELECT id, ordinal, title, created_at FROM episodes ORDER BY ordinal")
        .map_err(|error| query_failed(&error, database))?;
    let rows = statement
        .query_map([], to_episode)
        .map_err(|error| query_failed(&error, database))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| query_failed(&error, database))
}

/// Record `path` against `episode_id`. Reads the file's metadata and nothing else: it is never
/// opened for writing, never copied, never moved. See CONTRIBUTING.md §3.1.
pub fn attach_file(
    project: &mut Project,
    episode_id: i64,
    role: FileRole,
    path: &Path,
    now: SystemTime,
) -> Result<EpisodeFile, ProjectError> {
    let text = validate_path(path)?;
    let metadata = read_metadata(path)?;
    let database = project.database.database_path().to_path_buf();

    // Checked before the insert so a missing episode is not reported as a duplicate: both are a
    // constraint violation to SQLite.
    if !episode_exists(&project.database, episode_id)? {
        return Err(ProjectError::new(
            ProjectErrorKind::EpisodeNotFound,
            &database,
            format!("no episode {episode_id}"),
        ));
    }

    project
        .database
        .conn()
        .query_row(
            "INSERT INTO episode_files (episode_id, role, path, byte_length, modified_at, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             RETURNING id, episode_id, role, path, byte_length, modified_at, added_at",
            params![
                episode_id,
                role.as_str(),
                text,
                metadata.len() as i64,
                metadata.modified().ok().map(unix_seconds),
                unix_seconds(now),
            ],
            to_file,
        )
        .map_err(|error| {
            ProjectError::from_sqlite(&error, &database, ProjectErrorKind::DuplicateFile)
        })
}

/// Ordered by id, which is attach order.
pub fn files(project: &Project, episode_id: i64) -> Result<Vec<EpisodeFile>, ProjectError> {
    let database = project.database.database_path();
    let mut statement = project
        .database
        .conn()
        .prepare(
            "SELECT id, episode_id, role, path, byte_length, modified_at, added_at
             FROM episode_files WHERE episode_id = ?1 ORDER BY id",
        )
        .map_err(|error| query_failed(&error, database))?;
    let rows = statement
        .query_map(params![episode_id], to_file)
        .map_err(|error| query_failed(&error, database))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| query_failed(&error, database))
}

/// Remove one attachment record. The file on disk is not touched. Detaching a record that is
/// already gone is not a failure: what was asked for is not there either way.
pub fn detach_file(project: &mut Project, file_id: i64) -> Result<(), ProjectError> {
    let database = project.database.database_path().to_path_buf();
    project
        .database
        .conn()
        .execute("DELETE FROM episode_files WHERE id = ?1", params![file_id])
        .map_err(|error| query_failed(&error, &database))?;
    Ok(())
}

/// Remove an episode and, by cascade, its attachment records. No file on disk is touched.
pub fn delete_episode(project: &mut Project, episode_id: i64) -> Result<(), ProjectError> {
    let database = project.database.database_path().to_path_buf();
    let rows = project
        .database
        .conn()
        .execute("DELETE FROM episodes WHERE id = ?1", params![episode_id])
        .map_err(|error| query_failed(&error, &database))?;
    if rows == 0 {
        return Err(ProjectError::new(
            ProjectErrorKind::EpisodeNotFound,
            &database,
            format!("no episode {episode_id}"),
        ));
    }
    project.refresh_summary()?;
    Ok(())
}

/// Every statement here fails the same way: only `attach_file` can cause a constraint violation
/// that means something more precise.
fn query_failed(error: &rusqlite::Error, database: &Path) -> ProjectError {
    ProjectError::from_sqlite(error, database, ProjectErrorKind::QueryFailed)
}

fn read_summary(database: &Database) -> Result<ProjectSummary, ProjectError> {
    let path = database.database_path();
    let title: Option<String> = database
        .conn()
        .query_row("SELECT title FROM series WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|error| query_failed(&error, path))?;
    let Some(title) = title else {
        return Err(ProjectError::new(
            ProjectErrorKind::NotASubloreProject,
            path,
            "the schema is ours but the series row is missing",
        ));
    };
    let episode_count: i64 = database
        .conn()
        .query_row("SELECT COUNT(*) FROM episodes", [], |row| row.get(0))
        .map_err(|error| query_failed(&error, path))?;

    Ok(ProjectSummary {
        folder: database.folder().to_path_buf(),
        database: path.to_path_buf(),
        title,
        schema_version: database.version(),
        episode_count: episode_count.max(0) as usize,
    })
}

fn episode_exists(database: &Database, episode_id: i64) -> Result<bool, ProjectError> {
    let path = database.database_path();
    database
        .conn()
        .query_row(
            "SELECT 1 FROM episodes WHERE id = ?1",
            params![episode_id],
            |_| Ok(()),
        )
        .optional()
        .map(|found| found.is_some())
        .map_err(|error| query_failed(&error, path))
}

/// A path is stored exactly as the user spelled it: absolute, not resolved. Resolving would follow
/// a symlink the user chose on purpose, and would make two spellings of one file indistinguishable.
fn validate_path(path: &Path) -> Result<&str, ProjectError> {
    if path.as_os_str().is_empty() {
        return Err(ProjectError::new(
            ProjectErrorKind::InvalidPath,
            path,
            "empty path",
        ));
    }
    if !path.is_absolute() {
        return Err(ProjectError::new(
            ProjectErrorKind::PathNotAbsolute,
            path,
            "a relative path does not survive reopening the project somewhere else",
        ));
    }
    path.to_str().ok_or_else(|| {
        ProjectError::new(
            ProjectErrorKind::PathNotUtf8,
            path,
            "the path column is text, and this path is not valid UTF-8",
        )
    })
}

/// The only filesystem call in the attach path, and it is a read. See CONTRIBUTING.md §3.1.
fn read_metadata(path: &Path) -> Result<fs::Metadata, ProjectError> {
    let metadata = fs::metadata(path)
        .map_err(|error| ProjectError::from_io(&error, path, ProjectErrorKind::FileNotFound))?;
    if !metadata.is_file() {
        return Err(ProjectError::new(
            ProjectErrorKind::NotAFile,
            path,
            "not a regular file",
        ));
    }
    Ok(metadata)
}

/// Unix seconds, UTC. A stamp before 1970 is recorded as a negative second rather than failing.
fn unix_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_secs() as i64,
        Err(error) => -(error.duration().as_secs() as i64),
    }
}

fn to_episode(row: &Row<'_>) -> rusqlite::Result<Episode> {
    Ok(Episode {
        id: row.get(0)?,
        // Clamped, not cast: the database file is untrusted input, and a truncating cast would
        // turn a nonsense ordinal into a plausible one.
        ordinal: row.get::<_, i64>(1)?.clamp(0, i64::from(u32::MAX)) as u32,
        title: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn to_file(row: &Row<'_>) -> rusqlite::Result<EpisodeFile> {
    let spelling: String = row.get(2)?;
    // The column has a CHECK constraint, so an unknown role means the file was edited outside
    // Sublore. It is a read failure, not a role.
    let role = FileRole::parse(&spelling).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(2, format!("role {spelling:?}"), Type::Text)
    })?;
    Ok(EpisodeFile {
        id: row.get(0)?,
        episode_id: row.get(1)?,
        role,
        path: PathBuf::from(row.get::<_, String>(3)?),
        byte_length: row.get::<_, Option<i64>>(4)?.map(|size| size.max(0) as u64),
        modified_at: row.get(5)?,
        added_at: row.get(6)?,
    })
}
