//! The number a module tells one project from another by, through the file rather than through one
//! connection's memory. BACKLOG.md N33, and the Rust half of the criteria in
//! `module-lifecycle-tasks.md` §8.
//!
//! Real folders in a scratch directory, in the style of `migrations.rs` beside this file.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use sublore_project::layout::{CURRENT_VERSION, DATABASE_NAME};
use sublore_project::migrate::migrate_to;
use sublore_project::records::Project;
use sublore_project::NO_PROJECT_KEY;

const MADE_AT: u64 = 1_756_300_000;

fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!(
        "sublore-identity-test-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

fn made_at() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(MADE_AT)
}

fn key_of(folder: &Path) -> i64 {
    let project = Project::open(folder).expect("the project should open");
    let key = project.key();
    project.close().expect("the project should close cleanly");
    key
}

fn clean_up(root: &Path) {
    fs::remove_dir_all(root).ok();
}

#[test]
fn a_project_closed_and_opened_again_is_the_same_project() {
    let folder = scratch("reopen");
    let made = Project::create(&folder, "Series One", made_at()).expect("a project should be made");
    let first = made.key();
    assert_ne!(first, NO_PROJECT_KEY, "zero means no project is open");
    made.close().expect("the project should close cleanly");

    // Through the file: the connection that wrote the key is gone.
    assert_eq!(key_of(&folder), first);
    assert_eq!(
        key_of(&folder),
        first,
        "and it does not move on a third open"
    );

    clean_up(&folder);
}

#[test]
fn two_projects_are_two_keys() {
    let one = scratch("one");
    let other = scratch("other");
    let made = Project::create(&one, "One", made_at()).expect("a project should be made");
    let first = made.key();
    made.close().expect("the project should close cleanly");
    let made = Project::create(&other, "Other", made_at()).expect("a project should be made");
    let second = made.key();
    made.close().expect("the project should close cleanly");

    assert_ne!(
        first, second,
        "two projects a module must be able to tell apart"
    );

    clean_up(&one);
    clean_up(&other);
}

#[test]
fn a_project_moved_on_disk_keeps_its_key() {
    // The key is inside the file, so nothing about it is derived from where the file sits. A key
    // taken from the folder path would fail here and only here.
    let folder = scratch("moved");
    let made = Project::create(&folder, "Series One", made_at()).expect("a project should be made");
    let before = made.key();
    made.close().expect("the project should close cleanly");

    let moved = folder.with_file_name(format!(
        "{}-carried-elsewhere",
        folder
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
    ));
    fs::rename(&folder, &moved).expect("the project folder should be movable");

    assert_eq!(key_of(&moved), before);

    clean_up(&moved);
}

#[test]
fn a_project_written_by_an_older_build_gains_a_key_and_keeps_it() {
    let folder = scratch("older");
    let database = folder.join(DATABASE_NAME);

    // A database at version 2, which is what the build before this one wrote.
    let mut conn = Connection::open(&database).expect("a fresh file should open");
    assert_eq!(
        migrate_to(&mut conn, 2).expect("the older schema should build"),
        2
    );
    conn.execute(
        "INSERT INTO series (id, title, created_at) VALUES (1, 'Series One', ?1)",
        [MADE_AT as i64],
    )
    .expect("the series row should be insertable");
    conn.close().expect("the connection should close cleanly");

    let project = Project::open(&folder).expect("an older project should open");
    assert_eq!(project.summary().schema_version, CURRENT_VERSION);
    let gained = project.key();
    assert_ne!(gained, NO_PROJECT_KEY);
    project.close().expect("the project should close cleanly");

    assert_eq!(
        key_of(&folder),
        gained,
        "the key it gained is the one it keeps"
    );

    clean_up(&folder);
}
