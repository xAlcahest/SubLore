//! What one open document does under editing: the cue list the UI renders, the patch every change
//! produces, and the promise that a refused edit moves nothing. Written from the M2.3 acceptance
//! criteria in BACKLOG.md — edit a cue, save, reopen, the edit is there and the rest is
//! byte-identical; undo restores it — asserted here on bytes rather than through the app.
//!
//! The session is the only place the mutation API and the undo stack meet, so these tests drive it
//! the way the IPC layer does and compare every result against the fixture as it was read.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sublore_edit::diff::CuePatch;
use sublore_edit::error::EditErrorKind;
use sublore_edit::history::Run;
use sublore_edit::plan::Edit;
use sublore_edit::session::EditSession;
use sublore_formats::{AssField, SubtitleDocument, SubtitleFormat};

/// A typing pause, well inside `history::COALESCE_WINDOW`.
const KEYSTROKE: Duration = Duration::from_millis(50);
/// Wider than the window: two edits this far apart are two undo steps.
const APART: Duration = Duration::from_secs(2);

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/sublore-edit; the fixtures live two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate sits two levels under the repo root")
        .to_path_buf()
}

fn fixture_path(relative: &str) -> PathBuf {
    let path = repo_root().join("fixtures/subtitles").join(relative);
    assert!(path.is_file(), "missing fixture {}", path.display());
    path
}

fn fixture_bytes(relative: &str) -> Vec<u8> {
    let path = fixture_path(relative);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()))
}

fn format_of(relative: &str) -> SubtitleFormat {
    match Path::new(relative)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("srt") => SubtitleFormat::Srt,
        Some("vtt") => SubtitleFormat::Vtt,
        Some("ass") => SubtitleFormat::Ass,
        other => panic!("no parser for {other:?}"),
    }
}

fn document(relative: &str) -> SubtitleDocument {
    let bytes = fixture_bytes(relative);
    sublore_formats::parse(format_of(relative), &bytes)
        .unwrap_or_else(|error| panic!("{relative} is a clean fixture: {error}"))
}

/// A session over a fixture, opened from a path that is never written to by these tests.
fn session(relative: &str) -> EditSession {
    EditSession::open(fixture_path(relative), document(relative))
}

/// The M2.1 promise, asserted at the session level: outside the region the edit named, the file is
/// the same bytes it was. `start` is a file offset, so a BOM is already counted.
fn differs_only_in(before: &[u8], after: &[u8], start: usize, old_len: usize, new_len: usize) {
    assert_eq!(
        before.get(..start),
        after.get(..start),
        "the bytes before the edit moved"
    );
    assert_eq!(
        before.get(start + old_len..),
        after.get(start + new_len..),
        "the bytes after the edit moved"
    );
}

/// Where cue `index`'s text sits in the file: its body span shifted past a byte-order mark.
fn text_region(session: &EditSession, index: usize) -> (usize, usize) {
    let document = session.document();
    let cue = document.cues().nth(index).expect("the cue exists");
    let bom = if document.source().has_bom() { 3 } else { 0 };
    (bom + cue.text.start, cue.text.end - cue.text.start)
}

fn set_text(index: usize, text: &str) -> Edit {
    Edit::SetText {
        cue: index,
        text: text.to_owned(),
    }
}

fn texts(session: &EditSession) -> Vec<String> {
    session
        .views()
        .iter()
        .map(|view| view.text.clone())
        .collect()
}

#[test]
fn an_opened_session_lists_every_cue_and_has_nothing_to_undo() {
    let session = session("srt/clean/basic-lf.srt");

    assert_eq!(session.views().len(), 3, "three cues, three rows");
    assert_eq!(session.revision(), 0);
    assert!(!session.dirty(), "a file as it was read is not dirty");
    assert!(!session.can_undo());
    assert!(!session.can_redo());
    assert!(!session.truncated());
    assert_eq!(
        session.to_bytes(),
        fixture_bytes("srt/clean/basic-lf.srt"),
        "an unedited session serializes to the file it opened"
    );
    assert_eq!(
        session.path(),
        Some(fixture_path("srt/clean/basic-lf.srt").as_path()),
        "the session remembers where it came from"
    );

    let first = session.views().first().expect("a first row");
    assert_eq!(first.start_ms, 2_120);
    assert_eq!(first.end_ms, 4_880);
    assert_eq!(first.number, Some(1), "the file wrote an index line");
    assert!(!first.comment);
}

#[test]
fn editing_one_cue_leaves_every_other_byte_of_the_file_identical() {
    for relative in [
        "srt/clean/basic-lf.srt",
        "srt/clean/basic-crlf.srt",
        "srt/clean/bom-crlf.srt",
        "srt/clean/non-latin.srt",
        "vtt/clean/basic.vtt",
        "ass/clean/basic.ass",
    ] {
        let mut session = session(relative);
        let before = session.to_bytes();
        let (start, old_len) = text_region(&session, 1);

        let patch = session
            .apply(&set_text(1, "Rewritten line"), Run::New, Instant::now())
            .unwrap_or_else(|error| panic!("{relative}: editing cue 1 was refused: {error}"));

        assert_eq!(
            patch,
            CuePatch {
                from: 1,
                removed: 1,
                cues: vec![session.views()[1].clone()],
            },
            "{relative}: one row changed, so the patch is one row"
        );
        assert_eq!(session.views()[1].text, "Rewritten line", "{relative}");

        let after = session.to_bytes();
        differs_only_in(&before, &after, start, old_len, "Rewritten line".len());
        assert!(session.dirty(), "{relative}: an edited file is dirty");
        assert!(session.can_undo(), "{relative}");
        assert_eq!(session.revision(), 1, "{relative}");
    }
}

#[test]
fn undo_restores_the_exact_original_bytes_and_redo_the_edited_ones() {
    for relative in [
        "srt/clean/basic-lf.srt",
        "srt/clean/basic-crlf.srt",
        "srt/clean/bom-crlf.srt",
        "srt/clean/no-final-newline.srt",
        "srt/clean/numbering-gaps.srt",
        "vtt/clean/cue-settings.vtt",
        "ass/clean/basic.ass",
    ] {
        let mut session = session(relative);
        let original = session.to_bytes();

        session
            .apply(
                &set_text(0, "First line, rewritten"),
                Run::New,
                Instant::now(),
            )
            .unwrap_or_else(|error| panic!("{relative}: {error}"));
        let edited = session.to_bytes();
        assert_ne!(edited, original, "{relative}: the edit changed the file");

        let patch = session
            .undo()
            .unwrap_or_else(|error| panic!("{relative}: undo was refused: {error}"))
            .unwrap_or_else(|| panic!("{relative}: there was an edit to undo"));
        assert_eq!(patch.from, 0, "{relative}");
        assert_eq!(
            session.to_bytes(),
            original,
            "{relative}: undo restores the file byte for byte"
        );
        assert!(!session.dirty(), "{relative}: back at the opened bytes");
        assert!(session.can_redo(), "{relative}");

        session
            .redo()
            .unwrap_or_else(|error| panic!("{relative}: redo was refused: {error}"))
            .unwrap_or_else(|| panic!("{relative}: there was an edit to redo"));
        assert_eq!(
            session.to_bytes(),
            edited,
            "{relative}: redo restores the edited file byte for byte"
        );
        assert!(session.dirty(), "{relative}");
    }
}

#[test]
fn undo_at_the_bottom_and_redo_at_the_top_move_nothing() {
    let mut session = session("srt/clean/basic-lf.srt");
    let original = session.to_bytes();

    assert!(session.undo().expect("no step is not a failure").is_none());
    assert_eq!(session.revision(), 0, "nothing happened, nothing moved");
    assert_eq!(session.to_bytes(), original);

    session
        .apply(&set_text(0, "Changed"), Run::New, Instant::now())
        .expect("the edit lands");
    assert!(session.redo().expect("no step is not a failure").is_none());
    assert_eq!(session.revision(), 1, "the redo was a no-op");
}

#[test]
fn a_refused_edit_leaves_the_session_exactly_as_it_was() {
    let mut session = session("srt/clean/basic-lf.srt");
    let original = session.to_bytes();
    let rows = texts(&session);

    // A cue index past the end, and a text SRT cannot spell: both are refusals, never writes.
    for (edit, expected) in [
        (set_text(99, "nowhere"), EditErrorKind::NoSuchCue),
        (
            set_text(0, "first\n\nsecond"),
            EditErrorKind::UnwritableText,
        ),
        (Edit::Merge { cue: 2 }, EditErrorKind::NoSuchCue),
    ] {
        let error = session
            .apply(&edit, Run::New, Instant::now())
            .expect_err("this edit cannot be written");
        assert_eq!(error.kind, expected, "{edit:?}");
        assert_eq!(session.to_bytes(), original, "{edit:?}: the bytes moved");
        assert_eq!(texts(&session), rows, "{edit:?}: the rows moved");
        assert_eq!(session.revision(), 0, "{edit:?}: the revision moved");
        assert!(!session.dirty(), "{edit:?}: a refusal made the file dirty");
        assert!(!session.can_undo(), "{edit:?}: a refusal reached the stack");
    }
}

#[test]
fn typing_a_word_through_the_session_is_one_undo_step() {
    let mut session = session("srt/clean/basic-lf.srt");
    let original = session.to_bytes();
    let mut now = Instant::now();

    // Eight keystrokes on the same cue, each rewriting the whole text field, from a caller that
    // says so. The cue list is not such a caller: it sends one finished field per commit.
    for length in 1..=8 {
        let typed: String = "Kept safe".chars().take(length).collect();
        session
            .apply(&set_text(0, &typed), Run::Continues, now)
            .expect("each keystroke lands");
        now += KEYSTROKE;
    }
    assert_eq!(session.views()[0].text, "Kept saf");

    session
        .undo()
        .expect("one undo")
        .expect("there is a step to undo");
    assert_eq!(
        session.to_bytes(),
        original,
        "one undo takes the whole typed run back"
    );
    assert!(!session.can_undo(), "the run was one entry, not eight");
}

/// Regression: two finished edits of one cue used to merge whenever they landed inside the
/// coalescing window, so the first of them stopped being a place undo could go back to. M2.2.
#[test]
fn two_finished_edits_of_one_cue_are_two_undo_steps_however_fast_they_arrive() {
    let mut session = session("srt/clean/basic-lf.srt");
    let original = session.to_bytes();
    let now = Instant::now();

    session
        .apply(&set_text(0, "First draft."), Run::New, now)
        .expect("the first edit lands");
    let after_first = session.to_bytes();
    session
        .apply(
            &set_text(0, "Second, corrected draft."),
            Run::New,
            now + KEYSTROKE,
        )
        .expect("the second edit lands");

    session.undo().expect("undo").expect("a step");
    assert_eq!(session.views()[0].text, "First draft.");
    assert_eq!(
        session.to_bytes(),
        after_first,
        "the first draft is still a state the stack can go back to"
    );
    session.undo().expect("undo").expect("a second step");
    assert_eq!(session.to_bytes(), original, "and then the file as opened");
}

#[test]
fn edits_far_apart_are_separate_undo_steps() {
    let mut session = session("srt/clean/basic-lf.srt");
    let original = session.to_bytes();
    let mut now = Instant::now();

    session
        .apply(&set_text(0, "One"), Run::New, now)
        .expect("first edit");
    now += APART;
    let after_first = session.to_bytes();
    session
        .apply(&set_text(0, "Two"), Run::New, now)
        .expect("second edit");

    session.undo().expect("undo").expect("a step");
    assert_eq!(session.to_bytes(), after_first, "one step back, not two");
    session.undo().expect("undo").expect("a second step");
    assert_eq!(session.to_bytes(), original);
}

#[test]
fn saving_marks_the_session_clean_and_the_next_edit_dirty_again() {
    let mut session = session("srt/clean/basic-lf.srt");
    session
        .apply(&set_text(0, "Edited"), Run::New, Instant::now())
        .expect("the edit lands");
    assert!(session.dirty());

    session.mark_saved();
    assert!(
        !session.dirty(),
        "the bytes on disk are the bytes in memory"
    );

    session
        .apply(&set_text(1, "Edited too"), Run::New, Instant::now())
        .expect("the second edit lands");
    assert!(session.dirty(), "an edit after a save is unsaved work");

    session.undo().expect("undo").expect("a step");
    assert!(!session.dirty(), "undoing back to the save point is clean");
}

#[test]
fn an_edit_after_an_undo_drops_the_redo_tail() {
    let mut session = session("srt/clean/basic-lf.srt");
    let mut now = Instant::now();

    session
        .apply(&set_text(0, "One"), Run::New, now)
        .expect("first edit");
    now += APART;
    session.undo().expect("undo").expect("a step");
    assert!(session.can_redo());

    session
        .apply(&set_text(1, "Elsewhere"), Run::New, now)
        .expect("a new edit");
    assert!(
        !session.can_redo(),
        "the undone edit is unreachable once a new one is made"
    );
}

#[test]
fn every_mutation_kind_reaches_the_document_and_undoes_back_to_the_file() {
    let cases = [
        (
            "srt/clean/basic-lf.srt",
            Edit::Insert {
                before: 1,
                start_ms: 4_900,
                end_ms: 4_950,
                text: "Inserted".to_owned(),
            },
            1isize,
        ),
        ("srt/clean/basic-lf.srt", Edit::Delete { cue: 1 }, -1),
        (
            "srt/clean/basic-lf.srt",
            Edit::Split {
                cue: 0,
                text_offset: 5,
                at_ms: 3_000,
            },
            1,
        ),
        ("srt/clean/basic-lf.srt", Edit::Merge { cue: 0 }, -1),
        (
            "srt/clean/basic-lf.srt",
            Edit::SetTimes {
                cue: 0,
                start_ms: 2_500,
                end_ms: 4_500,
            },
            0,
        ),
        (
            "ass/clean/basic.ass",
            Edit::SetTimes {
                cue: 0,
                start_ms: 1_500,
                end_ms: 2_500,
            },
            0,
        ),
        (
            "vtt/clean/basic.vtt",
            Edit::Insert {
                before: 0,
                start_ms: 100,
                end_ms: 400,
                text: "Inserted".to_owned(),
            },
            1,
        ),
    ];

    for (relative, edit, delta) in cases {
        let mut session = session(relative);
        let original = session.to_bytes();
        let rows_before = session.views().len();

        session
            .apply(&edit, Run::New, Instant::now())
            .unwrap_or_else(|error| panic!("{relative} {edit:?}: {error}"));

        let rows_after = session.views().len();
        assert_eq!(
            rows_after as isize - rows_before as isize,
            delta,
            "{relative} {edit:?}: the row count moved by the wrong amount"
        );
        assert_ne!(session.to_bytes(), original, "{relative} {edit:?}");

        session
            .undo()
            .unwrap_or_else(|error| panic!("{relative} {edit:?}: undo refused: {error}"))
            .unwrap_or_else(|| panic!("{relative} {edit:?}: nothing to undo"));
        assert_eq!(
            session.to_bytes(),
            original,
            "{relative} {edit:?}: undo did not restore the file"
        );
        assert_eq!(session.views().len(), rows_before, "{relative} {edit:?}");
    }
}

#[test]
fn a_crlf_file_keeps_its_line_endings_and_the_wire_form_stays_normalized() {
    let mut session = session("srt/clean/basic-crlf.srt");
    session
        .apply(&set_text(0, "First\nSecond"), Run::New, Instant::now())
        .expect("two lines land");

    assert_eq!(
        session.views()[0].text,
        "First\nSecond",
        "the row the UI renders uses \\n whatever the file uses"
    );
    let bytes = session.to_bytes();
    let text = String::from_utf8(bytes).expect("still UTF-8");
    assert!(
        text.contains("First\r\nSecond"),
        "the file kept its CRLF endings: {text:?}"
    );
    assert!(
        !text.contains("First\nSecond"),
        "no LF-only break was written into a CRLF file"
    );
}

#[test]
fn the_rows_carry_what_the_file_wrote_and_nothing_is_renumbered() {
    let mut session = session("srt/clean/numbering-gaps.srt");
    let numbers: Vec<Option<u32>> = session.views().iter().map(|view| view.number).collect();
    assert!(
        numbers.iter().any(Option::is_some),
        "the fixture writes index lines"
    );

    session
        .apply(
            &Edit::Insert {
                before: 1,
                start_ms: 9_000,
                end_ms: 9_500,
                text: "Inserted".to_owned(),
            },
            Run::New,
            Instant::now(),
        )
        .expect("the insert lands");

    let after: Vec<Option<u32>> = session.views().iter().map(|view| view.number).collect();
    assert_eq!(
        after.first(),
        numbers.first(),
        "the cue before the insert kept its number"
    );
    assert_eq!(
        after.get(2..),
        numbers.get(1..),
        "no cue after the insert was renumbered"
    );
}

#[test]
fn an_ass_comment_is_listed_and_flagged() {
    let session = session("ass/clean/comments-and-semicolons.ass");
    assert!(
        session.views().iter().any(|view| view.comment),
        "a Comment: event is a row the editor lists"
    );
    assert!(
        session.views().iter().any(|view| !view.comment),
        "and a Dialogue: event is not flagged as one"
    );
}

#[test]
fn the_two_thousand_cue_fixture_opens_whole_and_patches_one_row() {
    let mut session = session("srt/clean/large-2000.srt");
    assert_eq!(session.views().len(), 2_000);

    let before = session.to_bytes();
    let (start, old_len) = text_region(&session, 42);
    let patch = session
        .apply(
            &set_text(42, "Edited by the cue list."),
            Run::New,
            Instant::now(),
        )
        .expect("editing row 42 lands");

    assert_eq!(patch.from, 42);
    assert_eq!(patch.removed, 1);
    assert_eq!(patch.cues.len(), 1, "one row crosses the wire, not 2000");
    differs_only_in(
        &before,
        &session.to_bytes(),
        start,
        old_len,
        "Edited by the cue list.".len(),
    );
}

#[test]
fn committing_an_unchanged_field_is_not_an_edit() {
    let mut session = session("srt/clean/basic-lf.srt");
    let original = session.to_bytes();
    let unchanged = session.views().first().expect("a first row").text.clone();

    // The UI commits a field on Enter and on blur, whether or not anything was typed into it.
    let patch = session
        .apply(&set_text(0, &unchanged), Run::New, Instant::now())
        .expect("re-sending the same text is accepted");

    assert_eq!(patch.removed, 0, "nothing was replaced");
    assert!(patch.cues.is_empty(), "so nothing crosses the wire");
    assert_eq!(session.to_bytes(), original);
    assert_eq!(session.revision(), 0, "nothing moved, nothing to refetch");
    assert!(!session.dirty(), "closing a field is not unsaved work");
    assert!(!session.can_undo(), "and it is not an undo step either");
}

#[test]
fn undo_and_redo_each_move_the_revision_and_a_no_step_call_does_not() {
    let mut session = session("srt/clean/basic-lf.srt");

    session
        .apply(&set_text(0, "Once"), Run::New, Instant::now())
        .expect("the edit lands");
    assert_eq!(session.revision(), 1);
    session.undo().expect("undo").expect("a step");
    assert_eq!(session.revision(), 2, "an undo changes the list too");
    session.redo().expect("redo").expect("a step");
    assert_eq!(session.revision(), 3);

    assert!(session.redo().expect("the top").is_none());
    assert_eq!(
        session.revision(),
        3,
        "a call that replayed nothing must not invalidate the caller's revision"
    );
}

#[test]
fn the_patch_of_an_undone_insert_removes_the_row_it_added() {
    let mut session = session("srt/clean/basic-lf.srt");

    let inserted = session
        .apply(
            &Edit::Insert {
                before: 1,
                start_ms: 4_900,
                end_ms: 4_950,
                text: "Wedged in".to_owned(),
            },
            Run::New,
            Instant::now(),
        )
        .expect("the insert lands");
    assert_eq!(inserted.from, 1);
    assert_eq!(inserted.removed, 0);
    assert_eq!(inserted.cues.len(), 1);

    // Undo carries no plan, so its patch is measured from the lists: same run, mirrored.
    let undone = session.undo().expect("undo").expect("a step");
    assert_eq!(undone.from, 1);
    assert_eq!(undone.removed, 1);
    assert!(undone.cues.is_empty());
    assert_eq!(session.views().len(), 3);
}

#[test]
fn a_non_latin_edit_survives_the_round_trip_byte_for_byte() {
    let mut session = session("srt/clean/non-latin.srt");
    let original = session.to_bytes();
    // Combining marks, RTL, CJK and an astral pair: the shapes a byte-offset bug mangles first.
    let planted = "Ολοκληρώθηκε · 完了 · مكتمل · éé\u{0301} · 🎬";

    session
        .apply(&set_text(1, planted), Run::New, Instant::now())
        .expect("the edit lands");
    assert_eq!(session.views()[1].text, planted);
    assert!(String::from_utf8(session.to_bytes())
        .expect("still UTF-8")
        .contains(planted));

    session.undo().expect("undo").expect("a step");
    assert_eq!(session.to_bytes(), original, "undo is exact through UTF-8");
}

#[test]
fn no_row_the_ui_renders_carries_a_line_terminator_the_textarea_would_eat() {
    // A textarea reads back "\n" whatever it was given, so a "\r\n" on the wire would come back
    // as "\n" and quietly convert that cue's endings. A lone "\r" is content to the parsers, so
    // it is not a terminator and must survive untouched. See BACKLOG.md M2.1.
    for relative in [
        "srt/clean/basic-crlf.srt",
        "srt/clean/bom-crlf.srt",
        "srt/clean/mixed-eol.srt",
        "vtt/clean/header-text-crlf.vtt",
    ] {
        let session = session(relative);
        assert!(
            session
                .views()
                .iter()
                .all(|view| !view.text.contains("\r\n")),
            "{relative}: a row reached the UI with a CRLF in it"
        );
    }

    let carried = session("srt/clean/mixed-eol.srt");
    assert!(
        carried.views().iter().any(|view| view.text.contains('\r')),
        "the lone carriage return this fixture plants is content, and stays content"
    );
}

#[test]
fn sixty_edits_across_the_large_fixture_undo_all_the_way_back() {
    let mut session = session("srt/clean/large-2000.srt");
    let original = session.to_bytes();
    let mut now = Instant::now();

    // Far apart in time and in cue, so nothing coalesces: sixty distinct entries.
    for step in 0..60usize {
        session
            .apply(&set_text(step * 7, &format!("Line {step}")), Run::New, now)
            .unwrap_or_else(|error| panic!("step {step}: {error}"));
        now += APART;
    }
    assert!(session.dirty());

    let mut undone = 0;
    while session.undo().expect("undo replays").is_some() {
        undone += 1;
    }
    assert_eq!(undone, 60, "one step per edit");
    assert_eq!(session.to_bytes(), original, "back to the file as opened");
    assert!(!session.dirty());
    assert!(!session.truncated(), "sixty is well inside the bound");
}

#[test]
fn a_document_with_no_file_is_unsaved_from_the_first_moment_and_edits_like_any_other() {
    let mut session = EditSession::untitled(document("srt/clean/basic-lf.srt"));

    assert_eq!(session.path(), None, "it has never had a file");
    assert!(
        session.dirty(),
        "every byte of it exists only here, so it is unsaved work"
    );
    assert!(!session.can_undo());
    assert!(!session.truncated());
    assert_eq!(session.views().len(), 3);

    let original = session.to_bytes();
    session
        .apply(&set_text(0, "Corrected"), Run::New, Instant::now())
        .expect("the same mutation API as a file-backed session");
    assert_eq!(texts(&session)[0], "Corrected");
    assert!(session.undo().expect("undo replays").is_some());
    assert_eq!(
        session.to_bytes(),
        original,
        "the same undo the editor uses"
    );

    // Still unsaved at the bottom of the stack: undoing back does not put bytes on a disk.
    assert!(session.dirty());
    session.mark_saved();
    assert!(!session.dirty(), "a write is what makes it saved");
}

#[test]
fn a_document_with_no_file_keeps_the_path_its_first_save_gave_it() {
    let mut session = EditSession::untitled(document("srt/clean/basic-lf.srt"));
    assert_eq!(session.path(), None);

    // What the first save adopts, so every save after it writes there (decision 24, B2).
    session.adopt_path(PathBuf::from("/tmp/episode-01.srt"));
    assert_eq!(session.path(), Some(Path::new("/tmp/episode-01.srt")));

    // Editing after the adoption does not take the file away again.
    session
        .apply(&set_text(0, "Corrected"), Run::New, Instant::now())
        .expect("the same mutation API");
    assert!(session.dirty());
    assert_eq!(session.path(), Some(Path::new("/tmp/episode-01.srt")));
}

// F1 at the session level: what a replace over many cues costs the undo stack. One step, whatever
// the count, because a replace the user has to press through cue by cue is not undone.

#[test]
fn one_undo_puts_back_every_cue_a_many_cue_edit_rewrote() {
    let original = fixture_bytes("srt/clean/basic-lf.srt");
    let mut session = session("srt/clean/basic-lf.srt");
    let now = Instant::now();

    session
        .apply(
            &Edit::SetTexts {
                edits: vec![
                    (0, "one".to_owned()),
                    (1, "two".to_owned()),
                    (2, "three".to_owned()),
                ],
            },
            Run::New,
            now,
        )
        .expect("three cues at once");
    assert!(session.dirty());
    assert!(session.can_undo());

    let patch = session
        .undo()
        .expect("the replay lands")
        .expect("there is a step to take");
    // The patch covers the whole run, so the grid redraws all three rather than one of them.
    assert_eq!(patch.from, 0);
    assert_eq!(patch.removed, 3);
    assert_eq!(session.to_bytes(), original);
    // One step, not three: a second undo has nothing left to take.
    assert!(!session.can_undo());
}

#[test]
fn a_many_cue_edit_that_writes_the_text_already_there_leaves_the_stack_alone() {
    let original = fixture_bytes("srt/clean/basic-lf.srt");
    let mut session = session("srt/clean/basic-lf.srt");
    let already: Vec<_> = session
        .views()
        .iter()
        .enumerate()
        .map(|(cue, view)| (cue, view.text.clone()))
        .collect();

    let patch = session
        .apply(&Edit::SetTexts { edits: already }, Run::New, Instant::now())
        .expect("writing what is already there is not a failure");

    // A replace whose matches all render back to the same bytes changed nothing, and a step the
    // user would press through for no reason is worse than no step.
    assert_eq!(patch.removed, 0);
    assert!(patch.cues.is_empty());
    assert!(!session.dirty());
    assert!(!session.can_undo());
    assert_eq!(session.to_bytes(), original);
}

#[test]
fn a_many_cue_edit_never_merges_into_the_keystroke_before_it() {
    let mut session = session("srt/clean/basic-lf.srt");
    let now = Instant::now();

    session
        .apply(
            &Edit::SetText {
                cue: 0,
                text: "typed".to_owned(),
            },
            Run::New,
            now,
        )
        .expect("a first edit");
    session
        .apply(
            &Edit::SetTexts {
                edits: vec![(0, "replaced".to_owned())],
            },
            // Inside the coalesce window and on the same cue: everything the history looks at to
            // merge, except the label, which is what keeps the two apart.
            Run::Continues,
            now + KEYSTROKE,
        )
        .expect("a replace over the same cue");

    session.undo().expect("the replay lands").expect("a step");
    let views = session.views();
    assert_eq!(
        views.first().map(|view| view.text.as_str()),
        Some("typed"),
        "one undo must take back the replace and leave the typing under it"
    );
}

// ---------------------------------------------------------------------------------------------
// Writing one ASS event field (docs/ass-field-write-tasks.md CF4)
// ---------------------------------------------------------------------------------------------

fn set_field(cue: usize, field: AssField, value: &str) -> Edit {
    Edit::SetField {
        cue,
        field,
        value: value.to_owned(),
    }
}

#[test]
fn two_fields_of_one_cue_are_two_undo_steps() {
    // CF4.1 and CF4.2. The field is on the undo label, so an Actor write and an Effect write on
    // the same cue cannot merge even when they land inside the coalescing window.
    let mut session = session("ass/clean/basic.ass");
    let original = fixture_bytes("ass/clean/basic.ass");
    let now = Instant::now();

    session
        .apply(&set_field(0, AssField::Actor, "Ingrid"), Run::New, now)
        .expect("the Name field is declared");
    session
        .apply(
            &set_field(0, AssField::Effect, "fad"),
            Run::Continues,
            now + KEYSTROKE,
        )
        .expect("the Effect field is declared");
    session
        .apply(&set_text(0, "Rewritten."), Run::Continues, now + KEYSTROKE)
        .expect("the text is editable");

    for step in 0..3 {
        session
            .undo()
            .unwrap_or_else(|error| panic!("undo {step} must replay: {error}"))
            .unwrap_or_else(|| panic!("undo {step} must find a step"));
    }
    assert_eq!(
        session.to_bytes(),
        original,
        "three writes must take three undos to take back"
    );
    assert!(
        !session.can_undo(),
        "the stack must hold exactly three steps"
    );
    assert!(!session.dirty(), "undoing back to the open bytes is clean");
}

#[test]
fn committing_a_field_unchanged_never_grows_the_undo_stack() {
    // CF4.4: a translator tabbing through the panel without typing leaves the file as they found
    // it, because the trimmed core makes an unchanged commit byte-identical.
    let mut session = session("ass/clean/basic.ass");
    let original = fixture_bytes("ass/clean/basic.ass");
    let now = Instant::now();

    for field in AssField::ALL {
        let current = session
            .views()
            .first()
            .map(|view| match field {
                AssField::Style => view.style.clone(),
                AssField::Actor => view.actor.clone(),
                // B5 has not landed, so the panel cannot read these five back yet; the file's own
                // bytes stand in for what it would show.
                _ => raw_core(&session, 0, field),
            })
            .expect("a first row");
        let patch = session
            .apply(&set_field(0, field, &current), Run::New, now + APART)
            .unwrap_or_else(|error| panic!("{field:?} must be committable: {error}"));
        assert_eq!(patch.cues.len(), 0, "{field:?} changed a row");
    }

    assert!(!session.can_undo(), "no step was recorded");
    assert!(!session.dirty(), "the document was never dirtied");
    assert_eq!(session.revision(), 0, "the revision never moved");
    assert_eq!(session.to_bytes(), original);
}

#[test]
fn a_field_committed_as_whitespace_writes_nothing_however_often_it_is_committed() {
    // The panel reads a field through the trim `field_core` applies, so a value that is only
    // padding displays as empty. Written verbatim it landed outside the core and appended a byte
    // and an undo step on every commit, which a combo committing on blur would do every time.
    let mut session = session("ass/clean/basic.ass");
    let original = fixture_bytes("ass/clean/basic.ass");
    let now = Instant::now();

    // Spaces only: a tab inside a value is refused outright (CF3.6), padding or not.
    for (round, value) in [" ", "  ", "   ", "    "].into_iter().enumerate() {
        let patch = session
            .apply(
                &set_field(0, AssField::Actor, value),
                Run::New,
                now + APART * (round as u32 + 1),
            )
            .unwrap_or_else(|error| panic!("round {round} must be committable: {error}"));
        assert_eq!(patch.cues.len(), 0, "round {round} changed a row");
        assert_eq!(
            session.to_bytes(),
            original,
            "round {round} moved a byte of the file"
        );
    }

    assert!(!session.can_undo(), "no step was recorded");
    assert!(!session.dirty(), "the document was never dirtied");
    assert_eq!(session.revision(), 0, "the revision never moved");

    // The other half of the same rule: a value that has anything in it keeps its own padding,
    // because a style may genuinely be named with one (C5.2).
    session
        .apply(
            &set_field(0, AssField::Actor, "Bo "),
            Run::New,
            now + APART * 9,
        )
        .expect("a padded value is writable");
    // On the file's own bytes, not through `raw_core`, which trims the space back off: that the
    // panel cannot show the difference is W4's accepted consequence, but the file must hold it.
    let bytes = String::from_utf8(session.to_bytes()).expect("the fixture is UTF-8");
    assert!(
        bytes.contains("Default,Bo ,0,0,0,"),
        "the value's own trailing space was trimmed on the way out"
    );
}

#[test]
fn a_field_write_after_a_save_leaves_the_document_clean_when_undone() {
    // CF4.3.
    let mut session = session("ass/clean/basic.ass");
    session.mark_saved();
    session
        .apply(
            &set_field(0, AssField::Actor, "Ingrid"),
            Run::New,
            Instant::now(),
        )
        .expect("the Name field is declared");
    assert!(session.dirty());
    session.undo().expect("a step to undo").expect("a patch");
    assert!(!session.dirty(), "undoing back to the save point is clean");
}

/// One field of one cue as the file spells it, trimmed the way a column renders it.
fn raw_core(session: &EditSession, cue: usize, field: AssField) -> String {
    let document = session.document();
    let Some(sublore_formats::CueDetail::Ass(event)) =
        document.cues().nth(cue).map(|cue| &cue.detail)
    else {
        panic!("cue {cue} is not an ASS event");
    };
    let Some(span) = event
        .field_index(field)
        .and_then(|at| event.fields.get(at).copied())
    else {
        panic!("cue {cue} declares no {field:?}");
    };
    document
        .slice(span)
        .trim_start_matches([' ', '\t'])
        .trim_end_matches([' ', '\t', '\r'])
        .to_owned()
}
