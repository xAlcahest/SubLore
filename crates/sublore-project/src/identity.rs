//! The number that tells one project from another. BACKLOG.md N33.
//!
//! **It lives inside the project's own database**, written once and read on every open. That is
//! what makes it survive a move: a project whose folder is given another name, carried to another
//! machine or restored from a backup is the same project to a module, because nothing about the key
//! is derived from where the file sits. A folder copied gives two projects one key, which is what
//! copying a project means.
//!
//! **Nothing is authorized by this number and nothing may ever be.** The host never compares two
//! keys and never decides anything with one: the database a module's statement runs on is resolved
//! from the open project on every statement. A collision costs a module a cache it drops on the
//! close edge it also receives, and costs the core nothing at all.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use crate::error::{ProjectError, ProjectErrorKind};

/// What `SubloreInvocation::project_key` carries when nothing is open, and therefore the one value
/// a key may never be.
pub const NO_PROJECT_KEY: i64 = 0;

/// Read this project's key, writing one first when the file has none.
///
/// One transaction, and IMMEDIATE for the reason `migrate.rs` gives: it takes the write lock up
/// front, so two processes opening one fresh project are serialized by the busy timeout instead of
/// racing between the read and the insert. A project written by a version 2 build therefore gains
/// its key the first time this build opens it, and never gains a second one.
pub fn ensure(conn: &mut Connection, path: &Path, now: SystemTime) -> Result<i64, ProjectError> {
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| failed(&error, path, "the identity transaction"))?;
    let held: Option<i64> = transaction
        .query_row(
            "SELECT project_key FROM project_identity WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| failed(&error, path, "reading the project key"))?;
    let key = match held {
        Some(key) => key,
        None => {
            let made = generate(now);
            transaction
                .execute(
                    "INSERT INTO project_identity (id, project_key) VALUES (1, ?1)",
                    [made],
                )
                .map_err(|error| failed(&error, path, "writing the project key"))?;
            made
        }
    };
    transaction
        .commit()
        .map_err(|error| failed(&error, path, "the identity transaction"))?;
    check(key, path)
}

/// A key read back out of a file is untrusted input like any other: a hand-edited zero would mean
/// "no project is open" everywhere it was carried, so it is refused rather than passed on.
fn check(key: i64, path: &Path) -> Result<i64, ProjectError> {
    if key == NO_PROJECT_KEY {
        return Err(ProjectError::new(
            ProjectErrorKind::NotASubloreProject,
            path,
            "the project carries a key of zero, which is not one any Sublore wrote",
        ));
    }
    Ok(key)
}

/// Sixty-four bits from the standard library's own randomly seeded hasher, mixed with the moment
/// the key is made. No new dependency, and no secret.
fn generate(now: SystemTime) -> i64 {
    let stamp = now
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default();
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(stamp);
    key_from(hasher.finish())
}

/// The key a raw draw becomes.
///
/// Zero is reserved, and it is mapped to one value rather than redrawn: a generator that can loop
/// is a generator that can hang, and this is total. The sign carries no meaning; the slot is
/// `int64_t` and only zero is read for anything.
fn key_from(raw: u64) -> i64 {
    match raw as i64 {
        NO_PROJECT_KEY => 1,
        drawn => drawn,
    }
}

fn failed(error: &rusqlite::Error, path: &Path, what: &str) -> ProjectError {
    ProjectError::new(
        ProjectErrorKind::QueryFailed,
        path,
        format!("{what} failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::{ensure, generate, key_from, NO_PROJECT_KEY};
    use crate::error::ProjectErrorKind;
    use crate::migrate::migrate;
    use rusqlite::Connection;
    use std::collections::HashSet;
    use std::path::Path;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn migrated() -> Connection {
        let mut conn = Connection::open_in_memory().expect("an in-memory database should open");
        migrate(&mut conn).expect("the schema should build");
        conn
    }

    #[test]
    fn a_generated_key_is_never_zero() {
        // Zero is what "no project is open" is spelled as, so a key of zero is a key that lies.
        assert_eq!(key_from(0), 1);
        for step in 0..2_000u64 {
            let at = UNIX_EPOCH + Duration::from_nanos(1_756_300_000_000_000_000 + step);
            assert_ne!(generate(at), NO_PROJECT_KEY, "draw {step}");
        }
    }

    #[test]
    fn two_projects_made_in_the_same_instant_still_get_different_keys() {
        // The clock is the mixed-in part and the hasher's seed is the rest, so two projects created
        // inside one tick are not two projects with one key.
        let at = UNIX_EPOCH + Duration::from_secs(1_756_300_000);
        let drawn: HashSet<i64> = (0..64).map(|_| generate(at)).collect();
        assert!(
            drawn.len() > 1,
            "the seed moves even when the clock does not"
        );
    }

    #[test]
    fn a_key_is_written_once_and_read_back_on_every_open() {
        let mut conn = migrated();
        let path = Path::new("");
        let first = ensure(&mut conn, path, SystemTime::now()).expect("a key should be written");
        assert_ne!(first, NO_PROJECT_KEY);
        // Later, and with a different instant: the row already there is the answer.
        let again = ensure(&mut conn, path, SystemTime::now()).expect("the key should be read");
        assert_eq!(again, first, "a project keeps the key it was given");
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM project_identity", [], |row| {
                row.get(0)
            })
            .expect("the identity table should be readable");
        assert_eq!(rows, 1, "one project, one row");
    }

    #[test]
    fn a_zero_written_into_the_file_by_hand_is_refused_rather_than_carried() {
        let mut conn = migrated();
        conn.execute(
            "INSERT INTO project_identity (id, project_key) VALUES (1, 0)",
            [],
        )
        .expect("the row should be insertable");
        let error = ensure(&mut conn, Path::new(""), SystemTime::now())
            .expect_err("zero is not a key any build wrote");
        assert_eq!(error.kind, ProjectErrorKind::NotASubloreProject);
    }
}
