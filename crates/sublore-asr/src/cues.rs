//! Word timestamps to subtitle cues, by one rule that is written down and cannot be configured.
//!
//! The rule in full, because a translator has to be able to predict it:
//!
//! 1. Words are normalised first: whitespace and control characters inside a word collapse to a
//!    single space, empty words are dropped, and times are made monotone. What arrives here came
//!    out of a child process, so nothing about its shape is assumed.
//! 2. One greedy pass. A cue takes words until the next one would break a rule, and then closes
//!    **before** that word. The four reasons, tested in this order: a silence of
//!    [`GAP_SPLIT_MS`] or more, a cue that would run past [`MAX_CUE_MS`], a cue that would grow
//!    past [`MAX_CHARS_PER_CUE`], and a sentence that just finished.
//! 3. One timing pass. Every cue gets at least [`MIN_CUE_MS`] on screen, stops [`MIN_GAP_MS`]
//!    before the next one starts, and stays inside the audio.
//! 4. Wrapping: one line if the text fits, otherwise two, split at the word boundary that makes
//!    them most even. A word wider than a line is never broken.
//!
//! The same words and the same duration therefore always produce the same cues, on any machine,
//! with no state and no randomness anywhere in the module. See BACKLOG.md M3.3.

use crate::transcript::Word;

/// Reading-speed limits. Two lines of 42 characters is what broadcast subtitling has settled on and
/// what every player can draw without reflowing.
pub const MAX_CHARS_PER_LINE: usize = 42;
pub const MAX_LINES: usize = 2;
pub const MAX_CHARS_PER_CUE: usize = MAX_CHARS_PER_LINE * MAX_LINES;

/// Longest a cue may stay on screen, and the shortest it may flash for.
pub const MAX_CUE_MS: u32 = 7_000;
pub const MIN_CUE_MS: u32 = 1_000;

/// A silence at least this long ends a cue: it is a pause a viewer sees.
pub const GAP_SPLIT_MS: u32 = 700;

/// Kept clear between two cues so a player never draws both on the same frame.
pub const MIN_GAP_MS: u32 = 40;

/// A word ending in one of these finished a sentence.
const SENTENCE_END: [char; 4] = ['.', '?', '!', '…'];

/// Stripped from the end of a word before looking for [`SENTENCE_END`], so `stop!"` still counts.
const CLOSING: [char; 5] = ['"', '\'', ')', ']', '»'];

/// One cue on its way to SRT: times in milliseconds, and the one or two lines it draws.
///
/// The lines never hold a line terminator and are never empty, because [`segment`] is the only
/// thing that builds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedCue {
    pub start_ms: u32,
    pub end_ms: u32,
    pub lines: Vec<String>,
}

/// Turn a transcript's words into cues.
///
/// `audio_duration_ms` is the ceiling for cue ends. A cue is never zero-length and never inverted,
/// so in the pathological case where the ceiling leaves no room at all the cue keeps 1 ms and may
/// end just past the audio; that beats writing a cue a player cannot draw.
pub fn segment(words: &[Word], audio_duration_ms: u32) -> Vec<GeneratedCue> {
    let mut drafts: Vec<Draft> = Vec::new();
    for word in normalise(words) {
        match drafts.last_mut() {
            Some(draft) if !closes_before(draft, &word) => draft.push(word),
            _ => drafts.push(Draft::new(word)),
        }
    }
    time(&drafts, audio_duration_ms)
}

/// A word as this module measures it: text with no surprises in it, and a character count taken
/// once.
struct Spoken {
    text: String,
    chars: usize,
    start_ms: u32,
    end_ms: u32,
}

/// A cue being filled: the words it holds, its span, and its width joined with single spaces.
struct Draft {
    words: Vec<String>,
    chars: usize,
    start_ms: u32,
    /// The last word's end, which is what the gap and duration rules measure against.
    end_ms: u32,
}

impl Draft {
    fn new(word: Spoken) -> Self {
        Self {
            words: vec![word.text],
            chars: word.chars,
            start_ms: word.start_ms,
            end_ms: word.end_ms,
        }
    }

    fn push(&mut self, word: Spoken) {
        self.chars += 1 + word.chars;
        self.end_ms = word.end_ms;
        self.words.push(word.text);
    }
}

/// The words a cue may be built from: no empty ones, no control characters, times that only ever
/// move forward. The boundary check for everything the sidecar hands over.
fn normalise(words: &[Word]) -> Vec<Spoken> {
    let mut spoken = Vec::with_capacity(words.len());
    let mut previous_end = 0;

    for word in words {
        // A NUL would make the rendered SRT undecodable and a newline would split the cue block,
        // so a run of either collapses to one space here. See BACKLOG.md M3.3.
        let text = word
            .text
            .split(|character: char| character.is_whitespace() || character.is_control())
            .filter(|piece| !piece.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            continue;
        }

        let start_ms = word.start_ms.max(previous_end);
        let end_ms = word.end_ms.max(start_ms);
        previous_end = end_ms;
        spoken.push(Spoken {
            chars: text.chars().count(),
            text,
            start_ms,
            end_ms,
        });
    }

    spoken
}

/// The whole segmentation rule: whether `draft` closes before `word` joins it.
fn closes_before(draft: &Draft, word: &Spoken) -> bool {
    word.start_ms.saturating_sub(draft.end_ms) >= GAP_SPLIT_MS
        || word.end_ms.saturating_sub(draft.start_ms) > MAX_CUE_MS
        || draft.chars + 1 + word.chars > MAX_CHARS_PER_CUE
        || draft.words.last().is_some_and(|last| ends_sentence(last))
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end_matches(CLOSING)
        .chars()
        .next_back()
        .is_some_and(|character| SENTENCE_END.contains(&character))
}

/// Give every cue its final times. Cues are walked in order and each one starts no earlier than the
/// previous one ended, which is what keeps the list strictly ordered whatever the words did.
fn time(drafts: &[Draft], audio_duration_ms: u32) -> Vec<GeneratedCue> {
    let mut cues = Vec::with_capacity(drafts.len());
    let mut floor = 0;

    for (index, draft) in drafts.iter().enumerate() {
        let start_ms = draft.start_ms.max(floor);
        let mut end_ms = draft.end_ms.max(start_ms.saturating_add(MIN_CUE_MS));
        if let Some(next) = drafts.get(index + 1) {
            end_ms = end_ms.min(next.start_ms.saturating_sub(MIN_GAP_MS));
        }
        end_ms = end_ms.min(audio_duration_ms);
        if end_ms <= start_ms {
            end_ms = start_ms.saturating_add(1);
        }

        floor = end_ms;
        cues.push(GeneratedCue {
            start_ms,
            end_ms,
            lines: wrap(&draft.words),
        });
    }

    cues
}

/// A third line would need a second split point and a different cue budget, so the wrapper says so
/// here rather than quietly ignoring a changed constant.
const _: () = assert!(MAX_LINES == 2, "wrap() splits a cue exactly once");

/// One line, or the two most even ones. `MAX_LINES` is 2, so a cue is split at most once.
fn wrap(words: &[String]) -> Vec<String> {
    let text = words.join(" ");
    if words.len() < 2 || text.chars().count() <= MAX_CHARS_PER_LINE {
        return vec![text];
    }

    let widths: Vec<usize> = words.iter().map(|word| word.chars().count()).collect();
    let total = text.chars().count();
    let mut first = 0;
    let mut best = (usize::MAX, 1);

    for boundary in 1..words.len() {
        first += widths[boundary - 1] + usize::from(boundary > 1);
        // The space at the boundary belongs to neither line.
        let difference = first.abs_diff(total - first - 1);
        if difference < best.0 {
            best = (difference, boundary);
        }
    }

    vec![words[..best.1].join(" "), words[best.1..].join(" ")]
}

#[cfg(test)]
mod tests {
    use super::{segment, GeneratedCue, MAX_CHARS_PER_CUE, MAX_CHARS_PER_LINE};
    use crate::transcript::Word;

    fn word(text: &str, start_ms: u32, end_ms: u32) -> Word {
        Word {
            text: text.to_owned(),
            start_ms,
            end_ms,
        }
    }

    fn lines(cue: &GeneratedCue) -> Vec<&str> {
        cue.lines.iter().map(String::as_str).collect()
    }

    #[test]
    fn a_word_wider_than_a_whole_cue_gets_a_cue_of_its_own() {
        let long = "x".repeat(MAX_CHARS_PER_CUE + 6);
        let cues = segment(&[word(&long, 0, 500), word("after", 600, 900)], 10_000);

        assert_eq!(cues.len(), 2, "the next word cannot fit beside it");
        assert_eq!(lines(&cues[0]), vec![long.as_str()], "and it is not broken");
        assert_eq!(lines(&cues[1]), vec!["after"]);
    }

    #[test]
    fn a_line_is_only_split_at_a_space() {
        let long = "y".repeat(MAX_CHARS_PER_LINE + 4);
        let cues = segment(&[word(&long, 0, 500), word("tail", 600, 900)], 10_000);

        assert_eq!(cues.len(), 1);
        assert_eq!(
            lines(&cues[0]),
            vec![long.as_str(), "tail"],
            "the over-long word keeps its own line"
        );
    }

    #[test]
    fn a_closing_quote_does_not_hide_the_end_of_a_sentence() {
        let closed = segment(&[word("done.\"", 0, 400), word("Next", 500, 900)], 10_000);
        assert_eq!(closed.len(), 2);

        let open = segment(&[word("done\"", 0, 400), word("Next", 500, 900)], 10_000);
        assert_eq!(open.len(), 1, "no sentence ended, so nothing splits");
    }

    #[test]
    fn a_cue_never_runs_into_the_one_after_it() {
        // Two words 100 ms apart: the first cue wants a full second and only gets 60 ms.
        let cues = segment(&[word("tight.", 0, 100), word("packed", 200, 300)], 10_000);

        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].end_ms, 160, "40 ms of gap is kept");
        assert!(cues[0].end_ms <= cues[1].start_ms);
    }

    #[test]
    fn an_audio_duration_of_zero_still_produces_a_document_the_parser_accepts() {
        // Nonsense in, but never an inverted or zero-length cue out.
        let cues = segment(&[word("a", 0, 10), word("b.", 100, 200)], 0);

        assert_eq!(cues.len(), 1);
        assert!(cues[0].start_ms < cues[0].end_ms);
    }

    #[test]
    fn words_timed_past_the_end_of_the_audio_keep_their_place() {
        // The duration is a ceiling, not a truncation. When the ceiling leaves no room the cue
        // keeps 1 ms and runs past the audio, because losing a transcribed word, or writing a cue
        // a player cannot draw, would both be worse.
        let cues = segment(&[word("over.", 0, 10), word("shoot", 30, 40)], 5);

        assert_eq!(cues.len(), 2);
        assert_eq!((cues[0].start_ms, cues[0].end_ms), (0, 1));
        assert_eq!((cues[1].start_ms, cues[1].end_ms), (30, 31));
        assert!(cues[0].end_ms <= cues[1].start_ms, "still in order");
    }

    #[test]
    fn words_with_no_text_left_after_normalising_are_dropped() {
        let cues = segment(&[word("\u{0}\t ", 0, 400), word("real", 500, 900)], 10_000);

        assert_eq!(cues.len(), 1);
        assert_eq!(lines(&cues[0]), vec!["real"]);
        assert_eq!(
            cues[0].start_ms, 500,
            "the empty word takes no time with it"
        );
    }
}
