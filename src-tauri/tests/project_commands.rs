//! What the project commands do, driven through their bodies rather than through IPC: the async
//! wrappers add a `spawn_blocking` and nothing else. The E2E spec covers the app; this covers the
//! outcomes a GUI test cannot see cheaply, above all what happens to the user's own files.
//! See BACKLOG.md M4.2, M4.3 and M4.4.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use sublore_lib::project::error::ProjectErrorCode;
use sublore_lib::project::{
    add_episode, attach_file, close, create, delete, open, ProjectView, SharedProject,
};

/// A scratch directory that removes itself, so a failed assertion never leaves litter behind.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sublore-m44-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch directory");
        Self { path }
    }

    /// A directory inside the scratch, created on the spot.
    fn dir(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::create_dir_all(&path).expect("scratch subdirectory");
        path
    }

    /// A file with known bytes, standing in for something of the user's.
    fn file(&self, relative: &str, bytes: &str) -> PathBuf {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory");
        }
        fs::write(&path, bytes).expect("writing a user file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// One app run's worth of state: the slot the commands read the open project out of.
fn app() -> SharedProject {
    SharedProject::default()
}

/// Size, bytes and modification time: everything that would change if Sublore wrote to a file.
fn snapshot(path: &Path) -> (Vec<u8>, u64, std::time::SystemTime) {
    let metadata = fs::metadata(path).expect("the user's file is there");
    (
        fs::read(path).expect("reading the user's file"),
        metadata.len(),
        metadata.modified().expect("a modification time"),
    )
}

fn entries(folder: &Path) -> BTreeSet<String> {
    fs::read_dir(folder)
        .expect("listing a directory")
        .map(|entry| entry.expect("a directory entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect()
}

fn only_episode(view: &ProjectView) -> &sublore_lib::project::EpisodeView {
    assert_eq!(view.episodes.len(), 1, "expected one episode: {view:?}");
    &view.episodes[0]
}

#[test]
fn a_project_survives_being_closed_and_opened_again_with_its_episodes_and_paths() {
    let scratch = Scratch::new("round-trip");
    let folder = scratch.dir("Series One");
    let media = scratch.file("user/ep01.mkv", "not really a video");
    let subtitle = scratch.file("user/ep01.srt", "1\n00:00:01,000 --> 00:00:02,000\nHello\n");

    let slot = app();
    let created = create(&slot, &text(&folder)).expect("a project is created");
    assert_eq!(created.title, "Series One");
    assert_eq!(created.folder, text(&folder));
    assert_eq!(created.schema_version, 1);
    assert!(created.episodes.is_empty());

    add_episode(&slot, "Pilot").expect("the first episode is added");
    let view = add_episode(&slot, "Второй").expect("the second episode is added");
    assert_eq!(view.episodes.len(), 2);
    let second = view.episodes[1].id;
    attach_file(&slot, second, "media", &text(&media)).expect("the video is attached");
    let view = attach_file(&slot, second, "source", &text(&subtitle)).expect("the subtitle too");

    // What the app shows before the restart, so the comparison after it is value for value.
    assert_eq!(view.episodes[0].title, "Pilot");
    assert_eq!(view.episodes[0].ordinal, 1);
    assert_eq!(view.episodes[1].title, "Второй");
    assert_eq!(view.episodes[1].ordinal, 2);
    assert_eq!(view.episodes[1].files.len(), 2);
    assert_eq!(view.episodes[1].files[0].role, "media");
    assert_eq!(view.episodes[1].files[0].path, text(&media));
    assert_eq!(view.episodes[1].files[1].role, "source");
    assert_eq!(view.episodes[1].files[1].path, text(&subtitle));

    // A second slot is a second app run: nothing of the first one is in memory any more.
    let restarted = app();
    let reopened = open(&restarted, &text(&folder)).expect("the project opens again");
    assert_eq!(
        reopened, view,
        "the reopened project differs from the one that was closed"
    );
}

#[test]
fn the_only_file_a_project_writes_is_its_own_database() {
    let scratch = Scratch::new("owned-files");
    let folder = scratch.dir("Series");
    let slot = app();
    create(&slot, &text(&folder)).expect("a project is created");
    add_episode(&slot, "Pilot").expect("an episode is added");

    let names = entries(&folder);
    assert!(names.contains("project.sublore"), "{names:?}");
    for name in &names {
        assert!(
            name.starts_with("project.sublore"),
            "Sublore left {name} in the user's folder"
        );
    }
}

#[test]
fn attaching_records_a_path_and_leaves_the_user_s_file_and_folder_untouched() {
    let scratch = Scratch::new("attach-safety");
    let folder = scratch.dir("Series");
    let media = scratch.file("user/ep01.mkv", "the user's only copy");
    let user_folder = media
        .parent()
        .expect("the media has a parent")
        .to_path_buf();
    let before = snapshot(&media);
    let listing = entries(&user_folder);

    let slot = app();
    create(&slot, &text(&folder)).expect("a project is created");
    let view = add_episode(&slot, "Pilot").expect("an episode is added");
    let view = attach_file(&slot, only_episode(&view).id, "media", &text(&media))
        .expect("the video is attached");

    let file = &only_episode(&view).files[0];
    assert_eq!(file.path, text(&media));
    assert_eq!(file.byte_length, Some(before.1));

    // CONTRIBUTING.md §3.1: source media is read-only. Bytes, size, modification time, and the folder
    // listing are all exactly what they were.
    assert_eq!(
        snapshot(&media),
        before,
        "attaching changed the user's file"
    );
    assert_eq!(entries(&user_folder), listing);
}

#[test]
fn a_command_that_needs_a_project_says_so_instead_of_guessing() {
    let scratch = Scratch::new("nothing-open");
    let media = scratch.file("user/ep01.mkv", "bytes");
    let slot = app();

    assert_eq!(
        add_episode(&slot, "Pilot")
            .expect_err("no project is open")
            .code,
        ProjectErrorCode::NoProjectOpen
    );
    assert_eq!(
        attach_file(&slot, 1, "media", &text(&media))
            .expect_err("no project is open")
            .code,
        ProjectErrorCode::NoProjectOpen
    );
    assert_eq!(
        delete(&slot).expect_err("no project is open").code,
        ProjectErrorCode::NoProjectOpen
    );
}

#[test]
fn every_way_of_naming_a_folder_wrong_gets_its_own_answer() {
    let scratch = Scratch::new("folder-errors");
    let empty = scratch.dir("empty");
    let file = scratch.file("a-file", "not a folder");
    let slot = app();

    for folder in ["", "   "] {
        assert_eq!(
            create(&slot, folder)
                .expect_err("an empty path is refused")
                .code,
            ProjectErrorCode::InvalidPath,
            "{folder:?}"
        );
    }
    assert_eq!(
        open(&slot, &text(&empty))
            .expect_err("there is no project there")
            .code,
        ProjectErrorCode::NoProjectHere
    );
    assert_eq!(
        open(&slot, &text(&scratch.path.join("nowhere")))
            .expect_err("there is no such folder")
            .code,
        ProjectErrorCode::FolderNotFound
    );
    assert_eq!(
        create(&slot, &text(&file))
            .expect_err("that is not a folder")
            .code,
        ProjectErrorCode::NotADirectory
    );
    // A failed open creates nothing: the folder the user named is exactly as they left it.
    assert!(entries(&empty).is_empty(), "{:?}", entries(&empty));

    create(&slot, &text(&empty)).expect("a project is created");
    assert_eq!(
        create(&slot, &text(&empty))
            .expect_err("a second project is refused")
            .code,
        ProjectErrorCode::AlreadyAProject
    );
}

#[test]
fn every_way_of_naming_a_file_wrong_gets_its_own_answer() {
    let scratch = Scratch::new("file-errors");
    let folder = scratch.dir("Series");
    let media = scratch.file("user/ep01.mkv", "bytes");
    let directory = scratch.dir("user");

    let slot = app();
    create(&slot, &text(&folder)).expect("a project is created");
    let view = add_episode(&slot, "Pilot").expect("an episode is added");
    let episode = only_episode(&view).id;

    for (path, expected) in [
        ("", ProjectErrorCode::InvalidPath),
        ("user/ep01.mkv", ProjectErrorCode::PathNotAbsolute),
    ] {
        assert_eq!(
            attach_file(&slot, episode, "media", path)
                .expect_err("refused")
                .code,
            expected,
            "{path:?}"
        );
    }
    assert_eq!(
        attach_file(
            &slot,
            episode,
            "media",
            &text(&scratch.path.join("gone.mkv"))
        )
        .expect_err("there is no such file")
        .code,
        ProjectErrorCode::FileNotFound
    );
    assert_eq!(
        attach_file(&slot, episode, "media", &text(&directory))
            .expect_err("a directory is not a file")
            .code,
        ProjectErrorCode::NotAFile
    );
    assert_eq!(
        attach_file(&slot, episode + 1000, "media", &text(&media))
            .expect_err("there is no such episode")
            .code,
        ProjectErrorCode::EpisodeNotFound
    );
    // A role the frontend never sends is our bug, not a situation the user created.
    assert_eq!(
        attach_file(&slot, episode, "reference", &text(&media))
            .expect_err("an unknown role is refused")
            .code,
        ProjectErrorCode::CommandFailed
    );

    attach_file(&slot, episode, "media", &text(&media)).expect("the video is attached");
    assert_eq!(
        attach_file(&slot, episode, "media", &text(&media))
            .expect_err("the same file twice is refused")
            .code,
        ProjectErrorCode::DuplicateFile
    );
}

#[test]
fn deleting_a_project_removes_sublore_s_own_files_and_nothing_of_the_user_s() {
    let scratch = Scratch::new("delete");
    let folder = scratch.dir("Series");
    let media = scratch.file("user/ep01.mkv", "the user's only copy of the video");
    let subtitle = scratch.file("user/ep01.srt", "1\n00:00:01,000 --> 00:00:02,000\nHello\n");
    // Two files the user keeps in the project folder itself, which Sublore did not create.
    let notes = scratch.file("Series/notes.txt", "my glossary notes");
    let stray = scratch.file("Series/project.sublore.bak", "a backup the user made");

    let before = [
        snapshot(&media),
        snapshot(&subtitle),
        snapshot(&notes),
        snapshot(&stray),
    ];

    let slot = app();
    create(&slot, &text(&folder)).expect("a project is created");
    let view = add_episode(&slot, "Pilot").expect("an episode is added");
    let episode = only_episode(&view).id;
    attach_file(&slot, episode, "media", &text(&media)).expect("the video is attached");
    attach_file(&slot, episode, "source", &text(&subtitle)).expect("the subtitle is attached");

    let outcome = delete(&slot).expect("the project is deleted");
    assert_eq!(outcome.folder, text(&folder));
    assert!(
        outcome
            .removed
            .contains(&text(&folder.join("project.sublore"))),
        "{outcome:?}"
    );
    for removed in &outcome.removed {
        assert!(
            Path::new(removed)
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("project.sublore")),
            "Sublore removed {removed}"
        );
    }

    // The acceptance criterion for M4.3: every file the project pointed at is still there, byte
    // for byte, and so is everything the user kept in the folder.
    assert_eq!(
        [
            snapshot(&media),
            snapshot(&subtitle),
            snapshot(&notes),
            snapshot(&stray)
        ],
        before,
        "deleting the project changed a file that was not Sublore's"
    );
    assert!(folder.is_dir(), "the folder the user chose was removed");
    assert!(!folder.join("project.sublore").exists());
    assert_eq!(
        entries(&folder),
        ["notes.txt".to_owned(), "project.sublore.bak".to_owned()]
            .into_iter()
            .collect()
    );

    // Nothing is open afterwards, so the next command says so rather than acting on a dead handle.
    assert_eq!(
        add_episode(&slot, "Second")
            .expect_err("the project is gone")
            .code,
        ProjectErrorCode::NoProjectOpen
    );
}

#[test]
fn opening_another_project_replaces_the_one_that_was_open() {
    let scratch = Scratch::new("replace");
    let first = scratch.dir("First");
    let second = scratch.dir("Second");

    let slot = app();
    create(&slot, &text(&first)).expect("the first project is created");
    add_episode(&slot, "Pilot").expect("an episode is added");
    let view = create(&slot, &text(&second)).expect("the second project is created");
    assert_eq!(view.title, "Second");
    assert!(
        view.episodes.is_empty(),
        "the first project's episodes leaked in: {view:?}"
    );

    let view = open(&slot, &text(&first)).expect("the first project opens again");
    assert_eq!(view.title, "First");
    assert_eq!(view.episodes.len(), 1);
}

#[test]
fn a_folder_holding_someone_else_s_file_named_like_ours_is_refused_and_left_alone() {
    let scratch = Scratch::new("foreign");
    let folder = scratch.dir("Series");
    let planted = scratch.file("Series/project.sublore", "this is not a database at all");
    let before = snapshot(&planted);

    let slot = app();
    let error = open(&slot, &text(&folder)).expect_err("that is not a Sublore project");
    assert_eq!(error.code, ProjectErrorCode::NotASubloreProject);
    assert_eq!(
        snapshot(&planted),
        before,
        "the file was altered while being refused"
    );
}

#[test]
fn closing_a_project_leaves_one_file_behind_and_it_opens_again() {
    let scratch = Scratch::new("close");
    let folder = scratch.dir("Series");
    let media = scratch.file("user/ep01.mkv", "bytes");

    let slot = app();
    create(&slot, &text(&folder)).expect("a project is created");
    let view = add_episode(&slot, "Pilot").expect("an episode is added");
    let view = attach_file(&slot, only_episode(&view).id, "media", &text(&media))
        .expect("the video is attached");

    close(&slot).expect("the project closes");
    // The claim the shutdown path makes: a clean close checkpoints the WAL, so what is left on the
    // user's disk is one file, not three.
    assert_eq!(
        entries(&folder),
        ["project.sublore".to_owned()].into_iter().collect()
    );

    let reopened = open(&app(), &text(&folder)).expect("the project opens again");
    assert_eq!(reopened, view);
}

#[test]
fn closing_when_nothing_is_open_is_not_a_failure() {
    let slot = app();
    close(&slot).expect("closing nothing is fine");
    close(&slot).expect("and closing it twice is fine too");
}
