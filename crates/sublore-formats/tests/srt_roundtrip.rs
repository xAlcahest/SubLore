//! SRT behavioral suite: every clean fixture survives parse then serialize byte for byte, and every
//! malformed one fails exactly where its sidecar says. See BACKLOG.md M1.1.

mod common;

use std::time::{Duration, Instant};

use sublore_formats::{
    Cue, CueDetail, Newline, Segment, SegmentKind, SrtCue, SubtitleDocument, SubtitleFormat,
};

const FORMAT: SubtitleFormat = SubtitleFormat::Srt;

/// Fixture-count guards: deleting a fixture must turn the suite red, not quietly shrink it.
const MIN_CLEAN: usize = 18;
const MIN_MALFORMED: usize = 9;

#[test]
fn round_trips_every_clean_fixture() {
    let dirs = common::dirs("srt");
    for (path, bytes) in common::fixtures(&dirs.clean, &["srt"], MIN_CLEAN) {
        common::assert_round_trip(FORMAT, &path, &bytes);
    }
}

#[test]
fn reports_every_malformed_fixture() {
    let dirs = common::dirs("srt");
    let fixtures = common::fixtures(&dirs.malformed, &["srt"], MIN_MALFORMED);
    let sidecars = common::fixtures(&dirs.malformed, &["expected"], MIN_MALFORMED);
    assert_eq!(
        sidecars.len(),
        fixtures.len(),
        "every sidecar needs a fixture and every fixture a sidecar"
    );
    for (path, bytes) in fixtures {
        common::assert_expected_error(FORMAT, &path, &bytes);
    }
}

#[test]
fn no_clean_fixture_parses_as_a_single_segment() {
    let dirs = common::dirs("srt");
    for (path, bytes) in common::fixtures(&dirs.clean, &["srt"], MIN_CLEAN) {
        let document = common::assert_round_trip(FORMAT, &path, &bytes);
        if document.cues().count() == 0 {
            continue;
        }
        assert!(
            document.segments().len() >= 2,
            "{}: a parser that lumps the file into one segment proves nothing",
            path.display()
        );
    }
}

#[test]
fn reads_the_baseline_file() {
    let document = clean("basic-lf.srt");
    let cues = cues(&document);
    assert_eq!(cues.len(), 3);
    assert_eq!(document.displayed_cue_count(), 3);
    assert_eq!(kinds(&document), ["cue", "blank", "cue", "blank", "cue"]);
    assert_eq!(cues[0].start.millis(), 2_120);
    assert_eq!(cues[0].end.millis(), 4_880);
    assert_eq!(cues[2].end.millis(), 11_760);
    assert_eq!(
        document.slice(cues[1].text),
        "Nobody had told the crew we were coming,\nso we sat on the dock until it got light."
    );
    assert!(!document.source().has_bom());
    assert_eq!(document.source().newline(), Newline::Lf);
}

#[test]
fn the_crlf_twin_carries_the_same_cues_in_different_bytes() {
    let document = clean("basic-crlf.srt");
    let cues = cues(&document);
    assert_eq!(cues.len(), 3);
    assert_eq!(cues[0].start.millis(), 2_120);
    assert_eq!(
        document.slice(cues[1].text),
        "Nobody had told the crew we were coming,\r\nso we sat on the dock until it got light."
    );
    assert_eq!(document.source().newline(), Newline::Crlf);
}

#[test]
fn a_lone_carriage_return_stays_inside_its_line() {
    let document = clean("mixed-eol.srt");
    let cues = cues(&document);
    assert_eq!(cues.len(), 3);
    assert_eq!(
        document.slice(cues[1].text),
        "The wind, that's all.\rIt does that at night."
    );
    assert_eq!(document.source().newline(), Newline::Mixed);
}

#[test]
fn a_bom_is_remembered_and_kept_out_of_the_body() {
    let document = clean("bom-crlf.srt");
    let cues = cues(&document);
    assert!(document.source().has_bom());
    assert_eq!(document.source().newline(), Newline::Crlf);
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].start.millis(), 72_400);
    assert_eq!(cues[1].end.millis(), 78_900);
}

#[test]
fn keeps_the_index_line_exactly_as_written() {
    let document = clean("numbering-gaps.srt");
    let numbers: Vec<Option<u32>> = document.cues().map(|cue| srt(cue).number).collect();
    assert_eq!(
        numbers,
        [Some(1), Some(2), Some(5), Some(5), Some(4), Some(7)]
    );
    let written: Vec<&str> = document
        .cues()
        .map(|cue| document.slice(srt(cue).number_span.expect("an index line")))
        .collect();
    assert_eq!(written, ["1", "2", "5", "5", "4", "0000007"]);
}

#[test]
fn keeps_impossible_timings_instead_of_fixing_them() {
    let document = clean("overlapping-cues.srt");
    assert_eq!(
        times(&document),
        [
            (10_000, 14_000),
            (12_500, 16_000),
            (20_000, 22_000),
            (20_000, 22_000),
            (30_000, 30_000),
            (40_000, 38_000),
        ]
    );
}

#[test]
fn keeps_the_dvd_coordinate_trailer() {
    let document = clean("coordinates-trailer.srt");
    let trailers: Vec<&str> = document
        .cues()
        .map(|cue| document.slice(srt(cue).timing_trailer.expect("a trailer")))
        .collect();
    assert_eq!(
        trailers,
        [
            "  X1:040 X2:600 Y1:400 Y2:496",
            "  X1:040 X2:600 Y1:400 Y2:496"
        ]
    );
}

#[test]
fn reads_dots_short_fractions_and_one_digit_hours() {
    let document = clean("dot-millis.srt");
    let cues = cues(&document);
    assert_eq!(
        times(&document),
        [(1_500, 3_750), (4_250, 6_000), (3_723_400, 3_725_750)]
    );
    assert_eq!(document.slice(cues[0].start.raw()), "00:00:01.5");
    assert_eq!(document.slice(cues[1].start.raw()), "0:00:04.250");
    assert_eq!(document.slice(cues[2].end.raw()), "1:02:05.75");
}

#[test]
fn a_block_may_start_at_its_timing_line() {
    let document = clean("no-index.srt");
    assert_eq!(document.cues().count(), 3);
    assert!(document
        .cues()
        .all(|cue| srt(cue).number.is_none() && srt(cue).number_span.is_none()));
}

#[test]
fn a_whitespace_only_line_separates_blocks_and_survives_the_round_trip() {
    // Blank means bytes of spaces, tabs and CR only, so a space-only line is a separator and never
    // becomes cue text. It still comes back byte for byte. See BACKLOG.md M1.1.
    let document = clean("empty-text.srt");
    let cues = cues(&document);
    assert_eq!(cues.len(), 3);
    assert!(cues[0].text.is_empty());
    assert!(cues[1].text.is_empty());
    assert_eq!(
        document.slice(cues[2].text),
        "Finally, somebody says something."
    );
    assert_eq!(kinds(&document), ["cue", "blank", "cue", "blank", "cue"]);
}

#[test]
fn trailing_whitespace_belongs_to_the_line_that_carries_it() {
    let document = clean("whitespace-noise.srt");
    let cues = cues(&document);
    assert_eq!(cues.len(), 2);
    assert_eq!(
        document.slice(srt(cues[0]).number_span.expect("an index line")),
        "1   "
    );
    assert_eq!(
        document.slice(srt(cues[0]).timing_trailer.expect("a trailing tab")),
        "\t"
    );
    assert_eq!(
        document.slice(cues[0].text),
        "Trailing spaces live at the end of this line.  \n\tAnd this one starts with a tab."
    );
    assert_eq!(document.slice(cues[1].text), "This one ends on a tab.\t");
}

#[test]
fn tags_entities_and_non_latin_text_are_payload_bytes() {
    let tagged = clean("tags-and-entities.srt");
    let markup = cues(&tagged);
    assert_eq!(markup.len(), 4);
    assert_eq!(
        tagged.slice(markup[2].text),
        "{\\an8}Up at the top, where the sign is."
    );
    assert_eq!(
        tagged.slice(markup[3].text),
        "Salt &amp; pepper, and 5 < 6 is not a tag."
    );

    let script = clean("non-latin.srt");
    let scripted = cues(&script);
    assert_eq!(scripted.len(), 4);
    assert_eq!(
        script.slice(scripted[0].text),
        "港に着いたとき、そこには誰もいなかった。"
    );
    assert!(script.slice(scripted[3].text).contains('\u{1d11e}'));
}

#[test]
fn blank_runs_at_both_ends_become_their_own_segments() {
    let document = clean("blank-line-quirks.srt");
    assert_eq!(kinds(&document), ["blank", "cue", "blank", "cue", "blank"]);
    assert_eq!(document.cues().count(), 2);
}

#[test]
fn a_file_may_end_without_a_terminator() {
    let document = clean("no-final-newline.srt");
    let cues = cues(&document);
    assert_eq!(cues.len(), 2);
    assert_eq!(document.slice(cues[1].text), "There. That is not a bird.");
}

#[test]
fn files_with_no_cues_still_round_trip() {
    let empty = clean("empty.srt");
    assert_eq!(empty.segments().len(), 0);
    assert_eq!(empty.cues().count(), 0);

    let blanks = clean("only-blank-lines.srt");
    assert_eq!(kinds(&blanks), ["blank"]);
    assert_eq!(blanks.cues().count(), 0);
}

#[test]
fn large_file_parses_quickly() {
    // A deliberately loose bound in a debug build: it catches accidental O(n^2), it does not measure
    // the CONTRIBUTING.md §7 budget, which is a release number the owner measures in the app.
    let path = common::dirs("srt").clean.join("large-2000.srt");
    let bytes = std::fs::read(&path).expect("the large fixture is readable");

    let started = Instant::now();
    let document = sublore_formats::parse(FORMAT, &bytes).expect("the large fixture parses");
    let rebuilt = document.to_bytes();
    let elapsed = started.elapsed();

    assert_eq!(document.cues().count(), 2_000);
    assert_eq!(rebuilt.len(), bytes.len());
    assert!(rebuilt == bytes, "the large fixture must round-trip");
    let cues = cues(&document);
    assert_eq!(cues[0].start.millis(), 1_000);
    assert_eq!(cues[1_999].end.millis(), 5_000_500);
    assert!(
        elapsed < Duration::from_secs(2),
        "parsing 2000 cues took {elapsed:?}"
    );
}

fn clean(name: &str) -> SubtitleDocument {
    let path = common::dirs("srt").clean.join(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
    common::assert_round_trip(FORMAT, &path, &bytes)
}

fn cues(document: &SubtitleDocument) -> Vec<&Cue> {
    document.cues().collect()
}

fn times(document: &SubtitleDocument) -> Vec<(u32, u32)> {
    document
        .cues()
        .map(|cue| (cue.start.millis(), cue.end.millis()))
        .collect()
}

fn kinds(document: &SubtitleDocument) -> Vec<&'static str> {
    document.segments().iter().map(kind_name).collect()
}

fn kind_name(segment: &Segment) -> &'static str {
    match segment.kind {
        SegmentKind::Header => "header",
        SegmentKind::Blank => "blank",
        SegmentKind::Meta => "meta",
        SegmentKind::Cue(_) => "cue",
    }
}

fn srt(cue: &Cue) -> &SrtCue {
    match &cue.detail {
        CueDetail::Srt(detail) => detail,
        other => panic!("an SRT cue must carry SRT detail, found {other:?}"),
    }
}
