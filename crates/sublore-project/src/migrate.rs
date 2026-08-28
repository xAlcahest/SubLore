//! Versioned schema migrations. BACKLOG.md M4.1.
//!
//! One transaction per step, and that transaction also writes `user_version`, so a step that fails
//! leaves the database exactly at the version it started from: nothing partial, ever.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, TransactionBehavior};

use crate::error::{ProjectError, ProjectErrorKind};
use crate::layout::{APPLICATION_ID, CURRENT_VERSION};

/// One schema step. Once a version has shipped its SQL is frozen: editing it changes what that
/// version means for every database already on a user's disk. `tests/migrations.rs` fails on any
/// edit, because the golden schema dump no longer matches.
pub(crate) struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

/// Migration 1. One project is one series, pinned by the CHECK on `series.id`.
///
/// No comment may go inside a CREATE statement: SQLite keeps the statement text verbatim in
/// `sqlite_master`, and the golden dump collapses newlines. Both foreign-key child columns are the
/// leading column of a UNIQUE index the constraints already build, so no extra index is needed.
const V1: &str = "
CREATE TABLE series (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    title TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE episodes (
    id INTEGER PRIMARY KEY,
    series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    title TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (series_id, ordinal)
);

CREATE TABLE episode_files (
    id INTEGER PRIMARY KEY,
    episode_id INTEGER NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('media', 'source', 'target')),
    path TEXT NOT NULL,
    byte_length INTEGER,
    modified_at INTEGER,
    added_at INTEGER NOT NULL,
    UNIQUE (episode_id, path)
);
";

/// Every migration, in ascending order. Frozen once shipped.
pub(crate) const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: V1,
}];

/// Bring `conn` up to [`CURRENT_VERSION`]. Returns the version it ended on.
pub fn migrate(conn: &mut Connection) -> Result<u32, ProjectError> {
    migrate_to(conn, CURRENT_VERSION)
}

/// Bring `conn` up to `target`, which no build can push past [`CURRENT_VERSION`].
pub fn migrate_to(conn: &mut Connection, target: u32) -> Result<u32, ProjectError> {
    apply(conn, MIGRATIONS, target)
}

pub(crate) fn apply(
    conn: &mut Connection,
    migrations: &[Migration],
    target: u32,
) -> Result<u32, ProjectError> {
    let path = path_of(conn);
    let highest = migrations.last().map_or(0, |migration| migration.version);
    if target > highest {
        return Err(ProjectError::new(
            ProjectErrorKind::MigrationFailed,
            &path,
            format!("no migration defines version {target}; the highest known is {highest}"),
        ));
    }

    let found = read_version(conn, &path)?;
    // The runner must not become a second way into a database this build cannot read.
    if found > target {
        return Err(ProjectError::new(
            ProjectErrorKind::SchemaTooNew {
                found,
                supported: target,
            },
            &path,
            format!("the database is at version {found}, this build migrates to {target}"),
        ));
    }

    for migration in migrations
        .iter()
        .filter(|migration| migration.version > found && migration.version <= target)
    {
        step(conn, migration, &path)?;
    }
    read_version(conn, &path)
}

fn step(conn: &mut Connection, migration: &Migration, path: &Path) -> Result<(), ProjectError> {
    // SQLite's own advice for schema changes: a table rebuild trips constraints mid-flight, and
    // `foreign_key_check` before the commit is what makes turning them off safe.
    set_foreign_keys(conn, false, path)?;
    let outcome = run(conn, migration, path);
    // Put the connection back the way every caller expects it, committed or rolled back.
    let restored = set_foreign_keys(conn, true, path);
    outcome.and(restored)
}

fn run(conn: &mut Connection, migration: &Migration, path: &Path) -> Result<(), ProjectError> {
    let version = migration.version;
    // IMMEDIATE takes the write lock up front instead of failing halfway through on a busy file.
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| failed(&error, path, version))?;

    transaction
        .execute_batch(migration.sql)
        .map_err(|error| failed(&error, path, version))?;
    transaction
        .pragma_update(None, "application_id", APPLICATION_ID)
        .map_err(|error| failed(&error, path, version))?;
    transaction
        .pragma_update(None, "user_version", version)
        .map_err(|error| failed(&error, path, version))?;

    let dangling = foreign_key_violations(&transaction, path, version)?;
    if dangling > 0 {
        return Err(ProjectError::new(
            ProjectErrorKind::MigrationFailed,
            path,
            format!("migration {version} left {dangling} dangling foreign key rows"),
        ));
    }

    transaction
        .commit()
        .map_err(|error| failed(&error, path, version))
}

fn foreign_key_violations(
    conn: &Connection,
    path: &Path,
    version: u32,
) -> Result<usize, ProjectError> {
    let mut dangling = 0usize;
    conn.pragma_query(None, "foreign_key_check", |_| {
        dangling += 1;
        Ok(())
    })
    .map_err(|error| failed(&error, path, version))?;
    Ok(dangling)
}

/// The integer form, not "ON"/"OFF": an unrecognised pragma value is a silent no-op in SQLite.
fn set_foreign_keys(conn: &Connection, on: bool, path: &Path) -> Result<(), ProjectError> {
    conn.pragma_update(None, "foreign_keys", i32::from(on))
        .map_err(|error| ProjectError::from_sqlite(&error, path, ProjectErrorKind::MigrationFailed))
}

fn failed(error: &rusqlite::Error, path: &Path, version: u32) -> ProjectError {
    let mapped = ProjectError::from_sqlite(error, path, ProjectErrorKind::MigrationFailed);
    // Corruption and permissions stay themselves: they are what the user has to act on.
    match mapped.kind {
        ProjectErrorKind::QueryFailed => ProjectError::new(
            ProjectErrorKind::MigrationFailed,
            path,
            format!("migration {version}: {}", mapped.detail),
        ),
        _ => mapped,
    }
}

/// Read as i64 and convert: SQLite keeps `user_version` as a signed 32-bit field, and a
/// hand-edited negative one is a header no build of Sublore wrote.
pub(crate) fn read_version(conn: &Connection, path: &Path) -> Result<u32, ProjectError> {
    let raw: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| ProjectError::from_sqlite(&error, path, ProjectErrorKind::QueryFailed))?;
    u32::try_from(raw).map_err(|_| {
        ProjectError::new(
            ProjectErrorKind::NotASubloreProject,
            path,
            format!("schema version {raw} is not a version any Sublore wrote"),
        )
    })
}

/// The header field is 32 bits and SQLite hands it back signed, so i32 always fits.
pub(crate) fn read_application_id(conn: &Connection, path: &Path) -> Result<i32, ProjectError> {
    conn.pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|error| ProjectError::from_sqlite(&error, path, ProjectErrorKind::QueryFailed))
}

fn path_of(conn: &Connection) -> PathBuf {
    conn.path().map(PathBuf::from).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{apply, Migration, MIGRATIONS};
    use crate::error::ProjectErrorKind;
    use crate::layout::{APPLICATION_ID, CURRENT_VERSION};
    use rusqlite::Connection;

    /// Three steps, so the runner is driven over more than the one version that ships today.
    const SYNTHETIC: &[Migration] = &[
        Migration {
            version: 1,
            sql: "CREATE TABLE one (id INTEGER PRIMARY KEY)",
        },
        Migration {
            version: 2,
            sql: "CREATE TABLE two (id INTEGER PRIMARY KEY)",
        },
        Migration {
            version: 3,
            sql: "CREATE TABLE three (id INTEGER PRIMARY KEY)",
        },
    ];

    /// The second step cannot run: `two` is already there when the runner reaches it.
    const BROKEN: &[Migration] = &[
        Migration {
            version: 1,
            sql: "CREATE TABLE one (id INTEGER PRIMARY KEY)",
        },
        Migration {
            version: 2,
            sql: "CREATE TABLE half (id INTEGER PRIMARY KEY); CREATE TABLE standing (id INTEGER)",
        },
        Migration {
            version: 3,
            sql: "CREATE TABLE three (id INTEGER PRIMARY KEY)",
        },
    ];

    fn memory() -> Connection {
        Connection::open_in_memory().expect("an in-memory database should open")
    }

    fn version(conn: &Connection) -> u32 {
        conn.pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user_version should be readable")
    }

    fn foreign_keys(conn: &Connection) -> i32 {
        conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("foreign_keys should be readable")
    }

    fn tables(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' \
                 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .expect("the table query should prepare");
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("the table query should run")
            .collect::<Result<Vec<_>, _>>()
            .expect("every table name should be readable");
        names
    }

    #[test]
    fn the_shipped_list_is_ascending_and_ends_at_the_current_version() {
        let mut expected = 1;
        for migration in MIGRATIONS {
            assert_eq!(
                migration.version, expected,
                "migrations run from 1 with no gaps"
            );
            expected += 1;
        }
        assert_eq!(
            MIGRATIONS.last().map(|migration| migration.version),
            Some(CURRENT_VERSION),
            "the highest migration is what this build calls current"
        );
    }

    #[test]
    fn a_fresh_database_runs_every_step_in_order() {
        let mut conn = memory();
        assert_eq!(
            apply(&mut conn, SYNTHETIC, 3).expect("three steps should run"),
            3
        );
        assert_eq!(version(&conn), 3);
        assert_eq!(tables(&conn), vec!["one", "three", "two"]);
        let stamped: i32 = conn
            .pragma_query_value(None, "application_id", |row| row.get(0))
            .expect("application_id should be readable");
        assert_eq!(stamped, APPLICATION_ID);
    }

    #[test]
    fn a_lower_target_stops_where_it_was_asked_to() {
        let mut conn = memory();
        assert_eq!(
            apply(&mut conn, SYNTHETIC, 2).expect("two steps should run"),
            2
        );
        assert_eq!(
            tables(&conn),
            vec!["one", "two"],
            "the third step is not run"
        );

        // And picking up later runs only what is left.
        assert_eq!(
            apply(&mut conn, SYNTHETIC, 3).expect("the last step should run"),
            3
        );
        assert_eq!(tables(&conn), vec!["one", "three", "two"]);
    }

    #[test]
    fn a_step_that_fails_takes_its_own_work_back_with_it() {
        let mut conn = memory();
        conn.execute_batch("CREATE TABLE standing (already INTEGER)")
            .expect("the standing table should be creatable");

        let error = apply(&mut conn, BROKEN, 3).expect_err("the second step cannot run");

        assert_eq!(error.kind, ProjectErrorKind::MigrationFailed);
        assert!(!error.detail.is_empty(), "the SQLite message is kept");
        assert_eq!(
            version(&conn),
            1,
            "the version stops at the last step that committed"
        );
        assert_eq!(
            tables(&conn),
            vec!["one", "standing"],
            "the first step stands, and `half` went back with the step that made it"
        );

        // Foreign keys are back on: the failed step did not leave the connection loose.
        assert_eq!(foreign_keys(&conn), 1);

        // And a fixed list picks up from where the failure stopped.
        assert_eq!(
            apply(&mut conn, SYNTHETIC, 3).expect("the repaired list should run"),
            3
        );
        assert_eq!(tables(&conn), vec!["one", "standing", "three", "two"]);
    }

    #[test]
    fn turning_foreign_keys_off_actually_turns_them_off() {
        let conn = memory();
        let path = std::path::Path::new("");
        super::set_foreign_keys(&conn, false, path).expect("the pragma should be settable");
        assert_eq!(foreign_keys(&conn), 0);
        super::set_foreign_keys(&conn, true, path).expect("the pragma should be settable");
        assert_eq!(foreign_keys(&conn), 1);
    }

    #[test]
    fn a_database_past_the_target_is_refused_and_left_alone() {
        let mut conn = memory();
        apply(&mut conn, SYNTHETIC, 3).expect("three steps should run");
        let before = tables(&conn);

        let error = apply(&mut conn, SYNTHETIC, 2).expect_err("a newer database is refused");

        assert_eq!(
            error.kind,
            ProjectErrorKind::SchemaTooNew {
                found: 3,
                supported: 2
            }
        );
        assert_eq!(version(&conn), 3, "the version is untouched");
        assert_eq!(tables(&conn), before, "the schema is untouched");
    }

    #[test]
    fn a_hand_edited_version_is_refused_rather_than_guessed_at() {
        let conn = memory();
        conn.pragma_update(None, "user_version", -1i32)
            .expect("the version should be settable");
        let error = super::read_version(&conn, std::path::Path::new(""))
            .expect_err("a negative version is not one we wrote");
        assert_eq!(error.kind, ProjectErrorKind::NotASubloreProject);
    }

    #[test]
    fn a_target_no_migration_defines_is_refused() {
        let mut conn = memory();
        let error = apply(&mut conn, SYNTHETIC, 4).expect_err("version 4 does not exist");

        assert_eq!(error.kind, ProjectErrorKind::MigrationFailed);
        assert_eq!(version(&conn), 0, "nothing ran");
        assert!(tables(&conn).is_empty());
    }
}
