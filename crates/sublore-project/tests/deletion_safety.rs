//! Behavioural tests for M4.3, written from the acceptance criterion in BACKLOG.md: deleting a
//! project whose episodes point at media and subtitles outside the project folder leaves every one
//! of those files byte-identical on disk, and no code path deletes outside the project folder.
//!
//! The last two tests read the crate's own source. They are what turns "we do not delete user
//! files" from a convention into a property: a new deletion anywhere in `src/` turns them red.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sublore_project::delete::delete_project;
use sublore_project::error::ProjectErrorKind;
use sublore_project::model::FileRole;
use sublore_project::records::{
    add_episode, attach_file, delete_episode, detach_file, files, Project,
};

const SRT: &[u8] = b"1\r\n00:00:01,000 --> 00:00:02,000\r\nthe user's line\r\n";
const MKV: &[u8] = b"\x1a\x45\xdf\xa3 the user's video, which Sublore only ever reads\n";

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!(
        "sublore-delete-test-{tag}-{}-{nanos}-{unique}",
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

fn write_bytes(path: &Path, bytes: &[u8]) -> PathBuf {
    fs::write(path, bytes)
        .unwrap_or_else(|error| panic!("{} should be writable: {error}", path.display()));
    path.to_path_buf()
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

/// Bytes and modification time. A file Sublore has not touched matches this exactly afterwards.
fn fingerprint(path: &Path) -> (Vec<u8>, Option<SystemTime>) {
    let modified = fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok());
    (read_bytes(path), modified)
}

// ---------------------------------------------------------------------------
// The acceptance criterion: real files on disk, outside the project folder.
// ---------------------------------------------------------------------------

#[test]
fn deleting_a_project_leaves_every_user_file_byte_identical() {
    let root = scratch("delete-project");
    let folder = make_dir(&root.join("project"));
    let media_dir = make_dir(&root.join("user").join("media"));
    let subs_dir = make_dir(&root.join("user").join("subs"));

    let user_files = [
        write_bytes(&media_dir.join("ep01.mkv"), MKV),
        write_bytes(&media_dir.join("ep02.mkv"), MKV),
        write_bytes(&subs_dir.join("ep01.srt"), SRT),
        write_bytes(&subs_dir.join("ep02.srt"), SRT),
    ];
    // Files the user keeps inside the project folder. Sublore did not create them, so it may not
    // remove them either.
    let notes = write_bytes(&folder.join("notes.txt"), b"my terminology decisions\n");
    let own_copy = write_bytes(&folder.join("ep01-my-own-copy.srt"), SRT);

    let mut project =
        Project::create(&folder, "Kaiba", at(1_756_000_000)).expect("a project is created");
    let first = add_episode(&mut project, "one", at(1_756_000_001)).expect("episode one");
    let second = add_episode(&mut project, "two", at(1_756_000_002)).expect("episode two");
    attach_file(
        &mut project,
        first.id,
        FileRole::Media,
        &user_files[0],
        at(3),
    )
    .expect("media one");
    attach_file(
        &mut project,
        first.id,
        FileRole::Source,
        &user_files[2],
        at(4),
    )
    .expect("subs one");
    attach_file(
        &mut project,
        second.id,
        FileRole::Media,
        &user_files[1],
        at(5),
    )
    .expect("media two");
    attach_file(
        &mut project,
        second.id,
        FileRole::Target,
        &user_files[3],
        at(6),
    )
    .expect("subs two");

    let before: Vec<(Vec<u8>, Option<SystemTime>)> =
        user_files.iter().map(|path| fingerprint(path)).collect();
    let notes_before = fingerprint(&notes);
    let copy_before = fingerprint(&own_copy);

    project.close().expect("the project closes cleanly");
    let outcome = delete_project(&folder).expect("the project is deleted");

    for (path, expected) in user_files.iter().zip(before) {
        assert!(path.exists(), "{} must still exist", path.display());
        assert_eq!(
            fingerprint(path),
            expected,
            "{} must be byte-identical, with its modification time untouched",
            path.display()
        );
        assert!(
            !outcome.removed.contains(path),
            "{} must not appear in what was removed",
            path.display()
        );
    }
    assert_eq!(
        fingerprint(&notes),
        notes_before,
        "the user's notes survive"
    );
    assert_eq!(
        fingerprint(&own_copy),
        copy_before,
        "the user's own copy survives"
    );

    assert!(
        folder.is_dir(),
        "the project folder itself is never removed"
    );
    assert!(
        !folder.join("project.sublore").exists(),
        "the database is gone"
    );
    assert_eq!(
        names_in(&folder),
        vec!["ep01-my-own-copy.srt", "notes.txt"],
        "only Sublore's own file left the folder"
    );
    assert_eq!(
        outcome.removed,
        vec![folder.join("project.sublore")],
        "a cleanly closed project is one file, and that is all that was removed"
    );
    assert!(outcome.left_in_place.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn removing_records_never_removes_the_files_they_point_at() {
    let root = scratch("records-only");
    let folder = make_dir(&root.join("project"));
    let user = make_dir(&root.join("user"));
    let media = write_bytes(&user.join("ep01.mkv"), MKV);
    let subtitle = write_bytes(&user.join("ep01.srt"), SRT);
    let before = (fingerprint(&media), fingerprint(&subtitle));

    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    let episode = add_episode(&mut project, "one", at(1)).expect("episode");
    let attached_media =
        attach_file(&mut project, episode.id, FileRole::Media, &media, at(2)).expect("media");
    attach_file(&mut project, episode.id, FileRole::Source, &subtitle, at(3)).expect("subtitle");

    detach_file(&mut project, attached_media.id).expect("the attachment record is removed");
    assert_eq!(
        files(&project, episode.id).expect("lists").len(),
        1,
        "detaching removed exactly one row"
    );
    assert!(media.exists(), "detaching must not remove the user's video");

    delete_episode(&mut project, episode.id).expect("the episode is removed");
    assert!(
        files(&project, episode.id).expect("lists").is_empty(),
        "the cascade removed the remaining row"
    );

    project.close().expect("closes");
    assert_eq!(
        (fingerprint(&media), fingerprint(&subtitle)),
        before,
        "both files are byte-identical, with untouched modification times"
    );
    assert_eq!(names_in(&user), vec!["ep01.mkv", "ep01.srt"]);
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Adversarial: things in the folder that are not what they are named.
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_symlinked_database_is_left_in_place_and_its_target_survives() {
    use std::os::unix::fs::symlink;

    let root = scratch("symlink");
    let elsewhere = make_dir(&root.join("elsewhere"));
    let folder = make_dir(&root.join("project"));

    // A real project, then a folder whose project.sublore is only a link to it.
    Project::create(&elsewhere, "Series", at(1_756_000_000))
        .expect("created")
        .close()
        .expect("closes");
    let target = elsewhere.join("project.sublore");
    let before = fingerprint(&target);
    let link = folder.join("project.sublore");
    symlink(&target, &link).expect("the symlink should be creatable");

    let outcome = delete_project(&folder).expect("a folder with a linked database is answerable");

    assert!(outcome.removed.is_empty(), "a link is not ours to remove");
    assert_eq!(outcome.left_in_place, vec![link.clone()]);
    assert!(link.symlink_metadata().is_ok(), "the link is still there");
    assert!(target.exists(), "the file the link points at survives");
    assert_eq!(fingerprint(&target), before, "and it is byte-identical");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_directory_named_like_the_database_is_left_in_place() {
    let root = scratch("directory");
    let folder = make_dir(&root.join("project"));
    let masquerading = make_dir(&folder.join("project.sublore"));
    let inside = write_bytes(&masquerading.join("keep.txt"), b"still mine\n");

    let outcome = delete_project(&folder).expect("a folder with a directory there is answerable");

    assert!(
        outcome.removed.is_empty(),
        "a directory is not ours to remove"
    );
    assert_eq!(outcome.left_in_place, vec![masquerading.clone()]);
    assert!(masquerading.is_dir(), "the directory is still there");
    assert_eq!(
        read_bytes(&inside),
        b"still mine\n",
        "and so is what is in it"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn only_the_exact_owned_names_are_removed() {
    let root = scratch("exact-names");
    let folder = make_dir(&root.join("project"));
    Project::create(&folder, "Series", at(1_756_000_000))
        .expect("created")
        .close()
        .expect("closes");

    let mut lookalikes = vec![
        write_bytes(
            &folder.join("project.sublore.bak"),
            b"a backup the user made\n",
        ),
        write_bytes(&folder.join("notproject.sublore"), b"someone else's file\n"),
        write_bytes(&folder.join("project.sublore-wal2"), b"not our journal\n"),
    ];
    // `Project.Sublore` is a different file from `project.sublore` only where the filesystem says
    // so. On Windows it is the same file, so writing it here would overwrite the project database
    // and the deletion under test would fail on a fixture that cannot exist there. The case this
    // guards cannot occur on that platform either, so nothing is lost by not asserting it.
    if !cfg!(windows) {
        lookalikes.push(write_bytes(
            &folder.join("Project.Sublore"),
            b"different name\n",
        ));
    }
    let before: Vec<(Vec<u8>, Option<SystemTime>)> =
        lookalikes.iter().map(|path| fingerprint(path)).collect();

    let outcome = delete_project(&folder).expect("the project is deleted");

    assert_eq!(outcome.removed, vec![folder.join("project.sublore")]);
    for (path, expected) in lookalikes.iter().zip(before) {
        assert!(path.exists(), "{} must survive", path.display());
        assert_eq!(
            fingerprint(path),
            expected,
            "{} is untouched",
            path.display()
        );
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn delete_refuses_a_folder_that_is_not_a_sublore_project() {
    let root = scratch("refusals");

    let empty = make_dir(&root.join("empty"));
    assert_eq!(
        delete_project(&empty)
            .expect_err("a folder with no project.sublore holds no project")
            .kind,
        ProjectErrorKind::NoProjectHere
    );
    assert_eq!(names_in(&empty), Vec::<String>::new());

    let missing = root.join("never-existed");
    assert_eq!(
        delete_project(&missing)
            .expect_err("a folder that is not there")
            .kind,
        ProjectErrorKind::NoProjectHere
    );
    assert!(!missing.exists(), "and it is not created either");

    assert_eq!(
        delete_project(Path::new(""))
            .expect_err("an empty path names nothing")
            .kind,
        ProjectErrorKind::InvalidPath
    );

    for (label, content) in [
        ("a text file", b"this is not a database\n".to_vec()),
        ("an empty file", Vec::new()),
        ("a short file", b"SQLite format 3\0".to_vec()),
    ] {
        let folder = make_dir(&root.join(label.replace(' ', "-")));
        let planted = write_bytes(&folder.join("project.sublore"), &content);
        let neighbour = write_bytes(&folder.join("keep.srt"), SRT);
        assert_eq!(
            delete_project(&folder).expect_err(label).kind,
            ProjectErrorKind::NotASubloreProject,
            "{label} is not a project to delete"
        );
        assert_eq!(read_bytes(&planted), content, "{label} is left untouched");
        assert_eq!(
            read_bytes(&neighbour),
            SRT,
            "and so is everything beside it"
        );
    }

    // Another application's SQLite database: ours in every way except the application id.
    let foreign = make_dir(&root.join("foreign"));
    Project::create(&foreign, "Series", at(1_756_000_000))
        .expect("created")
        .close()
        .expect("closes");
    let database = foreign.join("project.sublore");
    let mut bytes = read_bytes(&database);
    bytes[68..72].copy_from_slice(&0x4f_54_48_52_u32.to_be_bytes());
    write_bytes(&database, &bytes);
    assert_eq!(
        delete_project(&foreign)
            .expect_err("another application's database is not ours to delete")
            .kind,
        ProjectErrorKind::NotASubloreProject
    );
    assert_eq!(
        read_bytes(&database),
        bytes,
        "and it is left byte-identical"
    );

    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn attaching_a_read_only_file_in_a_read_only_directory_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let root = scratch("read-only");
    let folder = make_dir(&root.join("project"));
    let user = make_dir(&root.join("user"));
    let subtitle = write_bytes(&user.join("ep01.srt"), SRT);
    let before = fingerprint(&subtitle);
    let listing = names_in(&user);

    fs::set_permissions(&subtitle, fs::Permissions::from_mode(0o444)).expect("mode is settable");
    fs::set_permissions(&user, fs::Permissions::from_mode(0o555)).expect("mode is settable");

    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    let episode = add_episode(&mut project, "one", at(1)).expect("episode");
    let recorded = attach_file(&mut project, episode.id, FileRole::Source, &subtitle, at(2))
        .expect("a read-only file is attachable, because attaching only reads");
    assert_eq!(recorded.path, subtitle);
    project.close().expect("closes");

    assert_eq!(fingerprint(&subtitle), before, "the file is untouched");
    assert_eq!(names_in(&user), listing, "nothing appeared beside it");

    let _ = fs::set_permissions(&user, fs::Permissions::from_mode(0o755));
    let _ = fs::set_permissions(&subtitle, fs::Permissions::from_mode(0o644));
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Structural: the crate's own source is the evidence. See the M4 design, section 5.
// ---------------------------------------------------------------------------

/// Every `.rs` file under the crate's `src/`, as (path below `src/`, source text). Recursive, in
/// the shape `crates/sublore-asr/tests/no_network.rs` uses: a module in a subdirectory is source
/// like any other and the guards below must see it. See BACKLOG.md N9, S10.
fn crate_sources() -> Vec<(String, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    visit(&src, &src, &mut found);
    found.sort();
    assert!(
        found.len() >= 8,
        "the crate's modules should all be there: {found:?}"
    );
    found
}

fn visit(root: &Path, dir: &Path, sink: &mut Vec<(String, String)>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", dir.display()));
    for entry in entries {
        let path = entry.expect("a directory entry should be readable").path();
        if path.is_dir() {
            visit(root, &path, sink);
        } else if path.extension().is_some_and(|kind| kind == "rs") {
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
            sink.push((name, text));
        }
    }
}

fn occurrences(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

#[test]
fn only_delete_rs_may_remove_a_file() {
    // Everything that creates, replaces, copies or removes something on disk. `fs::copy` and
    // `fs::write` are on the list because M4.2 forbids copying a user's file just as flatly as
    // M4.3 forbids deleting one. A renaming import defeats a name list, so this is a floor.
    let patterns = [
        "remove_file",
        "remove_dir_all",
        "remove_dir",
        "rename",
        "File::create",
        "OpenOptions",
        "fs::copy",
        "fs::write",
    ];

    for (name, text) in crate_sources() {
        if name == "delete.rs" {
            continue;
        }
        for pattern in patterns {
            let count = occurrences(&text, pattern);
            // db.rs claims the database's path with `create_new` and undoes its own half-made
            // file; those two are the only exceptions in the crate. See BACKLOG.md M4.1.
            let allowed =
                usize::from(name == "db.rs" && matches!(pattern, "remove_file" | "OpenOptions"));
            assert_eq!(
                count, allowed,
                "{name} has {count} occurrence(s) of `{pattern}`, expected {allowed}: only \
                 delete.rs may touch the filesystem this way. See BACKLOG.md M4.3."
            );
        }
    }
}

#[test]
fn delete_rs_cannot_reach_a_path_the_user_gave_us() {
    let (_, text) = crate_sources()
        .into_iter()
        .find(|(name, _)| name == "delete.rs")
        .expect("delete.rs should be part of the crate");

    for forbidden in ["rusqlite", "episode_files", "SELECT", "read_dir", "WalkDir"] {
        assert_eq!(
            occurrences(&text, forbidden),
            0,
            "delete.rs must not mention `{forbidden}`: it deletes by name, and it can only do \
             that safely because it has no way to read a path out of the database. See \
             BACKLOG.md M4.3."
        );
    }

    // Names come from the frozen list, joined onto the folder. Nothing is built any other way.
    assert!(
        text.contains("OWNED_FILES"),
        "delete.rs removes the names in layout::OWNED_FILES and no others"
    );
    assert!(
        text.contains("symlink_metadata"),
        "delete.rs checks what an entry is without following it"
    );
}
