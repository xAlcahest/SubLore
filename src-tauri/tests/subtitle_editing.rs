//! What the editing commands do, driven through their bodies rather than through IPC: the async
//! wrappers add a `spawn_blocking` and nothing else. Written from BACKLOG.md M2.3:
//!
//! > open the 2000-cue fixture, edit a cue's text, save, reopen, the edit is there and the rest is
//! > byte-identical; undo restores it.
//!
//! The E2E spec proves the same thing through the window; this proves it on the bytes, including
//! the refusals a GUI test cannot reach cheaply.

use std::fs;
use std::path::{Path, PathBuf};

use sublore_edit::plan::Edit;
use sublore_lib::subtitle::error::SubtitleErrorCode;
use sublore_lib::subtitle::{
    apply_edit, close_session, open_session, redo, save, save_as, save_current, session_state,
    undo, SessionSlot, SessionState, MAX_SUBTITLE_BYTES,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn fixture(relative: &str) -> PathBuf {
    let path = repo_root().join(relative);
    assert!(path.is_file(), "missing fixture {}", path.display());
    path
}

/// A scratch directory that removes itself. Every file this suite opens is a copy inside it: the
/// committed fixtures are read-only inputs (CONTRIBUTING.md §3.1).
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sublore-m23-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch directory");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn backups(&self) -> PathBuf {
        self.path.join("backups")
    }

    /// A working copy of a committed fixture, which is what the tests edit and save.
    fn copy_of(&self, relative: &str) -> PathBuf {
        let source = fixture(relative);
        let name = source.file_name().expect("fixture name");
        let destination = self.join(&name.to_string_lossy());
        fs::copy(&source, &destination).expect("scratch copy");
        destination
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn text(cue: usize, text: &str) -> Edit {
    Edit::SetText {
        cue,
        text: text.to_owned(),
    }
}

fn open(slot: &SessionSlot, path: &Path) -> sublore_lib::subtitle::SubtitleOpened {
    open_session(slot, &path.to_string_lossy())
        .unwrap_or_else(|error| panic!("{} must open: {error}", path.display()))
}

#[test]
fn opening_hands_the_ui_the_whole_list_and_a_clean_slate() {
    let scratch = Scratch::new("open");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");

    let opened = open(&slot, &copy);

    assert_eq!(opened.summary.format, "srt");
    assert_eq!(
        opened.summary.cue_count, 3,
        "the count the status line shows"
    );
    assert_eq!(opened.cues.len(), 3);
    assert_eq!(
        opened.cues.first().map(|row| row.text.as_str()),
        Some("The harbour was empty when we got there.")
    );
    assert_eq!(opened.cues.first().and_then(|row| row.number), Some(1));
    assert_eq!(opened.revision, 0);
    assert!(!opened.dirty);
    assert!(!opened.can_undo);
    assert!(!opened.can_redo);
    assert!(!opened.truncated);
}

#[test]
fn an_ass_file_lists_its_comment_events_and_still_counts_only_drawn_ones() {
    let scratch = Scratch::new("ass-comments");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/ass/clean/comments-and-semicolons.ass");

    let opened = open(&slot, &copy);

    assert_eq!(opened.cues.len(), 5, "every event is a row, comments too");
    assert_eq!(
        opened.summary.cue_count, 3,
        "the status line counts what a player draws"
    );
    assert_eq!(
        opened.cues.iter().filter(|row| row.comment).count(),
        2,
        "and the rows say which is which"
    );
}

#[test]
fn an_edit_returns_the_run_it_changed_and_nothing_else() {
    let scratch = Scratch::new("edit");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    open(&slot, &copy);

    let patch = apply_edit(&slot, 0, text(1, "Rewritten by the test.")).expect("accepted");

    assert_eq!(patch.revision, 1);
    assert_eq!(patch.from, 1);
    assert_eq!(patch.removed, 1);
    assert_eq!(
        patch.cues.first().map(|row| row.text.as_str()),
        Some("Rewritten by the test.")
    );
    assert_eq!(patch.cue_count, 3);
    assert!(patch.dirty);
    assert!(patch.can_undo);
    assert!(!patch.can_redo);
    assert!(!patch.truncated);

    // Refused because the caller is one revision behind: a stale click never edits a moved list.
    let stale = apply_edit(&slot, 0, text(2, "too late")).expect_err("the revision moved");
    assert_eq!(stale.code, SubtitleErrorCode::StaleRevision);
}

#[test]
fn the_editing_commands_refuse_what_they_cannot_do_and_say_which() {
    let scratch = Scratch::new("refusals");
    let slot = SessionSlot::default();

    assert_eq!(
        apply_edit(&slot, 0, text(0, "nothing is open"))
            .expect_err("no document")
            .code,
        SubtitleErrorCode::NoDocument
    );
    assert_eq!(
        undo(&slot, 0).expect_err("no document").code,
        SubtitleErrorCode::NoDocument
    );
    assert_eq!(
        save(&slot, 0, scratch.backups())
            .expect_err("no document")
            .code,
        SubtitleErrorCode::NoDocument
    );

    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    let before = fs::read(&copy).expect("copy read");
    open(&slot, &copy);

    assert_eq!(
        apply_edit(&slot, 0, text(99, "nowhere"))
            .expect_err("there is no cue 99")
            .code,
        SubtitleErrorCode::InvalidCue
    );
    assert_eq!(
        apply_edit(&slot, 0, text(0, "one\n\ntwo"))
            .expect_err("a blank line splits an SRT cue")
            .code,
        SubtitleErrorCode::UnwritableText
    );
    assert_eq!(
        apply_edit(&slot, 0, Edit::Merge { cue: 2 })
            .expect_err("the last cue has nothing to merge with")
            .code,
        SubtitleErrorCode::InvalidCue
    );
    assert_eq!(
        apply_edit(
            &slot,
            0,
            Edit::Split {
                cue: 0,
                text_offset: 5_000,
                at_ms: 3_000,
            },
        )
        .expect_err("there is no offset 5000 in that text")
        .code,
        SubtitleErrorCode::EditRefused
    );

    // Every one of those refusals left the file and the session exactly as they were.
    let saved = save(&slot, 0, scratch.backups()).expect("a clean save");
    assert_eq!(fs::read(&copy).expect("copy read"), before);
    assert!(saved.backup_path.is_some(), "an overwrite is backed up");
}

#[test]
fn text_bigger_than_sublore_can_re_open_is_refused_before_it_is_applied() {
    let scratch = Scratch::new("too-large");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    let before = fs::read(&copy).expect("copy read");
    open(&slot, &copy);

    let huge = "x".repeat(usize::try_from(MAX_SUBTITLE_BYTES).unwrap_or(0) + 1);
    let error = apply_edit(&slot, 0, text(0, &huge)).expect_err("past the open limit");

    assert_eq!(error.code, SubtitleErrorCode::TooLarge);
    assert_eq!(
        save(&slot, 0, scratch.backups())
            .expect("the session is untouched")
            .bytes_written,
        before.len() as u64
    );
}

#[test]
fn undo_and_redo_walk_the_bytes_back_and_forward() {
    let scratch = Scratch::new("undo");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-crlf.srt");
    let original = fs::read(&copy).expect("copy read");
    open(&slot, &copy);

    apply_edit(&slot, 0, text(0, "First.")).expect("accepted");
    apply_edit(&slot, 1, text(2, "Third.")).expect("accepted");

    let undone = undo(&slot, 2).expect("a step to undo");
    assert_eq!(undone.revision, 3);
    assert!(undone.can_redo);
    assert!(undone.dirty, "one edit is still applied");

    undo(&slot, 3).expect("a second step");
    save(&slot, 4, scratch.backups()).expect("a save");
    assert_eq!(
        fs::read(&copy).expect("copy read"),
        original,
        "undone all the way back is the file that was opened, byte for byte"
    );

    // A save writes bytes; it does not move the list, so the caller's revision still holds.
    let redone = redo(&slot, 4).expect("a step to redo");
    assert!(
        redone.dirty,
        "redoing past the saved position dirties again"
    );
    assert_eq!(
        redone.cues.first().map(|row| row.text.as_str()),
        Some("First.")
    );

    // Nothing left to redo is an answer, not a failure, and it does not move the revision.
    redo(&slot, 5).expect("the second step");
    let top = redo(&slot, 6).expect("the top of the stack");
    assert_eq!(top.revision, 6);
    assert!(!top.can_redo);
}

/// Regression: two commits of one cue arriving inside the coalescing window used to become one
/// undo step, so the state between them stopped existing. See BACKLOG.md M2.2.
#[test]
fn two_commits_of_one_cue_in_quick_succession_are_two_undo_steps() {
    let scratch = Scratch::new("commits");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-crlf.srt");
    let original = fs::read(&copy).expect("copy read");
    open(&slot, &copy);

    apply_edit(&slot, 0, text(0, "First draft.")).expect("accepted");
    apply_edit(&slot, 1, text(0, "Second, corrected draft.")).expect("accepted");

    let undone = undo(&slot, 2).expect("a step to undo");
    assert_eq!(
        undone.cues.first().map(|row| row.text.as_str()),
        Some("First draft."),
        "the first commit is a state undo can still reach"
    );
    assert!(undone.can_undo, "and the file as opened is below it");

    undo(&slot, 3).expect("a second step");
    save(&slot, 4, scratch.backups()).expect("a save");
    assert_eq!(
        fs::read(&copy).expect("copy read"),
        original,
        "two commits, two undos, the file that was opened"
    );
}

#[test]
fn saving_writes_the_edit_where_the_file_came_from_and_keeps_a_backup() {
    let scratch = Scratch::new("save");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    let original = fs::read(&copy).expect("copy read");
    open(&slot, &copy);
    apply_edit(&slot, 0, text(1, "Saved text.")).expect("accepted");

    let saved = save(&slot, 1, scratch.backups()).expect("a save");

    assert_eq!(Path::new(&saved.path), copy.as_path());
    let backup = saved.backup_path.expect("overwriting keeps a backup");
    assert_eq!(fs::read(&backup).expect("backup read"), original);
    assert!(
        Path::new(&backup).starts_with(scratch.backups()),
        "backups stay in Sublore's own directory: {backup}"
    );

    let written = fs::read(&copy).expect("copy read");
    assert!(String::from_utf8_lossy(&written).contains("Saved text."));

    // Saved means clean: closing without discarding is allowed, and re-opening is too.
    close_session(&slot, false).expect("a saved file closes");
    let reopened = open(&slot, &copy);
    assert_eq!(
        reopened.cues.get(1).map(|row| row.text.as_str()),
        Some("Saved text."),
        "the edit is there when the file is opened again"
    );
}

#[test]
fn save_as_writes_a_copy_and_leaves_the_file_being_edited_unsaved() {
    let scratch = Scratch::new("save-as");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    let original = fs::read(&copy).expect("copy read");
    let elsewhere = scratch.join("elsewhere.srt");
    open(&slot, &copy);
    apply_edit(&slot, 0, text(1, "Only in the copy.")).expect("accepted");

    let saved = save_as(&slot, 1, &elsewhere.to_string_lossy(), scratch.backups()).expect("a copy");

    assert_eq!(Path::new(&saved.path), elsewhere.as_path());
    assert!(
        String::from_utf8_lossy(&fs::read(&elsewhere).expect("copy read"))
            .contains("Only in the copy.")
    );
    assert_eq!(
        fs::read(&copy).expect("source read"),
        original,
        "save-as does not write the file the session was opened from"
    );

    // The file being edited still has unsaved work, and closing it must say so.
    assert_eq!(
        close_session(&slot, false)
            .expect_err("the edit is still unsaved")
            .code,
        SubtitleErrorCode::UnsavedChanges
    );
}

#[test]
fn opening_another_file_over_unsaved_work_is_refused_until_it_is_discarded() {
    let scratch = Scratch::new("unsaved");
    let slot = SessionSlot::default();
    let first = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    let second = scratch.copy_of("fixtures/subtitles/vtt/clean/basic.vtt");
    open(&slot, &first);
    apply_edit(&slot, 0, text(0, "Unsaved.")).expect("accepted");

    let refused = open_session(&slot, &second.to_string_lossy()).expect_err("unsaved work");
    assert_eq!(refused.code, SubtitleErrorCode::UnsavedChanges);
    assert_eq!(
        undo(&slot, 1)
            .expect("the first file is still the open one")
            .from,
        0,
        "the refusal did not close anything"
    );

    // Discarding is the user's decision, taken through its own command.
    apply_edit(&slot, 2, text(0, "Unsaved again.")).expect("accepted");
    close_session(&slot, true).expect("discarding is allowed");
    let opened = open(&slot, &second);
    assert_eq!(opened.summary.format, "vtt");
    assert_eq!(
        fs::read(&first).expect("first read"),
        fs::read(fixture("fixtures/subtitles/srt/clean/basic-lf.srt")).expect("fixture read"),
        "discarding an edit never writes it"
    );
}

#[test]
fn the_two_thousand_cue_fixture_edits_saves_and_reopens_with_every_other_byte_identical() {
    let scratch = Scratch::new("large");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/large-2000.srt");
    let original = fs::read(&copy).expect("copy read");
    let opened = open(&slot, &copy);
    assert_eq!(opened.cues.len(), 2_000);
    let replaced = opened
        .cues
        .get(42)
        .map(|row| row.text.clone())
        .expect("cue 42 exists");

    apply_edit(&slot, 0, text(42, "Edited line forty-three.")).expect("accepted");
    save(&slot, 1, scratch.backups()).expect("a save");

    // Block by block, because this fixture repeats its lines: only the one that was edited may
    // differ, and it may differ only in its text.
    let written = String::from_utf8(fs::read(&copy).expect("copy read")).expect("utf-8");
    let before = String::from_utf8(original.clone()).expect("utf-8");
    let old_blocks: Vec<&str> = before.split("\n\n").collect();
    let new_blocks: Vec<&str> = written.split("\n\n").collect();
    assert_eq!(
        old_blocks.len(),
        new_blocks.len(),
        "no block was added or lost"
    );
    for (index, (was, is)) in old_blocks.iter().zip(new_blocks.iter()).enumerate() {
        if index == 42 {
            assert_eq!(
                *was,
                format!("43\n00:01:46,000 --> 00:01:48,000\n{replaced}")
            );
            assert_eq!(
                *is,
                "43\n00:01:46,000 --> 00:01:48,000\nEdited line forty-three."
            );
        } else {
            assert_eq!(was, is, "block {index} was not the one edited");
        }
    }

    // Reopened from disk, the edit is there and the list is otherwise the list that was there.
    close_session(&slot, false).expect("a saved file closes");
    let reopened = open(&slot, &copy);
    assert_eq!(reopened.cues.len(), 2_000);
    assert_eq!(
        reopened.cues.get(42).map(|row| row.text.as_str()),
        Some("Edited line forty-three.")
    );
    assert_eq!(
        reopened.cues.first().map(|row| row.text.as_str()),
        opened.cues.first().map(|row| row.text.as_str())
    );
    assert_eq!(
        reopened.cues.last().map(|row| row.text.as_str()),
        opened.cues.last().map(|row| row.text.as_str())
    );
}

#[test]
fn undo_puts_the_file_back_the_way_it_was_before_the_save() {
    let scratch = Scratch::new("undo-save");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/large-2000.srt");
    let original = fs::read(&copy).expect("copy read");
    open(&slot, &copy);

    apply_edit(&slot, 0, text(42, "Edited line forty-three.")).expect("accepted");
    save(&slot, 1, scratch.backups()).expect("a save");
    assert_ne!(fs::read(&copy).expect("copy read"), original);

    undo(&slot, 1).expect("a step to undo");
    save(&slot, 2, scratch.backups()).expect("a second save");
    assert_eq!(
        fs::read(&copy).expect("copy read"),
        original,
        "undo and save restore the file exactly"
    );
}

#[test]
fn every_mutation_the_ipc_offers_reaches_the_model() {
    let scratch = Scratch::new("mutations");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    open(&slot, &copy);

    let times = apply_edit(
        &slot,
        0,
        Edit::SetTimes {
            cue: 0,
            start_ms: 2_500,
            end_ms: 4_000,
        },
    )
    .expect("times are editable");
    assert_eq!(times.cues.first().map(|row| row.start_ms), Some(2_500));

    let inserted = apply_edit(
        &slot,
        1,
        Edit::Insert {
            before: 1,
            start_ms: 4_100,
            end_ms: 4_500,
            text: "Wedged in.".to_owned(),
        },
    )
    .expect("an insert is accepted");
    assert_eq!(inserted.cue_count, 4);

    let split = apply_edit(
        &slot,
        2,
        Edit::Split {
            cue: 1,
            text_offset: 6,
            at_ms: 4_300,
        },
    )
    .expect("a split is accepted");
    assert_eq!(split.cue_count, 5);

    let merged = apply_edit(&slot, 3, Edit::Merge { cue: 1 }).expect("a merge is accepted");
    assert_eq!(merged.cue_count, 4);

    let deleted = apply_edit(&slot, 4, Edit::Delete { cue: 1 }).expect("a delete is accepted");
    assert_eq!(deleted.cue_count, 3);

    // Five mutations, five undo steps, and the file that comes back is the file that went in.
    for revision in 5..10 {
        undo(&slot, revision).expect("a step to undo");
    }
    save(&slot, 10, scratch.backups()).expect("a save");
    assert_eq!(
        fs::read(&copy).expect("copy read"),
        fs::read(fixture("fixtures/subtitles/srt/clean/basic-lf.srt")).expect("fixture read")
    );
}

// ---------------------------------------------------------------------------------------------
// The close gate's two entry points (BACKLOG N1). The gate is the last thing standing between a
// panic and the user's work, so its reads and its write are proved here, not assumed.
// ---------------------------------------------------------------------------------------------

#[test]
fn session_state_reports_clean_dirty_and_no_document() {
    let scratch = Scratch::new("gate-state");
    let slot = SessionSlot::default();
    assert_eq!(
        session_state(&slot),
        SessionState::Clean,
        "no document open is nothing to lose"
    );

    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    open(&slot, &copy);
    assert_eq!(session_state(&slot), SessionState::Clean);

    apply_edit(&slot, 0, text(0, "Edited.")).expect("accepted");
    assert_eq!(session_state(&slot), SessionState::Dirty);

    save(&slot, 1, scratch.backups()).expect("a save");
    assert_eq!(session_state(&slot), SessionState::Clean);
}

#[test]
fn session_state_answers_unknown_rather_than_waiting_for_a_held_lock() {
    let slot = SessionSlot::default();
    let held = slot.lock().expect("fresh lock");
    // The gate runs on the main loop and this mutex is held across file I/O: waiting here would
    // freeze the window, and an unknown answer sends the user to the dialog instead.
    assert_eq!(session_state(&slot), SessionState::Unknown);
    drop(held);
}

#[test]
fn save_current_writes_without_a_caller_revision() {
    let scratch = Scratch::new("gate-save");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    let original = fs::read(&copy).expect("copy read");
    open(&slot, &copy);
    apply_edit(&slot, 0, text(0, "Saved by the gate.")).expect("accepted");

    save_current(&slot, scratch.backups())
        .expect("the gate saves")
        .expect("a dirty session writes");

    let written = fs::read(&copy).expect("copy read");
    assert_ne!(written, original, "the edit reached the file");
    assert!(
        String::from_utf8(written)
            .expect("utf-8")
            .contains("Saved by the gate."),
        "the edit that reached the file is the one that was made"
    );
    assert_eq!(session_state(&slot), SessionState::Clean);
}

#[test]
fn save_current_still_saves_through_a_poisoned_lock() {
    let scratch = Scratch::new("gate-poison");
    let slot = std::sync::Arc::new(SessionSlot::default());
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    open(&slot, &copy);
    apply_edit(&slot, 0, text(0, "Work worth keeping.")).expect("accepted");

    // Poison it the way a panicking command would.
    let poisoner = std::sync::Arc::clone(&slot);
    let _ = std::thread::spawn(move || {
        let _guard = poisoner.lock().expect("held");
        panic!("a command died holding the session lock");
    })
    .join();
    assert!(slot.is_poisoned(), "the lock is poisoned");

    // The gate exists for exactly this moment: a save that refuses here loses the work the dialog
    // just promised to keep.
    save_current(&slot, scratch.backups())
        .expect("the gate saves through the poison")
        .expect("a dirty session writes");

    let written = String::from_utf8(fs::read(&copy).expect("copy read")).expect("utf-8");
    assert!(
        written.contains("Work worth keeping."),
        "the edit survived the poisoned lock"
    );
}

#[test]
fn save_current_writes_nothing_when_the_session_is_clean() {
    let scratch = Scratch::new("gate-clean");
    let slot = SessionSlot::default();
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    let before = fs::metadata(&copy)
        .expect("copy")
        .modified()
        .expect("mtime");
    open(&slot, &copy);

    // The gate can open on a session that is merely busy. Writing then would change the mtime of a
    // file the user only opened, and would overwrite whatever another program put there since.
    let outcome = save_current(&slot, scratch.backups()).expect("no error");
    assert!(outcome.is_none(), "a clean session writes nothing");
    assert_eq!(
        fs::metadata(&copy)
            .expect("copy")
            .modified()
            .expect("mtime"),
        before,
        "the file the user only opened was not touched"
    );
}

#[test]
fn save_current_clears_the_poison_so_the_app_stays_usable() {
    let scratch = Scratch::new("gate-unpoison");
    let slot = std::sync::Arc::new(SessionSlot::default());
    let copy = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    open(&slot, &copy);
    apply_edit(&slot, 0, text(0, "Kept.")).expect("accepted");

    let poisoner = std::sync::Arc::clone(&slot);
    let _ = std::thread::spawn(move || {
        let _guard = poisoner.lock().expect("held");
        panic!("a command died holding the session lock");
    })
    .join();

    save_current(&slot, scratch.backups())
        .expect("the gate saves")
        .expect("a dirty session writes");

    // Without this the window survives a Cancel as a brick: every later command refuses on a
    // poison flag that the successful save just proved harmless.
    assert!(!slot.is_poisoned(), "the poison was cleared");
    // Saving does not advance the revision: the document did not change, only the disk did.
    apply_edit(&slot, 1, text(0, "Still editable.")).expect("the session still takes edits");
}
