//! M3.3's acceptance criteria as behaviour: the same words always produce the same cues, the cues
//! are ordered and inside the media, and the committed fixtures render exactly what the rule says
//! they should. See BACKLOG.md M3.3.

mod cue_fixtures;

use cue_fixtures::{generated_case, load, CASES};
use sublore_asr::cues::{
    self, GeneratedCue, MAX_CHARS_PER_CUE, MAX_CHARS_PER_LINE, MAX_CUE_MS, MAX_LINES,
};
use sublore_asr::render;
use sublore_asr::Word;

/// How many generated word lists every property is checked against.
const GENERATED_CASES: u64 = 1_000;

#[test]
fn every_fixture_renders_the_srt_the_rule_says_it_should() {
    for name in CASES {
        let case = load(name);
        let rendered = render::srt(&cues::segment(&case.words, case.audio_duration_ms));

        // The lossy comparison first: it is the one that prints a readable diff.
        assert_eq!(
            String::from_utf8_lossy(&rendered),
            String::from_utf8_lossy(&case.expected_srt),
            "{name}: does not match fixtures/asr/{name}.expected.srt"
        );
        assert_eq!(
            rendered, case.expected_srt,
            "{name}: differs from fixtures/asr/{name}.expected.srt byte for byte"
        );
    }
}

#[test]
fn the_same_words_always_produce_the_same_cues() {
    for name in CASES {
        let case = load(name);
        let once = cues::segment(&case.words, case.audio_duration_ms);
        let twice = cues::segment(&case.words, case.audio_duration_ms);
        assert_eq!(once, twice, "{name}: segmentation is not deterministic");
    }

    for seed in 0..GENERATED_CASES {
        let (words, audio_duration_ms) = generated_case(seed);
        let once = cues::segment(&words, audio_duration_ms);
        let twice = cues::segment(&words, audio_duration_ms);
        assert_eq!(
            once, twice,
            "seed {seed}: segmentation is not deterministic"
        );
    }
}

#[test]
fn generated_word_lists_always_produce_an_ordered_cue_list_inside_the_media() {
    for seed in 0..GENERATED_CASES {
        let (words, audio_duration_ms) = generated_case(seed);
        let cues = cues::segment(&words, audio_duration_ms);

        let mut previous_end = 0;
        for (index, cue) in cues.iter().enumerate() {
            assert!(
                cue.start_ms < cue.end_ms,
                "seed {seed}, cue {index}: {}..{} is not a positive span",
                cue.start_ms,
                cue.end_ms
            );
            assert!(
                cue.start_ms >= previous_end,
                "seed {seed}, cue {index}: starts at {} inside the previous cue, which ended at {previous_end}",
                cue.start_ms
            );
            assert!(
                cue.end_ms <= audio_duration_ms,
                "seed {seed}, cue {index}: ends at {} past the {audio_duration_ms} ms of audio",
                cue.end_ms
            );
            previous_end = cue.end_ms;
        }
    }
}

#[test]
fn every_word_reaches_exactly_one_cue_in_the_order_it_was_spoken() {
    for seed in 0..GENERATED_CASES {
        let (words, audio_duration_ms) = generated_case(seed);
        let cues = cues::segment(&words, audio_duration_ms);

        let spoken: Vec<&str> = cues
            .iter()
            .flat_map(|cue| cue.lines.iter())
            .flat_map(|line| line.split(' '))
            .collect();
        let expected: Vec<&str> = words.iter().map(|word| word.text.as_str()).collect();
        assert_eq!(
            spoken, expected,
            "seed {seed}: words were lost or reordered"
        );
    }
}

#[test]
fn generated_cues_stay_inside_the_reading_budget() {
    for seed in 0..GENERATED_CASES {
        let (words, audio_duration_ms) = generated_case(seed);
        let cues = cues::segment(&words, audio_duration_ms);

        for (index, cue) in cues.iter().enumerate() {
            let where_ = format!("seed {seed}, cue {index}");
            assert!(
                (1..=MAX_LINES).contains(&cue.lines.len()),
                "{where_}: {} lines",
                cue.lines.len()
            );

            // A line runs past the width only when no split of this cue could have avoided it:
            // words are never broken, so a cue of three long words has nowhere to go.
            let splittable = could_fit_in_two_lines(cue);
            for line in &cue.lines {
                assert!(
                    line.chars().count() <= MAX_CHARS_PER_LINE || !splittable,
                    "{where_}: line of {} characters, but a split existed that fits: {line:?}",
                    line.chars().count()
                );
            }

            assert!(
                chars(cue) <= MAX_CHARS_PER_CUE || word_count(cue) == 1,
                "{where_}: {} characters over more than one word",
                chars(cue)
            );
            assert!(
                cue.end_ms - cue.start_ms <= MAX_CUE_MS || word_count(cue) == 1,
                "{where_}: {} ms over more than one word",
                cue.end_ms - cue.start_ms
            );
        }
    }
}

#[test]
fn a_silence_a_full_cue_a_wide_cue_and_a_finished_sentence_each_split() {
    // The four reasons a cue closes, one fixture each, asserted as cue counts so a rule that stops
    // firing cannot hide behind a byte comparison.
    for (name, expected) in [
        ("short-sentence", 1),
        ("gap-split", 2),
        ("duration-split", 2),
        ("width-split", 2),
        ("sentence-split", 2),
        ("empty", 0),
    ] {
        let case = load(name);
        let cues = cues::segment(&case.words, case.audio_duration_ms);
        assert_eq!(cues.len(), expected, "{name}: wrong number of cues");
    }
}

#[test]
fn a_word_list_with_nothing_in_it_produces_no_cues_and_no_bytes() {
    let cues = cues::segment(&[], 60_000);
    assert!(cues.is_empty());
    assert!(render::srt(&cues).is_empty());
}

#[test]
fn words_that_arrive_out_of_order_or_with_junk_in_them_are_repaired_not_trusted() {
    // whisper's offsets were monotone in every run measured, but this crate reads a child process:
    // a word that overlaps the one before it, or carries a control character, must not reach SRT.
    let words = [
        Word {
            text: "  first\u{0}line ".to_owned(),
            start_ms: 900,
            end_ms: 400,
        },
        Word {
            text: "second".to_owned(),
            start_ms: 100,
            end_ms: 1_200,
        },
        Word {
            text: "   ".to_owned(),
            start_ms: 1_300,
            end_ms: 1_400,
        },
    ];
    let cues = cues::segment(&words, 30_000);

    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].lines, vec!["first line second".to_owned()]);
    assert_eq!(cues[0].start_ms, 900);
    assert_eq!(cues[0].end_ms, 1_900, "the 1 s minimum applies");
}

fn chars(cue: &GeneratedCue) -> usize {
    // The lines were split at a space, so putting them back gives the cue's own width.
    cue.lines
        .iter()
        .map(|line| line.chars().count())
        .sum::<usize>()
        + cue.lines.len()
        - 1
}

fn word_count(cue: &GeneratedCue) -> usize {
    cue.lines.iter().map(|line| line.split(' ').count()).sum()
}

/// Whether some word boundary in this cue puts both halves inside the line width. Generated words
/// never hold a space, so splitting the rendered lines back on spaces gives the cue's word list.
fn could_fit_in_two_lines(cue: &GeneratedCue) -> bool {
    let words: Vec<&str> = cue.lines.iter().flat_map(|line| line.split(' ')).collect();
    (1..words.len()).any(|boundary| {
        [&words[..boundary], &words[boundary..]]
            .iter()
            .all(|half| half.join(" ").chars().count() <= MAX_CHARS_PER_LINE)
    })
}
