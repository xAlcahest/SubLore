//! WEBVTT: the header block, `NOTE`/`STYLE`/`REGION` blocks, and cues. See BACKLOG.md M1.2.
//!
//! Block-oriented: a run of blank lines separates blocks, a block runs to the next blank line or to
//! EOF, and the header is the run of non-blank lines the file opens with, so `X-TIMESTAMP-MAP` and
//! any other header line travels with `WEBVTT`. Payloads are never inspected: voice tags, class
//! tags, timestamp tags and entities are bytes that pass straight through.

use crate::cue::{Cue, CueDetail, VttCue};
use crate::document::{Segment, SegmentKind, SubtitleDocument, SubtitleFormat};
use crate::error::{ParseError, ParseErrorKind};
use crate::span::Span;
use crate::text::SourceText;
use crate::timecode::{parse_timecode, Timecode};

/// Blocks kept verbatim. The keyword opens the line and stands alone as a word.
const META_KEYWORDS: [&str; 3] = ["NOTE", "STYLE", "REGION"];

/// The only timing separator WEBVTT has.
const ARROW: &str = "-->";

/// Parse a decoded WEBVTT body into a tiling document. Frozen seam: [`crate::parse`] dispatches here.
pub(crate) fn parse(source: SourceText) -> Result<SubtitleDocument, ParseError> {
    if !opens_the_file(source.line_text(1)) {
        return Err(ParseError::at(&source, 0, ParseErrorKind::MissingVttHeader));
    }

    let lines = source.line_count();
    let mut segments = Vec::new();

    // Line 1 holds the header word, so the header block is the non-blank run starting there.
    let mut line = 1u32;
    let mut last = run_end(&source, line, false);
    segments.push(Segment {
        span: block_span(&source, line, last),
        kind: SegmentKind::Header,
    });

    while let Some(next) = last.checked_add(1) {
        line = next;
        if line > lines {
            break;
        }
        let blank = is_blank(source.line_text(line));
        last = run_end(&source, line, blank);
        let kind = if blank {
            SegmentKind::Blank
        } else {
            block_kind(&source, line, last)?
        };
        segments.push(Segment {
            span: block_span(&source, line, last),
            kind,
        });
    }

    Ok(SubtitleDocument::new(SubtitleFormat::Vtt, source, segments))
}

/// What the non-blank block spanning `first..=last` is: metadata, or a cue.
fn block_kind(source: &SourceText, first: u32, last: u32) -> Result<SegmentKind, ParseError> {
    if is_meta(source.line_text(first)) {
        return Ok(SegmentKind::Meta);
    }

    // A cue may open with an identifier, which is any line the arrow is not on.
    let timing = if source.line_text(first).contains(ARROW) {
        first
    } else if first < last && source.line_text(first + 1).contains(ARROW) {
        first + 1
    } else {
        let at = first_content(source, first);
        return Err(ParseError::at(source, at, ParseErrorKind::ExpectedTiming));
    };

    let (start, end, settings) = timing_line(source, timing)?;
    let id = (timing > first).then(|| content_span(source, first));
    let text = if timing < last {
        Span::new(
            source.line_span(timing + 1).start,
            content_end(source, last),
        )
    } else {
        // A cue with no payload lines: an empty span parked at the end of its timing line.
        let after = content_end(source, timing);
        Span::new(after, after)
    };

    Ok(SegmentKind::Cue(Cue {
        start,
        end,
        text,
        detail: CueDetail::Vtt(VttCue { id, settings }),
    }))
}

/// `start --> end [settings]`, read left to right so every failure reports where it stopped.
fn timing_line(
    source: &SourceText,
    line: u32,
) -> Result<(Timecode, Timecode, Option<Span>), ParseError> {
    let body = source.body();
    let content = content_span(source, line);

    let at = skip_spaces(body, content.start, content.end);
    let (start, after_start) =
        parse_timecode(body, at).map_err(|kind| ParseError::at(source, at, kind))?;

    let arrow = skip_spaces(body, after_start, content.end);
    if body.get(arrow..arrow.saturating_add(ARROW.len())) != Some(ARROW) {
        return Err(ParseError::at(
            source,
            arrow,
            ParseErrorKind::ExpectedTiming,
        ));
    }

    let at = skip_spaces(body, arrow + ARROW.len(), content.end);
    let (end, after_end) =
        parse_timecode(body, at).map_err(|kind| ParseError::at(source, at, kind))?;

    Ok((start, end, trimmed(body, after_end, content.end)))
}

fn opens_the_file(line: &str) -> bool {
    match line.strip_prefix("WEBVTT") {
        Some(rest) => rest.is_empty() || rest.starts_with([' ', '\t', '\r']),
        None => false,
    }
}

fn is_meta(line: &str) -> bool {
    META_KEYWORDS
        .iter()
        .any(|keyword| match line.strip_prefix(keyword) {
            Some(rest) => rest.is_empty() || rest.starts_with([' ', '\t', '\r']),
            None => false,
        })
}

/// A blank line separates blocks. Trailing spaces, tabs and a lone `\r` do not make it content.
fn is_blank(line: &str) -> bool {
    line.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

/// The last line of the run of lines starting at `first` whose blankness is `blank`.
fn run_end(source: &SourceText, first: u32, blank: bool) -> u32 {
    let lines = source.line_count();
    let mut last = first;
    while last < lines && is_blank(source.line_text(last + 1)) == blank {
        last += 1;
    }
    last
}

/// Every byte of `first..=last`, terminators included: what the segment owns.
fn block_span(source: &SourceText, first: u32, last: u32) -> Span {
    Span::new(source.line_span(first).start, source.line_span(last).end)
}

/// The line without its terminator: what the grammar reads.
fn content_span(source: &SourceText, line: u32) -> Span {
    let start = source.line_span(line).start;
    Span::new(start, start + source.line_text(line).len())
}

fn content_end(source: &SourceText, line: u32) -> usize {
    content_span(source, line).end
}

fn first_content(source: &SourceText, line: u32) -> usize {
    let content = content_span(source, line);
    skip_spaces(source.body(), content.start, content.end)
}

fn skip_spaces(body: &str, from: usize, to: usize) -> usize {
    let bytes = body.as_bytes();
    let mut at = from;
    while at < to && matches!(bytes.get(at), Some(b' ' | b'\t')) {
        at += 1;
    }
    at
}

/// `from..to` without its outer spaces and tabs, or `None` when nothing is left.
fn trimmed(body: &str, from: usize, to: usize) -> Option<Span> {
    let bytes = body.as_bytes();
    let start = skip_spaces(body, from, to);
    let mut end = to;
    while end > start && matches!(bytes.get(end - 1), Some(b' ' | b'\t')) {
        end -= 1;
    }
    (start < end).then(|| Span::new(start, end))
}

#[cfg(test)]
mod tests {
    use crate::cue::CueDetail;
    use crate::document::{SegmentKind, SubtitleDocument, SubtitleFormat};
    use crate::error::{ParseError, ParseErrorKind};

    fn parse(text: &str) -> Result<SubtitleDocument, ParseError> {
        crate::parse(SubtitleFormat::Vtt, text.as_bytes())
    }

    #[test]
    fn a_header_only_file_holds_no_cues_and_still_round_trips() {
        for text in [
            "WEBVTT",
            "WEBVTT\n",
            "WEBVTT\r\n",
            "WEBVTT - with a title\n",
        ] {
            let document = parse(text).expect("a header alone is a whole file");
            assert_eq!(document.cues().count(), 0);
            assert_eq!(document.segments().len(), 1);
            assert_eq!(document.to_bytes(), text.as_bytes());
            assert!(document.check_coverage().is_ok());
        }
    }

    #[test]
    fn the_header_word_must_stand_alone_on_the_first_line() {
        for text in ["WEBVTTX\n", "webvtt\n", " WEBVTT\n", "\nWEBVTT\n", ""] {
            let error = parse(text).expect_err("only WEBVTT opens a vtt file");
            assert_eq!(error.kind, ParseErrorKind::MissingVttHeader);
            assert_eq!((error.line, error.column), (1, 1));
        }
    }

    #[test]
    fn junk_before_the_arrow_is_reported_where_it_sits() {
        let error = parse("WEBVTT\n\n00:00:01.000 x --> 00:00:02.000\nHello.\n")
            .expect_err("the arrow must follow the start timestamp");
        assert_eq!(error.kind, ParseErrorKind::ExpectedTiming);
        assert_eq!((error.line, error.column), (3, 14));
    }

    #[test]
    fn a_missing_end_timestamp_is_a_bad_timecode() {
        let error = parse("WEBVTT\n\n00:00:01.000 -->\nHello.\n")
            .expect_err("the end timestamp is not optional");
        assert_eq!(error.kind, ParseErrorKind::BadTimecode);
        assert_eq!(error.line, 3);
    }

    #[test]
    fn an_hour_past_the_ceiling_keeps_its_own_kind() {
        let error = parse("WEBVTT\n\n999999999:00:00.000 --> 999999999:00:01.000\nHello.\n")
            .expect_err("the hours are past the ceiling");
        assert_eq!(error.kind, ParseErrorKind::TimecodeOutOfRange);
    }

    #[test]
    fn a_block_with_no_timing_line_stops_the_parse() {
        let error = parse("WEBVTT\n\njust some\nloose text\n")
            .expect_err("a block that is not metadata must carry a timing line");
        assert_eq!(error.kind, ParseErrorKind::ExpectedTiming);
        assert_eq!((error.line, error.column), (3, 1));
    }

    #[test]
    fn a_word_that_only_starts_like_a_keyword_is_an_identifier() {
        let document = parse("WEBVTT\n\nNOTES\n00:00:01.000 --> 00:00:02.000\nSo it is a cue.\n")
            .expect("NOTES opens a cue, not a note");
        assert_eq!(document.cues().count(), 1);
        assert!(matches!(document.segments()[2].kind, SegmentKind::Cue(_)));
    }

    #[test]
    fn settings_lose_their_padding_and_the_file_keeps_its_bytes() {
        let text = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000 \t align:start \t\nHello.\n";
        let document = parse(text).expect("padded settings are tolerated");
        let cue = document.cues().next().expect("the fixture holds one cue");
        let CueDetail::Vtt(detail) = &cue.detail else {
            panic!("a vtt cue must carry vtt detail");
        };
        let settings = detail.settings.expect("the timing line carries settings");
        assert_eq!(document.slice(settings), "align:start");
        assert_eq!(document.to_bytes(), text.as_bytes());
    }
}
