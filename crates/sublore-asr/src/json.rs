//! whisper's `-ojf` JSON to a word list. See BACKLOG.md M3.1.
//!
//! The token offsets whisper reports are already the word-level timestamps CONTRIBUTING.md §2 asks for;
//! `-dtw` was measured against them and disagreed by hundreds of milliseconds, so it is not used.
//! Tokens are sub-word pieces: a leading space is what starts a new word, and that rule plus the
//! special-token filter is the whole conversion.

use serde_json::Value;

use crate::error::{AsrError, AsrErrorKind};
use crate::transcript::Word;

/// What one run's JSON says, before segmentation turns it into cues.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedTranscript {
    /// whisper's own language code, empty when it did not report one.
    pub language: String,
    pub words: Vec<Word>,
}

pub fn parse_transcript(bytes: &[u8]) -> Result<ParsedTranscript, AsrError> {
    // Measured on this machine: whisper writes raw token bytes, and a decode that stops mid
    // character leaves a truncated UTF-8 sequence in the file. Losing a whole run over one broken
    // byte would be the wrong trade, so the byte becomes U+FFFD and the rest of the transcript
    // survives. See BACKLOG.md M3.1.
    let text = String::from_utf8_lossy(bytes);
    let root: Value = serde_json::from_str(&text)
        .map_err(|error| no_output(format!("the JSON did not parse: {error}")))?;
    let segments = root
        .get("transcription")
        .and_then(Value::as_array)
        .ok_or_else(|| no_output("the JSON has no transcription array"))?;

    let language = root
        .get("result")
        .and_then(|result| result.get("language"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let mut words: Vec<Word> = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let tokens = segment
            .get("tokens")
            .and_then(Value::as_array)
            .ok_or_else(|| no_output(format!("segment {index} has no tokens array")))?;
        for token in tokens {
            let text = token
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| no_output(format!("a token in segment {index} has no text")))?;
            if is_special(text) || text.is_empty() {
                continue;
            }
            let (start_ms, end_ms) = offsets(token, index)?;
            // A leading space is where whisper puts a word boundary; anything else continues the
            // word being built, which is how ' Sub' + 'l' + 'oor' becomes one word.
            match words.last_mut() {
                Some(last) if !text.starts_with(' ') => {
                    last.text.push_str(text);
                    last.end_ms = end_ms;
                }
                _ => words.push(Word {
                    text: text.to_owned(),
                    start_ms,
                    end_ms,
                }),
            }
        }
    }

    Ok(ParsedTranscript {
        language,
        words: tidy(words),
    })
}

/// `[_BEG_]`, `[_TT_216]` and friends are decoder bookkeeping, not speech.
fn is_special(text: &str) -> bool {
    text.starts_with("[_") && text.ends_with(']')
}

fn offsets(token: &Value, segment: usize) -> Result<(u32, u32), AsrError> {
    let offsets = token
        .get("offsets")
        .ok_or_else(|| no_output(format!("a token in segment {segment} has no offsets")))?;
    let from = milliseconds(offsets.get("from"))
        .ok_or_else(|| no_output(format!("a token in segment {segment} has no offsets.from")))?;
    let to = milliseconds(offsets.get("to"))
        .ok_or_else(|| no_output(format!("a token in segment {segment} has no offsets.to")))?;
    Ok((from, to))
}

/// Milliseconds as whisper writes them: a non-negative integer. A negative or absurd value is
/// clamped rather than wrapped, because a cue time is never allowed to be nonsense.
fn milliseconds(value: Option<&Value>) -> Option<u32> {
    let number = value?.as_i64()?;
    Some(number.clamp(0, u32::MAX as i64) as u32)
}

/// Trim the joined text, drop what is left empty, and make the timeline monotone. Whisper's
/// offsets came out ordered in every run measured here, but nothing downstream may depend on that.
fn tidy(words: Vec<Word>) -> Vec<Word> {
    let mut tidied: Vec<Word> = Vec::with_capacity(words.len());
    for mut word in words {
        let trimmed = word.text.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() != word.text.len() {
            word.text = trimmed.to_owned();
        }
        word.end_ms = word.end_ms.max(word.start_ms);
        if let Some(previous) = tidied.last() {
            word.start_ms = word.start_ms.max(previous.end_ms);
            word.end_ms = word.end_ms.max(word.start_ms);
        }
        tidied.push(word);
    }
    tidied
}

fn no_output(detail: impl Into<String>) -> AsrError {
    AsrError::new(AsrErrorKind::NoOutput, detail)
}

#[cfg(test)]
mod tests {
    use super::{parse_transcript, ParsedTranscript};
    use crate::error::AsrErrorKind;
    use crate::transcript::Word;

    fn words(json: &str) -> Vec<Word> {
        parse_transcript(json.as_bytes())
            .expect("the fixture should parse")
            .words
    }

    fn segment(tokens: &str) -> String {
        format!(r#"{{"result":{{"language":"en"}},"transcription":[{{"tokens":[{tokens}]}}]}}"#)
    }

    fn token(text: &str, from: i64, to: i64) -> String {
        format!(r#"{{"text":{text:?},"offsets":{{"from":{from},"to":{to}}}}}"#)
    }

    #[test]
    fn sub_word_tokens_join_into_one_word_with_the_outer_timestamps() {
        let json = segment(&format!(
            "{},{},{},{}",
            token("[_BEG_]", 0, 0),
            token(" Sub", 20, 200),
            token("l", 210, 240),
            token("oor", 280, 450)
        ));
        assert_eq!(
            words(&json),
            vec![Word {
                text: "Subloor".to_owned(),
                start_ms: 20,
                end_ms: 450
            }]
        );
    }

    #[test]
    fn every_special_token_spelling_is_dropped() {
        let json = segment(&format!(
            "{},{},{},{}",
            token("[_BEG_]", 0, 0),
            token("[_TT_216]", 0, 0),
            token(" hello", 100, 300),
            token("[_TT_428]", 300, 300)
        ));
        assert_eq!(words(&json).len(), 1);
    }

    #[test]
    fn the_language_comes_from_the_result_block() {
        let parsed = parse_transcript(segment(&token(" hi", 0, 10)).as_bytes()).expect("parses");
        assert_eq!(
            parsed,
            ParsedTranscript {
                language: "en".to_owned(),
                words: vec![Word {
                    text: "hi".to_owned(),
                    start_ms: 0,
                    end_ms: 10
                }]
            }
        );
    }

    #[test]
    fn a_missing_language_is_empty_rather_than_a_failure() {
        let json = r#"{"transcription":[{"tokens":[{"text":" hi","offsets":{"from":0,"to":5}}]}]}"#;
        assert_eq!(
            parse_transcript(json.as_bytes()).expect("parses").language,
            ""
        );
    }

    #[test]
    fn out_of_order_offsets_are_repaired_into_a_monotone_timeline() {
        let json = segment(&format!(
            "{},{},{}",
            token(" one", 500, 400),
            token(" two", 100, 900),
            token(" three", 950, 950)
        ));
        let words = words(&json);
        assert_eq!(
            words[0],
            Word {
                text: "one".to_owned(),
                start_ms: 500,
                end_ms: 500
            }
        );
        assert_eq!(
            words[1],
            Word {
                text: "two".to_owned(),
                start_ms: 500,
                end_ms: 900
            }
        );
        assert_eq!(
            words[2],
            Word {
                text: "three".to_owned(),
                start_ms: 950,
                end_ms: 950
            }
        );
    }

    #[test]
    fn a_negative_offset_is_clamped_not_wrapped() {
        let json = segment(&token(" hi", -5, 40));
        assert_eq!(words(&json)[0].start_ms, 0);
    }

    #[test]
    fn whitespace_only_tokens_never_become_words() {
        let json = segment(&format!("{},{}", token("   ", 0, 10), token(" hi", 20, 40)));
        assert_eq!(words(&json).len(), 1);
    }

    #[test]
    fn an_empty_transcription_array_parses_to_no_words() {
        let parsed = parse_transcript(br#"{"transcription":[]}"#).expect("parses");
        assert!(parsed.words.is_empty());
    }

    #[test]
    fn a_character_whisper_cut_in_half_costs_that_character_and_nothing_else() {
        // The exact shape a real run produced on a tone: the decoder stopped inside a U+266A.
        let mut json = Vec::new();
        json.extend_from_slice(
            br#"{"result":{"language":"en"},"transcription":[{"tokens":[{"text":" "#,
        );
        json.extend_from_slice(&[0xe2, 0x99, 0xaa]);
        json.extend_from_slice(br#"","offsets":{"from":0,"to":100}},{"text":" "#);
        json.extend_from_slice(&[0xe2, 0x99]);
        json.extend_from_slice(br#"","offsets":{"from":100,"to":200}}]}]}"#);

        let parsed = parse_transcript(&json).expect("one broken byte must not lose the run");
        assert_eq!(parsed.words.len(), 2);
        assert_eq!(parsed.words[0].text, "\u{266a}");
        assert_eq!(
            parsed.words[1].text, "\u{fffd}",
            "the cut character is marked, not guessed"
        );
    }

    #[test]
    fn the_committed_capture_of_a_real_run_parses_into_its_words() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/asr/whisper-tiny-en.json");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let parsed = parse_transcript(&bytes).expect("a real capture should parse");

        assert_eq!(parsed.language, "en");
        let text = parsed
            .words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("terminology"), "{text}");
        assert!(text.contains("episode."), "{text}");
        assert!(
            !text.contains("[_"),
            "special tokens must not reach the cues: {text}"
        );
        let mut previous = 0;
        for word in &parsed.words {
            assert!(word.start_ms >= previous, "{word:?}");
            assert!(word.end_ms >= word.start_ms, "{word:?}");
            previous = word.end_ms;
        }
        assert_eq!(
            parsed.words.len(),
            19,
            "the capture is two segments of committed test data: {:?}",
            parsed.words
        );
    }

    #[test]
    fn broken_json_is_a_structured_failure_not_a_panic() {
        for (json, note) in [
            (&b"{"[..], "truncated"),
            (&b""[..], "empty"),
            (&br#"{"transcription":{}}"#[..], "wrong type"),
            (&br#"{"transcription":[{}]}"#[..], "no tokens"),
            (
                &br#"{"transcription":[{"tokens":[{"text":" hi"}]}]}"#[..],
                "no offsets",
            ),
            (
                &br#"{"transcription":[{"tokens":[{"text":" hi","offsets":{"from":0}}]}]}"#[..],
                "half an offset",
            ),
            (
                &br#"{"transcription":[{"tokens":[{"offsets":{"from":0,"to":1}}]}]}"#[..],
                "no text",
            ),
        ] {
            let error = parse_transcript(json).expect_err(note);
            assert_eq!(error.kind, AsrErrorKind::NoOutput, "{note}");
            assert!(
                !error.detail.is_empty(),
                "{note} needs a detail for the log"
            );
        }
    }
}
