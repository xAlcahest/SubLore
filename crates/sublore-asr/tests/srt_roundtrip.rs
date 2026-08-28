//! Generated cues become a real document: the SRT this crate renders is parsed by M1, passes M1's
//! coverage guard, and writes itself back byte for byte. Nothing here is a second serializer, which
//! is the point: the app saves the bytes M1 produces, so those are the bytes under test.
//! See BACKLOG.md M3.3.

mod cue_fixtures;

use cue_fixtures::{generated_case, load, CASES};
use sublore_asr::{cues, render};
use sublore_formats::{SubtitleFormat, MAX_TIMECODE_MS};

/// Generated word lists are cheap, but each one parses a document; a few hundred is enough to cover
/// every shape the segmenter can emit.
const GENERATED_CASES: u64 = 250;

#[test]
fn every_fixture_reopens_as_the_document_it_was_saved_as() {
    for name in CASES {
        let case = load(name);
        let cues = cues::segment(&case.words, case.audio_duration_ms);
        let rendered = render::srt(&cues);

        let document = sublore_formats::parse(SubtitleFormat::Srt, &rendered)
            .unwrap_or_else(|error| panic!("{name}: generated SRT does not parse: {error}"));

        assert!(
            document.check_coverage().is_ok(),
            "{name}: the M1 coverage guard rejected the generated document"
        );
        assert_eq!(
            document.to_bytes(),
            rendered,
            "{name}: reopening changed the bytes"
        );
        assert_eq!(
            document.displayed_cue_count(),
            cues.len(),
            "{name}: the document holds a different number of cues"
        );
    }
}

#[test]
fn every_fixture_keeps_its_times_and_text_through_the_parser() {
    for name in CASES {
        let case = load(name);
        let cues = cues::segment(&case.words, case.audio_duration_ms);
        let rendered = render::srt(&cues);
        let document = sublore_formats::parse(SubtitleFormat::Srt, &rendered)
            .unwrap_or_else(|error| panic!("{name}: generated SRT does not parse: {error}"));

        // Zipping would hide a short document, so the lengths are checked before the contents.
        assert_eq!(
            document.cues().count(),
            cues.len(),
            "{name}: the document holds a different number of cues"
        );

        for (index, (generated, parsed)) in cues.iter().zip(document.cues()).enumerate() {
            assert_eq!(
                parsed.start.millis(),
                generated.start_ms,
                "{name}, cue {index}: start moved"
            );
            assert_eq!(
                parsed.end.millis(),
                generated.end_ms,
                "{name}, cue {index}: end moved"
            );
            assert_eq!(
                document.slice(parsed.text),
                generated.lines.join("\n"),
                "{name}, cue {index}: text moved"
            );
        }
    }
}

#[test]
fn generated_word_lists_always_render_a_document_that_reopens_unchanged() {
    for seed in 0..GENERATED_CASES {
        let (words, audio_duration_ms) = generated_case(seed);
        let cues = cues::segment(&words, audio_duration_ms);
        let rendered = render::srt(&cues);

        let document = sublore_formats::parse(SubtitleFormat::Srt, &rendered)
            .unwrap_or_else(|error| panic!("seed {seed}: generated SRT does not parse: {error}"));

        assert!(
            document.check_coverage().is_ok(),
            "seed {seed}: the M1 coverage guard rejected the generated document"
        );
        assert_eq!(document.to_bytes(), rendered, "seed {seed}: bytes changed");
        assert_eq!(
            document.displayed_cue_count(),
            cues.len(),
            "seed {seed}: cue count changed"
        );
    }
}

#[test]
fn a_generated_transcript_survives_the_real_save_path() {
    // The criterion is "saved as SRT and reopened byte-identically", so this goes through the
    // atomic write the app saves with, not through a buffer.
    let case = load("cjk");
    let cues = cues::segment(&case.words, case.audio_duration_ms);
    let rendered = render::srt(&cues);

    let path =
        std::env::temp_dir().join(format!("sublore-m33-roundtrip-{}.srt", std::process::id()));
    sublore_io::atomic::write_atomic(&path, &rendered)
        .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    let reopened = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("read back {}: {error}", path.display()));
    // Removed before the assertions so a failure cannot leave the file behind.
    let _ = std::fs::remove_file(&path);

    assert_eq!(reopened, rendered, "the file on disk is not what we wrote");
    let document = sublore_formats::parse(SubtitleFormat::Srt, &reopened)
        .unwrap_or_else(|error| panic!("the saved file does not reopen: {error}"));
    assert_eq!(document.to_bytes(), rendered, "reopening changed the bytes");
    assert_eq!(document.displayed_cue_count(), cues.len());
}

#[test]
fn the_generated_srt_is_lf_only_with_no_bom_and_one_blank_line_per_cue() {
    let case = load("width-split");
    let rendered = render::srt(&cues::segment(&case.words, case.audio_duration_ms));

    assert!(!rendered.starts_with(&[0xEF, 0xBB, 0xBF]), "no BOM");
    assert!(!rendered.contains(&b'\r'), "LF only");
    assert!(
        rendered.ends_with(b"\n\n"),
        "a blank line after the last cue"
    );

    let text = String::from_utf8(rendered).expect("generated SRT is UTF-8");
    let numbers: Vec<&str> = text
        .split("\n\n")
        .filter(|block| !block.is_empty())
        .map(|block| block.lines().next().unwrap_or_default())
        .collect();
    assert_eq!(numbers, ["1", "2"], "1-based numbering with no gaps");
}

#[test]
fn a_time_past_what_srt_can_spell_is_a_parse_error_not_a_broken_file() {
    // render is a dumb emitter by design; the guarantee is that anything the SRT grammar cannot
    // hold comes back as a structured error from parse, never as a file the app would save.
    let words = [sublore_asr::Word {
        text: "late".to_owned(),
        start_ms: MAX_TIMECODE_MS,
        end_ms: u32::MAX,
    }];
    let rendered = render::srt(&cues::segment(&words, u32::MAX));

    let error = sublore_formats::parse(SubtitleFormat::Srt, &rendered)
        .expect_err("a timecode past 999:59:59,999 cannot be parsed back");
    assert_eq!(
        error.kind,
        sublore_formats::ParseErrorKind::TimecodeOutOfRange
    );
}

#[test]
fn text_that_looks_like_srt_syntax_stays_inside_its_own_cue() {
    // A transcript is not trusted input: a word that spells a timing arrow, a blank line or a NUL
    // would each split or break the block it sits in if it reached the file as written.
    let words = [
        word("-->", 0, 400),
        word("1", 500, 900),
        word("a\n\nb", 1_000, 1_400),
        word("c\u{0}d", 1_500, 1_900),
    ];
    let cues = cues::segment(&words, 10_000);
    let rendered = render::srt(&cues);

    assert_eq!(cues.len(), 1, "nothing here is a reason to split");
    let document = sublore_formats::parse(SubtitleFormat::Srt, &rendered)
        .unwrap_or_else(|error| panic!("generated SRT does not parse: {error}"));
    assert_eq!(document.displayed_cue_count(), 1, "still one cue");
    assert!(document.check_coverage().is_ok());
    assert_eq!(document.to_bytes(), rendered);
    assert_eq!(
        String::from_utf8(rendered).expect("SRT is UTF-8"),
        "1\n00:00:00,000 --> 00:00:01,900\n--> 1 a b c d\n\n"
    );
}

fn word(text: &str, start_ms: u32, end_ms: u32) -> sublore_asr::Word {
    sublore_asr::Word {
        text: text.to_owned(),
        start_ms,
        end_ms,
    }
}
