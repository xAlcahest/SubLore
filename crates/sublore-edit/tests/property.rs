//! M2.1 property suite: the acceptance criterion "a property test over random edit sequences never
//! produces a document that fails the guard", asserted per step rather than once at the end.
//!
//! The generator is hand-rolled and seeded rather than a dependency: our failing cases are already
//! small (a seed plus at most 24 operations, all printed), the workspace ships no third-party
//! crates, and generating edits that are *usually* valid needs custom logic either way. Every
//! failure prints its fixture, seed, step and request, so a red run is reproducible with one
//! command.

use std::path::{Path, PathBuf};

use sublore_edit::error::EditErrorKind;
use sublore_edit::plan::{edit, Edit, Edited};
use sublore_formats::{SubtitleDocument, SubtitleFormat, MAX_TIMECODE_MS};

/// Fixtures chosen because each is a shape that has already broken something: terminators, a byte
/// order mark, a missing final newline, blank-line runs, empty text, no index lines, non-Latin
/// text, an empty file, VTT metadata and settings, a shuffled ASS `Format:` line, and ASS events
/// with a blank run on each side. Fixtures holding a lone carriage return inside cue text stay out:
/// `assert_format_invariant` reads every cue, including the ones no edit in the run touched.
const FIXTURES: [&str; 18] = [
    "srt/clean/basic-lf.srt",
    "srt/clean/basic-crlf.srt",
    "srt/clean/bom-crlf.srt",
    "srt/clean/no-final-newline.srt",
    "srt/clean/blank-line-quirks.srt",
    "srt/clean/empty-text.srt",
    "srt/clean/no-index.srt",
    "srt/clean/non-latin.srt",
    "srt/clean/empty.srt",
    "vtt/clean/basic.vtt",
    "vtt/clean/note-style-region.vtt",
    "vtt/clean/cue-settings.vtt",
    "vtt/clean/header-text-crlf.vtt",
    "vtt/clean/empty-cue-text.vtt",
    "ass/clean/basic.ass",
    "ass/clean/field-order-shuffled.ass",
    "ass/clean/text-with-commas.ass",
    "ass/clean/blank-between-events.ass",
];

const LARGE: &str = "srt/clean/large-2000.srt";

const SEEDS: u64 = 64;
/// Fewer runs on the 2000-cue fixture, where one edit cycle costs milliseconds in a debug build.
const LARGE_SEEDS: u64 = 4;
const EDITS_PER_RUN: usize = 24;

/// xorshift64*, seeded and deterministic. No dependency, and the seed is printed with every
/// failure.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // A zero state would stay zero for ever.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.0 = state;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in `0..limit`, or 0 when the range is empty.
    fn below(&mut self, limit: usize) -> usize {
        let limit = u64::try_from(limit).unwrap_or(u64::MAX);
        if limit == 0 {
            return 0;
        }
        usize::try_from(self.next() % limit).unwrap_or(0)
    }
}

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

#[test]
fn random_edit_sequences_never_produce_a_document_that_fails_the_guard() {
    for fixture in FIXTURES {
        let mut applied = 0usize;
        for seed in 0..SEEDS {
            applied += run(fixture, seed);
        }
        // Anti-vacuity: between 30% and 60% of generated steps apply today. A planner that
        // started refusing everything would still satisfy every other assertion here.
        assert!(
            applied * 5 >= usize::try_from(SEEDS).unwrap_or(0) * EDITS_PER_RUN,
            "{fixture}: only {applied} generated edits applied, so little was proved"
        );
    }
}

#[test]
fn random_edit_sequences_hold_on_the_two_thousand_cue_fixture() {
    let mut applied = 0usize;
    for seed in 0..LARGE_SEEDS {
        applied += run(LARGE, seed);
    }
    assert!(
        applied * 5 >= usize::try_from(LARGE_SEEDS).unwrap_or(0) * EDITS_PER_RUN,
        "{LARGE}: only {applied} generated edits applied, so little was proved"
    );
}

/// One run: a sequence of generated edits, every invariant asserted after every step, then the
/// whole sequence undone through the inverse splices and checked against the file byte for byte.
fn run(fixture: &str, seed: u64) -> usize {
    let path = root().join(fixture);
    let original = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
    let format = format_of(&path);
    let mut document = sublore_formats::parse(format, &original)
        .unwrap_or_else(|error| panic!("{fixture} must parse: {error}"));

    let mut rng = Rng::new(seed);
    let mut splices = Vec::new();
    let mut applied = 0usize;

    for step in 0..EDITS_PER_RUN {
        let request = random_edit(&mut rng, &document);
        let where_ = format!("{fixture} seed {seed} step {step}: {request:?}");
        let before = document.to_bytes();
        let before_count = document.cues().count();

        match edit(&document, &request) {
            Err(error) => {
                // A refusal the plan chose is covered behaviour. A refusal from the net means the
                // plan produced bytes it could not stand behind, which is a bug in the plan.
                assert!(
                    !matches!(
                        error.kind,
                        EditErrorKind::Unverified
                            | EditErrorKind::Reparse
                            | EditErrorKind::BadRange
                            | EditErrorKind::StaleSplice
                    ),
                    "{where_}: the guard caught the plan itself: {error}"
                );
                assert_eq!(
                    document.to_bytes(),
                    before,
                    "{where_}: a refused edit must leave the document untouched"
                );
            }
            Ok(result) => {
                assert_step(&document, &before, before_count, &result, &where_);
                splices.push(result.splice.clone());
                document = result.document;
                applied += 1;
            }
        }
    }

    // Every splice's inverse, in reverse order, restores the file exactly. This is M2.2's criterion
    // proved at the model layer, and it is free here because a splice is its own inverse's inverse.
    let mut body = document.source().body().to_owned();
    for splice in splices.iter().rev() {
        body = sublore_edit::splice::apply(&body, &splice.inverse())
            .unwrap_or_else(|error| panic!("{fixture} seed {seed}: undo was refused: {error}"));
    }
    let mut restored = Vec::new();
    if document.source().has_bom() {
        restored.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    restored.extend_from_slice(body.as_bytes());
    assert_eq!(
        restored,
        original,
        "{fixture} seed {seed}: undoing {} edits must restore the file byte for byte",
        splices.len()
    );

    applied
}

fn assert_step(
    before_document: &SubtitleDocument,
    before: &[u8],
    before_count: usize,
    result: &Edited,
    where_: &str,
) {
    // The M1 guard, re-asserted at the property level rather than trusted.
    assert_eq!(
        result.document.check_coverage(),
        Ok(()),
        "{where_}: the segments must still tile the body"
    );

    let after = result.document.to_bytes();
    let base = usize::from(before_document.source().has_bom()) * 3;
    let at = base + result.splice.at;
    let removed = result.splice.removed.len();
    let inserted = result.splice.inserted.len();

    // Byte locality: the acceptance criterion, asserted per step.
    assert_eq!(before.get(..at), after.get(..at), "{where_}: head moved");
    assert_eq!(
        before.get(at + removed..),
        after.get(at + inserted..),
        "{where_}: tail moved"
    );
    assert_eq!(
        after.len(),
        before.len() + inserted - removed,
        "{where_}: the file grew by something other than the edit"
    );
    assert_eq!(
        before.get(at..at + removed),
        Some(result.splice.removed.as_bytes()),
        "{where_}: the splice does not describe the bytes it replaced"
    );
    assert_eq!(
        after.get(at..at + inserted),
        Some(result.splice.inserted.as_bytes()),
        "{where_}: the splice does not describe the bytes it wrote"
    );

    let after_count = result.document.cues().count();
    let delta = isize::try_from(after_count).unwrap_or(isize::MAX)
        - isize::try_from(before_count).unwrap_or(isize::MAX);
    assert_eq!(delta, result.cue_delta, "{where_}: the cue delta is wrong");

    assert_format_invariant(&result.document, where_);

    // Saving and reopening changes nothing.
    let reopened = sublore_formats::parse(result.document.format(), &after)
        .unwrap_or_else(|error| panic!("{where_}: the edited bytes must reopen: {error}"));
    assert_eq!(
        reopened.to_bytes(),
        after,
        "{where_}: reopening moved bytes"
    );
    assert_eq!(
        reopened.cues().count(),
        after_count,
        "{where_}: reopening changed the cue count"
    );
}

/// What each format can hold: no blank line inside SRT or VTT cue text, no line break at all inside
/// an ASS event. Either would split a cue in two the next time the file is opened.
fn assert_format_invariant(document: &SubtitleDocument, where_: &str) {
    for (index, cue) in document.cues().enumerate() {
        let text = document.slice(cue.text);
        if text.is_empty() {
            continue;
        }
        match document.format() {
            SubtitleFormat::Ass => assert!(
                !text.contains(['\n', '\r']),
                "{where_}: ass event {index} grew a line break: {text:?}"
            ),
            SubtitleFormat::Srt | SubtitleFormat::Vtt => {
                for line in text.split('\n') {
                    assert!(
                        !line
                            .bytes()
                            .all(|byte| matches!(byte, b' ' | b'\t' | b'\r')),
                        "{where_}: cue {index} holds a blank line: {text:?}"
                    );
                }
            }
        }
    }
}

/// Arguments are usually valid and sometimes deliberately not: an out-of-range index, a blank line
/// inside the text, an offset past the end, a value the precision cannot spell. Refusals are
/// covered behaviour, not skipped steps.
fn random_edit(rng: &mut Rng, document: &SubtitleDocument) -> Edit {
    let count = document.cues().count();
    let cue = if rng.below(8) == 0 {
        count.saturating_add(rng.below(3))
    } else {
        rng.below(count.max(1))
    };

    match rng.below(6) {
        0 => Edit::SetText {
            cue,
            text: random_text(rng),
        },
        1 => {
            let (start_ms, end_ms) = random_times(rng);
            Edit::SetTimes {
                cue,
                start_ms,
                end_ms,
            }
        }
        2 => {
            let (start_ms, end_ms) = random_times(rng);
            Edit::Insert {
                before: rng.below(count.saturating_add(1)),
                start_ms,
                end_ms,
                text: random_text(rng),
            }
        }
        3 => Edit::Delete { cue },
        4 => {
            let length = document
                .cues()
                .nth(cue)
                .map_or(0, |target| document.slice(target.text).len());
            Edit::Split {
                cue,
                text_offset: rng.below(length.saturating_add(2)),
                at_ms: random_split_ms(rng, document, cue),
            }
        }
        _ => Edit::Merge { cue },
    }
}

const TEXTS: [&str; 12] = [
    "Hello",
    "Ciao, mondo",
    "line one\nline two",
    "",
    "   ",
    "one\n\ntwo",
    "trailing\n",
    "a\rb",
    "\u{65e5}\u{672c}\u{8a9e}",
    "-->",
    "12345",
    "text, with, commas",
];

fn random_text(rng: &mut Rng) -> String {
    let pick = rng.below(TEXTS.len() + 1);
    match TEXTS.get(pick) {
        Some(text) => (*text).to_owned(),
        None => "long ".repeat(1 + rng.below(20)),
    }
}

/// Values that are usually representable at one fraction digit, and sometimes are not.
fn random_times(rng: &mut Rng) -> (u32, u32) {
    let start = match rng.below(12) {
        0 => MAX_TIMECODE_MS,
        1 => MAX_TIMECODE_MS.saturating_add(1),
        2 => 1_234,
        _ => u32::try_from(rng.below(600_000)).unwrap_or(0) / 100 * 100,
    };
    let end = start.saturating_add(u32::try_from(rng.below(10_000)).unwrap_or(0) / 100 * 100);
    (start, end)
}

fn random_split_ms(rng: &mut Rng, document: &SubtitleDocument, cue: usize) -> u32 {
    let Some(target) = document.cues().nth(cue) else {
        return u32::try_from(rng.below(10_000)).unwrap_or(0);
    };
    let (low, high) = (
        target.start.millis().min(target.end.millis()),
        target.start.millis().max(target.end.millis()),
    );
    if rng.below(8) == 0 {
        return high.saturating_add(1_000);
    }
    let span = usize::try_from(high.saturating_sub(low)).unwrap_or(0);
    low.saturating_add(u32::try_from(rng.below(span.saturating_add(1))).unwrap_or(0))
}
