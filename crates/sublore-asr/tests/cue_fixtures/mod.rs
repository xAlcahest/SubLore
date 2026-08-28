//! The committed word lists in `fixtures/asr/`, and the generator the property tests share.
//!
//! Two test binaries include this module and each uses part of it, so the unused half would
//! otherwise be a warning in the other. See BACKLOG.md M3.3.
#![allow(dead_code)]

use std::path::PathBuf;
use sublore_asr::Word;

/// Every case in `fixtures/asr/`. Named here so a fixture that stops being loaded is a compile-time
/// edit, not a silently skipped file.
pub const CASES: [&str; 9] = [
    "short-sentence",
    "width-split",
    "duration-split",
    "gap-split",
    "sentence-split",
    "single-long-word",
    "cjk",
    "ends-at-duration",
    "empty",
];

pub struct Case {
    pub name: &'static str,
    pub words: Vec<Word>,
    pub audio_duration_ms: u32,
    /// `<name>.expected.srt`, byte for byte.
    pub expected_srt: Vec<u8>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/asr")
}

/// Reads `<name>.words.json` and `<name>.expected.srt`. A missing or malformed fixture panics with
/// the path in the message: a fixture that cannot be read is a broken test, never a skipped one.
pub fn load(name: &'static str) -> Case {
    let dir = fixtures_dir();
    let words_path = dir.join(format!("{name}.words.json"));
    let srt_path = dir.join(format!("{name}.expected.srt"));

    let raw = std::fs::read_to_string(&words_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", words_path.display()));
    let value: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("parse {}: {error}", words_path.display()));

    let audio_duration_ms = u32::try_from(
        value["audio_duration_ms"]
            .as_u64()
            .unwrap_or_else(|| panic!("{name}: audio_duration_ms must be a number")),
    )
    .unwrap_or_else(|_| panic!("{name}: audio_duration_ms does not fit in u32"));

    let words = value["words"]
        .as_array()
        .unwrap_or_else(|| panic!("{name}: words must be an array"))
        .iter()
        .map(|entry| Word {
            text: entry["text"]
                .as_str()
                .unwrap_or_else(|| panic!("{name}: every word needs a string text"))
                .to_owned(),
            start_ms: number(name, entry, "start_ms"),
            end_ms: number(name, entry, "end_ms"),
        })
        .collect();

    let expected_srt = std::fs::read(&srt_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", srt_path.display()));

    Case {
        name,
        words,
        audio_duration_ms,
        expected_srt,
    }
}

fn number(name: &str, entry: &serde_json::Value, field: &str) -> u32 {
    let value = entry[field]
        .as_u64()
        .unwrap_or_else(|| panic!("{name}: {field} must be a number"));
    u32::try_from(value).unwrap_or_else(|_| panic!("{name}: {field} does not fit in u32"))
}

/// A linear congruential generator, written out here so the property tests need no dev-dependency
/// and produce the same 1000 cases on every machine. Constants from Knuth's MMIX.
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(2).wrapping_add(1))
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // The low bits of an LCG cycle far too fast to sample.
        self.0 >> 32
    }

    /// Uniform enough for test data: `0..limit`, and `0` when `limit` is `0`.
    pub fn below(&mut self, limit: u32) -> u32 {
        match limit {
            0 => 0,
            limit => (self.next() % u64::from(limit)) as u32,
        }
    }
}

/// Characters that make character counting matter: ASCII, an accented pair, and two CJK ideographs
/// that are three bytes each.
const ALPHABET: [char; 16] = [
    'a', 'b', 'c', 'd', 'e', 'i', 'o', 'r', 's', 't', 'u', 'n', 'é', 'ü', '字', '幕',
];
const ENDINGS: [char; 4] = ['.', '?', '!', '…'];

/// One reproducible word list plus the audio duration it came from.
///
/// The words are monotone, carry no whitespace, and every one of them is at least 50 ms long and
/// ends at or before the duration, which is exactly the shape whisper's offsets have.
pub fn generated_case(seed: u64) -> (Vec<Word>, u32) {
    let mut rng = Lcg::new(seed);
    let count = rng.below(40);
    let mut words = Vec::with_capacity(count as usize);
    let mut clock = rng.below(500);

    for _ in 0..count {
        let start = clock + rng.below(1_500);
        let end = start + 50 + rng.below(850);
        words.push(Word {
            text: token(&mut rng),
            start_ms: start,
            end_ms: end,
        });
        clock = end;
    }

    (words, clock + rng.below(2_000))
}

fn token(rng: &mut Lcg) -> String {
    let length = 1 + rng.below(13);
    let mut text: String = (0..length)
        .map(|_| ALPHABET[rng.below(ALPHABET.len() as u32) as usize])
        .collect();
    // One word in four finishes a sentence, and half of those close a quote after it.
    if rng.below(4) == 0 {
        text.push(ENDINGS[rng.below(ENDINGS.len() as u32) as usize]);
        if rng.below(2) == 0 {
            text.push('"');
        }
    }
    text
}
