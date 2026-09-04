//! M2.1 behavioral suite, written from the acceptance criteria in BACKLOG.md:
//!
//! - mutating one cue and saving leaves every other byte of the file identical;
//! - a mutation that would break segment coverage is refused with a structured error, never
//!   written.
//!
//! Every assertion is made against the bytes on the way out, not against the model, because the
//! bytes are what the user keeps.

use std::path::{Path, PathBuf};

use sublore_edit::diff::{self, CueView};
use sublore_edit::error::EditErrorKind;
use sublore_edit::plan::{edit, Edit, Edited, Expectation, ExpectedCue};
use sublore_edit::verify::verify;
use sublore_formats::{AssEventKind, CueDetail, SubtitleDocument, SubtitleFormat, MAX_TIMECODE_MS};

/// Fixture-count guard: deleting fixtures must turn this suite red, not quietly shrink it.
const MIN_CLEAN: usize = 43;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("subtitles")
}

fn format_of(path: &Path) -> SubtitleFormat {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("srt") => SubtitleFormat::Srt,
        Some("vtt") => SubtitleFormat::Vtt,
        Some("ass" | "ssa") => SubtitleFormat::Ass,
        other => panic!("{other:?} is not a subtitle fixture extension"),
    }
}

/// Open a committed fixture. Nothing in this suite ever writes inside `fixtures/`.
fn open(relative: &str) -> (SubtitleDocument, Vec<u8>) {
    let path = root().join(relative);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
    let document = sublore_formats::parse(format_of(&path), &bytes)
        .unwrap_or_else(|error| panic!("{} must parse: {error}", path.display()));
    assert_eq!(
        document.to_bytes(),
        bytes,
        "{}: the fixture must round-trip before it is edited",
        path.display()
    );
    (document, bytes)
}

/// A document built from a string, for shapes no committed fixture has.
fn parse(format: SubtitleFormat, text: &str) -> SubtitleDocument {
    sublore_formats::parse(format, text.as_bytes()).expect("the sample parses")
}

fn clean_fixtures() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for directory in ["srt", "vtt", "ass"] {
        let clean = root().join(directory).join("clean");
        let entries = std::fs::read_dir(&clean)
            .unwrap_or_else(|error| panic!("{} is unreadable: {error}", clean.display()));
        for entry in entries {
            let path = entry
                .unwrap_or_else(|error| panic!("unreadable fixture entry: {error}"))
                .path();
            if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("srt" | "vtt" | "ass" | "ssa")
            ) {
                found.push(path);
            }
        }
    }
    found.sort();
    assert!(
        found.len() >= MIN_CLEAN,
        "the clean tree holds {} fixtures, this suite guards at least {MIN_CLEAN}",
        found.len()
    );
    found
}

/// The acceptance criterion itself: the bytes outside the splice are copied, not rebuilt, and the
/// splice stays inside the block of the cue that was named.
fn assert_bytes_outside_the_edit_are_identical(
    original: &[u8],
    before: &SubtitleDocument,
    edited: &Edited,
    label: &str,
) {
    let after = edited.document.to_bytes();
    let base = usize::from(before.source().has_bom()) * 3;
    let at = base + edited.splice.at;
    let removed = edited.splice.removed.len();
    let inserted = edited.splice.inserted.len();

    assert_eq!(
        original.get(..at),
        after.get(..at),
        "{label}: the bytes before the edit moved"
    );
    assert_eq!(
        original.get(at + removed..),
        after.get(at + inserted..),
        "{label}: the bytes after the edit moved"
    );
    assert_eq!(
        after.len(),
        original.len() + inserted - removed,
        "{label}: the file grew by something other than the edit"
    );
}

/// Every cue the edit did not name reads back with the same times and the same text.
fn assert_other_cues_intact(
    before: &SubtitleDocument,
    after: &SubtitleDocument,
    from: usize,
    removed: usize,
    added: usize,
    label: &str,
) {
    let old = diff::views(before);
    let new = diff::views(after);
    for (index, view) in old.iter().enumerate() {
        if index >= from && index < from + removed {
            continue;
        }
        let shifted = if index < from {
            index
        } else {
            index - removed + added
        };
        assert_eq!(
            Some(view),
            new.get(shifted),
            "{label}: cue {index} was not edited and changed"
        );
    }
}

/// Saving and reopening changes nothing: the edited document is the document the file would give.
fn assert_reopens_identically(after: &SubtitleDocument, label: &str) {
    assert_eq!(
        after.check_coverage(),
        Ok(()),
        "{label}: the edited document must tile its body"
    );
    let bytes = after.to_bytes();
    let reopened = sublore_formats::parse(after.format(), &bytes)
        .unwrap_or_else(|error| panic!("{label}: the edited bytes must reopen: {error}"));
    assert_eq!(
        reopened.to_bytes(),
        bytes,
        "{label}: reopening changed bytes"
    );
    assert_eq!(
        reopened.cues().count(),
        after.cues().count(),
        "{label}: reopening changed the cue count"
    );
}

fn views(document: &SubtitleDocument) -> Vec<CueView> {
    diff::views(document)
}

// ---------------------------------------------------------------------------------------------
// The acceptance criteria
// ---------------------------------------------------------------------------------------------

#[test]
fn editing_one_cue_leaves_every_other_byte_of_every_clean_fixture_identical() {
    let mut edited_any = 0usize;
    for path in clean_fixtures() {
        let relative = path.display().to_string();
        let bytes = std::fs::read(&path).expect("fixture is readable");
        let document = sublore_formats::parse(format_of(&path), &bytes).expect("fixture parses");
        let count = document.cues().count();
        if count == 0 {
            continue;
        }
        let target = count / 2;

        let result = edit(
            &document,
            &Edit::SetText {
                cue: target,
                text: "Sublore edit".to_owned(),
            },
        )
        .unwrap_or_else(|error| panic!("{relative}: cue {target} must be editable: {error}"));

        assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, &relative);
        assert_other_cues_intact(&document, &result.document, target, 1, 1, &relative);
        assert_reopens_identically(&result.document, &relative);
        assert_eq!(
            views(&result.document)
                .get(target)
                .map(|view| view.text.as_str()),
            Some("Sublore edit"),
            "{relative}: the edited cue must read back what was written"
        );
        assert_eq!(
            result.cue_delta, 0,
            "{relative}: editing text moves no cue count"
        );
        edited_any += 1;
    }
    assert!(
        edited_any >= 30,
        "only {edited_any} fixtures held a cue to edit"
    );
}

#[test]
fn editing_times_leaves_every_other_byte_of_every_clean_fixture_identical() {
    for path in clean_fixtures() {
        let relative = path.display().to_string();
        let bytes = std::fs::read(&path).expect("fixture is readable");
        let document = sublore_formats::parse(format_of(&path), &bytes).expect("fixture parses");
        let count = document.cues().count();
        if count == 0 {
            continue;
        }
        let target = count / 2;
        let (start, end) = {
            let cue = document.cues().nth(target).expect("the cue exists");
            (cue.start.millis(), cue.end.millis())
        };
        // A whole second keeps whatever fraction the file already spells, at any precision.
        let (start, end) = (start.saturating_add(1_000), end.saturating_add(1_000));
        if start > MAX_TIMECODE_MS || end > MAX_TIMECODE_MS {
            continue;
        }

        let result = edit(
            &document,
            &Edit::SetTimes {
                cue: target,
                start_ms: start,
                end_ms: end,
            },
        )
        .unwrap_or_else(|error| panic!("{relative}: cue {target} times must be editable: {error}"));

        assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, &relative);
        assert_other_cues_intact(&document, &result.document, target, 1, 1, &relative);
        assert_reopens_identically(&result.document, &relative);
        let view = views(&result.document).remove(target);
        assert_eq!((view.start_ms, view.end_ms), (start, end), "{relative}");
    }
}

#[test]
fn a_mutation_that_would_break_the_file_is_refused_and_nothing_is_written() {
    let cases: [(&str, Edit, EditErrorKind); 8] = [
        (
            "srt/clean/basic-lf.srt",
            Edit::SetText {
                cue: 0,
                text: "one\n\ntwo".to_owned(),
            },
            EditErrorKind::UnwritableText,
        ),
        (
            "vtt/clean/basic.vtt",
            Edit::SetText {
                cue: 0,
                text: "one\n \ntwo".to_owned(),
            },
            EditErrorKind::UnwritableText,
        ),
        (
            "ass/clean/basic.ass",
            Edit::SetText {
                cue: 0,
                text: "one\ntwo".to_owned(),
            },
            EditErrorKind::UnwritableText,
        ),
        (
            "srt/clean/basic-lf.srt",
            Edit::SetText {
                cue: 0,
                text: "trailing\n".to_owned(),
            },
            EditErrorKind::UnwritableText,
        ),
        (
            "srt/clean/basic-lf.srt",
            Edit::SetText {
                cue: 99,
                text: "nowhere".to_owned(),
            },
            EditErrorKind::NoSuchCue,
        ),
        (
            "srt/clean/basic-lf.srt",
            Edit::SetTimes {
                cue: 0,
                start_ms: MAX_TIMECODE_MS + 1,
                end_ms: MAX_TIMECODE_MS + 1,
            },
            EditErrorKind::UnwritableTimecode,
        ),
        (
            // The ASS fixture spells centiseconds, so a millisecond value cannot be written back.
            "ass/clean/basic.ass",
            Edit::SetTimes {
                cue: 0,
                start_ms: 1_234,
                end_ms: 5_000,
            },
            EditErrorKind::UnwritableTimecode,
        ),
        (
            "srt/clean/basic-lf.srt",
            Edit::Merge { cue: 99 },
            EditErrorKind::NoSuchCue,
        ),
    ];

    for (fixture, request, expected) in cases {
        let (document, bytes) = open(fixture);
        let error = edit(&document, &request)
            .err()
            .unwrap_or_else(|| panic!("{fixture}: {request:?} must be refused"));
        assert_eq!(error.kind, expected, "{fixture}: {request:?}");
        assert_eq!(
            document.to_bytes(),
            bytes,
            "{fixture}: a refused edit must leave the document untouched"
        );
    }
}

#[test]
fn a_timecode_past_the_ceiling_is_refused() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");

    // The ceiling itself is writable: the fixture spells two hour digits, and widening to the
    // three the value needs loses nothing.
    let edited = edit(
        &document,
        &Edit::SetTimes {
            cue: 0,
            start_ms: MAX_TIMECODE_MS,
            end_ms: MAX_TIMECODE_MS,
        },
    )
    .expect("999:59:59,999 is inside the range the formats can spell");
    let written = String::from_utf8(edited.document.to_bytes()).expect("still utf-8");
    assert!(
        written.contains("999:59:59,999 --> 999:59:59,999"),
        "the widened timestamp: {written:?}"
    );

    let error = edit(
        &document,
        &Edit::SetTimes {
            cue: 0,
            start_ms: MAX_TIMECODE_MS + 1,
            end_ms: MAX_TIMECODE_MS + 1,
        },
    )
    .expect_err("one millisecond past the ceiling is not a timestamp any of the formats hold");
    assert_eq!(error.kind, EditErrorKind::UnwritableTimecode);
    assert_eq!(document.to_bytes(), bytes);
}

#[test]
fn writing_into_a_cue_with_no_text_makes_a_text_line_not_a_timing_trailer() {
    // The empty text span is parked at the end of the timing line: writing into it directly would
    // grow a trailer, and the cue would still read back empty. See BACKLOG.md M2.1.
    for (fixture, cue) in [
        ("srt/clean/empty-text.srt", 0),
        ("vtt/clean/empty-cue-text.vtt", 0),
    ] {
        let (document, bytes) = open(fixture);
        let before = views(&document);
        assert_eq!(
            before.get(cue).map(|view| view.text.as_str()),
            Some(""),
            "{fixture}: the fixture's cue {cue} must start empty"
        );

        let result = edit(
            &document,
            &Edit::SetText {
                cue,
                text: "Something at last.".to_owned(),
            },
        )
        .unwrap_or_else(|error| panic!("{fixture}: {error}"));

        assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, fixture);
        assert_eq!(
            views(&result.document)
                .get(cue)
                .map(|view| view.text.as_str()),
            Some("Something at last."),
            "{fixture}: the text must land in the cue, not beside it"
        );
        assert_eq!(
            result.document.cues().count(),
            document.cues().count(),
            "{fixture}: writing text must not add a cue"
        );
        assert_reopens_identically(&result.document, fixture);
    }
}

#[test]
fn clearing_a_cue_s_text_removes_its_text_line_and_leaves_the_cue() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let result = edit(
        &document,
        &Edit::SetText {
            cue: 1,
            text: String::new(),
        },
    )
    .expect("a cue may be emptied");

    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "clear");
    assert_eq!(result.document.cues().count(), 3);
    assert_eq!(views(&result.document)[1].text, "");
    assert_other_cues_intact(&document, &result.document, 1, 1, 1, "clear");
    assert_reopens_identically(&result.document, "clear");

    let error = edit(
        &result.document,
        &Edit::SetText {
            cue: 1,
            text: String::new(),
        },
    )
    .expect_err("emptying an empty cue asks for nothing");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);
}

#[test]
fn a_crlf_file_keeps_its_terminators() {
    let (document, bytes) = open("srt/clean/basic-crlf.srt");
    let result = edit(
        &document,
        &Edit::SetText {
            cue: 0,
            text: "first line\nsecond line".to_owned(),
        },
    )
    .expect("a crlf cue takes multi-line text");

    let after = result.document.to_bytes();
    assert!(
        String::from_utf8_lossy(&after).contains("first line\r\nsecond line"),
        "the new line break must be written the way the file writes them"
    );
    assert_eq!(
        after.iter().filter(|&&byte| byte == b'\n').count(),
        after.windows(2).filter(|pair| pair == b"\r\n").count(),
        "no lone line feed may appear in a crlf file"
    );
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "crlf");
    assert_reopens_identically(&result.document, "crlf");
}

#[test]
fn a_byte_order_mark_survives_an_edit() {
    for fixture in [
        "srt/clean/bom-crlf.srt",
        "ass/clean/bom-lf.ass",
        "vtt/clean/header-text-crlf.vtt",
    ] {
        let (document, bytes) = open(fixture);
        assert!(document.source().has_bom(), "{fixture} must carry a bom");
        let result = edit(
            &document,
            &Edit::SetText {
                cue: 0,
                text: "Edited.".to_owned(),
            },
        )
        .unwrap_or_else(|error| panic!("{fixture}: {error}"));

        let after = result.document.to_bytes();
        assert_eq!(after.get(..3), Some(&[0xEF, 0xBB, 0xBF][..]), "{fixture}");
        assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, fixture);
    }
}

#[test]
fn dvd_coordinates_and_cue_settings_survive_a_timing_edit() {
    let (document, bytes) = open("srt/clean/coordinates-trailer.srt");
    let result = edit(
        &document,
        &Edit::SetTimes {
            cue: 0,
            start_ms: 2_000,
            end_ms: 4_000,
        },
    )
    .expect("times are editable");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert!(
        after.contains("00:00:02,000 --> 00:00:04,000  X1:040 X2:600 Y1:400 Y2:496"),
        "the trailer must survive untouched: {after}"
    );
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "trailer");

    let (document, bytes) = open("vtt/clean/cue-settings.vtt");
    let result = edit(
        &document,
        &Edit::SetTimes {
            cue: 0,
            start_ms: 1_500,
            end_ms: 3_000,
        },
    )
    .expect("times are editable");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert!(
        after.contains("00:00:01.500 --> 00:00:03.000 align:start position:10%,line-left"),
        "the cue settings must survive untouched: {after}"
    );
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "settings");
}

#[test]
fn a_timing_edit_is_spelled_the_way_the_file_spells_timings() {
    let (document, _) = open("srt/clean/dot-millis.srt");
    let result = edit(
        &document,
        &Edit::SetTimes {
            cue: 0,
            start_ms: 2_500,
            end_ms: 4_750,
        },
    )
    .expect("a tenth and a hundredth are both representable here");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert!(
        after.contains("00:00:02.5 --> 00:00:04.75"),
        "the separator and the fraction width must be mirrored: {after}"
    );

    let error = edit(
        &document,
        &Edit::SetTimes {
            cue: 0,
            start_ms: 2_550,
            end_ms: 4_750,
        },
    )
    .expect_err("one fraction digit cannot hold 2550 ms");
    assert_eq!(error.kind, EditErrorKind::UnwritableTimecode);
}

// ---------------------------------------------------------------------------------------------
// Insert, delete, split, merge
// ---------------------------------------------------------------------------------------------

#[test]
fn inserting_a_cue_never_renumbers_the_cues_after_it() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let result = edit(
        &document,
        &Edit::Insert {
            before: 1,
            start_ms: 4_900,
            end_ms: 4_990,
            text: "Squeezed in.".to_owned(),
        },
    )
    .expect("a cue may be inserted");

    let after = views(&result.document);
    assert_eq!(after.len(), 4);
    assert_eq!(after[1].text, "Squeezed in.");
    assert_eq!(
        after[1].number,
        Some(2),
        "the new block mirrors its neighbour"
    );
    assert_eq!(
        after.iter().map(|view| view.number).collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(2), Some(3)],
        "no cue the user did not edit may be renumbered"
    );
    assert_eq!(result.cue_delta, 1);
    assert_other_cues_intact(&document, &result.document, 1, 0, 1, "insert");
    assert_reopens_identically(&result.document, "insert");

    // Byte locality holds for an insert too: the splice inserts and copies, it never rebuilds.
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "insert");
}

#[test]
fn inserting_mirrors_the_neighbours_shape_without_borrowing_its_identity() {
    let (document, _) = open("srt/clean/no-index.srt");
    let result = edit(
        &document,
        &Edit::Insert {
            before: 1,
            start_ms: 3_050,
            end_ms: 3_090,
            text: "Still no numbering.".to_owned(),
        },
    )
    .expect("a numberless file keeps its style");
    assert!(
        views(&result.document)
            .iter()
            .all(|view| view.number.is_none()),
        "a file without index lines must not grow one"
    );
    assert_reopens_identically(&result.document, "no-index");

    let (document, _) = open("vtt/clean/cue-identifiers.vtt");
    let result = edit(
        &document,
        &Edit::Insert {
            before: 1,
            start_ms: 2_900,
            end_ms: 2_950,
            text: "No identifier here.".to_owned(),
        },
    )
    .expect("a vtt cue may be inserted");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert_eq!(
        after.matches("opening-line").count(),
        1,
        "an identifier must never be duplicated: {after}"
    );
    assert!(after.contains("00:00:02.900 --> 00:00:02.950\nNo identifier here."));
    assert_reopens_identically(&result.document, "vtt insert");
}

#[test]
fn inserting_into_an_ass_file_copies_every_field_it_does_not_own() {
    let (document, _) = open("ass/clean/basic.ass");
    let result = edit(
        &document,
        &Edit::Insert {
            before: 1,
            start_ms: 3_990,
            end_ms: 4_110,
            text: "Copied shape.".to_owned(),
        },
    )
    .expect("an event may be inserted");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert!(
        after.contains("Dialogue: 0,0:00:03.99,0:00:04.11,Default,,0,0,0,,Copied shape.\r\n"),
        "the new event must satisfy the section's Format line: {after}"
    );
    assert_reopens_identically(&result.document, "ass insert");
}

#[test]
fn an_ass_file_with_no_event_refuses_an_insert_rather_than_guessing_its_fields() {
    let document = parse(
        SubtitleFormat::Ass,
        "[Script Info]\nTitle: Nothing timed yet\n\n[Events]\nFormat: Layer, Start, End, Text\n",
    );
    let error = edit(
        &document,
        &Edit::Insert {
            before: 0,
            start_ms: 0,
            end_ms: 1_000,
            text: "Guessed".to_owned(),
        },
    )
    .expect_err("there is no event to copy a shape from");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);
}

#[test]
fn inserting_into_a_file_with_no_cues_writes_the_first_one() {
    for (format, text, fixture) in [
        (SubtitleFormat::Srt, String::new(), "empty"),
        (
            SubtitleFormat::Srt,
            std::fs::read_to_string(root().join("srt/clean/only-blank-lines.srt"))
                .expect("fixture is readable"),
            "only-blank-lines",
        ),
        (SubtitleFormat::Vtt, "WEBVTT\n".to_owned(), "header only"),
        (
            SubtitleFormat::Vtt,
            "WEBVTT".to_owned(),
            "header, no newline",
        ),
    ] {
        let document = parse(format, &text);
        assert_eq!(document.cues().count(), 0, "{fixture}");
        let result = edit(
            &document,
            &Edit::Insert {
                before: 0,
                start_ms: 1_000,
                end_ms: 2_000,
                text: "The first line.".to_owned(),
            },
        )
        .unwrap_or_else(|error| panic!("{fixture}: {error}"));

        assert_eq!(result.document.cues().count(), 1, "{fixture}");
        assert_eq!(
            views(&result.document)[0].text,
            "The first line.",
            "{fixture}"
        );
        assert!(
            String::from_utf8_lossy(&result.document.to_bytes()).starts_with(&text),
            "{fixture}: the bytes that were there must still open the file"
        );
        assert_reopens_identically(&result.document, fixture);
    }
}

#[test]
fn appending_to_a_file_without_a_final_newline_keeps_it_without_one() {
    for fixture in [
        "srt/clean/no-final-newline.srt",
        "ass/clean/no-trailing-newline.ass",
        "vtt/clean/blank-and-eof.vtt",
    ] {
        let (document, bytes) = open(fixture);
        assert_ne!(
            bytes.last(),
            Some(&b'\n'),
            "{fixture} ends without a newline"
        );
        let count = document.cues().count();
        let result = edit(
            &document,
            &Edit::Insert {
                before: count,
                start_ms: 30_000,
                end_ms: 31_000,
                text: "Appended.".to_owned(),
            },
        )
        .unwrap_or_else(|error| panic!("{fixture}: {error}"));

        let after = result.document.to_bytes();
        assert_ne!(
            after.last(),
            Some(&b'\n'),
            "{fixture}: the missing final newline is a quirk, not a defect to fix"
        );
        assert_eq!(result.document.cues().count(), count + 1, "{fixture}");
        assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, fixture);
        assert_reopens_identically(&result.document, fixture);
    }
}

#[test]
fn deleting_a_cue_takes_its_blank_line_with_it() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let result = edit(&document, &Edit::Delete { cue: 1 }).expect("a cue may be deleted");

    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert_eq!(result.document.cues().count(), 2);
    assert_eq!(result.cue_delta, -1);
    assert!(!after.contains("Nobody had told the crew"));
    assert!(
        after.contains("The harbour was empty when we got there.\n\n3\n"),
        "the gap must not grow: {after}"
    );
    assert_other_cues_intact(&document, &result.document, 1, 1, 0, "delete");
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "delete");
    assert_reopens_identically(&result.document, "delete");
}

#[test]
fn deleting_the_last_cue_of_a_file_leaves_a_file_that_still_opens() {
    let document = parse(
        SubtitleFormat::Srt,
        "1\n00:00:01,000 --> 00:00:02,000\nAlone.\n",
    );
    let result = edit(&document, &Edit::Delete { cue: 0 }).expect("the only cue may go");
    assert!(result.document.to_bytes().is_empty());
    assert_eq!(result.document.cues().count(), 0);
    assert_reopens_identically(&result.document, "empty out");

    // Deleting the last of several takes the blank line that separated it from the one before.
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let result = edit(&document, &Edit::Delete { cue: 2 }).expect("the last cue may go");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert!(
        after.ends_with("so we sat on the dock until it got light.\n"),
        "{after}"
    );
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "delete last");
}

#[test]
fn deleting_an_ass_event_takes_only_its_own_line() {
    let (document, bytes) = open("ass/clean/comments-and-semicolons.ass");
    let before = document.cues().count();
    let result = edit(&document, &Edit::Delete { cue: 1 }).expect("an event may be deleted");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");

    assert_eq!(result.document.cues().count(), before - 1);
    assert!(
        after.contains("[Events]\r\n"),
        "the section header must stay"
    );
    assert!(
        after.contains("; Timing pass by Marek"),
        "comments must stay"
    );
    assert!(!after.contains("You are on the schedule for tonight."));
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "ass delete");
    assert_reopens_identically(&result.document, "ass delete");
}

/// Regression: an event with a blank run on both sides used to leave the two runs adjacent, which
/// reparse merges into one, so the plan's segment count was wrong and the delete was refused.
#[test]
fn deleting_an_ass_event_between_two_blank_runs_takes_one_of_them() {
    let (document, bytes) = open("ass/clean/blank-between-events.ass");
    let result = edit(&document, &Edit::Delete { cue: 1 }).expect("a middle event may be deleted");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");

    assert_eq!(result.document.cues().count(), 2);
    assert!(!after.contains("The second line."));
    assert!(
        after.contains("The first line.\n\nDialogue: 0,0:00:05.10"),
        "one blank line must still separate the two survivors: {after}"
    );
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "ass delete middle");
    assert_reopens_identically(&result.document, "ass delete middle");

    // The same shape with runs of different lengths, and CRLF against LF in one file.
    let (document, bytes) = open("ass/clean/blank-lines-between-sections.ass");
    let result = edit(&document, &Edit::Delete { cue: 1 }).expect("the last event may be deleted");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");

    assert_eq!(result.document.cues().count(), 1);
    assert!(!after.contains("And a blank line sits above it."));
    assert!(
        after.ends_with("return sits inside this line.\n\n"),
        "the blank above the deleted event stays: {after:?}"
    );
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "ass delete last");
    assert_reopens_identically(&result.document, "ass delete last");
}

#[test]
fn splitting_a_cue_keeps_its_shape_and_gives_the_second_half_the_next_number() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let text = views(&document)[1].text.clone();
    let offset = text
        .find('\n')
        .expect("cue 2 of the fixture holds two lines");

    let result = edit(
        &document,
        &Edit::Split {
            cue: 1,
            text_offset: offset,
            at_ms: 6_500,
        },
    )
    .expect("a two-line cue may be split");

    let after = views(&result.document);
    assert_eq!(after.len(), 4);
    assert_eq!(after[1].text, "Nobody had told the crew we were coming,");
    assert_eq!(after[2].text, "so we sat on the dock until it got light.");
    assert_eq!((after[1].start_ms, after[1].end_ms), (5_000, 6_500));
    assert_eq!((after[2].start_ms, after[2].end_ms), (6_500, 8_340));
    assert_eq!(after[1].number, Some(2));
    assert_eq!(
        after[2].number,
        Some(3),
        "the second half takes the next number"
    );
    assert_eq!(
        after[3].number,
        Some(3),
        "the cue after it is not renumbered"
    );
    assert_other_cues_intact(&document, &result.document, 1, 1, 2, "split");
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "split");
    assert_reopens_identically(&result.document, "split");
}

#[test]
fn splitting_refuses_an_offset_that_would_leave_a_half_with_no_text() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    for offset in [0, views(&document)[0].text.len()] {
        let error = edit(
            &document,
            &Edit::Split {
                cue: 0,
                text_offset: offset,
                at_ms: 3_000,
            },
        )
        .expect_err("an empty half is not a cue");
        assert_eq!(error.kind, EditErrorKind::NotApplicable);
    }

    let error = edit(
        &document,
        &Edit::Split {
            cue: 0,
            text_offset: 4,
            at_ms: 99_000,
        },
    )
    .expect_err("the split point must lie inside the cue");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);
    assert_eq!(document.to_bytes(), bytes);
}

#[test]
fn splitting_refuses_an_offset_that_cuts_a_character() {
    let (document, _) = open("srt/clean/non-latin.srt");
    let text = views(&document)[0].text.clone();
    let inside = text
        .char_indices()
        .find(|(_, character)| character.len_utf8() > 1)
        .map(|(index, _)| index + 1)
        .expect("the fixture holds a multi-byte character");
    assert!(
        !text.is_char_boundary(inside),
        "the offset must land inside a character for this test to mean anything"
    );

    let error = edit(
        &document,
        &Edit::Split {
            cue: 0,
            text_offset: inside,
            at_ms: 2_000,
        },
    )
    .expect_err("an offset inside a character is not a split point");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);
}

#[test]
fn merging_two_cues_spans_their_times_and_joins_their_text() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let result = edit(&document, &Edit::Merge { cue: 0 }).expect("two cues may be merged");

    let after = views(&result.document);
    assert_eq!(after.len(), 2);
    assert_eq!(
        after[0].text,
        "The harbour was empty when we got there.\nNobody had told the crew we were coming,\nso we sat on the dock until it got light."
    );
    assert_eq!((after[0].start_ms, after[0].end_ms), (2_120, 8_340));
    assert_eq!(result.cue_delta, -1);
    assert_other_cues_intact(&document, &result.document, 0, 2, 1, "merge");
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "merge");
    assert_reopens_identically(&result.document, "merge");
}

#[test]
fn merging_ass_events_joins_them_with_the_escape_the_format_uses() {
    let (document, _) = open("ass/clean/basic.ass");
    let result = edit(&document, &Edit::Merge { cue: 0 }).expect("two events may be merged");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert!(
        after.contains(
            "The harbour freezes over by December.\\NThen we sail in November, like everyone else."
        ),
        "an ass merge joins with \\N: {after}"
    );
    assert_eq!(result.document.cues().count(), document.cues().count() - 1);
    assert_reopens_identically(&result.document, "ass merge");
}

#[test]
fn merging_refuses_to_swallow_content_that_belongs_to_neither_cue() {
    let document = parse(
        SubtitleFormat::Vtt,
        "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nOne.\n\nNOTE the client asked for this\n\n00:00:03.000 --> 00:00:04.000\nTwo.\n",
    );
    let error = edit(&document, &Edit::Merge { cue: 0 })
        .expect_err("a NOTE between two cues belongs to neither");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);

    let document = parse(
        SubtitleFormat::Ass,
        concat!(
            "[Events]\n",
            "Format: Layer, Start, End, Text\n",
            "Dialogue: 0,0:00:01.00,0:00:02.00,One\n",
            "; a note the encoder scripts read\n",
            "Dialogue: 0,0:00:03.00,0:00:04.00,Two\n",
        ),
    );
    let error = edit(&document, &Edit::Merge { cue: 0 })
        .expect_err("a comment line between two events belongs to neither");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);
}

#[test]
fn ass_comment_events_are_addressed_by_cue_index_and_stay_comments() {
    let (document, bytes) = open("ass/clean/comments-and-semicolons.ass");
    assert_eq!(document.cues().count(), 5);
    assert_eq!(document.displayed_cue_count(), 3);
    assert!(
        views(&document)[0].comment,
        "cue 0 of the fixture is a Comment"
    );

    let result = edit(
        &document,
        &Edit::SetText {
            cue: 0,
            text: "TODO: still checking".to_owned(),
        },
    )
    .expect("a comment event is editable");

    let after = views(&result.document);
    assert!(
        after[0].comment,
        "an edit must not promote a comment to a line"
    );
    assert_eq!(after[0].text, "TODO: still checking");
    assert_eq!(result.document.displayed_cue_count(), 3);
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "ass comment");
}

// ---------------------------------------------------------------------------------------------
// The guard, and the algebra undo leans on
// ---------------------------------------------------------------------------------------------

#[test]
fn every_mutation_re_runs_the_coverage_guard_and_can_be_undone_byte_for_byte() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let requests = [
        Edit::SetText {
            cue: 0,
            text: "Rewritten.".to_owned(),
        },
        Edit::SetTimes {
            cue: 1,
            start_ms: 5_500,
            end_ms: 8_000,
        },
        Edit::Insert {
            before: 3,
            start_ms: 12_000,
            end_ms: 13_000,
            text: "Appended.".to_owned(),
        },
        Edit::Delete { cue: 0 },
        Edit::Split {
            cue: 1,
            text_offset: 5,
            at_ms: 6_000,
        },
        Edit::Merge { cue: 0 },
    ];

    for request in requests {
        let result = edit(&document, &request)
            .unwrap_or_else(|error| panic!("{request:?} must apply: {error}"));
        assert_eq!(
            result.document.check_coverage(),
            Ok(()),
            "{request:?}: the coverage guard must hold after every mutation"
        );

        // The inverse splice is what undo replays; it must restore the file exactly.
        let restored = sublore_edit::plan::replay(
            &result.document,
            &result.splice.inverse(),
            -result.cue_delta,
        )
        .unwrap_or_else(|error| panic!("{request:?} must be undoable: {error}"));
        assert_eq!(
            restored.to_bytes(),
            bytes,
            "{request:?}: undo must restore the exact original bytes"
        );
    }
}

#[test]
fn a_stale_splice_is_refused_rather_than_applied_at_the_wrong_place() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let first = edit(
        &document,
        &Edit::SetText {
            cue: 0,
            text: "Rewritten once.".to_owned(),
        },
    )
    .expect("the first edit applies");

    // The splice that was computed against the original bytes no longer matches the edited ones.
    let error = sublore_edit::plan::replay(&first.document, &first.splice, 0)
        .expect_err("a splice may only be applied to the bytes it recorded");
    assert_eq!(error.kind, EditErrorKind::StaleSplice);
    assert_eq!(document.to_bytes(), bytes);
}

#[test]
fn a_replay_that_moves_the_cue_count_the_wrong_way_is_refused() {
    let (document, _) = open("srt/clean/basic-lf.srt");
    let deletion = edit(&document, &Edit::Delete { cue: 1 }).expect("a cue may be deleted");
    let error = sublore_edit::plan::replay(&document, &deletion.splice, 0)
        .expect_err("the recorded delta must match what the bytes do");
    assert_eq!(error.kind, EditErrorKind::Unverified);
}

#[test]
fn an_ass_event_keeps_its_kind_and_its_fields_through_a_timing_edit() {
    let (document, bytes) = open("ass/clean/field-order-shuffled.ass");
    let result = edit(
        &document,
        &Edit::SetTimes {
            cue: 0,
            start_ms: 62_000,
            end_ms: 66_000,
        },
    )
    .expect("a shuffled Format line is still editable");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert!(
        after.contains("Dialogue: 0:01:02.00,0:01:06.00,0,Default,,0,0,0,,The field order is legal, just unusual."),
        "start and end must be written where the Format line puts them: {after}"
    );
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "shuffled");
    assert_reopens_identically(&result.document, "shuffled");
}

#[test]
fn an_ass_event_with_commas_in_its_text_keeps_them() {
    let (document, bytes) = open("ass/clean/text-with-commas.ass");
    let result = edit(
        &document,
        &Edit::SetText {
            cue: 1,
            text: "Rope, lamp, battery, map, and the kettle.".to_owned(),
        },
    )
    .expect("commas belong to the last field");
    assert_eq!(
        views(&result.document)[1].text,
        "Rope, lamp, battery, map, and the kettle."
    );
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "commas");
    assert_reopens_identically(&result.document, "commas");
    assert!(matches!(
        result.document.cues().nth(1).map(|cue| &cue.detail),
        Some(CueDetail::Ass(event)) if event.kind == AssEventKind::Dialogue
    ));
}

#[test]
fn a_cue_can_be_inserted_with_no_text_at_all() {
    // The shape the property test walked into: a block with a timing line and nothing under it,
    // which is what empty-text.srt already holds.
    for relative in ["srt/clean/basic-lf.srt", "vtt/clean/basic.vtt"] {
        let (before, original) = open(relative);
        let count = before.cues().count();
        let edited = edit(
            &before,
            &Edit::Insert {
                before: 1,
                start_ms: 4_900,
                end_ms: 4_950,
                text: String::new(),
            },
        )
        .unwrap_or_else(|error| panic!("{relative}: {error}"));

        assert_eq!(edited.document.cues().count(), count + 1, "{relative}");
        assert_eq!(
            views(&edited.document)
                .get(1)
                .map(|view| view.text.as_str()),
            Some(""),
            "{relative}"
        );
        assert_bytes_outside_the_edit_are_identical(&original, &before, &edited, relative);
        assert_reopens_identically(&edited.document, relative);
        assert_other_cues_intact(&before, &edited.document, 1, 0, 1, relative);
    }
}

#[test]
fn verify_catches_a_line_break_smuggled_into_an_ass_field() {
    // The net under the pre-check: this edit parses and round-trips, and only the segment-kind
    // prediction sees the second half of the text turn into a metadata line.
    let (before, bytes) = open("ass/clean/basic.ass");
    let text = String::from_utf8(bytes).expect("the fixture is utf-8");
    let broken = text.replacen(
        "The harbour freezes over by December.",
        "The harbour\r\nfreezes over by December.",
        1,
    );
    let after = sublore_formats::parse(SubtitleFormat::Ass, broken.as_bytes())
        .expect("the broken file still parses, which is why verify has to catch it");

    let segments_from = before
        .segments()
        .iter()
        .position(|segment| matches!(segment.kind, sublore_formats::SegmentKind::Cue(_)))
        .expect("the fixture holds events");
    let expectation = Expectation {
        from: 0,
        removed: 1,
        cues: vec![ExpectedCue {
            text_raw: "The harbour\r\nfreezes over by December.".to_owned(),
            start_ms: 1_340,
            end_ms: 3_980,
        }],
        segments_from,
        segments_removed: 1,
        segments_inserted: 1,
    };
    let error = verify(&before, &after, &expectation)
        .expect_err("a cue that grew a metadata line is not the document the plan predicted");
    assert_eq!(error.kind, EditErrorKind::Unverified);
}

#[test]
fn a_carriage_return_against_a_line_break_costs_that_byte_and_only_that_cue() {
    // "a\r\r\n" is a line whose own content ends with a carriage return. The normalized wire form
    // cannot tell that `\r` from a terminator, so editing this cue's text drops it. That is the
    // documented cost of the wire form, and this test pins its blast radius: one cue, undoable,
    // every other byte untouched. Editing anything else about the cue keeps the byte.
    let text = "1\r\n00:00:01,000 --> 00:00:02,000\r\na\r\r\nb\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nUntouched.\r\n";
    let original = text.as_bytes().to_vec();
    let document = parse(SubtitleFormat::Srt, text);
    assert_eq!(
        document.slice(document.cues().next().expect("cue 0").text),
        "a\r\r\nb"
    );

    let rewritten = edit(
        &document,
        &Edit::SetText {
            cue: 0,
            text: views(&document)[0].text.clone(),
        },
    )
    .expect("the cue stays editable; the wire form is what loses the byte");

    assert_eq!(
        rewritten
            .document
            .slice(rewritten.document.cues().next().expect("cue 0").text),
        "a\r\nb",
        "the carriage return against the line break is the byte the wire form cannot carry"
    );
    assert_bytes_outside_the_edit_are_identical(&original, &document, &rewritten, "lone cr");
    assert_other_cues_intact(&document, &rewritten.document, 0, 1, 1, "lone cr");
    assert_reopens_identically(&rewritten.document, "lone cr");

    // Undo puts the byte back, which is what makes the cost recoverable rather than data loss.
    let restored = sublore_edit::plan::replay(&rewritten.document, &rewritten.splice.inverse(), 0)
        .expect("the edit is undoable");
    assert_eq!(restored.to_bytes(), original);

    // A trailing carriage return would change the line count, so that one is refused outright.
    let error = edit(
        &document,
        &Edit::SetText {
            cue: 0,
            text: "ends with a return\r".to_owned(),
        },
    )
    .expect_err("a trailing carriage return cannot be written back");
    assert_eq!(error.kind, EditErrorKind::UnwritableText);
}

#[test]
fn an_index_line_wider_than_the_scanner_reads_is_refused() {
    // The SRT scanner stops at nine digits, so a tenth would be read as cue text, not an index.
    let document = parse(
        SubtitleFormat::Srt,
        "999999999\n00:00:01,000 --> 00:00:02,000\nThe last number.\n",
    );
    let error = edit(
        &document,
        &Edit::Insert {
            before: 1,
            start_ms: 3_000,
            end_ms: 4_000,
            text: "One too many.".to_owned(),
        },
    )
    .expect_err("the next index would be ten digits wide");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);
    assert_eq!(document.cues().count(), 1);
}

#[test]
fn merging_refuses_a_join_that_would_spell_a_terminator_it_did_not_mean() {
    // The first cue's text ends with a carriage return of its own. Joining it to the next cue with
    // a line break would spell `\r\n`, which the wire form cannot tell from one terminator, so the
    // merge is refused rather than guessed at. Conservative on purpose: nothing is written.
    let text = "1\r\n00:00:01,000 --> 00:00:02,000\r\nends with a return\r\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nSecond.\r\n";
    let document = parse(SubtitleFormat::Srt, text);
    assert!(
        views(&document)[0].text.ends_with('\r'),
        "the fixture carries the quirk"
    );

    let error = edit(&document, &Edit::Merge { cue: 0 })
        .expect_err("the join would spell a terminator the file did not have");
    assert_eq!(error.kind, EditErrorKind::UnwritableText);
    assert_eq!(document.to_bytes(), text.as_bytes());
}

// F1: one edit that rewrites many cues. The splice runs from the first byte any of them changes to
// the last, so the cues between ride along unchanged, and that is what makes it one undo step
// rather than one per cue. See docs/find-replace-tasks.md.

#[test]
fn rewriting_the_first_and_last_cue_carries_the_one_between_them_along() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let untouched =
        "Nobody had told the crew we were coming,\nso we sat on the dock until it got light.";

    let result = edit(
        &document,
        &Edit::SetTexts {
            edits: vec![
                (0, "The harbour was full.".to_owned()),
                (2, "By then the fog had lifted.".to_owned()),
            ],
        },
    )
    .expect("two cues in file order are editable");

    // The span really does cross the middle cue, which is the whole point: a plan that spliced each
    // cue separately would carry none of it and would be two undo steps.
    assert!(
        result.splice.removed.contains(untouched),
        "the splice must span the cue between the two it rewrites: {:?}",
        result.splice.removed
    );
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    assert!(after.contains("The harbour was full."));
    assert!(after.contains("By then the fog had lifted."));
    assert!(
        after.contains(untouched),
        "the cue between them must read back exactly as it was: {after}"
    );
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "two of three");
    assert_reopens_identically(&result.document, "two of three");
}

#[test]
fn a_many_cue_edit_naming_one_cue_plans_the_bytes_the_single_cue_edit_plans() {
    let (document, _) = open("srt/clean/basic-lf.srt");
    let text = "Rewritten.";

    let one = sublore_edit::plan::plan(
        &document,
        &Edit::SetText {
            cue: 1,
            text: text.to_owned(),
        },
    )
    .expect("the single-cue plan");
    let many = sublore_edit::plan::plan(
        &document,
        &Edit::SetTexts {
            edits: vec![(1, text.to_owned())],
        },
    )
    .expect("the many-cue plan naming one");

    // Same bytes, same prediction. Only the label differs, and deliberately: the history coalesces
    // by label, and a replace must never merge into the keystroke before it.
    assert_eq!(many.splice, one.splice);
    assert_eq!(many.expect.from, one.expect.from);
    assert_eq!(many.expect.removed, one.expect.removed);
    assert_eq!(many.expect.cues, one.expect.cues);
    assert_eq!(many.expect.segments_from, one.expect.segments_from);
    assert_eq!(many.expect.segments_removed, one.expect.segments_removed);
    assert_eq!(many.expect.segments_inserted, one.expect.segments_inserted);
    assert_ne!(many.label.kind, one.label.kind);
}

#[test]
fn a_cue_named_twice_is_refused_and_writes_nothing() {
    let (document, bytes) = open("srt/clean/basic-lf.srt");
    let error = edit(
        &document,
        &Edit::SetTexts {
            edits: vec![(1, "first".to_owned()), (1, "second".to_owned())],
        },
    )
    .expect_err("one cue cannot take two writes");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);
    assert_eq!(document.to_bytes(), bytes);
}

#[test]
fn cues_out_of_file_order_are_refused() {
    let (document, _) = open("srt/clean/basic-lf.srt");
    let error = edit(
        &document,
        &Edit::SetTexts {
            edits: vec![(2, "last".to_owned()), (0, "first".to_owned())],
        },
    )
    .expect_err("the span is built in file order, so the list must be in it");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);
}

#[test]
fn a_text_edit_naming_no_cues_is_refused_rather_than_recorded() {
    let (document, _) = open("srt/clean/basic-lf.srt");
    let error = edit(&document, &Edit::SetTexts { edits: Vec::new() })
        .expect_err("a replace that matched nothing must not become an undo step");
    assert_eq!(error.kind, EditErrorKind::NotApplicable);
}

#[test]
fn a_many_cue_edit_reaching_past_the_last_cue_is_refused() {
    let (document, _) = open("srt/clean/basic-lf.srt");
    let error = edit(
        &document,
        &Edit::SetTexts {
            edits: vec![(1, "here".to_owned()), (9, "nowhere".to_owned())],
        },
    )
    .expect_err("cue 9 does not exist in a three cue file");
    assert_eq!(error.kind, EditErrorKind::NoSuchCue);
}

#[test]
fn every_cue_of_an_ass_file_can_be_rewritten_at_once() {
    let (document, bytes) = open("ass/clean/basic.ass");
    let count = document.cues().count();
    let edits: Vec<_> = (0..count).map(|cue| (cue, format!("line {cue}"))).collect();

    let result = edit(&document, &Edit::SetTexts { edits }).expect("every cue at once");
    let after = String::from_utf8(result.document.to_bytes()).expect("utf-8");
    for cue in 0..count {
        assert!(
            after.contains(&format!("line {cue}")),
            "cue {cue} was not rewritten: {after}"
        );
    }
    assert_bytes_outside_the_edit_are_identical(&bytes, &document, &result, "every ass cue");
    assert_reopens_identically(&result.document, "every ass cue");
}
