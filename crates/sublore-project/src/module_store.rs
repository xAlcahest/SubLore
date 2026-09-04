//! Running a module's own statements on the host's connection, with the guard on for each one.
//!
//! The module never opens the database. `rusqlite` is built `bundled`, so a module that opened its
//! own would put a second copy of SQLite in the process, and two copies do not share the file
//! locking bookkeeping that makes POSIX advisory locks survive a `close()`. The failure mode is a
//! lock silently dropped and one project file written by two writers that each believed they were
//! alone, which is the database loss CONTRIBUTING.md §3.4 forbids. See `module-abi.md` §4.7.
//!
//! **The guard is installed here rather than at the call site**, so the pair cannot come apart: a
//! caller that forgot to take it off would hold the core's own statements to a module's rules.

use rusqlite::types::{Value, ValueRef};
use rusqlite::{Connection, Error as SqlError};

use crate::error::{ProjectError, ProjectErrorKind};
use crate::module_guard::{clear, guard};
use crate::records::Project;

/// One value in or out of a module's statement. The wire shape of `SubloreValue`, owned.
///
/// Owned rather than borrowed because a row is handed to the caller after the statement has moved
/// on: `rusqlite` invalidates a `ValueRef` at the next step, and a module that read one afterwards
/// would be reading whatever the cursor is on now.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Cell {
    fn read(value: ValueRef<'_>) -> Self {
        match value {
            ValueRef::Null => Self::Null,
            ValueRef::Integer(number) => Self::Int(number),
            ValueRef::Real(number) => Self::Real(number),
            ValueRef::Text(bytes) => Self::Text(String::from_utf8_lossy(bytes).into_owned()),
            ValueRef::Blob(bytes) => Self::Blob(bytes.to_vec()),
        }
    }

    fn bound(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Int(number) => Value::Integer(*number),
            Self::Real(number) => Value::Real(*number),
            Self::Text(text) => Value::Text(text.clone()),
            Self::Blob(bytes) => Value::Blob(bytes.clone()),
        }
    }
}

/// Why a module's statement did not run. Narrower than `ProjectError` on purpose: these are the
/// three a module can tell apart, and the interface has a code for each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreRefusal {
    /// The id is not one a module may declare, or the guard refused the statement.
    Denied,
    /// More than one statement in the text, which is refused rather than run.
    MoreThanOneStatement,
    /// The statement failed on its own terms. `detail` is for the log, never for the user.
    Failed(String),
}

/// Install the guard, run `work`, take the guard off whatever happened.
///
/// The pair is here and nowhere else. `work` returning early or failing cannot skip the second
/// half, which is what keeps the core's own statements out of a module's rules.
fn guarded<T>(
    conn: &Connection,
    id: &str,
    work: impl FnOnce(&Connection) -> Result<T, StoreRefusal>,
) -> Result<T, StoreRefusal> {
    guard(conn, id).map_err(|_| StoreRefusal::Denied)?;
    let answer = work(conn);
    // A guard that will not come off is worse than the statement failing: every later core
    // statement would be held to this module's rules, so it is said out loud and not swallowed.
    if let Err(error) = clear(conn) {
        return Err(StoreRefusal::Failed(format!(
            "the module guard would not come off: {error}"
        )));
    }
    answer
}

/// What a refusal from SQLite means to a module.
fn refusal_of(error: SqlError) -> StoreRefusal {
    match error {
        // rusqlite's own `prepare` detects a second statement and refuses it, measured by reading
        // `rusqlite-0.40.2/src/lib.rs:792-798` on 2026-09-04. A trailing semicolon with only
        // whitespace or a comment after it is not one, which is the behaviour §4.7 wanted.
        SqlError::MultipleStatement => StoreRefusal::MoreThanOneStatement,
        // The authorizer's refusal arrives as an ordinary SQLite error, so a module cannot tell a
        // denied table from a missing one, and that is deliberate: it learns what it may reach by
        // being told no, never by being told what is there.
        SqlError::SqliteFailure(inner, _)
            if inner.code == rusqlite::ErrorCode::AuthorizationForStatementDenied =>
        {
            StoreRefusal::Denied
        }
        other => StoreRefusal::Failed(other.to_string()),
    }
}

/// Run one statement for module `id`, pushing each row to `on_row` until it says to stop.
///
/// Returns how many rows were pushed. `on_row` returning false ends the walk, which is what lets a
/// module that wants the first row pay for the first row.
pub fn run(
    project: &Project,
    id: &str,
    sql: &str,
    params: &[Cell],
    on_row: &mut dyn FnMut(&[Cell]) -> bool,
) -> Result<usize, StoreRefusal> {
    guarded(project.connection(), id, |conn| {
        let mut statement = conn.prepare(sql).map_err(refusal_of)?;
        let bound: Vec<Value> = params.iter().map(Cell::bound).collect();
        let columns = statement.column_count();
        let mut rows = statement
            .query(rusqlite::params_from_iter(bound))
            .map_err(refusal_of)?;

        let mut pushed = 0usize;
        loop {
            let Some(row) = rows.next().map_err(refusal_of)? else {
                break;
            };
            let mut cells = Vec::with_capacity(columns);
            for index in 0..columns {
                cells.push(Cell::read(row.get_ref(index).map_err(refusal_of)?));
            }
            pushed += 1;
            if !on_row(&cells) {
                break;
            }
        }
        Ok(pushed)
    })
}

/// Run `work` inside one transaction, committing on `Ok` and rolling back on anything else.
///
/// IMMEDIATE, so the write lock is taken up front rather than halfway through, which is what
/// `migrate::run` already does and what stops a module leaving a transaction open by returning
/// early: there is no way out of here that does not commit or roll back.
pub fn transaction<T>(
    project: &mut Project,
    id: &str,
    work: impl FnOnce(&Connection) -> Result<T, StoreRefusal>,
) -> Result<T, StoreRefusal> {
    let conn = project.connection_mut();
    conn.execute_batch("BEGIN IMMEDIATE").map_err(refusal_of)?;
    // The guard goes on inside the transaction and comes off before it is closed, because BEGIN
    // and COMMIT are the core's own statements and a module's rules have nothing to say about them.
    let answer = guarded(conn, id, work);
    let closing = if answer.is_ok() {
        conn.execute_batch("COMMIT")
    } else {
        conn.execute_batch("ROLLBACK")
    };
    match (answer, closing) {
        (Ok(value), Ok(())) => Ok(value),
        // The work succeeded and the commit did not, so nothing was written and the module has to
        // be told: reporting success here would be reporting a write that is not there.
        (Ok(_), Err(error)) => Err(refusal_of(error)),
        (Err(refusal), _) => Err(refusal),
    }
}

/// The version a module's own tables are at, from the ladder the core owns (§6.2).
///
/// Read by the core rather than by the module, and written by the core in the module's own
/// transaction, because the row a module may touch is not something an authorizer can see: a
/// `WHERE` is not among its arguments.
pub fn schema_version(project: &Project, id: &str) -> Result<Option<u32>, ProjectError> {
    project
        .connection()
        .query_row(
            "SELECT version FROM module_schema WHERE module_id = ?1",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .map(|version| Some(u32::try_from(version).unwrap_or(u32::MAX)))
        .or_else(|error| match error {
            SqlError::QueryReturnedNoRows => Ok(None),
            other => Err(ProjectError::new(
                ProjectErrorKind::MigrationFailed,
                std::path::Path::new(""),
                format!("the module ladder could not be read: {other}"),
            )),
        })
}
