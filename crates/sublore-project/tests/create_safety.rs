//! Behavioural tests for the two ways creating a project could cost a user data: two creates
//! racing one folder, where the loser's cleanup took the winner's finished database with it, and a
//! link standing where the database goes, which would put a project outside the folder the user
//! chose. Both are CONTRIBUTING.md §3; see BACKLOG.md M4.1 and M4.2.
//!
//! Real files in a scratch directory, in the style of `tests/deletion_safety.rs`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sublore_project::error::ProjectErrorKind;
use sublore_project::layout::{CURRENT_VERSION, DATABASE_NAME};
use sublore_project::records::Project;
use sublore_project::{Database, ProjectError};

/// Eight creates on one folder, twenty times over. One folder is one project, however many callers
/// ask for it at once.
const RACING_THREADS: usize = 8;
const RACING_ROUNDS: usize = 20;

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

/// A directory of this test's own, under the OS temp dir. Removed at the end of the test.
fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!(
        "sublore-create-test-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

fn make_dir(path: &Path) -> PathBuf {
    fs::create_dir_all(path)
        .unwrap_or_else(|error| panic!("{} should be creatable: {error}", path.display()));
    path.to_path_buf()
}

// ---------------------------------------------------------------------------
// Two creates, one folder.
// ---------------------------------------------------------------------------

#[test]
fn creates_racing_one_folder_leave_exactly_one_project_and_keep_it() {
    for round in 0..RACING_ROUNDS {
        let root = scratch(&format!("race-{round}"));
        let folder = make_dir(&root.join("project"));

        let start = Arc::new(Barrier::new(RACING_THREADS));
        let mut runners = Vec::with_capacity(RACING_THREADS);
        for _ in 0..RACING_THREADS {
            let folder = folder.clone();
            let start = Arc::clone(&start);
            runners.push(thread::spawn(move || -> Result<(), ProjectError> {
                start.wait();
                Database::create(&folder, "Kaiba", at(1_756_000_000))?
                    .close()
                    .expect("a database that was made should close");
                Ok(())
            }));
        }

        let outcomes: Vec<Result<(), ProjectError>> = runners
            .into_iter()
            .map(|runner| runner.join().expect("a create thread should not panic"))
            .collect();

        let made = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
        assert_eq!(
            made, 1,
            "round {round}: one folder is one project, whatever the callers did: {outcomes:?}"
        );
        // The one that won is still on disk, and it is a whole project: the loser's cleanup taking
        // it away is the data loss this test exists for. See CONTRIBUTING.md §3.
        assert!(
            folder.join(DATABASE_NAME).is_file(),
            "round {round}: the project that was made must still be there"
        );
        let project = Project::open(&folder)
            .unwrap_or_else(|error| panic!("round {round}: the project should reopen: {error}"));
        assert_eq!(project.summary().title, "Kaiba");
        assert_eq!(project.summary().schema_version, CURRENT_VERSION);
        assert_eq!(project.summary().episode_count, 0);
        project.close().expect("the project should close");

        for outcome in &outcomes {
            if let Err(error) = outcome {
                assert_eq!(
                    error.kind,
                    ProjectErrorKind::AlreadyAProject,
                    "round {round}: a create that lost the race says the folder is taken"
                );
            }
        }

        let _ = fs::remove_dir_all(&root);
    }
}

// ---------------------------------------------------------------------------
// A link where the database goes. Unix only: it is where a link costs nothing to plant.
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod links {
    use super::{at, make_dir, scratch, Arc, Database, ProjectErrorKind, DATABASE_NAME};
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use sublore_project::layout::OWNED_FILES;

    /// How many creates run against a thread that keeps swapping its link into the database's place.
    const PLANTING_ATTEMPTS: usize = 20_000;

    fn write_bytes(path: &Path, bytes: &[u8]) -> PathBuf {
        fs::write(path, bytes)
            .unwrap_or_else(|error| panic!("{} should be writable: {error}", path.display()));
        path.to_path_buf()
    }

    fn size_of(path: &Path) -> u64 {
        fs::metadata(path)
            .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()))
            .len()
    }

    #[test]
    fn a_link_standing_where_the_database_goes_is_refused_and_its_target_untouched() {
        let root = scratch("planted-link");
        let folder = make_dir(&root.join("project"));
        let outside = make_dir(&root.join("outside"));
        // Empty, which is the case a create could once mistake for a database of its own making.
        let target = write_bytes(&outside.join("some-other-app.dat"), b"");
        let link = folder.join(DATABASE_NAME);
        symlink(&target, &link).expect("the symlink should be creatable");

        let error = Database::create(&folder, "Kaiba", at(1_756_000_000))
            .expect_err("a link standing in the database's place is not an empty folder");

        assert_eq!(error.kind, ProjectErrorKind::AlreadyAProject);
        assert_eq!(
            size_of(&target),
            0,
            "nothing may be written through the link, into a file outside the project folder"
        );
        assert!(
            link.symlink_metadata().is_ok(),
            "the link is left as it was"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_link_swapped_in_while_a_project_is_created_is_never_written_through() {
        let root = scratch("raced-link");
        let folder = make_dir(&root.join("project"));
        let outside = make_dir(&root.join("outside"));
        let target = write_bytes(&outside.join("some-other-app.dat"), b"");
        let database = folder.join(DATABASE_NAME);

        // Another program with write access to the folder, planting its link as fast as it can.
        let stop = Arc::new(AtomicBool::new(false));
        let planter = thread::spawn({
            let database = database.clone();
            let target = target.clone();
            let stop = Arc::clone(&stop);
            move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = fs::remove_file(&database);
                    let _ = symlink(&target, &database);
                }
            }
        });

        let mut refused = 0;
        let mut created = 0;
        for attempt in 0..PLANTING_ATTEMPTS {
            match Database::create(&folder, "Kaiba", at(1_756_000_000)) {
                Ok(made) => {
                    created += 1;
                    let _ = made.close();
                }
                // Only this kind says the create found the link in its way; a create whose file
                // vanished under it comes back as something else. See BACKLOG.md N9, S8.
                Err(error) if error.kind == ProjectErrorKind::AlreadyAProject => refused += 1,
                Err(_) => {}
            }
            let _ = fs::remove_file(&database);
            assert_eq!(
                size_of(&target),
                0,
                "attempt {attempt}: a project was written into a file outside the folder the user \
                 chose. See CONTRIBUTING.md §3."
            );
        }

        stop.store(true, Ordering::Relaxed);
        planter
            .join()
            .expect("the planting thread should not panic");

        // Anti-vacuity, after crates/sublore-edit/tests/property.rs: all-refused means no create
        // ever ran, all-created means the planter never landed. See BACKLOG.md N9, S8.
        assert!(
            refused > 0 && created > 0,
            "{refused} creates found the link in the way and {created} got through, out of \
             {PLANTING_ATTEMPTS}: with one of those at zero the two threads took turns and the \
             swap was never raced"
        );

        // The folder still works afterwards: the race left nothing standing in the way. The
        // journals go too, so what is created below is a project and not a recovery.
        for name in OWNED_FILES {
            let _ = fs::remove_file(folder.join(name));
        }
        Database::create(&folder, "Kaiba", at(1_756_000_000))
            .expect("a project is still creatable in the folder")
            .close()
            .expect("the database should close");
        assert_eq!(
            size_of(&target),
            0,
            "the file outside the project folder is still empty"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
