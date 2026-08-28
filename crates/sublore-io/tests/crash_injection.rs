//! Behavioural tests for M1.4, written from the acceptance criteria in BACKLOG.md: a save
//! interrupted at any step leaves the destination holding the old content in full or the new
//! content in full, never a truncated or mixed file.
//!
//! The interruption is a real `abort()`, so it cannot be observed in-process: every case re-runs
//! this test binary as a child with a fault point armed, then reads what the child left on disk.
//! Same shape as `src-tauri/tests/crash_safety.rs`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sublore_io::atomic::save_with_backup;
use sublore_io::backup::BackupStore;
use sublore_io::fault::{FaultPoint, ENV_VAR};

/// Set by the parent to turn `fault_child` from a no-op into one save.
const CHILD_ENV: &str = "SUBLORE_IO_CHILD";
const DEST_ENV: &str = "SUBLORE_IO_DEST";
const STORE_ENV: &str = "SUBLORE_IO_STORE";
/// Each point is interrupted repeatedly, per the acceptance criteria.
const RUNS: usize = 5;
const CHILD_TIMEOUT: Duration = Duration::from_secs(20);
/// The reserved temp name from `atomic.rs`. A change here is a change the user sees in their folder.
const TEMP_PREFIX: &str = ".sublore-tmp-";

const OLD: &[u8] = b"1\r\n00:00:01,000 --> 00:00:02,000\r\nold line\r\n\nstray\n";
/// Long enough that a half-write is unmistakable, and not ASCII.
const NEW: &[u8] =
    b"1\r\n00:00:01,000 --> 00:00:02,000\r\n\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e ~~~~~~~~\r\n";
const KEEP: &[u8] = b"the user's other file\n";

const POINTS: [FaultPoint; 6] = [
    FaultPoint::AfterBackup,
    FaultPoint::AfterTempCreated,
    FaultPoint::DuringWrite,
    FaultPoint::AfterWrite,
    FaultPoint::AfterSync,
    FaultPoint::AfterRename,
];

// ---------------------------------------------------------------------------
// The child half. A no-op unless the parent asked for one save.
// ---------------------------------------------------------------------------

#[test]
fn fault_child() {
    if env::var(CHILD_ENV).is_err() {
        return;
    }
    let destination = PathBuf::from(env::var(DEST_ENV).expect("the parent sets the destination"));
    let store = BackupStore::new(PathBuf::from(
        env::var(STORE_ENV).expect("the parent sets the store root"),
    ));

    // Reaching this line at all is the failure the parent reports: the point must abort first.
    match save_with_backup(&destination, NEW, &store) {
        Ok(outcome) => eprintln!("the save returned {outcome:?}"),
        Err(error) => eprintln!("the save failed: {error}"),
    }
}

// ---------------------------------------------------------------------------
// The parent half.
// ---------------------------------------------------------------------------

/// A directory of this test's own, under the OS temp dir. Removed at the end of the case.
fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!(
        "sublore-io-crash-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// Re-run this binary with only `fault_child` selected, one fault point armed.
fn spawn_child(point: FaultPoint, destination: &Path, store_root: &Path) -> Child {
    let exe = env::current_exe().expect("the test binary should have a path");
    Command::new(exe)
        .args(["--exact", "fault_child", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .env(DEST_ENV, destination)
        .env(STORE_ENV, store_root)
        .env(ENV_VAR, point.as_str())
        .stdout(Stdio::null())
        .spawn()
        .expect("the child process should start")
}

/// Wait for `child`, killing it if it outlives the timeout. A save that hangs is a failure too.
fn wait_bounded(mut child: Child) -> ExitStatus {
    let deadline = Instant::now() + CHILD_TIMEOUT;
    loop {
        match child
            .try_wait()
            .expect("the child status should be readable")
        {
            Some(status) => return status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the child did not exit within {CHILD_TIMEOUT:?}");
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn read_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()))
}

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

/// One interrupted save, from an empty directory to the assertions on what it left behind.
fn interrupted_save(point: FaultPoint, existing: bool, run: usize) {
    let tag = format!(
        "{}-{}",
        point.as_str(),
        if existing { "over" } else { "new" }
    );
    let root = scratch(&tag);
    let subs = root.join("subs");
    let store_root = root.join("store");
    fs::create_dir_all(&subs).expect("the subtitle directory should be creatable");
    let destination = subs.join("ep01.srt");
    let neighbour = subs.join("keep.txt");
    fs::write(&neighbour, KEEP).expect("the neighbour should be writable");
    if existing {
        fs::write(&destination, OLD).expect("the destination should be writable");
    }
    let case = format!("{} run {run} (existing: {existing})", point.as_str());

    let status = wait_bounded(spawn_child(point, &destination, &store_root));
    assert!(
        !status.success(),
        "{case}: the child completed the save instead of aborting ({status:?}); \
         a release build compiles the trip points out"
    );

    // The destination is whole, at one version or the other. Nothing in between is acceptable.
    match (point, existing) {
        (FaultPoint::AfterRename, _) => assert_eq!(
            read_bytes(&destination),
            NEW,
            "{case}: after the rename the destination holds the new content"
        ),
        (_, true) => assert_eq!(
            read_bytes(&destination),
            OLD,
            "{case}: before the rename the destination is untouched"
        ),
        (_, false) => assert!(
            !destination.exists(),
            "{case}: before the rename nothing is created"
        ),
    }
    assert_eq!(
        read_bytes(&neighbour),
        KEEP,
        "{case}: a neighbour is untouched"
    );

    for name in names_in(&subs) {
        let known = name == "ep01.srt" || name == "keep.txt" || name.starts_with(TEMP_PREFIX);
        assert!(
            known,
            "{case}: a crash may only leave a reserved temp name behind, found {name}"
        );
    }

    let backups = BackupStore::new(store_root)
        .list(&destination)
        .expect("the store should list");
    if existing {
        assert_eq!(
            backups.len(),
            1,
            "{case}: the backup is written before anything else"
        );
        assert_eq!(
            read_bytes(&backups[0]),
            OLD,
            "{case}: the backup is a complete copy of the old content"
        );
    } else {
        assert!(
            backups.is_empty(),
            "{case}: there was nothing to back up, so no backup exists"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Acceptance: interrupt the save repeatedly, at every step.
// ---------------------------------------------------------------------------

#[test]
fn a_crash_never_tears_a_file_it_overwrites() {
    for point in POINTS {
        for run in 0..RUNS {
            interrupted_save(point, true, run);
        }
    }
}

#[test]
fn a_crash_never_leaves_a_partial_new_file() {
    for point in POINTS {
        for run in 0..RUNS {
            interrupted_save(point, false, run);
        }
    }
}

#[test]
fn an_unknown_fault_value_arms_nothing() {
    for point in POINTS {
        assert_eq!(FaultPoint::parse(point.as_str()), Some(point));
    }
    for value in [
        "",
        " ",
        "nonsense",
        "AfterSync",
        "after_sync",
        "after-rename ",
    ] {
        assert_eq!(
            FaultPoint::parse(value),
            None,
            "{value:?} must not arm a fault point"
        );
    }
}
