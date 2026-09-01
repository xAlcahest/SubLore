//! What a finished transcription does to the open document, driven through the command bodies
//! rather than through IPC: the async wrapper adds the question a person answers, and nothing else.
//! Written from BACKLOG.md M3.5.
//!
//! The cues are the committed capture of a real whisper run, rendered by the same `done_payload`
//! the app stores, so what is adopted here is what the app adopts.

use std::fs;
use std::path::{Path, PathBuf};

use sublore_asr::json::parse_transcript;
use sublore_asr::transcript::{Backend, Transcript};
use sublore_edit::plan::Edit;
use sublore_lib::dialog::CloseAnswer;
use sublore_lib::subtitle::error::SubtitleErrorCode;
use sublore_lib::subtitle::{
    adopt_answered, adopt_if_clean, apply_edit, open_session, save, save_as, save_current,
    save_current_as, SessionSlot,
};

/// The fixture the E2E spec transcribes is 60 s long; cue times are clamped to it.
const AUDIO_MS: u32 = 60_000;

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

/// A scratch directory that removes itself. The committed fixtures are read-only inputs
/// (CONTRIBUTING.md §3.1), so everything written here is a copy inside it.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sublore-m35-{name}-{}", std::process::id()));
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

/// The SRT a real run leaves behind, through the same call the app stores it from.
fn transcribed_srt() -> Vec<u8> {
    let path = repo_root().join("fixtures/asr/whisper-tiny-en.json");
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let parsed = parse_transcript(&bytes).expect("the committed capture parses");
    sublore_lib::asr::done_payload(
        1,
        Transcript {
            language: parsed.language,
            words: parsed.words,
            backend: Backend::Cpu,
            fell_back_to_cpu: false,
            audio_duration_ms: AUDIO_MS,
        },
    )
    .expect("the capture segments and parses")
    .srt
}

fn text(cue: usize, text: &str) -> Edit {
    Edit::SetText {
        cue,
        text: text.to_owned(),
    }
}

/// A session holding the fixture with one cue edited, which is unsaved work in the way.
fn edited_document(slot: &SessionSlot, path: &Path) {
    open_session(slot, &path.to_string_lossy()).expect("the fixture opens");
    apply_edit(slot, 0, text(0, "Edited and not saved")).expect("the edit lands");
}

#[test]
fn a_finished_transcription_becomes_the_open_document_and_touches_no_disk() {
    let slot = SessionSlot::default();
    let srt = transcribed_srt();

    let opened = adopt_if_clean(&slot, &srt)
        .expect("nothing is open, so nothing is in the way")
        .expect("no question needed");

    assert!(opened.cues.len() > 1, "the capture is more than one cue");
    assert_eq!(opened.summary.path, None, "it has never had a file");
    assert_eq!(opened.summary.format, "srt");
    assert_eq!(opened.revision, 0);
    assert!(!opened.can_undo);
    assert!(
        opened.dirty,
        "these bytes exist nowhere else, so it is unsaved work from the start"
    );
    assert!(
        opened
            .cues
            .iter()
            .any(|cue| cue.text.contains("terminology")),
        "the cues are the ones whisper produced: {:?}",
        opened.cues.first()
    );
}

#[test]
fn the_adopted_document_edits_and_saves_like_any_other_and_the_edit_reaches_disk() {
    let scratch = Scratch::new("adopted-save");
    let slot = SessionSlot::default();
    adopt_if_clean(&slot, &transcribed_srt())
        .expect("nothing is in the way")
        .expect("no question needed");

    let patch = apply_edit(&slot, 0, text(0, "Corrected by hand")).expect("the same editing API");
    assert_eq!(patch.cues[0].text, "Corrected by hand");
    assert!(patch.can_undo, "the same undo the editor uses");

    // Nowhere to write it back to until a destination is named, which is what the save chooser asks
    // the user for (decision 24, B2).
    let refused = save(&slot, patch.revision, scratch.backups())
        .expect_err("a document with no file cannot be saved in place");
    assert_eq!(refused.code, SubtitleErrorCode::NoPath);

    let destination = scratch.join("from-transcription.srt");
    let written = save_as(
        &slot,
        patch.revision,
        &destination.to_string_lossy(),
        scratch.backups(),
    )
    .expect("naming a destination writes it");
    assert!(written.backup_path.is_none(), "nothing was there before");
    assert!(
        !written.dirty,
        "a document with no file of its own is not unsaved work once it has been written"
    );

    // Reopened from disk, the edit is there: the same assertion the E2E makes through the window.
    let reopened = open_session(&SessionSlot::default(), &destination.to_string_lossy())
        .expect("the written file opens");
    assert_eq!(reopened.cues[0].text, "Corrected by hand");
    assert_eq!(reopened.cues.len(), patch.cue_count);
}

#[test]
fn a_first_save_adopts_the_path_it_is_given_and_the_next_save_writes_there() {
    let scratch = Scratch::new("first-save");
    let slot = SessionSlot::default();
    adopt_if_clean(&slot, &transcribed_srt())
        .expect("nothing is in the way")
        .expect("no question needed");

    // The path the user chose in the save chooser, which this document has never had (decision 24,
    // B2). The command layer asks; this is what it does with the answer.
    let chosen = scratch.join("episode-01.srt");
    let first = save_as(&slot, 0, &chosen.to_string_lossy(), scratch.backups())
        .expect("the first save writes where it was told");
    assert_eq!(first.path, chosen.to_string_lossy());
    assert!(!first.dirty, "its bytes are on disk now");

    // Adopted: an edit and a save in place, with no destination named, reach that same file.
    let patch =
        apply_edit(&slot, 0, text(0, "Corrected after the first save")).expect("still open");
    let again = save(&slot, patch.revision, scratch.backups())
        .expect("a document that has a file can be saved in place");
    assert_eq!(again.path, chosen.to_string_lossy());
    assert!(!again.dirty);
    assert!(
        again.backup_path.is_some(),
        "the first save's file was overwritten, so it was kept (CONTRIBUTING.md §3.3)"
    );
    let on_disk = String::from_utf8(fs::read(&chosen).expect("readable")).expect("UTF-8");
    assert!(
        on_disk.contains("Corrected after the first save"),
        "{on_disk}"
    );
}

#[test]
fn the_close_gates_save_writes_a_document_with_no_file_where_it_is_told() {
    let scratch = Scratch::new("gate-first-save");
    let slot = SessionSlot::default();
    adopt_if_clean(&slot, &transcribed_srt())
        .expect("nothing is in the way")
        .expect("no question needed");

    // Without a path the gate's own save has nowhere to go, which is what makes it ask.
    let refused = save_current(&slot, scratch.backups())
        .expect_err("a document with no file cannot be saved in place");
    assert_eq!(refused.code, SubtitleErrorCode::NoPath);

    let chosen = scratch.join("from-the-gate.srt");
    let written = save_current_as(&slot, &chosen.to_string_lossy(), scratch.backups())
        .expect("the answer to that question is a path, and this writes there");
    assert_eq!(written.path, chosen.to_string_lossy());
    assert!(!written.dirty, "the gate may close the window now");
    let on_disk = String::from_utf8(fs::read(&chosen).expect("readable")).expect("UTF-8");
    assert!(on_disk.contains("terminology"), "the cues are on disk");

    // Nothing more to write, so a second pass over the same session is not another save.
    assert!(save_current(&slot, scratch.backups())
        .expect("the document is clean")
        .is_none());
}

#[test]
fn the_gates_save_writes_the_document_s_own_file_when_it_was_given_one_while_it_asked() {
    let scratch = Scratch::new("gate-overtaken");
    let slot = SessionSlot::default();
    let file = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    edited_document(&slot, &file);

    // The interleaving this guards: the gate asked a document with no file where it goes, and by
    // the time the answer came the document had a file and edits that are not in it.
    let elsewhere = scratch.join("not-this-one.srt");
    let written = save_current_as(&slot, &elsewhere.to_string_lossy(), scratch.backups())
        .expect("a document with a file has somewhere to be saved");

    assert_eq!(written.path, file.to_string_lossy(), "its own file");
    assert!(!written.dirty, "the gate may close the window now");
    assert!(!elsewhere.exists(), "the stale answer wrote nothing");
    let saved = String::from_utf8(fs::read(&file).expect("readable")).expect("UTF-8");
    assert!(saved.contains("Edited and not saved"), "{saved}");
}

#[test]
fn unsaved_work_in_the_way_stops_the_replacement_until_it_is_answered() {
    let scratch = Scratch::new("in-the-way");
    let slot = SessionSlot::default();
    let file = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    edited_document(&slot, &file);

    assert!(
        adopt_if_clean(&slot, &transcribed_srt())
            .expect("asking is not a failure")
            .is_none(),
        "a dirty document is replaced only after the user has answered"
    );
    // Nothing moved while the question stands: the document is still the edited fixture.
    let still_there = apply_edit(&slot, 1, text(1, "Still the same document")).expect("still open");
    assert_eq!(still_there.cue_count, 3);
}

#[test]
fn cancel_leaves_both_the_document_and_the_transcription_intact() {
    let scratch = Scratch::new("cancel");
    let slot = SessionSlot::default();
    let file = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    let before = fs::read(&file).expect("readable");
    edited_document(&slot, &file);

    let srt = transcribed_srt();
    assert!(
        adopt_answered(&slot, &srt, CloseAnswer::Cancel, scratch.backups())
            .expect("cancelling is not a failure")
            .is_none()
    );

    // The document kept its edit, and the file on disk was never written.
    let patch = apply_edit(&slot, 1, text(1, "The edit survived")).expect("still open");
    assert_eq!(patch.cue_count, 3, "still the three-cue fixture");
    assert_eq!(fs::read(&file).expect("readable"), before);

    // And the result is still there to be adopted on a second answer.
    let opened = adopt_answered(&slot, &srt, CloseAnswer::Discard, scratch.backups())
        .expect("discarding replaces it")
        .expect("not cancelled");
    assert!(opened.cues.len() > 1);
}

#[test]
fn discard_replaces_the_document_and_writes_nothing() {
    let scratch = Scratch::new("discard");
    let slot = SessionSlot::default();
    let file = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    let before = fs::read(&file).expect("readable");
    edited_document(&slot, &file);

    let opened = adopt_answered(
        &slot,
        &transcribed_srt(),
        CloseAnswer::Discard,
        scratch.backups(),
    )
    .expect("discarding replaces it")
    .expect("not cancelled");

    assert_eq!(opened.summary.path, None);
    assert!(opened.cues.len() > 1);
    // Discarded means the edits are dropped, never that the file is rewritten.
    assert_eq!(fs::read(&file).expect("readable"), before);
}

#[test]
fn save_writes_the_edits_before_the_transcription_takes_their_place() {
    let scratch = Scratch::new("save");
    let slot = SessionSlot::default();
    let file = scratch.copy_of("fixtures/subtitles/srt/clean/basic-lf.srt");
    edited_document(&slot, &file);

    let opened = adopt_answered(
        &slot,
        &transcribed_srt(),
        CloseAnswer::Save,
        scratch.backups(),
    )
    .expect("saving then replacing")
    .expect("not cancelled");
    assert!(
        opened.cues.len() > 1,
        "the transcription is now the document"
    );

    let saved = String::from_utf8(fs::read(&file).expect("readable")).expect("UTF-8");
    assert!(
        saved.contains("Edited and not saved"),
        "the edit reached disk before it was replaced: {saved}"
    );
}

#[test]
fn a_save_that_cannot_be_written_replaces_nothing() {
    let scratch = Scratch::new("save-refused");
    let slot = SessionSlot::default();
    // The document in the way is itself a transcription: it has no file, so Save has nowhere to go.
    adopt_if_clean(&slot, &transcribed_srt())
        .expect("nothing is in the way")
        .expect("no question needed");
    apply_edit(&slot, 0, text(0, "Work that must not be lost")).expect("the edit lands");

    let refused = adopt_answered(
        &slot,
        &transcribed_srt(),
        CloseAnswer::Save,
        scratch.backups(),
    )
    .expect_err("the save has nowhere to write");
    assert_eq!(refused.code, SubtitleErrorCode::NoPath);

    // The work is still in the session, which is the whole point of refusing.
    let patch = apply_edit(&slot, 1, text(1, "Still here")).expect("still open");
    assert!(patch.dirty);
}
