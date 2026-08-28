//! Opening and creating the project database. BACKLOG.md M4.1.
//!
//! `create` takes the database's path in one atomic step, so two callers racing one folder cannot
//! both write into it. `open` probes the file header before it writes anything: a database that is
//! not ours, or one written by a newer Sublore, is refused while it is still untouched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use crate::error::{ProjectError, ProjectErrorKind};
use crate::layout::{self, APPLICATION_ID, CURRENT_VERSION};
use crate::migrate;

/// How long a statement waits for another connection's write lock.
const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

/// An open project database. One file, one connection, one series.
#[derive(Debug)]
pub struct Database {
    conn: Connection,
    folder: PathBuf,
    database: PathBuf,
    version: u32,
}

impl Database {
    /// Create `folder/project.sublore`, migrate it to [`CURRENT_VERSION`] and insert the single
    /// series row. The folder must already exist: Sublore never makes directories in the user's
    /// filesystem.
    pub fn create(folder: &Path, title: &str, now: SystemTime) -> Result<Self, ProjectError> {
        check_folder(folder)?;
        let database = layout::database_path(folder);
        claim(&database)?;

        // No CREATE: the file to open is the one just claimed, never a new one at that name.
        // NOFOLLOW so a link swapped in behind the claim is refused instead of written through.
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let conn = Connection::open_with_flags(&database, flags).map_err(|error| {
            ProjectError::from_sqlite(&error, &database, ProjectErrorKind::QueryFailed)
        })?;
        // The claim was an empty file; anything in it now arrived from outside, and that is
        // neither ours to write into nor to take back below.
        check_empty(&conn, &database)?;

        let mut db = Self {
            conn,
            folder: folder.to_path_buf(),
            database,
            version: 0,
        };
        match db.build(title, now) {
            Ok(()) => Ok(db),
            Err(error) => Err(db.undo(error)),
        }
    }

    /// Open `folder/project.sublore`: probe the header, refuse a newer schema, apply the connection
    /// pragmas, then migrate forward. A missing file is an error, never a silent create.
    pub fn open(folder: &Path) -> Result<Self, ProjectError> {
        check_folder(folder)?;
        let database = layout::database_path(folder);
        match fs::metadata(&database) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => {
                return Err(ProjectError::new(
                    ProjectErrorKind::NotADirectory,
                    &database,
                    "the project database is not a regular file",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(ProjectError::new(
                    ProjectErrorKind::NoProjectHere,
                    &database,
                    "this folder holds no project",
                ))
            }
            Err(error) => {
                return Err(ProjectError::from_io(
                    &error,
                    &database,
                    ProjectErrorKind::QueryFailed,
                ))
            }
        }

        // No CREATE, and no URI: a user path that happens to start with "file:" stays a path.
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(&database, flags).map_err(|error| {
            ProjectError::from_sqlite(&error, &database, ProjectErrorKind::QueryFailed)
        })?;

        // Header reads only. Nothing is written until the version has been accepted.
        let stamped = migrate::read_application_id(&conn, &database)?;
        if stamped != APPLICATION_ID {
            return Err(ProjectError::new(
                ProjectErrorKind::NotASubloreProject,
                &database,
                format!("application id {stamped:#010x}, not Sublore's"),
            ));
        }
        let found = migrate::read_version(&conn, &database)?;
        if found == 0 {
            // `create` commits the schema and the id together, so ours is never version 0.
            return Err(ProjectError::new(
                ProjectErrorKind::NotASubloreProject,
                &database,
                "the file carries no schema version",
            ));
        }
        if found > CURRENT_VERSION {
            return Err(ProjectError::new(
                ProjectErrorKind::SchemaTooNew {
                    found,
                    supported: CURRENT_VERSION,
                },
                &database,
                format!("the project is at version {found}, this build reads {CURRENT_VERSION}"),
            ));
        }

        let mut db = Self {
            conn,
            folder: folder.to_path_buf(),
            database,
            version: found,
        };
        apply_pragmas(&db.conn, &db.database)?;
        db.version = migrate::migrate(&mut db.conn)?;
        Ok(db)
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub fn database_path(&self) -> &Path {
        &self.database
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Close the connection, surfacing a failure rather than swallowing it in a drop.
    pub fn close(self) -> Result<(), ProjectError> {
        let database = self.database;
        self.conn.close().map_err(|(_, error)| {
            ProjectError::from_sqlite(&error, &database, ProjectErrorKind::QueryFailed)
        })
    }

    fn build(&mut self, title: &str, now: SystemTime) -> Result<(), ProjectError> {
        apply_pragmas(&self.conn, &self.database)?;
        self.version = migrate::migrate(&mut self.conn)?;
        // Data, not schema: a database built by the runner alone has the schema and no rows, which
        // is what the migration round-trip test needs.
        self.conn
            .execute(
                "INSERT INTO series (id, title, created_at) VALUES (1, ?1, ?2)",
                (title, unix_seconds(now)),
            )
            .map_err(|error| {
                ProjectError::from_sqlite(&error, &self.database, ProjectErrorKind::WriteFailed)
            })?;
        Ok(())
    }

    /// Take back a half-made project. `claim` created the file in one atomic step and no second
    /// caller can have taken the path since, so it holds nothing of the user's. Closing first lets
    /// SQLite clear its own journals. The one deletion outside `delete.rs`; see BACKLOG.md M4.3.
    fn undo(self, error: ProjectError) -> ProjectError {
        let database = self.database;
        drop(self.conn);
        // The original failure is the one the user can act on; a stale file is the lesser problem.
        let _ = fs::remove_file(&database);
        error
    }
}

/// Unix seconds, UTC. Instants before the epoch, which a wrong system clock can produce, stay
/// negative rather than wrapping.
pub(crate) fn unix_seconds(now: SystemTime) -> i64 {
    match now.duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).unwrap_or(i64::MAX),
        Err(before) => -i64::try_from(before.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

/// Take the database's path before anything opens it. `create_new` is one atomic step and fails if
/// a file, a directory or a link is already there, so two creates racing one folder can never both
/// go on to write, and the loser never has a database of anyone's to take back. See CLAUDE.md §3.
fn claim(database: &Path) -> Result<(), ProjectError> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(database)
    {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(ProjectError::new(
            ProjectErrorKind::AlreadyAProject,
            database,
            "the folder already holds a project",
        )),
        Err(error) => Err(ProjectError::from_io(
            &error,
            database,
            ProjectErrorKind::WriteFailed,
        )),
    }
}

/// A database `create` may adopt: one it just made, holding nothing at all.
fn check_empty(conn: &Connection, database: &Path) -> Result<(), ProjectError> {
    let objects: i64 = conn
        .query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0))
        .map_err(|error| {
            ProjectError::from_sqlite(&error, database, ProjectErrorKind::QueryFailed)
        })?;
    let stamped = migrate::read_application_id(conn, database)?;
    let version = migrate::read_version(conn, database)?;
    if objects == 0 && stamped == 0 && version == 0 {
        return Ok(());
    }
    Err(ProjectError::new(
        ProjectErrorKind::AlreadyAProject,
        database,
        format!("the file is not empty: {objects} objects, application id {stamped:#010x}, version {version}"),
    ))
}

fn check_folder(folder: &Path) -> Result<(), ProjectError> {
    if folder.as_os_str().is_empty() {
        return Err(ProjectError::new(
            ProjectErrorKind::InvalidPath,
            folder,
            "the project folder path is empty",
        ));
    }
    match fs::metadata(folder) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(ProjectError::new(
            ProjectErrorKind::NotADirectory,
            folder,
            "the project path is not a directory",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(ProjectError::new(
            ProjectErrorKind::FolderNotFound,
            folder,
            error.to_string(),
        )),
        Err(error) => Err(ProjectError::from_io(
            &error,
            folder,
            ProjectErrorKind::FolderNotFound,
        )),
    }
}

/// Applied on every connection, after the header has been accepted.
fn apply_pragmas(conn: &Connection, path: &Path) -> Result<(), ProjectError> {
    // Never inherited: a bundled SQLite defaults foreign keys on, a system one off.
    set_pragma(conn, "foreign_keys", 1, path)?;
    // A committed transaction is on disk. WAL with NORMAL can lose one on power loss.
    set_pragma(conn, "synchronous", 2, path)?;
    // The file came from the user's disk; its schema is untrusted input.
    set_pragma(conn, "trusted_schema", 0, path)?;
    conn.busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| ProjectError::from_sqlite(&error, path, ProjectErrorKind::QueryFailed))?;
    // Best effort: a filesystem without shared memory keeps its previous journal mode, and a
    // project on a network share must still open. The database is correct either way.
    let _mode: Result<String, _> =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0));
    Ok(())
}

fn set_pragma(conn: &Connection, name: &str, value: i32, path: &Path) -> Result<(), ProjectError> {
    conn.pragma_update(None, name, value)
        .map_err(|error| ProjectError::from_sqlite(&error, path, ProjectErrorKind::QueryFailed))
}

#[cfg(test)]
mod tests {
    use super::{apply_pragmas, check_empty, unix_seconds};
    use crate::error::ProjectErrorKind;
    use rusqlite::Connection;
    use std::path::Path;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn the_connection_pragmas_are_the_ones_that_were_asked_for() {
        let conn = Connection::open_in_memory().expect("an in-memory database should open");
        apply_pragmas(&conn, Path::new("")).expect("the pragmas should apply");
        for (name, expected) in [
            ("foreign_keys", 1),
            ("synchronous", 2),
            ("trusted_schema", 0),
        ] {
            let actual: i32 = conn
                .pragma_query_value(None, name, |row| row.get(0))
                .unwrap_or_else(|error| panic!("{name} should be readable: {error}"));
            assert_eq!(actual, expected, "{name}");
        }
    }

    #[test]
    fn create_only_adopts_a_database_with_nothing_in_it() {
        let database = Path::new("project.sublore");
        let conn = Connection::open_in_memory().expect("an in-memory database should open");
        check_empty(&conn, database).expect("a brand new database is empty");

        conn.execute_batch("CREATE TABLE someone_elses (id INTEGER)")
            .expect("the foreign table should be creatable");
        assert_eq!(
            check_empty(&conn, database)
                .expect_err("a database holding tables is not ours to write into")
                .kind,
            ProjectErrorKind::AlreadyAProject
        );

        let stamped = Connection::open_in_memory().expect("an in-memory database should open");
        stamped
            .pragma_update(None, "application_id", 0x4F_54_48_52_i32)
            .expect("the application id should be settable");
        assert_eq!(
            check_empty(&stamped, database)
                .expect_err("a stamped database is not ours to write into")
                .kind,
            ProjectErrorKind::AlreadyAProject
        );
    }

    #[test]
    fn timestamps_are_unix_seconds_and_never_wrap() {
        assert_eq!(unix_seconds(UNIX_EPOCH), 0);
        assert_eq!(
            unix_seconds(UNIX_EPOCH + Duration::from_secs(1_756_300_000)),
            1_756_300_000
        );
        assert_eq!(unix_seconds(UNIX_EPOCH - Duration::from_secs(90)), -90);
    }
}
