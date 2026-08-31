//! Behavioural tests for M4.1, written from the acceptance criteria in BACKLOG.md:
//!
//! - creating a project produces a database at the chosen path with the current schema version;
//! - a database written at version N migrates forward with its schema and every row intact;
//! - a database from a newer version is refused with a readable error and never altered;
//! - a migration that fails leaves the database exactly at the version it started from.
//!
//! Real files in a scratch directory, in the style of `crates/sublore-io/tests/atomic_save.rs`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::types::ValueRef;
use rusqlite::Connection;

use sublore_project::error::ProjectErrorKind;
use sublore_project::layout::{APPLICATION_ID, CURRENT_VERSION, DATABASE_NAME};
use sublore_project::migrate::{migrate, migrate_to};
use sublore_project::Database;

/// A fixed instant, so `created_at` is an assertion and not a moving target.
const MADE_AT: u64 = 1_756_300_000;

/// Every column of every version 1 table, in the order the schema declares them. A later version
/// may add columns; these are the ones that must survive a migration untouched.
const ROW_QUERIES: [(&str, &str); 3] = [
    (
        "series",
        "SELECT id, title, created_at FROM series ORDER BY id",
    ),
    (
        "episodes",
        "SELECT id, series_id, ordinal, title, created_at FROM episodes ORDER BY id",
    ),
    (
        "episode_files",
        "SELECT id, episode_id, role, path, byte_length, modified_at, added_at \
         FROM episode_files ORDER BY id",
    ),
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A directory of this test's own, under the OS temp dir. Removed at the end of the test.
fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!(
        "sublore-project-test-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

fn made_at() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(MADE_AT)
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/projects")
        .join(name);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
    // .gitattributes pins these to LF; normalise anyway so a stray CRLF cannot fake a failure.
    text.replace("\r\n", "\n")
}

fn open_raw(database: &Path) -> Connection {
    Connection::open(database)
        .unwrap_or_else(|error| panic!("{} should open: {error}", database.display()))
}

fn close_raw(conn: Connection) {
    conn.close().expect("the connection should close cleanly");
}

fn user_version(conn: &Connection) -> u32 {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("user_version should be readable")
}

fn application_id(conn: &Connection) -> i32 {
    conn.pragma_query_value(None, "application_id", |row| row.get(0))
        .expect("application_id should be readable")
}

/// The normalised schema: one `sqlite_master` row per line, internal whitespace collapsed. Stable
/// across SQLite versions because the `sql` column is the exact text the migration wrote.
fn schema_dump(conn: &Connection) -> String {
    let mut stmt = conn
        .prepare(
            "SELECT type, name, tbl_name, sql FROM sqlite_master \
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .expect("the schema query should prepare");
    let mut dump = String::new();
    let mut rows = stmt.query([]).expect("the schema query should run");
    while let Some(row) = rows.next().expect("a schema row should be readable") {
        let kind: String = row.get(0).expect("type");
        let name: String = row.get(1).expect("name");
        let table: String = row.get(2).expect("tbl_name");
        let sql: Option<String> = row.get(3).expect("sql");
        let sql = sql.unwrap_or_default();
        let collapsed = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        dump.push_str(&format!("{kind}|{name}|{table}|{collapsed}\n"));
    }
    dump
}

/// Every row of every version 1 table, as text, ordered by table then primary key.
fn row_dump(conn: &Connection) -> String {
    let mut dump = String::new();
    for (table, sql) in ROW_QUERIES {
        let mut stmt = conn
            .prepare(sql)
            .unwrap_or_else(|error| panic!("the {table} query should prepare: {error}"));
        let columns = stmt.column_count();
        let mut rows = stmt
            .query([])
            .unwrap_or_else(|error| panic!("the {table} query should run: {error}"));
        while let Some(row) = rows.next().expect("a row should be readable") {
            let mut line = String::from(table);
            for column in 0..columns {
                line.push('|');
                line.push_str(&render(
                    row.get_ref(column).expect("a value should be readable"),
                ));
            }
            dump.push_str(&line);
            dump.push('\n');
        }
    }
    dump
}

fn render(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "<null>".to_string(),
        ValueRef::Integer(number) => number.to_string(),
        ValueRef::Real(number) => format!("{number:?}"),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Blob(bytes) => format!("blob:{}", bytes.len()),
    }
}

fn table_names(conn: &Connection) -> Vec<String> {
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

/// Every name in `dir`, sorted, so a leftover shows up in the assertion message.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", dir.display()))
        .map(|entry| {
            entry
                .expect("a directory entry should be readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn read_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()))
}

fn clean_up(root: &Path) {
    fs::remove_dir_all(root).ok();
}

// ---------------------------------------------------------------------------
// Creating a project
// ---------------------------------------------------------------------------

#[test]
fn creating_a_project_writes_a_database_at_the_current_schema_version() {
    let folder = scratch("create");
    let database = folder.join(DATABASE_NAME);

    let project = Database::create(&folder, "Series One", made_at())
        .expect("a fresh folder should take a project");

    assert_eq!(project.version(), CURRENT_VERSION);
    assert_eq!(project.database_path(), database.as_path());
    assert_eq!(project.folder(), folder.as_path());
    assert!(database.is_file(), "the database is a file in the folder");
    assert_eq!(application_id(project.conn()), APPLICATION_ID);
    assert_eq!(user_version(project.conn()), CURRENT_VERSION);
    let journal: String = project
        .conn()
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("journal_mode should be readable");
    assert_eq!(
        journal, "wal",
        "WAL is best effort, but a local filesystem must get it"
    );

    let (title, created_at): (String, i64) = project
        .conn()
        .query_row("SELECT title, created_at FROM series", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("the series row should be there");
    assert_eq!(title, "Series One");
    assert_eq!(created_at, MADE_AT as i64, "the caller's instant is stored");

    assert_eq!(
        schema_dump(project.conn()),
        fixture(&format!("schema-v{CURRENT_VERSION}.sql")),
        "a new project carries the current schema, exactly"
    );

    project.close().expect("the project should close cleanly");
    assert_eq!(
        names_in(&folder),
        vec![DATABASE_NAME.to_string()],
        "a clean close leaves one file in the user's folder"
    );

    clean_up(&folder);
}

#[test]
fn a_project_reopens_with_everything_where_it_was_left() {
    let folder = scratch("reopen");
    Database::create(&folder, "Series One", made_at())
        .expect("a fresh folder should take a project")
        .close()
        .expect("the project should close cleanly");

    let project = Database::open(&folder).expect("the project should reopen");
    assert_eq!(project.version(), CURRENT_VERSION);
    let title: String = project
        .conn()
        .query_row("SELECT title FROM series", [], |row| row.get(0))
        .expect("the series row should still be there");
    assert_eq!(title, "Series One");
    project.close().expect("the project should close cleanly");

    clean_up(&folder);
}

#[test]
fn a_second_create_is_refused_and_the_first_project_is_untouched() {
    let folder = scratch("already");
    Database::create(&folder, "Series One", made_at())
        .expect("a fresh folder should take a project")
        .close()
        .expect("the project should close cleanly");
    let database = folder.join(DATABASE_NAME);
    let before = read_bytes(&database);

    let error = Database::create(&folder, "Something Else", made_at())
        .expect_err("a folder that already holds a project is refused");

    assert_eq!(error.kind, ProjectErrorKind::AlreadyAProject);
    assert_eq!(read_bytes(&database), before, "the first project is intact");

    clean_up(&folder);
}

// ---------------------------------------------------------------------------
// Old database -> migrate -> verify (CONTRIBUTING.md §2)
// ---------------------------------------------------------------------------

/// With CURRENT_VERSION at 1 the migrate half of this round trip is a no-op, and the test says so
/// rather than pretending otherwise: what it pins today is the schema and the rows. Adding version
/// 2 makes it a real migration with no rewrite.
#[test]
fn a_version_1_database_keeps_its_schema_and_every_row_through_migration() {
    let folder = scratch("roundtrip");
    let database = folder.join(DATABASE_NAME);

    // A database written at version 1, by the same runner that wrote every user's version 1.
    let mut conn = open_raw(&database);
    assert_eq!(
        migrate_to(&mut conn, 1).expect("a fresh file should migrate to version 1"),
        1
    );
    conn.execute_batch(&fixture("seed-v1.sql"))
        .expect("the version 1 seed rows should load");
    let rows_before = row_dump(&conn);
    assert_eq!(
        rows_before.lines().count(),
        10,
        "the seed plants one series, three episodes and six files"
    );
    close_raw(conn);

    // Migrate the file on disk, the way opening an older project does.
    let mut conn = open_raw(&database);
    let reached = migrate(&mut conn).expect("a version 1 database should migrate forward");

    assert_eq!(reached, CURRENT_VERSION);
    assert_eq!(user_version(&conn), CURRENT_VERSION);
    assert_eq!(application_id(&conn), APPLICATION_ID);
    assert_eq!(
        schema_dump(&conn),
        fixture(&format!("schema-v{CURRENT_VERSION}.sql")),
        "the migrated schema is the current one, exactly"
    );
    assert_eq!(
        row_dump(&conn),
        rows_before,
        "every row survives, value for value"
    );
    close_raw(conn);

    clean_up(&folder);
}

#[test]
fn a_migrated_database_opens_as_a_project() {
    let folder = scratch("roundtrip-open");
    let database = folder.join(DATABASE_NAME);

    let mut conn = open_raw(&database);
    migrate_to(&mut conn, 1).expect("a fresh file should migrate to version 1");
    conn.execute_batch(&fixture("seed-v1.sql"))
        .expect("the version 1 seed rows should load");
    close_raw(conn);

    let project = Database::open(&folder).expect("a version 1 database should open");
    assert_eq!(project.version(), CURRENT_VERSION);
    let episodes: i64 = project
        .conn()
        .query_row("SELECT COUNT(*) FROM episodes", [], |row| row.get(0))
        .expect("the episode count should be readable");
    assert_eq!(episodes, 3, "the seeded episodes are still there");
    project.close().expect("the project should close cleanly");

    clean_up(&folder);
}

// ---------------------------------------------------------------------------
// A database from the future
// ---------------------------------------------------------------------------

#[test]
fn a_database_from_a_newer_version_is_refused_and_never_altered() {
    let folder = scratch("too-new");
    let database = folder.join(DATABASE_NAME);
    Database::create(&folder, "Series One", made_at())
        .expect("a fresh folder should take a project")
        .close()
        .expect("the project should close cleanly");

    let future = CURRENT_VERSION + 1;
    let conn = open_raw(&database);
    conn.pragma_update(None, "user_version", future)
        .expect("the version should be settable");
    close_raw(conn);

    let before = read_bytes(&database);
    let before_names = names_in(&folder);

    let error = Database::open(&folder).expect_err("a newer project is refused");

    assert_eq!(
        error.kind,
        ProjectErrorKind::SchemaTooNew {
            found: future,
            supported: CURRENT_VERSION
        },
        "the error carries both numbers, so the UI can name them"
    );
    assert_eq!(read_bytes(&database), before, "not one byte is written");
    assert_eq!(
        names_in(&folder),
        before_names,
        "nothing is added beside it"
    );

    clean_up(&folder);
}

#[test]
fn the_runner_is_not_a_second_way_into_a_newer_database() {
    let folder = scratch("too-new-runner");
    let database = folder.join(DATABASE_NAME);
    let mut conn = open_raw(&database);
    migrate_to(&mut conn, CURRENT_VERSION).expect("a fresh file should migrate");
    let future = CURRENT_VERSION + 1;
    conn.pragma_update(None, "user_version", future)
        .expect("the version should be settable");
    let schema_before = schema_dump(&conn);

    let error = migrate(&mut conn).expect_err("the runner refuses a database past its target");

    assert_eq!(
        error.kind,
        ProjectErrorKind::SchemaTooNew {
            found: future,
            supported: CURRENT_VERSION
        }
    );
    assert_eq!(user_version(&conn), future, "the version is left alone");
    assert_eq!(
        schema_dump(&conn),
        schema_before,
        "the schema is left alone"
    );
    close_raw(conn);

    clean_up(&folder);
}

// ---------------------------------------------------------------------------
// A migration that fails
// ---------------------------------------------------------------------------

#[test]
fn a_failed_migration_leaves_the_database_at_the_version_it_started_from() {
    let folder = scratch("failed-migration");
    let database = folder.join(DATABASE_NAME);
    let mut conn = open_raw(&database);
    // A table named like the second one migration 1 creates: the step gets halfway and stops.
    conn.execute_batch(
        "CREATE TABLE episodes (kept TEXT NOT NULL); \
         INSERT INTO episodes (kept) VALUES ('the user''s own row');",
    )
    .expect("the standing table should be creatable");

    let error = migrate(&mut conn).expect_err("the step cannot create a table that is there");

    assert_eq!(error.kind, ProjectErrorKind::MigrationFailed);
    assert!(
        !error.detail.is_empty(),
        "the SQLite message is kept for logs"
    );
    assert_eq!(user_version(&conn), 0, "a failed step commits no version");
    assert_eq!(application_id(&conn), 0, "and no application id");
    assert_eq!(
        table_names(&conn),
        vec!["episodes".to_string()],
        "the table the step created before it failed is rolled back with it"
    );
    let kept: String = conn
        .query_row("SELECT kept FROM episodes", [], |row| row.get(0))
        .expect("the standing row should be readable");
    assert_eq!(kept, "the user's own row");
    close_raw(conn);

    clean_up(&folder);
}

// ---------------------------------------------------------------------------
// Files that are not a Sublore project
// ---------------------------------------------------------------------------

#[test]
fn opening_something_that_is_not_a_sublore_project_is_refused_and_writes_nothing() {
    let root = scratch("not-a-project");

    let empty = root.join("empty");
    fs::create_dir_all(&empty).expect("the folder should be creatable");
    assert_eq!(
        Database::open(&empty)
            .expect_err("an empty folder holds no project")
            .kind,
        ProjectErrorKind::NoProjectHere
    );

    assert_eq!(
        Database::open(&root.join("nowhere"))
            .expect_err("a folder that is not there cannot be opened")
            .kind,
        ProjectErrorKind::FolderNotFound
    );

    let cases: [(&str, &[u8]); 2] = [
        ("zero-byte", b""),
        (
            "text",
            b"1\n00:00:01,000 --> 00:00:02,000\nnot a database\n",
        ),
    ];
    for (tag, bytes) in cases {
        let folder = root.join(tag);
        fs::create_dir_all(&folder).expect("the folder should be creatable");
        let database = folder.join(DATABASE_NAME);
        fs::write(&database, bytes).expect("the file should be writable");
        let error = Database::open(&folder).expect_err("this file is not a Sublore project");
        assert_eq!(error.kind, ProjectErrorKind::NotASubloreProject, "{tag}");
        assert_eq!(read_bytes(&database), bytes, "{tag} is left untouched");
    }

    // Another application's SQLite database: same file format, a different owner.
    let foreign = root.join("foreign");
    fs::create_dir_all(&foreign).expect("the folder should be creatable");
    let database = foreign.join(DATABASE_NAME);
    let conn = open_raw(&database);
    conn.pragma_update(None, "application_id", 0x4F_54_48_52_i32)
        .expect("the application id should be settable");
    conn.execute_batch("CREATE TABLE notes (body TEXT)")
        .expect("the foreign schema should be creatable");
    close_raw(conn);
    let before = read_bytes(&database);

    let error = Database::open(&foreign).expect_err("another application's database is refused");
    assert_eq!(error.kind, ProjectErrorKind::NotASubloreProject);
    assert_eq!(read_bytes(&database), before, "it is left untouched");

    clean_up(&root);
}
