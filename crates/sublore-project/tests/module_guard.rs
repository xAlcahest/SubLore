//! What a module may and may not do to a real project database, per `module-abi.md` §4.7.
//!
//! Every check runs against a migrated file rather than a bare connection: the tables a module must
//! not reach have to exist for a refusal to mean anything, and `user_version` has to be the core's
//! before a module tries to write it.
//!
//! The last check is the one that says the rest are real. It removes the guard and shows the pragma
//! succeeding, which is the shape rusqlite's own tests use and the only way to tell a refusal from
//! a statement that was never going to work.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use sublore_project::layout::CURRENT_VERSION;
use sublore_project::migrate::migrate;
use sublore_project::module_guard::{clear, guard};

/// The id the checks below install the guard for.
const MODULE: &str = "fixture";

/// A directory of this test's own. Same shape as the other suites here: no dev-dependency.
fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!(
        "sublore-guard-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// A migrated project database, at the version this build writes.
fn project(tag: &str) -> (PathBuf, Connection) {
    let dir = scratch(tag);
    let path = dir.join("project.sublore");
    let mut conn = Connection::open(&path).expect("the database should open");
    let reached = migrate(&mut conn).expect("a fresh file should migrate");
    assert_eq!(reached, CURRENT_VERSION);
    (dir, conn)
}

fn user_version(conn: &Connection) -> u32 {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("user_version should be readable")
}

#[test]
fn a_module_may_make_and_use_its_own_tables() {
    let (dir, conn) = project("own");
    guard(&conn, MODULE).expect("the guard should install");

    conn.execute_batch("CREATE TABLE m_fixture_notes (id INTEGER PRIMARY KEY, note TEXT NOT NULL)")
        .expect("a module may create its own table");
    conn.execute(
        "INSERT INTO m_fixture_notes (id, note) VALUES (?1, ?2)",
        rusqlite::params![1, "kept"],
    )
    .expect("a module may write its own table");
    let note: String = conn
        .query_row("SELECT note FROM m_fixture_notes WHERE id = 1", [], |row| {
            row.get(0)
        })
        .expect("a module may read its own table");
    assert_eq!(note, "kept");

    clear(&conn).expect("the guard should come off");
    drop(conn);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_cannot_write_the_version_the_core_owns() {
    let (dir, conn) = project("pragma");
    let before = user_version(&conn);
    assert_eq!(before, CURRENT_VERSION);

    guard(&conn, MODULE).expect("the guard should install");
    let refused = conn.execute_batch("PRAGMA user_version = 99");
    assert!(
        refused.is_err(),
        "a module wrote the core's own version: a free core would refuse this project for ever"
    );
    clear(&conn).expect("the guard should come off");
    assert_eq!(user_version(&conn), before, "and it is unchanged");

    // The other half of the claim. Without the guard the same statement works, which is what says
    // the refusal above was this authorizer and not SQLite refusing something else.
    conn.execute_batch("PRAGMA user_version = 99")
        .expect("with no guard the pragma is an ordinary statement");
    assert_eq!(user_version(&conn), 99);

    drop(conn);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_cannot_write_the_schema_table_directly() {
    // The bookkeeping a schema change emits on `sqlite_master` is permitted, so this is the check
    // that says permitting it costs nothing: SQLite refuses a direct write unless `writable_schema`
    // is on, and turning that on is a pragma, which the guard denies. Two refusals, and neither is
    // the other's.
    let (dir, conn) = project("schematable");
    guard(&conn, MODULE).expect("the guard should install");

    assert!(
        conn.execute_batch("PRAGMA writable_schema = ON").is_err(),
        "a module turned on the switch that makes the schema table writable"
    );
    assert!(
        conn.execute_batch(
            "INSERT INTO sqlite_master (type, name, tbl_name, rootpage, sql) \
             VALUES ('table', 'planted', 'planted', 2, 'CREATE TABLE planted (x)')"
        )
        .is_err(),
        "a module wrote a row into the schema table"
    );

    clear(&conn).expect("the guard should come off");
    drop(conn);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_may_index_its_own_table_and_not_the_core_s() {
    let (dir, conn) = project("index");
    guard(&conn, MODULE).expect("the guard should install");

    conn.execute_batch("CREATE TABLE m_fixture_notes (id INTEGER PRIMARY KEY, note TEXT NOT NULL)")
        .expect("its own table");
    conn.execute_batch("CREATE INDEX m_fixture_notes_by_note ON m_fixture_notes (note)")
        .expect("an index named after itself, on its own table");
    assert!(
        conn.execute_batch("CREATE INDEX plain_name ON m_fixture_notes (id)")
            .is_err(),
        "an index outside the module's own names was created"
    );
    // `episodes` rather than `series`, measured: a table with no index of its own emits no action
    // at all, so `REINDEX series` succeeds by doing nothing and would have proved nothing here.
    assert!(
        conn.execute_batch("REINDEX episodes").is_err(),
        "a module rebuilt an index belonging to the core"
    );
    assert!(
        conn.execute_batch("REINDEX").is_err(),
        "a module rebuilt every index in the user's project"
    );

    clear(&conn).expect("the guard should come off");
    drop(conn);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_cannot_read_the_core_tables() {
    let (dir, conn) = project("core");
    guard(&conn, MODULE).expect("the guard should install");

    assert!(
        conn.prepare("SELECT id, title FROM episodes").is_err(),
        "a module read the user's own episode list"
    );
    assert!(
        conn.prepare("SELECT path FROM episode_files").is_err(),
        "a module read the paths of the user's own files"
    );
    assert!(
        conn.execute_batch("DROP TABLE series").is_err(),
        "a module dropped a core table"
    );

    clear(&conn).expect("the guard should come off");
    drop(conn);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_cannot_reach_another_module_s_tables() {
    let (dir, conn) = project("other");
    // Made by the core, before the guard, so the refusal below is about ownership rather than about
    // the table not existing.
    conn.execute_batch("CREATE TABLE m_other_notes (id INTEGER PRIMARY KEY)")
        .expect("the core may create anything");
    // The trap the prefix rule exists for: `fixtures` is a different module from `fixture`.
    conn.execute_batch("CREATE TABLE m_fixtures_notes (id INTEGER PRIMARY KEY)")
        .expect("the core may create anything");

    guard(&conn, MODULE).expect("the guard should install");
    assert!(
        conn.prepare("SELECT id FROM m_other_notes").is_err(),
        "a module read a table belonging to another"
    );
    assert!(
        conn.prepare("SELECT id FROM m_fixtures_notes").is_err(),
        "the prefix stopped at the id and not at the underscore after it"
    );
    assert!(
        conn.execute_batch("CREATE TABLE m_other_more (id INTEGER PRIMARY KEY)")
            .is_err(),
        "a module created a table under another module's name"
    );

    clear(&conn).expect("the guard should come off");
    drop(conn);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_cannot_reach_a_second_file() {
    let (dir, conn) = project("attach");
    let elsewhere = dir.join("elsewhere.sqlite");
    guard(&conn, MODULE).expect("the guard should install");

    let refused = conn.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS other",
        elsewhere.display()
    ));
    assert!(refused.is_err(), "a module attached a database of its own");
    assert!(
        !elsewhere.exists(),
        "and the refusal happened before a file was made"
    );

    clear(&conn).expect("the guard should come off");
    drop(conn);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_may_count_its_own_ladder_and_nothing_else_in_that_table() {
    let (dir, conn) = project("ladder");
    guard(&conn, MODULE).expect("the guard should install");

    // The core owns which row a module may touch, because a WHERE is not among the authorizer's
    // arguments: this only says the table itself is reachable, which section 6.2 requires.
    assert!(
        conn.prepare("SELECT version FROM module_schema WHERE module_id = 'fixture'")
            .is_ok(),
        "a module could not read the counter its own ladder lives on"
    );
    assert!(
        conn.execute_batch("DROP TABLE module_schema").is_err(),
        "a module dropped the counter table the core owns"
    );

    clear(&conn).expect("the guard should come off");
    drop(conn);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_id_that_is_not_one_is_refused_before_it_becomes_a_table_name() {
    let (dir, conn) = project("badid");
    // The id is pasted into a prefix, so the only safe answer to a value that is not already safe
    // is to refuse it rather than to escape it.
    assert!(guard(&conn, "Fixture").is_err());
    assert!(guard(&conn, "fix ture").is_err());
    assert!(guard(&conn, "").is_err());
    assert!(guard(&conn, "fixture'; DROP TABLE series; --").is_err());

    // And a refused install leaves no guard behind: the core's own statements still work.
    conn.prepare("SELECT id FROM episodes")
        .expect("the core reads its own tables");

    drop(conn);
    fs::remove_dir_all(&dir).ok();
}
