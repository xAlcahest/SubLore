//! Behavioural tests for M4.2, written from the acceptance criteria in BACKLOG.md: a project that
//! is created, filled and closed reopens with the same episodes and the same paths in the same
//! order; attaching a file records a path and never touches the file; and a database that is
//! corrupt or not a Sublore project fails with a stable error and is left exactly as it was.
//!
//! Real files in a scratch directory, in the style of `crates/sublore-io/tests/atomic_save.rs`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sublore_project::error::ProjectErrorKind;
use sublore_project::model::FileRole;
use sublore_project::records::{
    add_episode, attach_file, delete_episode, episodes, files, Project,
};

/// Subtitle bytes, not tidy text: a CRLF file and a non-ASCII one.
const SRT: &[u8] = b"1\r\n00:00:01,000 --> 00:00:02,000\r\nhello\r\n";
const SRT_IT: &[u8] = b"1\n00:00:01,000 --> 00:00:02,000\n\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e\n";
/// Not a real container, and it never needs to be: nothing here opens it.
const MKV: &[u8] = b"\x1a\x45\xdf\xa3 not really a matroska file, and no test may open it\n";

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
        "sublore-project-test-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// `root/project` for Sublore, `root/user` for files that are none of Sublore's business.
fn workspace(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = scratch(tag);
    let folder = root.join("project");
    let user = root.join("user");
    fs::create_dir_all(&folder).expect("the project folder should be creatable");
    fs::create_dir_all(&user).expect("the user directory should be creatable");
    (root, folder, user)
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

/// Name, bytes and modification time of everything in `dir`. The whole point of M4.2's second
/// criterion is that this is identical before and after Sublore has looked at the folder.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>, Option<SystemTime>)> {
    names_in(dir)
        .into_iter()
        .map(|name| {
            let path = dir.join(&name);
            let modified = fs::metadata(&path)
                .ok()
                .and_then(|meta| meta.modified().ok());
            (name, read_bytes(&path), modified)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Acceptance: create, add two episodes with files, close, reopen, everything is there.
// ---------------------------------------------------------------------------

#[test]
fn a_reopened_project_has_the_same_episodes_and_paths_in_the_same_order() {
    let (root, folder, user) = workspace("reopen");
    let media_one = write_bytes(&user.join("ep01.mkv"), MKV);
    let subs_one = write_bytes(&user.join("ep01.srt"), SRT);
    let target_one = write_bytes(&user.join("ep01.it.srt"), SRT_IT);
    let media_two = write_bytes(&user.join("ep02.mkv"), MKV);
    let subs_two = write_bytes(&user.join("ep02.srt"), SRT);

    let mut project =
        Project::create(&folder, "Kaiba", at(1_756_000_000)).expect("a project should be created");
    assert_eq!(project.summary().title, "Kaiba");
    assert_eq!(project.summary().folder, folder);
    assert_eq!(project.summary().episode_count, 0);

    let first = add_episode(&mut project, "Warp", at(1_756_000_001)).expect("episode one");
    let second = add_episode(&mut project, "Chroniko", at(1_756_000_002)).expect("episode two");
    assert_eq!((first.ordinal, second.ordinal), (1, 2));
    assert_eq!(project.summary().episode_count, 2);

    // Attached in this order on purpose: `files` must return attach order, not role order.
    attach_file(
        &mut project,
        first.id,
        FileRole::Media,
        &media_one,
        at(1_756_000_003),
    )
    .expect("the media file attaches");
    attach_file(
        &mut project,
        first.id,
        FileRole::Source,
        &subs_one,
        at(1_756_000_004),
    )
    .expect("the source file attaches");
    attach_file(
        &mut project,
        first.id,
        FileRole::Target,
        &target_one,
        at(1_756_000_005),
    )
    .expect("the target file attaches");
    attach_file(
        &mut project,
        second.id,
        FileRole::Media,
        &media_two,
        at(1_756_000_006),
    )
    .expect("the second media file attaches");
    attach_file(
        &mut project,
        second.id,
        FileRole::Source,
        &subs_two,
        at(1_756_000_007),
    )
    .expect("the second source file attaches");

    project.close().expect("the project should close cleanly");

    let reopened = Project::open(&folder).expect("the project should reopen");
    assert_eq!(reopened.summary().title, "Kaiba");
    assert_eq!(reopened.summary().episode_count, 2);
    assert_eq!(reopened.summary().database, folder.join("project.sublore"));

    let listed = episodes(&reopened).expect("the episodes should list");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0], first, "episode one survives whole");
    assert_eq!(listed[1], second, "episode two survives whole");

    let first_files = files(&reopened, first.id).expect("the first episode's files should list");
    assert_eq!(
        first_files
            .iter()
            .map(|file| (file.role, file.path.clone()))
            .collect::<Vec<_>>(),
        vec![
            (FileRole::Media, media_one.clone()),
            (FileRole::Source, subs_one.clone()),
            (FileRole::Target, target_one.clone()),
        ],
        "the paths and the attach order both survive"
    );
    assert_eq!(first_files[1].byte_length, Some(SRT.len() as u64));
    assert_eq!(first_files[1].added_at, 1_756_000_004);
    assert!(
        first_files[1].modified_at.is_some(),
        "a readable file records its modification time"
    );

    let second_files = files(&reopened, second.id).expect("the second episode's files should list");
    assert_eq!(
        second_files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>(),
        vec![media_two, subs_two]
    );

    reopened.close().expect("the reopened project closes");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ordinals_continue_after_a_reopen() {
    let (root, folder, _user) = workspace("ordinals");
    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    add_episode(&mut project, "one", at(1_756_000_001)).expect("episode one");
    add_episode(&mut project, "two", at(1_756_000_002)).expect("episode two");
    project.close().expect("closes");

    let mut reopened = Project::open(&folder).expect("reopens");
    let third = add_episode(&mut reopened, "three", at(1_756_000_003)).expect("episode three");
    assert_eq!(
        third.ordinal, 3,
        "the ordinal continues, it does not restart"
    );
    assert_eq!(reopened.summary().episode_count, 3);
    reopened.close().expect("closes");
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Acceptance: attaching records a path and metadata, and touches nothing.
// ---------------------------------------------------------------------------

#[test]
fn attaching_records_a_path_and_leaves_the_user_s_folder_exactly_as_it_was() {
    let (root, folder, user) = workspace("attach");
    let media = write_bytes(&user.join("ep01.mkv"), MKV);
    let subtitle = write_bytes(&user.join("ep01.srt"), SRT);
    write_bytes(&user.join("notes.txt"), b"the user's own notes\n");
    let before = snapshot(&user);

    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    let episode = add_episode(&mut project, "one", at(1_756_000_001)).expect("episode");
    let recorded = attach_file(
        &mut project,
        episode.id,
        FileRole::Media,
        &media,
        at(1_756_000_002),
    )
    .expect("the media file attaches");
    attach_file(
        &mut project,
        episode.id,
        FileRole::Source,
        &subtitle,
        at(1_756_000_003),
    )
    .expect("the subtitle attaches");

    assert_eq!(recorded.path, media, "the path is stored exactly as given");
    assert_eq!(recorded.byte_length, Some(MKV.len() as u64));
    assert_eq!(recorded.episode_id, episode.id);
    assert_eq!(recorded.added_at, 1_756_000_002);

    assert_eq!(
        snapshot(&user),
        before,
        "attaching may not add, remove, rewrite or restamp anything in the user's folder"
    );
    assert_eq!(
        names_in(&user),
        vec!["ep01.mkv", "ep01.srt", "notes.txt"],
        "no copy of the user's file appears beside it"
    );

    project.close().expect("closes");
    assert_eq!(
        names_in(&folder),
        vec!["project.sublore"],
        "a closed project is one file, and no copy of the user's media"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_path_is_stored_as_the_user_spelled_it() {
    let (root, folder, user) = workspace("spelling");
    write_bytes(&user.join("ep01.srt"), SRT);
    // The same file, spelled with a redundant component. Sublore records spelling, not identity.
    let awkward = user.join(".").join("ep01.srt");

    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    let episode = add_episode(&mut project, "one", at(1_756_000_001)).expect("episode");
    let recorded = attach_file(
        &mut project,
        episode.id,
        FileRole::Source,
        &awkward,
        at(1_756_000_002),
    )
    .expect("the file attaches");
    assert_eq!(
        recorded.path, awkward,
        "no canonicalisation, no symlink resolution"
    );

    project.close().expect("closes");
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Acceptance: a bad database fails readably and is left untouched.
// ---------------------------------------------------------------------------

/// Overwrite `count` bytes at `offset` without changing the file's length.
fn scribble(path: &Path, offset: usize, count: usize) {
    let mut bytes = read_bytes(path);
    assert!(
        bytes.len() >= offset + count,
        "the database is long enough to damage"
    );
    for byte in bytes.iter_mut().skip(offset).take(count) {
        *byte = 0x5a;
    }
    write_bytes(path, &bytes);
}

#[test]
fn a_corrupt_database_fails_with_a_stable_error_and_is_left_untouched() {
    let (root, folder, _user) = workspace("corrupt");
    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    add_episode(&mut project, "one", at(1_756_000_001)).expect("episode");
    project.close().expect("closes");

    let database = folder.join("project.sublore");
    // Page one, past the header: the application id still says Sublore, the content does not.
    scribble(&database, 100, 900);
    let before = read_bytes(&database);

    let error = Project::open(&folder).expect_err("a corrupt database must not open");
    assert_eq!(error.kind, ProjectErrorKind::DatabaseCorrupt);
    assert!(
        !error.detail.is_empty(),
        "the SQLite message is kept for logs"
    );
    assert_eq!(
        read_bytes(&database),
        before,
        "a refused open changes nothing"
    );
    assert_eq!(
        names_in(&folder),
        vec!["project.sublore"],
        "no journal is left behind"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_file_that_is_not_a_sublore_project_is_refused_and_left_untouched() {
    let (root, folder, _user) = workspace("foreign");
    let database = folder.join("project.sublore");

    // Another application's SQLite database: ours in every way except the application id.
    Project::create(&folder, "Series", at(1_756_000_000))
        .expect("created")
        .close()
        .expect("closes");
    let mut bytes = read_bytes(&database);
    bytes[68..72].copy_from_slice(&0x4f_54_48_52_u32.to_be_bytes());
    write_bytes(&database, &bytes);

    let error = Project::open(&folder).expect_err("another application's database must be refused");
    assert_eq!(error.kind, ProjectErrorKind::NotASubloreProject);
    assert_eq!(
        read_bytes(&database),
        bytes,
        "a refused open changes nothing"
    );

    for (label, content) in [
        ("a text file", b"this is not a database\n".to_vec()),
        ("an empty file", Vec::new()),
        ("a truncated header", read_bytes(&database)[..60].to_vec()),
    ] {
        write_bytes(&database, &content);
        let error = Project::open(&folder).expect_err(label);
        assert_eq!(
            error.kind,
            ProjectErrorKind::NotASubloreProject,
            "{label} is not a Sublore project"
        );
        assert_eq!(read_bytes(&database), content, "{label} is left untouched");
    }

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn opening_and_creating_report_what_the_folder_actually_is() {
    let (root, folder, _user) = workspace("folders");

    let error = Project::open(&folder).expect_err("an empty folder holds no project");
    assert_eq!(error.kind, ProjectErrorKind::NoProjectHere);

    Project::create(&folder, "Series", at(1_756_000_000))
        .expect("created")
        .close()
        .expect("closes");
    let before = read_bytes(&folder.join("project.sublore"));

    let error = Project::create(&folder, "Again", at(1_756_000_010))
        .expect_err("a second create must not overwrite the first project");
    assert_eq!(error.kind, ProjectErrorKind::AlreadyAProject);
    assert_eq!(
        read_bytes(&folder.join("project.sublore")),
        before,
        "the existing project is untouched"
    );

    let missing = root.join("nowhere");
    assert_eq!(
        Project::create(&missing, "Series", at(1_756_000_020))
            .expect_err("a missing folder cannot hold a project")
            .kind,
        ProjectErrorKind::FolderNotFound
    );
    assert!(!missing.exists(), "create never makes the folder");

    let a_file = write_bytes(&root.join("plain.txt"), b"not a folder\n");
    assert_eq!(
        Project::create(&a_file, "Series", at(1_756_000_030))
            .expect_err("a file is not a project folder")
            .kind,
        ProjectErrorKind::NotADirectory
    );
    assert_eq!(
        read_bytes(&a_file),
        b"not a folder\n",
        "the file is untouched"
    );

    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Attach refuses what it cannot record honestly.
// ---------------------------------------------------------------------------

#[test]
fn attach_refuses_a_path_it_cannot_record_honestly() {
    let (root, folder, user) = workspace("attach-refusals");
    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    let episode = add_episode(&mut project, "one", at(1_756_000_001)).expect("episode");
    let now = at(1_756_000_002);

    let cases: Vec<(&str, PathBuf, ProjectErrorKind)> = vec![
        (
            "an empty path",
            PathBuf::new(),
            ProjectErrorKind::InvalidPath,
        ),
        (
            "a relative path",
            PathBuf::from("ep01.srt"),
            ProjectErrorKind::PathNotAbsolute,
        ),
        (
            "a file that is not there",
            user.join("missing.srt"),
            ProjectErrorKind::FileNotFound,
        ),
        ("a directory", user.clone(), ProjectErrorKind::NotAFile),
    ];
    for (label, path, expected) in cases {
        let error =
            attach_file(&mut project, episode.id, FileRole::Source, &path, now).expect_err(label);
        assert_eq!(error.kind, expected, "{label} must be refused");
    }
    assert!(
        files(&project, episode.id).expect("lists").is_empty(),
        "a refused attach leaves no row"
    );
    assert_eq!(
        names_in(&user),
        Vec::<String>::new(),
        "a refused attach creates nothing"
    );

    project.close().expect("closes");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn attach_refuses_an_unknown_episode_and_a_duplicate() {
    let (root, folder, user) = workspace("attach-duplicates");
    let subtitle = write_bytes(&user.join("ep01.srt"), SRT);
    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    let episode = add_episode(&mut project, "one", at(1_756_000_001)).expect("episode");

    assert_eq!(
        attach_file(
            &mut project,
            episode.id + 999,
            FileRole::Source,
            &subtitle,
            at(1)
        )
        .expect_err("an episode that does not exist cannot hold a file")
        .kind,
        ProjectErrorKind::EpisodeNotFound
    );

    attach_file(&mut project, episode.id, FileRole::Source, &subtitle, at(2)).expect("attaches");
    assert_eq!(
        attach_file(&mut project, episode.id, FileRole::Target, &subtitle, at(3))
            .expect_err("the same path twice on one episode is a duplicate")
            .kind,
        ProjectErrorKind::DuplicateFile
    );
    assert_eq!(
        files(&project, episode.id).expect("lists").len(),
        1,
        "the refused attach left no row"
    );

    // The same path on a different episode is a different attachment, and is allowed.
    let other = add_episode(&mut project, "two", at(1_756_000_004)).expect("episode two");
    attach_file(&mut project, other.id, FileRole::Source, &subtitle, at(4))
        .expect("the same file may belong to two episodes");

    project.close().expect("closes");
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_utf8_is_refused_rather_than_mangled() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let (root, folder, user) = workspace("not-utf8");
    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    let episode = add_episode(&mut project, "one", at(1_756_000_001)).expect("episode");

    let mut bytes = user.as_os_str().as_bytes().to_vec();
    bytes.extend_from_slice(b"/ep\xff01.srt");
    let path = PathBuf::from(OsStr::from_bytes(&bytes));

    assert_eq!(
        attach_file(&mut project, episode.id, FileRole::Source, &path, at(2))
            .expect_err("a path this column cannot hold is refused, never replaced")
            .kind,
        ProjectErrorKind::PathNotUtf8
    );

    project.close().expect("closes");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn deleting_an_episode_removes_its_rows_and_nothing_else() {
    let (root, folder, user) = workspace("delete-episode");
    let subtitle = write_bytes(&user.join("ep01.srt"), SRT);
    let kept = write_bytes(&user.join("ep02.srt"), SRT);

    let mut project = Project::create(&folder, "Series", at(1_756_000_000)).expect("created");
    let first = add_episode(&mut project, "one", at(1)).expect("episode one");
    let second = add_episode(&mut project, "two", at(2)).expect("episode two");
    attach_file(&mut project, first.id, FileRole::Source, &subtitle, at(3)).expect("attaches");
    attach_file(&mut project, second.id, FileRole::Source, &kept, at(4)).expect("attaches");

    delete_episode(&mut project, first.id).expect("the episode is deleted");

    assert_eq!(
        episodes(&project).expect("lists"),
        vec![second.clone()],
        "only the other episode is left"
    );
    assert!(
        files(&project, first.id).expect("lists").is_empty(),
        "the attachment rows went with it"
    );
    assert_eq!(
        files(&project, second.id).expect("lists").len(),
        1,
        "the other episode's rows are untouched"
    );
    assert_eq!(project.summary().episode_count, 1);
    assert_eq!(
        delete_episode(&mut project, first.id)
            .expect_err("an episode that is already gone is not there to delete")
            .kind,
        ProjectErrorKind::EpisodeNotFound
    );

    assert_eq!(
        read_bytes(&subtitle),
        SRT,
        "the user's file is still on disk"
    );
    assert_eq!(read_bytes(&kept), SRT);
    project.close().expect("closes");
    let _ = fs::remove_dir_all(&root);
}
