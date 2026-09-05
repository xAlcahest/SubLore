//! WEBVTT fixtures: byte-exact round trips, structured failures, and proof that the parser
//! understood the file rather than copying it. See BACKLOG.md M1.2.

use sublore_formats::{
    Cue, CueDetail, Newline, SegmentKind, SubtitleDocument, SubtitleFormat, VttCue,
};

mod common;

use common::{assert_expected_error, assert_round_trip, dirs, fixtures};

/// Deleting fixtures must turn the suite red, not green.
const MIN_CLEAN: usize = 10;
const MIN_MALFORMED: usize = 6;

#[test]
fn round_trips_every_clean_fixture() {
    let clean = dirs("vtt").clean;
    for (path, bytes) in fixtures(&clean, &["vtt"], MIN_CLEAN) {
        assert_round_trip(SubtitleFormat::Vtt, &path, &bytes);
    }
}

#[test]
fn reports_every_malformed_fixture() {
    let malformed = dirs("vtt").malformed;
    let broken = fixtures(&malformed, &["vtt"], MIN_MALFORMED);
    for (path, bytes) in &broken {
        assert_expected_error(SubtitleFormat::Vtt, path, bytes);
    }

    // A sidecar with no fixture would be a test that never runs.
    for (path, _) in fixtures(&malformed, &["expected"], MIN_MALFORMED) {
        let fixture = path.with_extension("");
        assert!(
            broken.iter().any(|(candidate, _)| *candidate == fixture),
            "{} describes a fixture that is not there",
            path.display()
        );
    }
}

#[test]
fn no_clean_fixture_parses_as_a_single_segment() {
    let clean = dirs("vtt").clean;
    for (path, bytes) in fixtures(&clean, &["vtt"], MIN_CLEAN) {
        let document = assert_round_trip(SubtitleFormat::Vtt, &path, &bytes);
        assert!(
            document.cues().count() > 0,
            "{} holds no cues, so it proves nothing",
            path.display()
        );
        assert!(
            document.segments().len() > 1,
            "{} was lumped into one segment",
            path.display()
        );
    }
}

#[test]
fn basic_reads_three_cues_with_their_timings() {
    let (_, document) = clean("basic.vtt");
    assert_eq!(document.cues().count(), 3);
    assert_eq!(document.displayed_cue_count(), 3);
    assert_eq!(
        times(&document),
        [(2_400, 5_120), (5_400, 8_960), (9_200, 11_500)]
    );
    assert_eq!(
        texts(&document)[0],
        "Right, so this is the part where I explain\nwhy the engine is on the passenger seat."
    );
    assert_eq!(
        kinds(&document),
        ["header", "blank", "cue", "blank", "cue", "blank", "cue"]
    );
    assert!(!document.source().has_bom());
    assert_eq!(document.source().newline(), Newline::Lf);
    assert_eq!(ids(&document), [None, None, None]);
}

#[test]
fn header_text_and_a_bom_survive_crlf() {
    let (bytes, document) = clean("header-text-crlf.vtt");
    assert!(document.source().has_bom());
    assert_eq!(document.source().newline(), Newline::Crlf);
    assert_eq!(bytes.len(), document.source().byte_len());
    assert_eq!(
        document.slice(document.segments()[0].span),
        "WEBVTT - Episode 1\r\n"
    );
    assert!(matches!(document.segments()[0].kind, SegmentKind::Header));
    assert_eq!(document.cues().count(), 2);
    assert_eq!(times(&document), [(0, 3_000), (3_500, 7_250)]);
    assert_eq!(ids(&document), [Some("1".to_owned()), Some("2".to_owned())]);
    assert_eq!(texts(&document)[1], "Nothing happened. Again.");
}

#[test]
fn notes_styles_and_regions_are_metadata_not_cues() {
    let (_, document) = clean("note-style-region.vtt");
    assert_eq!(
        kinds(&document),
        [
            "header", "blank", "meta", "blank", "meta", "blank", "meta", "blank", "cue", "blank",
            "meta", "blank", "cue"
        ]
    );
    assert_eq!(document.cues().count(), 2);
    assert_eq!(ids(&document)[0], Some("intro".to_owned()));
    assert_eq!(settings(&document)[0], Some("region:top-bar".to_owned()));
    assert!(document
        .slice(document.segments()[4].span)
        .starts_with("STYLE\n::cue {"));
}

#[test]
fn cue_identifiers_are_kept_as_written() {
    let (_, document) = clean("cue-identifiers.vtt");
    assert_eq!(
        ids(&document),
        [
            Some("opening-line".to_owned()),
            Some("42".to_owned()),
            Some("second act - beat 3".to_owned())
        ]
    );
    assert_eq!(times(&document)[1], (3_000, 4_900));
}

#[test]
fn cue_settings_are_kept_verbatim() {
    let (_, document) = clean("cue-settings.vtt");
    assert_eq!(
        settings(&document),
        [
            Some("align:start position:10%,line-left size:35% vertical:rl region:fred".to_owned()),
            Some("line:90% align:center".to_owned()),
            None
        ]
    );
    assert_eq!(texts(&document)[2], "No settings on this one at all.");
}

#[test]
fn short_timestamps_resolve_and_keep_their_spelling() {
    let (_, document) = clean("short-timestamps.vtt");
    assert_eq!(
        times(&document),
        [(1_000, 3_500), (3_600, 6_000), (599_900, 602_000)]
    );
    let spellings: Vec<&str> = document
        .cues()
        .map(|cue| document.slice(cue.start.raw()))
        .collect();
    assert_eq!(spellings, ["00:01.000", "00:00:03.600", "09:59.900"]);
}

#[test]
fn voice_and_inline_tags_stay_inside_the_payload() {
    let (_, document) = clean("voice-and-tags.vtt");
    let payloads = texts(&document);
    assert_eq!(
        payloads[0],
        "<v Fred>Do you hear that? <00:00:01.500>It's the neighbours again."
    );
    assert!(payloads[1].contains("<c.loud>") && payloads[1].contains("&amp;"));
    assert!(payloads[2].starts_with("<ruby>漢<rt>かん</rt>"));
    assert_eq!(times(&document)[0], (1_000, 4_000));
}

#[test]
fn non_latin_payloads_are_untouched() {
    let (_, document) = clean("non-latin.vtt");
    assert_eq!(document.cues().count(), 4);
    let payloads = texts(&document);
    assert_eq!(payloads[0], "黙って座っていろ。船が沈むまでだ。");
    assert!(payloads[3].contains('\u{1f468}') && payloads[3].contains('\u{200d}'));
}

#[test]
fn blank_runs_collapse_into_one_segment_and_eof_needs_no_newline() {
    let (bytes, document) = clean("blank-and-eof.vtt");
    assert_ne!(bytes.last(), Some(&b'\n'));
    assert_eq!(kinds(&document), ["header", "blank", "cue", "blank", "cue"]);
    assert_eq!(document.slice(document.segments()[3].span), "\n\t\n\n");
    assert_eq!(times(&document), [(1_000, 2_000), (2_500, 3_500)]);
    assert_eq!(
        texts(&document)[1],
        "And no newline at the end of this file."
    );
}

#[test]
fn the_timestamp_map_rides_with_the_header() {
    let (_, document) = clean("timestamp-map.vtt");
    assert_eq!(
        document.slice(document.segments()[0].span),
        "WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:900000\n"
    );
    assert_eq!(document.cues().count(), 2);
    assert_eq!(times(&document)[0], (10_000, 12_500));
}

#[test]
fn overlapping_and_backwards_cues_are_preserved_not_judged() {
    let (_, document) = clean("overlapping.vtt");
    assert_eq!(
        times(&document),
        [
            (1_000, 5_000),
            (3_000, 4_000),
            (4_000, 4_000),
            (9_000, 7_000)
        ]
    );
}

#[test]
fn a_lone_carriage_return_does_not_split_a_payload() {
    let (_, document) = clean("mixed-eol.vtt");
    assert_eq!(document.source().newline(), Newline::Mixed);
    assert_eq!(document.cues().count(), 3);
    let payload = texts(&document)[1];
    assert_eq!(
        payload,
        "This line carries a stray \rcarriage return and keeps going."
    );
    assert!(!payload.contains('\n'));
}

#[test]
fn a_cue_with_no_payload_gets_an_empty_span() {
    let (_, document) = clean("empty-cue-text.vtt");
    assert_eq!(document.cues().count(), 2);
    let first = document.cues().next().expect("the fixture holds two cues");
    assert!(first.text.is_empty());
    assert_eq!(document.slice(first.text), "");
    assert_eq!(
        texts(&document)[1],
        "The cue above this one has no payload at all."
    );
}

// ---------------------------------------------------------------- helpers

fn clean(name: &str) -> (Vec<u8>, SubtitleDocument) {
    let path = dirs("vtt").clean.join(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
    let document = assert_round_trip(SubtitleFormat::Vtt, &path, &bytes);
    (bytes, document)
}

fn times(document: &SubtitleDocument) -> Vec<(u32, u32)> {
    document
        .cues()
        .map(|cue| (cue.start.millis(), cue.end.millis()))
        .collect()
}

fn texts(document: &SubtitleDocument) -> Vec<&str> {
    document
        .cues()
        .map(|cue| document.slice(cue.text))
        .collect()
}

fn ids(document: &SubtitleDocument) -> Vec<Option<String>> {
    document
        .cues()
        .map(|cue| detail(cue).id.map(|span| document.slice(span).to_owned()))
        .collect()
}

fn settings(document: &SubtitleDocument) -> Vec<Option<String>> {
    document
        .cues()
        .map(|cue| {
            detail(cue)
                .settings
                .map(|span| document.slice(span).to_owned())
        })
        .collect()
}

fn detail(cue: &Cue) -> &VttCue {
    match &cue.detail {
        CueDetail::Vtt(vtt) => vtt,
        other => panic!("a VTT cue must carry VTT detail, got {other:?}"),
    }
}

fn kinds(document: &SubtitleDocument) -> Vec<&'static str> {
    document
        .segments()
        .iter()
        .map(|segment| match segment.kind {
            SegmentKind::Header => "header",
            SegmentKind::Blank => "blank",
            SegmentKind::Meta => "meta",
            SegmentKind::Cue(_) => "cue",
        })
        .collect()
}

#[test]
fn no_clean_fixture_declares_a_style() {
    // Only ASS has a styles section, so every row of one of these files carries an empty style.
    // See styles-and-fields-tasks.md S6.5.
    let clean = dirs("vtt").clean;
    for (path, bytes) in fixtures(&clean, &["vtt"], MIN_CLEAN) {
        let document = assert_round_trip(SubtitleFormat::Vtt, &path, &bytes);
        assert!(
            document.ass_styles().is_empty(),
            "{} declares a style it cannot have",
            path.display()
        );
    }
}
