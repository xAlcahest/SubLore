//! What a module's own statements may do to a real project, per `module-abi.md` §4.7 and H6.
//!
//! Every check runs against a project the test made, because the tables a module must not reach
//! have to exist for a refusal to mean anything. The last one is what says the rest are real: it
//! shows the core's own statement working after each call, which is the guard coming off.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sublore_project::module_store::{run, schema_version, transaction, Cell, StoreRefusal};
use sublore_project::records::{episodes, Project};

/// The id every check below runs as.
const MODULE: &str = "fixture";

/// A directory of this test's own. The same shape as the other suites here: no dev-dependency.
fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!(
        "sublore-store-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

fn project(tag: &str) -> (PathBuf, Project) {
    let dir = scratch(tag);
    let folder = dir.join("Series");
    fs::create_dir_all(&folder).expect("the project folder should be creatable");
    let project =
        Project::create(&folder, "A series", SystemTime::now()).expect("a project should be made");
    (dir, project)
}

/// Nothing collected: most checks here are about whether a statement runs at all.
fn nowhere(_: &[Cell]) -> bool {
    true
}

#[test]
fn a_module_makes_its_own_table_and_reads_back_what_it_wrote() {
    let (dir, project) = project("own");

    run(
        &project,
        MODULE,
        "CREATE TABLE m_fixture_notes (id INTEGER PRIMARY KEY, note TEXT NOT NULL)",
        &[],
        &mut nowhere,
    )
    .expect("a module may create its own table");
    run(
        &project,
        MODULE,
        "INSERT INTO m_fixture_notes (id, note) VALUES (?1, ?2)",
        &[Cell::Int(1), Cell::Text("kept".into())],
        &mut nowhere,
    )
    .expect("a module may write its own table");

    let mut read: Vec<Cell> = Vec::new();
    let pushed = run(
        &project,
        MODULE,
        "SELECT note FROM m_fixture_notes WHERE id = ?1",
        &[Cell::Int(1)],
        &mut |cells| {
            read.extend_from_slice(cells);
            true
        },
    )
    .expect("a module may read its own table");
    assert_eq!(pushed, 1);
    assert_eq!(read, vec![Cell::Text("kept".into())]);

    // The guard came off: the core reads its own tables after the module's call.
    episodes(&project).expect("the core reads its own tables afterwards");
    drop(project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_cannot_write_the_version_the_core_owns() {
    let (dir, project) = project("pragma");
    let refused = run(
        &project,
        MODULE,
        "PRAGMA user_version = 99",
        &[],
        &mut nowhere,
    );
    assert_eq!(refused, Err(StoreRefusal::Denied));

    // And the core still opens it, which is what the denial is for: a project whose header a
    // module had bumped would be refused by the next free core for ever.
    episodes(&project).expect("the core reads its own tables afterwards");
    drop(project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_cannot_read_the_user_s_own_records() {
    let (dir, project) = project("core");
    assert_eq!(
        run(
            &project,
            MODULE,
            "SELECT id, title FROM episodes",
            &[],
            &mut nowhere
        ),
        Err(StoreRefusal::Denied)
    );
    assert_eq!(
        run(
            &project,
            MODULE,
            "SELECT path FROM episode_files",
            &[],
            &mut nowhere
        ),
        Err(StoreRefusal::Denied)
    );
    episodes(&project).expect("the core reads its own tables afterwards");
    drop(project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_second_statement_behind_a_semicolon_is_refused_and_neither_one_runs() {
    let (dir, project) = project("two");
    let refused = run(
        &project,
        MODULE,
        "CREATE TABLE m_fixture_a (x INTEGER); CREATE TABLE m_fixture_b (x INTEGER)",
        &[],
        &mut nowhere,
    );
    assert_eq!(refused, Err(StoreRefusal::MoreThanOneStatement));

    // Neither, not just the second: nothing is smuggled behind a semicolon and nothing in front of
    // one runs on its way to being refused.
    for table in ["m_fixture_a", "m_fixture_b"] {
        let asked = run(
            &project,
            MODULE,
            &format!("SELECT count(*) FROM {table}"),
            &[],
            &mut nowhere,
        );
        assert!(
            matches!(asked, Err(StoreRefusal::Failed(_))),
            "{table} exists, so the first statement ran: {asked:?}"
        );
    }

    // A trailing semicolon with nothing after it is one statement, not two.
    run(
        &project,
        MODULE,
        "CREATE TABLE m_fixture_notes (x INTEGER);",
        &[],
        &mut nowhere,
    )
    .expect("a statement that merely ends in a semicolon is one statement");

    drop(project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_sink_that_stops_the_walk_is_pushed_once() {
    let (dir, project) = project("stop");
    run(
        &project,
        MODULE,
        "CREATE TABLE m_fixture_notes (id INTEGER PRIMARY KEY)",
        &[],
        &mut nowhere,
    )
    .expect("its own table");
    for id in 1..=3 {
        run(
            &project,
            MODULE,
            "INSERT INTO m_fixture_notes (id) VALUES (?1)",
            &[Cell::Int(id)],
            &mut nowhere,
        )
        .expect("its own rows");
    }

    let mut seen = 0usize;
    let pushed = run(
        &project,
        MODULE,
        "SELECT id FROM m_fixture_notes ORDER BY id",
        &[],
        &mut |_| {
            seen += 1;
            false
        },
    )
    .expect("its own table");
    assert_eq!(
        (pushed, seen),
        (1, 1),
        "a caller that wants one row pays for one"
    );

    drop(project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_transaction_that_fails_leaves_the_table_as_it_was() {
    let (dir, mut project) = project("rollback");
    run(
        &project,
        MODULE,
        "CREATE TABLE m_fixture_notes (id INTEGER PRIMARY KEY)",
        &[],
        &mut nowhere,
    )
    .expect("its own table");

    let refused: Result<(), StoreRefusal> = transaction(&mut project, MODULE, |conn| {
        conn.execute_batch("INSERT INTO m_fixture_notes (id) VALUES (7)")
            .expect("the row goes in inside the transaction");
        // The module's own work refuses, which is the only thing that decides a rollback.
        Err(StoreRefusal::Failed("the module gave up".into()))
    });
    assert!(refused.is_err());

    let mut rows = 0usize;
    run(
        &project,
        MODULE,
        "SELECT id FROM m_fixture_notes",
        &[],
        &mut |_| {
            rows += 1;
            true
        },
    )
    .expect("its own table");
    assert_eq!(rows, 0, "the row the failed transaction wrote is not there");

    drop(project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_transaction_that_succeeds_keeps_what_it_wrote() {
    let (dir, mut project) = project("commit");
    run(
        &project,
        MODULE,
        "CREATE TABLE m_fixture_notes (id INTEGER PRIMARY KEY)",
        &[],
        &mut nowhere,
    )
    .expect("its own table");

    transaction(&mut project, MODULE, |conn| {
        conn.execute_batch("INSERT INTO m_fixture_notes (id) VALUES (7)")
            .map_err(|error| StoreRefusal::Failed(error.to_string()))
    })
    .expect("the transaction commits");

    let mut rows = 0usize;
    run(
        &project,
        MODULE,
        "SELECT id FROM m_fixture_notes",
        &[],
        &mut |_| {
            rows += 1;
            true
        },
    )
    .expect("its own table");
    assert_eq!(rows, 1);

    // And the core is not inside a transaction afterwards, which a missing commit would leave it in.
    episodes(&project).expect("the core reads its own tables afterwards");
    drop(project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_transaction_holds_a_module_to_its_own_tables_too() {
    let (dir, mut project) = project("guarded");
    let refused: Result<(), StoreRefusal> = transaction(&mut project, MODULE, |conn| {
        match conn.execute_batch("PRAGMA user_version = 99") {
            Ok(()) => Ok(()),
            Err(error) => Err(StoreRefusal::Failed(error.to_string())),
        }
    });
    assert!(
        refused.is_err(),
        "the guard is off inside a transaction, so a module could write the core's own version"
    );
    episodes(&project).expect("the core reads its own tables afterwards");
    drop(project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_module_ladder_reads_as_absent_before_anything_writes_it() {
    let (dir, project) = project("ladder");
    assert_eq!(
        schema_version(&project, MODULE).expect("the ladder is readable"),
        None,
        "a module that has never stored anything is at no version"
    );
    drop(project);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_id_that_is_not_one_never_reaches_a_statement() {
    let (dir, project) = project("badid");
    for bad in ["Fixture", "fix ture", "", "fixture'; DROP TABLE series; --"] {
        assert_eq!(
            run(&project, bad, "SELECT 1", &[], &mut nowhere),
            Err(StoreRefusal::Denied),
            "{bad:?} was accepted as a module id"
        );
    }
    episodes(&project).expect("the core reads its own tables afterwards");
    drop(project);
    fs::remove_dir_all(&dir).ok();
}
