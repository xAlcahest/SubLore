//! Behavioural tests for M1.4, written from the acceptance criteria in BACKLOG.md: overwriting an
//! existing file always leaves a timestamped backup, the rolling cap is enforced, and a failure at
//! any step leaves the destination exactly as it was. The crash half lives in `crash_injection.rs`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Barrier;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sublore_io::atomic::{copy_atomic, save_with_backup, write_atomic};
use sublore_io::backup::{BackupStore, BACKUP_CAP};
use sublore_io::error::IoErrorKind;

/// The old content, with a CRLF and a bare LF: subtitle bytes, not tidy text.
const OLD: &[u8] = b"1\r\n00:00:01,000 --> 00:00:02,000\r\nold line\r\n\nstray\n";
/// Deliberately a different length from `OLD`, and not ASCII, so a torn write cannot pass.
const NEW: &[u8] =
    b"1\r\n00:00:01,000 --> 00:00:02,000\r\n\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e\r\n";
/// A neighbouring file that no save may ever touch.
const KEEP: &[u8] = b"the user's other file\n";
/// The reserved temp name from `atomic.rs`. A change here is a change the user sees in their folder.
const TEMP_PREFIX: &str = ".sublore-tmp-";

// ---------------------------------------------------------------------------
// Helpers. Same shape as `src-tauri/tests/crash_safety.rs`: no dev-dependency.
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
        "sublore-io-test-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// The scratch layout every test uses: a subtitle directory and a backup store beside it.
fn workspace(tag: &str) -> (PathBuf, PathBuf, BackupStore) {
    let root = scratch(tag);
    let subs = root.join("subs");
    fs::create_dir_all(&subs).expect("the subtitle directory should be creatable");
    let store = BackupStore::new(root.join("store"));
    (root, subs, store)
}

fn write_bytes(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes)
        .unwrap_or_else(|error| panic!("{} should be writable: {error}", path.display()));
}

fn read_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()))
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

fn assert_no_temp_files(dir: &Path) {
    let strays: Vec<String> = names_in(dir)
        .into_iter()
        .filter(|name| name.starts_with(TEMP_PREFIX))
        .collect();
    assert!(
        strays.is_empty(),
        "a completed save must leave no temp file in {}, found {strays:?}",
        dir.display()
    );
}

/// The two permission criteria need a directory this process cannot write into. Root ignores the
/// mode and so do some mounts, and there the case cannot be expressed at all. That is a run which
/// has to go red, never one that returns green having asserted nothing (CONTRIBUTING.md 5.4).
#[cfg(unix)]
fn require_mode_is_enforced(directory: &Path, cleanup: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let probe = directory.join(".mode-probe");
    let Ok(file) = fs::File::create(&probe) else {
        return;
    };
    drop(file);
    let _ = fs::remove_file(&probe);
    let _ = fs::set_permissions(directory, fs::Permissions::from_mode(0o700));
    let _ = fs::remove_dir_all(cleanup);
    panic!(
        "this process can write into a 0o500 directory, so the two permission criteria in \
         CONTRIBUTING.md section 3 cannot be exercised here and this run proves nothing about \
         them. Run the suite as an ordinary user on a filesystem that honours mode bits: not as \
         root, not under fakeroot, not on a mount that drops them."
    );
}

// ---------------------------------------------------------------------------
// Acceptance: overwriting keeps a backup, and the bytes land exactly.
// ---------------------------------------------------------------------------

#[test]
fn a_save_replaces_the_destination_and_keeps_a_backup() {
    let (root, subs, store) = workspace("save");
    let destination = subs.join("ep01.srt");
    let neighbour = subs.join("keep.txt");
    write_bytes(&destination, OLD);
    write_bytes(&neighbour, KEEP);

    let outcome = save_with_backup(&destination, NEW, &store).expect("the save should succeed");

    assert_eq!(outcome.destination, destination);
    assert_eq!(outcome.bytes_written, NEW.len() as u64);
    assert_eq!(
        read_bytes(&destination),
        NEW,
        "the new bytes must land whole"
    );
    assert_eq!(
        read_bytes(&neighbour),
        KEEP,
        "a neighbour must not be touched"
    );
    assert_eq!(names_in(&subs), vec!["ep01.srt", "keep.txt"]);

    let backup = outcome
        .backup
        .expect("overwriting an existing file must keep a backup");
    assert_eq!(read_bytes(&backup), OLD, "the backup holds the old content");

    let newer = b"2\r\n00:00:03,000 --> 00:00:04,000\r\nnewer\r\n";
    let second = save_with_backup(&destination, newer, &store).expect("the second save succeeds");
    assert_eq!(read_bytes(&destination), newer);

    let backups = store.list(&destination).expect("the store should list");
    assert_eq!(backups.len(), 2, "each overwrite keeps its own backup");
    assert_eq!(
        read_bytes(&backups[0]),
        NEW,
        "the list is newest first, so the second backup leads"
    );
    assert_eq!(read_bytes(&backups[1]), OLD, "the first backup survives");
    assert_eq!(
        read_bytes(&second.backup.expect("the second save backs up too")),
        NEW
    );
    assert_no_temp_files(&subs);
    assert_no_temp_files(&store.dir_for(&destination));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn saving_to_a_new_path_keeps_no_backup() {
    let (root, subs, store) = workspace("fresh");
    let destination = subs.join("ep02.srt");

    let outcome = save_with_backup(&destination, NEW, &store).expect("the save should succeed");

    assert!(
        outcome.backup.is_none(),
        "there is nothing to back up when the file did not exist"
    );
    assert_eq!(read_bytes(&destination), NEW);
    assert!(
        !store.dir_for(&destination).exists(),
        "no backup means no backup directory"
    );
    assert_no_temp_files(&subs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_zero_byte_payload_still_replaces_the_destination() {
    let (root, subs, store) = workspace("empty");
    let destination = subs.join("ep03.srt");
    write_bytes(&destination, OLD);

    let outcome = save_with_backup(&destination, b"", &store).expect("the save should succeed");

    assert_eq!(outcome.bytes_written, 0);
    assert_eq!(
        read_bytes(&destination),
        b"",
        "an empty save is still a save"
    );
    assert_eq!(
        read_bytes(&outcome.backup.expect("the old content is backed up")),
        OLD
    );
    assert_no_temp_files(&subs);
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Acceptance: the rolling cap, and what it must never delete.
// ---------------------------------------------------------------------------

#[test]
fn the_backup_cap_drops_the_oldest_and_leaves_foreign_files_alone() {
    let (root, subs, store) = workspace("cap");
    let destination = subs.join("ep01.srt");
    // Names that look close enough to a backup to be dangerous, and must survive anyway.
    let foreign = [
        "notes.txt",
        "README.bak",
        "ep01.srt.2026082-101500.bak",
        "ep01.srt.20260823-1015.bak",
        "ep01.srt.20260823-101500.txt",
        "ep01.srt.20260823-101500-100.bak",
        "other.srt.20260823-101500.bak",
    ];

    let mut versions = Vec::new();
    for index in 0..12u32 {
        let content = format!("version {index}\n").into_bytes();
        write_bytes(&destination, &content);
        let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000 + u64::from(index) * 3_600);
        let backup = store
            .archive(&destination, now)
            .expect("the archive should succeed")
            .expect("the file exists, so it is archived");
        assert_eq!(read_bytes(&backup), content, "a backup is a full copy");
        versions.push(content);

        if index == 0 {
            for name in foreign {
                write_bytes(&store.dir_for(&destination).join(name), b"not a backup\n");
            }
        }
    }

    let kept = store.list(&destination).expect("the store should list");
    assert_eq!(kept.len(), BACKUP_CAP, "the cap is the number that remain");
    for (offset, path) in kept.iter().enumerate() {
        let expected = &versions[versions.len() - 1 - offset];
        assert_eq!(
            &read_bytes(path),
            expected,
            "position {offset} of a newest-first list"
        );
    }

    let survivors = names_in(&store.dir_for(&destination));
    for name in foreign {
        assert!(
            survivors.contains(&name.to_owned()),
            "{name} is not a Sublore backup and must never be deleted, found {survivors:?}"
        );
    }
    assert_no_temp_files(&store.dir_for(&destination));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_failed_backup_aborts_before_the_destination_is_touched() {
    let root = scratch("backup-fail");
    let subs = root.join("subs");
    fs::create_dir_all(&subs).expect("the subtitle directory should be creatable");
    let destination = subs.join("ep01.srt");
    write_bytes(&destination, OLD);
    // A store root that is a regular file: the per-file directory can never be created.
    let store_root = root.join("store");
    write_bytes(&store_root, b"not a directory\n");
    let store = BackupStore::new(store_root);

    let error =
        save_with_backup(&destination, NEW, &store).expect_err("a broken store must fail the save");

    assert_eq!(error.kind, IoErrorKind::BackupFailed);
    assert_eq!(
        read_bytes(&destination),
        OLD,
        "the destination keeps the old bytes when the backup fails"
    );
    assert_eq!(names_in(&subs), vec!["ep01.srt"]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn listing_a_file_that_was_never_saved_is_empty() {
    let (root, subs, store) = workspace("list-empty");
    let backups = store
        .list(&subs.join("never.srt"))
        .expect("listing an unknown file is not an error");
    assert!(backups.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn same_named_files_in_different_folders_get_different_backup_directories() {
    let (root, subs, store) = workspace("keys");
    let other = root.join("other");
    fs::create_dir_all(&other).expect("the second directory should be creatable");
    let first = subs.join("ep01.srt");
    let second = other.join("ep01.srt");
    write_bytes(&first, OLD);
    write_bytes(&second, KEEP);

    let first_dir = store.dir_for(&first);
    let second_dir = store.dir_for(&second);
    assert_ne!(
        first_dir, second_dir,
        "two files with the same name must not share a backup directory"
    );
    let readable = first_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    assert!(
        readable.starts_with("ep01.srt-"),
        "the directory name stays readable, got {readable}"
    );

    save_with_backup(&first, NEW, &store).expect("the first save should succeed");
    assert_eq!(store.list(&first).expect("list").len(), 1);
    assert!(
        store.list(&second).expect("list").is_empty(),
        "the other file has no backups of its own"
    );
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Acceptance: refusals, and what a refusal must not create.
// ---------------------------------------------------------------------------

#[test]
fn a_directory_destination_is_refused() {
    let (root, subs, store) = workspace("dir-dest");
    let destination = subs.join("a-directory");
    fs::create_dir_all(&destination).expect("the directory should be creatable");

    let error = write_atomic(&destination, NEW).expect_err("a directory is not a destination");
    assert_eq!(error.kind, IoErrorKind::NotAFile);

    let error =
        save_with_backup(&destination, NEW, &store).expect_err("same through the save path");
    assert_eq!(error.kind, IoErrorKind::NotAFile);
    assert!(
        names_in(&destination).is_empty(),
        "a refused save creates nothing"
    );
    assert_eq!(names_in(&subs), vec!["a-directory"]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn empty_and_parentless_paths_are_refused() {
    let root = scratch("bad-paths");
    let store = BackupStore::new(root.join("store"));
    for path in [Path::new(""), Path::new("/")] {
        let error =
            write_atomic(path, NEW).expect_err("a path with no directory is not a destination");
        assert_eq!(
            error.kind,
            IoErrorKind::InvalidPath,
            "{} must be rejected as a path, not attempted",
            path.display()
        );
        let error = save_with_backup(path, NEW, &store).expect_err("same through the save path");
        assert_eq!(error.kind, IoErrorKind::InvalidPath);
    }
    assert!(
        !root.join("store").exists(),
        "a refused save creates no backup store"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_missing_parent_directory_fails_without_creating_it() {
    let (root, subs, store) = workspace("no-parent");
    let missing = subs.join("nope");
    let destination = missing.join("ep01.srt");

    let error = save_with_backup(&destination, NEW, &store).expect_err("the directory is missing");

    assert_eq!(error.kind, IoErrorKind::TempCreateFailed);
    assert!(
        !missing.exists(),
        "a save never creates directories the user did not make"
    );
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Acceptance: copy_atomic, the archiving half.
// ---------------------------------------------------------------------------

#[test]
fn copying_lands_the_whole_file_and_reports_its_length() {
    let (root, subs, _store) = workspace("copy");
    let source = subs.join("ep01.srt");
    let destination = subs.join("copy.srt");
    write_bytes(&source, OLD);

    let copied = copy_atomic(&source, &destination).expect("the copy should succeed");

    assert_eq!(copied, OLD.len() as u64);
    assert_eq!(read_bytes(&destination), OLD);
    assert_eq!(read_bytes(&source), OLD, "the source is read-only");
    assert_no_temp_files(&subs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn copying_a_missing_source_creates_nothing() {
    let (root, subs, _store) = workspace("copy-missing");
    let destination = subs.join("copy.srt");

    let error =
        copy_atomic(&subs.join("gone.srt"), &destination).expect_err("the source does not exist");

    assert_eq!(error.kind, IoErrorKind::ReadFailed);
    assert!(
        !destination.exists(),
        "a failed copy creates no destination"
    );
    assert!(names_in(&subs).is_empty());
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Acceptance: concurrent saves never mix two versions.
// ---------------------------------------------------------------------------

#[test]
fn concurrent_saves_leave_one_complete_version() {
    let (root, subs, store) = workspace("threads");
    let destination = subs.join("ep01.srt");
    write_bytes(&destination, OLD);
    let payloads: Vec<Vec<u8>> = (0..8u32)
        .map(|index| format!("payload {index}\n").repeat(512).into_bytes())
        .collect();
    let barrier = Barrier::new(payloads.len());

    let results = thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .iter()
            .map(|payload| {
                let (barrier, store, destination) = (&barrier, &store, &destination);
                scope.spawn(move || {
                    barrier.wait();
                    save_with_backup(destination, payload, store)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a saving thread must not panic"))
            .collect::<Vec<_>>()
    });

    // Unix replaces an open file freely; Windows can refuse the rename while another handle is on
    // the destination, which is an honest error, not a torn file. See BACKLOG.md M1.4.
    #[cfg(unix)]
    for result in &results {
        assert!(result.is_ok(), "every save should succeed: {result:?}");
    }
    #[cfg(not(unix))]
    {
        assert!(
            results.iter().any(|result| result.is_ok()),
            "at least one save must succeed"
        );
        for result in results.iter().filter(|result| result.is_err()) {
            let kind = result.as_ref().err().map(|error| error.kind);
            assert!(
                matches!(
                    kind,
                    Some(IoErrorKind::RenameFailed) | Some(IoErrorKind::PermissionDenied)
                ),
                "a losing save may only fail on the rename, got {kind:?}"
            );
        }
    }

    let landed = read_bytes(&destination);
    assert!(
        payloads.contains(&landed),
        "the destination holds one complete payload, not a mix ({} bytes)",
        landed.len()
    );
    let backups = store.list(&destination).expect("the store should list");
    for backup in &backups {
        let content = read_bytes(backup);
        assert!(
            content == OLD || payloads.contains(&content),
            "a backup must be a complete version ({} bytes)",
            content.len()
        );
    }
    #[cfg(unix)]
    assert_eq!(
        backups.len(),
        payloads.len(),
        "every save archived what it replaced"
    );
    assert_no_temp_files(&subs);
    assert_no_temp_files(&store.dir_for(&destination));
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Acceptance: unix file semantics the user would notice.
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_symlink_destination_is_written_through() {
    let (root, subs, store) = workspace("symlink");
    let target = subs.join("real.srt");
    let link = subs.join("link.srt");
    write_bytes(&target, OLD);
    std::os::unix::fs::symlink(&target, &link).expect("the symlink should be creatable");

    let outcome = save_with_backup(&link, NEW, &store).expect("the save should succeed");

    let kind = fs::symlink_metadata(&link)
        .expect("the link should still be there")
        .file_type();
    assert!(
        kind.is_symlink(),
        "the user's symlink must survive the save"
    );
    assert_eq!(read_bytes(&target), NEW, "the bytes land on the real file");
    assert_eq!(
        outcome.destination,
        fs::canonicalize(&target).expect("the target should resolve")
    );
    assert_eq!(
        read_bytes(&outcome.backup.expect("the old content is backed up")),
        OLD
    );
    assert_no_temp_files(&subs);
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn a_store_root_that_cannot_be_written_stops_the_save() {
    use std::os::unix::fs::PermissionsExt;

    let root = scratch("store-perm");
    let subs = root.join("subs");
    let store_root = root.join("store");
    fs::create_dir_all(&subs).expect("the subtitle directory should be creatable");
    fs::create_dir_all(&store_root).expect("the store root should be creatable");
    let destination = subs.join("ep01.srt");
    write_bytes(&destination, OLD);
    fs::set_permissions(&store_root, fs::Permissions::from_mode(0o500))
        .expect("the mode should be settable");

    require_mode_is_enforced(&store_root, &root);

    let store = BackupStore::new(store_root.clone());
    let error = save_with_backup(&destination, NEW, &store)
        .expect_err("a store that cannot be written must fail the save");

    // A permission problem keeps its own kind: it is the one the user can act on.
    assert_eq!(error.kind, IoErrorKind::PermissionDenied);
    let _ = fs::set_permissions(&store_root, fs::Permissions::from_mode(0o700));
    assert_eq!(
        read_bytes(&destination),
        OLD,
        "no backup means the destination is never written"
    );
    assert_eq!(names_in(&subs), vec!["ep01.srt"]);
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn the_destination_mode_is_preserved() {
    use std::os::unix::fs::PermissionsExt;

    let (root, subs, store) = workspace("mode");
    let destination = subs.join("ep01.srt");
    write_bytes(&destination, OLD);
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o640))
        .expect("the mode should be settable");

    save_with_backup(&destination, NEW, &store).expect("the save should succeed");

    let mode = fs::metadata(&destination)
        .expect("the destination should be there")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o640, "a save must not change who can read the file");
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn a_read_only_directory_reports_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    let (root, subs, _store) = workspace("read-only");
    let destination = subs.join("ep01.srt");
    write_bytes(&destination, OLD);
    fs::set_permissions(&subs, fs::Permissions::from_mode(0o500))
        .expect("the mode should be settable");

    require_mode_is_enforced(&subs, &root);

    let error = write_atomic(&destination, NEW).expect_err("a read-only directory refuses a save");

    assert_eq!(error.kind, IoErrorKind::PermissionDenied);
    let _ = fs::set_permissions(&subs, fs::Permissions::from_mode(0o700));
    assert_eq!(
        read_bytes(&destination),
        OLD,
        "the old bytes are still there"
    );
    assert_no_temp_files(&subs);
    let _ = fs::remove_dir_all(&root);
}
