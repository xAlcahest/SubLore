//! ASS/SSA fixtures. Every clean file must come back byte for byte, and every malformed file must
//! fail on the line its sidecar names. See BACKLOG.md M1.3.

mod common;

use std::path::{Path, PathBuf};

use common::{assert_expected_error, assert_round_trip, dirs, fixtures};
use sublore_formats::{
    AssEvent, AssEventKind, AssField, Cue, CueDetail, Newline, SegmentKind, SubtitleDocument,
    SubtitleFormat,
};

const FORMAT: SubtitleFormat = SubtitleFormat::Ass;

/// Guards against a green suite that tests nothing: deleting fixtures must turn CI red.
const MIN_CLEAN: usize = 17;
const MIN_MALFORMED: usize = 7;

/// `.ssa` is the legacy spelling of the same grammar and lives in the same tree.
const EXTENSIONS: [&str; 2] = ["ass", "ssa"];

/// The shared harness checks the spans every format has. This adds the ones only an ASS event
/// carries: the descriptor, every declared field, and the field the text is taken from.
fn round_trip(path: &Path, bytes: &[u8]) -> SubtitleDocument {
    let document = assert_round_trip(FORMAT, path, bytes);
    let name = path.display();
    for (index, segment) in document.segments().iter().enumerate() {
        let SegmentKind::Cue(cue) = &segment.kind else {
            continue;
        };
        let CueDetail::Ass(event) = &cue.detail else {
            panic!("{name}: segment {index} is a cue without an ASS event");
        };
        assert!(
            event.text_field < event.fields.len(),
            "{name}: segment {index} points at field {} of {}",
            event.text_field,
            event.fields.len()
        );
        assert_eq!(
            event.fields.get(event.text_field).copied(),
            Some(cue.text),
            "{name}: segment {index} must take its text from the field it declares"
        );
        // A named field at or past the text would be pointing inside the dialogue, and the column
        // reads it through `slice`, which is a `debug_assert!` and an empty string in release.
        for field in AssField::ALL {
            let Some(at) = event.field_index(field) else {
                continue;
            };
            assert!(
                at < event.text_field,
                "{name}: segment {index} names its {} at field {at}, at or past the text field {}",
                field.as_str(),
                event.text_field
            );
        }
        for span in std::iter::once(event.descriptor).chain(event.fields.iter().copied()) {
            assert!(
                span.start >= segment.span.start && span.end <= segment.span.end,
                "{name}: segment {index} has a span {span:?} outside {:?}",
                segment.span
            );
        }
    }
    document
}

/// Parse one clean fixture by name, proving its round-trip on the way in.
fn open(name: &str) -> SubtitleDocument {
    let path = dirs("ass").clean.join(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    round_trip(&path, &bytes)
}

fn sidecar_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".expected");
    path.with_file_name(name)
}

fn cue_list(document: &SubtitleDocument) -> Vec<&Cue> {
    document.cues().collect()
}

fn event(cue: &Cue) -> &AssEvent {
    match &cue.detail {
        CueDetail::Ass(event) => event,
        other => panic!("an ASS cue must carry an ASS event, found {other:?}"),
    }
}

fn kind_name(kind: &SegmentKind) -> &'static str {
    match kind {
        SegmentKind::Header => "Header",
        SegmentKind::Blank => "Blank",
        SegmentKind::Meta => "Meta",
        SegmentKind::Cue(_) => "Cue",
    }
}

fn kind_sequence(document: &SubtitleDocument) -> Vec<&'static str> {
    document
        .segments()
        .iter()
        .map(|segment| kind_name(&segment.kind))
        .collect()
}

#[test]
fn round_trips_every_clean_fixture() {
    let clean = dirs("ass").clean;
    for (path, bytes) in fixtures(&clean, &EXTENSIONS, MIN_CLEAN) {
        round_trip(&path, &bytes);
    }
}

#[test]
fn reports_every_malformed_fixture() {
    let malformed = dirs("ass").malformed;
    let cases = fixtures(&malformed, &EXTENSIONS, MIN_MALFORMED);
    for (path, bytes) in &cases {
        assert_expected_error(FORMAT, path, bytes);
    }

    // A sidecar without a fixture, or a fixture without a sidecar, is a hole in the suite.
    let sidecars: Vec<PathBuf> = std::fs::read_dir(&malformed)
        .expect("the malformed directory is readable")
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("expected"))
        .collect();
    assert_eq!(
        sidecars.len(),
        cases.len(),
        "every malformed fixture needs exactly one sidecar"
    );
    for (path, _) in &cases {
        let sidecar = sidecar_path(path);
        assert!(
            sidecar.is_file(),
            "{} has no sidecar next to it",
            path.display()
        );
    }
}

#[test]
fn no_clean_fixture_parses_as_a_single_segment() {
    let clean = dirs("ass").clean;
    for (path, bytes) in fixtures(&clean, &EXTENSIONS, MIN_CLEAN) {
        let document = round_trip(&path, &bytes);
        let cues = document.cues().count();
        assert!(cues > 0, "{} must yield cues", path.display());
        assert!(
            document.segments().len() >= 2,
            "{} lumped the whole file into {} segment(s)",
            path.display(),
            document.segments().len()
        );
    }
}

#[test]
fn a_plain_crlf_file_keeps_its_dialogues_and_timings() {
    let document = open("basic.ass");
    let cues = cue_list(&document);

    assert_eq!(cues.len(), 3);
    assert_eq!(document.displayed_cue_count(), 3);
    assert_eq!(document.source().newline(), Newline::Crlf);
    assert!(!document.source().has_bom());

    assert_eq!(cues[0].start.millis(), 1_340);
    assert_eq!(cues[0].end.millis(), 3_980);
    assert_eq!(cues[2].end.millis(), 9_440);
    assert_eq!(document.slice(cues[0].start.raw()), "0:00:01.34");

    let second = event(cues[1]);
    assert_eq!(second.kind, AssEventKind::Dialogue);
    assert_eq!(document.slice(second.descriptor), "Dialogue");
    assert_eq!(second.fields.len(), 10);
    assert_eq!(second.text_field, 9);
    assert_eq!(document.slice(second.fields[4]), "Ingrid");
    assert_eq!(
        document.slice(cues[1].text),
        "Then we sail in November, like everyone else."
    );
}

#[test]
fn a_bom_and_a_line_by_line_tiling_survive() {
    let document = open("bom-lf.ass");

    assert!(document.source().has_bom());
    assert_eq!(document.source().newline(), Newline::Lf);
    assert_eq!(
        kind_sequence(&document),
        vec!["Meta", "Meta", "Meta", "Blank", "Meta", "Meta", "Cue", "Cue"]
    );

    let cues = cue_list(&document);
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].start.millis(), 500);
    assert_eq!(cues[1].end.millis(), 5_600);
    assert_eq!(document.slice(cues[0].text), "The ferry leaves at six.");
}

#[test]
fn comment_events_are_cues_but_never_displayed() {
    let document = open("comments-and-semicolons.ass");
    let cues = cue_list(&document);

    assert_eq!(cues.len(), 5);
    assert_eq!(document.displayed_cue_count(), 3);
    // Line by line, `;` comments and `Style:` lines included: metadata is kept, never folded away.
    assert_eq!(
        kind_sequence(&document),
        vec![
            "Meta", "Meta", "Meta", "Meta", "Meta", "Blank", "Meta", "Meta", "Meta", "Blank",
            "Meta", "Meta", "Cue", "Cue", "Cue", "Cue", "Cue",
        ]
    );
    let kinds: Vec<AssEventKind> = cues.iter().map(|cue| event(cue).kind).collect();
    assert_eq!(
        kinds,
        vec![
            AssEventKind::Comment,
            AssEventKind::Dialogue,
            AssEventKind::Comment,
            AssEventKind::Dialogue,
            AssEventKind::Dialogue,
        ]
    );
    assert_eq!(document.slice(event(cues[0]).descriptor), "Comment");
    assert_eq!(
        document.slice(cues[0].text),
        "TODO: check the sign timing against the raw"
    );
}

#[test]
fn the_text_field_keeps_every_comma_it_was_written_with() {
    let document = open("text-with-commas.ass");
    let cues = cue_list(&document);

    assert_eq!(cues.len(), 2);
    assert_eq!(event(cues[0]).fields.len(), 10);
    assert_eq!(
        document.slice(cues[0].text),
        r"Bring the rope, the lamp, and the spare battery,\Nand do not, under any circumstance,\hforget the map."
    );
    assert_eq!(document.slice(cues[1].text), "Rope, lamp, battery, map.");
}

#[test]
fn a_shuffled_format_line_still_finds_start_and_end() {
    let document = open("field-order-shuffled.ass");
    let cues = cue_list(&document);

    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].start.millis(), 62_500);
    assert_eq!(cues[0].end.millis(), 65_000);
    assert_eq!(cues[1].start.millis(), 65_100);
    assert_eq!(
        document.slice(cues[0].text),
        "The field order is legal, just unusual."
    );
}

#[test]
fn the_legacy_ssa_dialogue_keeps_its_marked_field() {
    let document = open("ssa-v4.ssa");
    let cues = cue_list(&document);

    assert_eq!(cues.len(), 2);
    // Fields are kept as written, leading space included: nothing is trimmed on the way in.
    assert_eq!(document.slice(event(cues[0]).fields[0]), " Marked=0");
    assert_eq!(cues[0].start.millis(), 2_100);
    assert_eq!(cues[1].end.millis(), 8_000);
    assert_eq!(
        document.slice(cues[0].text),
        "It never rains here, it only pours."
    );
}

#[test]
fn mixed_terminators_and_a_lone_carriage_return_are_content() {
    let document = open("blank-lines-between-sections.ass");
    let cues = cue_list(&document);

    assert_eq!(document.source().newline(), Newline::Mixed);
    assert_eq!(cues.len(), 2);
    assert_eq!(
        document.slice(cues[0].text),
        "A carriage\rreturn sits inside this line."
    );
    assert_eq!(kind_sequence(&document).first().copied(), Some("Blank"));
}

#[test]
fn unknown_sections_travel_through_as_metadata() {
    let document = open("unknown-sections.ass");

    assert_eq!(document.cues().count(), 2);
    assert_eq!(document.displayed_cue_count(), 2);
    let blob = document
        .segments()
        .iter()
        .filter(|segment| matches!(segment.kind, SegmentKind::Meta))
        .any(|segment| document.slice(segment.span).starts_with("!!W`4"));
    assert!(blob, "the [Fonts] blob must survive as metadata");
}

#[test]
fn override_tags_and_drawings_stay_inside_the_text_field() {
    let document = open("override-tags.ass");
    let cues = cue_list(&document);

    assert_eq!(cues.len(), 4);
    assert_eq!(
        document.slice(cues[0].text),
        r"{\an8\pos(320,50)\fad(200,200)}CLOSED FOR THE SEASON"
    );
    assert_eq!(
        document.slice(cues[1].text),
        r"{\p1}m 0 0 l 100 0 l 100 60 l 0 60{\p0}"
    );
    assert_eq!(
        document.slice(cues[2].text),
        r"{\i1}He wrote {curly braces} on the board{\i0}, twice."
    );
}

#[test]
fn non_latin_text_and_style_names_survive() {
    let document = open("non-latin.ass");
    let cues = cue_list(&document);

    assert_eq!(cues.len(), 4);
    assert_eq!(document.slice(event(cues[0]).fields[3]), "見出し");
    assert_eq!(document.slice(cues[0].text), "東京の夜は、雨の匂いがする。");
    assert_eq!(document.slice(cues[1].text), "هل وصلت الرسالة؟");
    assert_eq!(
        document.slice(cues[3].text),
        "\u{1f469}\u{200d}\u{1f680} \u{1f44d}\u{1f3fd} किताबें \u{20bb7}"
    );
}

#[test]
fn trailing_whitespace_inside_fields_is_left_alone() {
    let document = open("styles-many.ass");
    let cues = cue_list(&document);

    assert_eq!(cues.len(), 3);
    assert_eq!(document.slice(event(cues[2]).fields[3]), "Sign Top ");
    assert_eq!(document.slice(cues[2].text), "HARBOUR OFFICE ");
}

#[test]
fn a_file_that_ends_without_a_newline_is_still_tiled() {
    let document = open("no-trailing-newline.ass");
    let body = document.source().body();

    assert!(!body.ends_with('\n'));
    assert_eq!(document.cues().count(), 1);
    let last = document
        .segments()
        .last()
        .expect("a parsed file has segments");
    assert_eq!(last.span.end, body.len());
    assert!(matches!(last.kind, SegmentKind::Cue(_)));
}
