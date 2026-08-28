//! SRT: blocks separated by blank lines, each an optional index line, a timing line, then text.
//!
//! Two rules decide every block, and both are frozen here:
//! - A line is blank when its bytes are all spaces, tabs and CR, so a whitespace-only line separates
//!   blocks and never becomes cue text. Its bytes still travel out untouched, inside a `Blank`
//!   segment.
//! - A line is a timing line when it holds `-->`. That split keeps a broken timestamp reporting
//!   `BadTimecode` where it is, instead of a vague "expected a timing line" one block earlier.
//!
//! Nothing is repaired: numbering gaps, DVD coordinate trailers, dotted fractions and impossible
//! timings are recorded as written. See BACKLOG.md M1.1.

use crate::cue::{Cue, CueDetail, SrtCue};
use crate::document::{Segment, SegmentKind, SubtitleDocument, SubtitleFormat};
use crate::error::{ParseError, ParseErrorKind};
use crate::span::Span;
use crate::text::SourceText;
use crate::timecode::{parse_timecode, Timecode};

/// Wider than any real cue index; past this the line is text that happens to be digits.
const MAX_INDEX_DIGITS: usize = 9;

const ARROW: &str = "-->";

/// Parse a decoded SRT body into a tiling document. Frozen seam: [`crate::parse`] dispatches here.
pub(crate) fn parse(source: SourceText) -> Result<SubtitleDocument, ParseError> {
    let total = source.line_count();
    let mut segments = Vec::new();
    let mut line = 1;

    while line <= total {
        // Only an empty body has a line with no bytes, and it owns no segments at all.
        if source.line_span(line).is_empty() {
            break;
        }
        line = if is_blank(source.line_text(line)) {
            blank_run(&source, line, &mut segments)
        } else {
            cue_block(&source, line, &mut segments)?
        };
    }

    Ok(SubtitleDocument::new(SubtitleFormat::Srt, source, segments))
}

/// One `Blank` segment for the whole run of blank lines, and the line after it.
fn blank_run(source: &SourceText, first: u32, segments: &mut Vec<Segment>) -> u32 {
    let mut last = first;
    while last < source.line_count() && is_blank(source.line_text(last + 1)) {
        last += 1;
    }
    segments.push(Segment {
        span: Span::new(source.line_span(first).start, source.line_span(last).end),
        kind: SegmentKind::Blank,
    });
    last + 1
}

/// One `Cue` segment running from the block's first line through the terminator of its last text
/// line, and the line after it.
fn cue_block(
    source: &SourceText,
    first: u32,
    segments: &mut Vec<Segment>,
) -> Result<u32, ParseError> {
    let total = source.line_count();
    let index = index_line(source, first);
    let timing_line = match index {
        Some(_) if first < total && has_arrow(source.line_text(first + 1)) => first + 1,
        // An index line with nothing timed after it: point where the timing should have been.
        Some(_) => {
            let offset = if first < total {
                first_content(source, first + 1)
            } else {
                source.line_span(first).end
            };
            return Err(ParseError::at(
                source,
                offset,
                ParseErrorKind::ExpectedTiming,
            ));
        }
        None if has_arrow(source.line_text(first)) => first,
        None => {
            let offset = first_content(source, first);
            return Err(ParseError::at(
                source,
                offset,
                ParseErrorKind::ExpectedTiming,
            ));
        }
    };

    let timing = parse_timing(source, timing_line)?;

    let mut last = timing_line;
    while last < total && !is_blank(source.line_text(last + 1)) {
        last += 1;
    }
    let text = if last > timing_line {
        Span::new(
            source.line_span(timing_line + 1).start,
            content_end(source, last),
        )
    } else {
        // A cue with no text lines still needs a position: the end of its timing line.
        let end = content_end(source, timing_line);
        Span::new(end, end)
    };

    let (number, number_span) =
        index.map_or((None, None), |(number, span)| (Some(number), Some(span)));
    segments.push(Segment {
        span: Span::new(source.line_span(first).start, source.line_span(last).end),
        kind: SegmentKind::Cue(Cue {
            start: timing.start,
            end: timing.end,
            text,
            detail: CueDetail::Srt(SrtCue {
                number,
                number_span,
                timing_trailer: timing.trailer,
            }),
        }),
    });
    Ok(last + 1)
}

struct Timing {
    start: Timecode,
    end: Timecode,
    trailer: Option<Span>,
}

/// `start`, spaces, `-->`, spaces, `end`, then whatever the line still carries.
fn parse_timing(source: &SourceText, line: u32) -> Result<Timing, ParseError> {
    let body = source.body();
    let start_of_line = source.line_span(line).start;
    let end_of_content = content_end(source, line);

    let (start, after_start) = timecode(source, start_of_line)?;
    let at_arrow = skip_blanks(body, after_start, end_of_content);
    if !body
        .get(at_arrow..end_of_content)
        .unwrap_or("")
        .starts_with(ARROW)
    {
        return Err(ParseError::at(
            source,
            at_arrow,
            ParseErrorKind::ExpectedTiming,
        ));
    }
    let after_arrow = skip_blanks(body, at_arrow + ARROW.len(), end_of_content);
    let (end, after_end) = timecode(source, after_arrow)?;

    let trailer = if after_end < end_of_content {
        // DVD coordinates and stray junk: kept as written, terminator excluded.
        Some(Span::new(after_end, end_of_content))
    } else {
        None
    };
    Ok(Timing {
        start,
        end,
        trailer,
    })
}

fn timecode(source: &SourceText, offset: usize) -> Result<(Timecode, usize), ParseError> {
    parse_timecode(source.body(), offset).map_err(|kind| ParseError::at(source, offset, kind))
}

fn is_blank(text: &str) -> bool {
    text.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

fn has_arrow(text: &str) -> bool {
    text.contains(ARROW)
}

/// The index value and the line without its terminator, when the line holds nothing but digits.
fn index_line(source: &SourceText, line: u32) -> Option<(u32, Span)> {
    let text = source.line_text(line);
    let digits = text
        .trim_start_matches(' ')
        .trim_end_matches([' ', '\t', '\r']);
    if digits.is_empty()
        || digits.len() > MAX_INDEX_DIGITS
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let number = digits.parse().ok()?;
    let start = source.line_span(line).start;
    Some((number, Span::new(start, start + text.len())))
}

/// Where the line's first byte that is not a space or a tab sits, so an error points at it.
fn first_content(source: &SourceText, line: u32) -> usize {
    let text = source.line_text(line);
    let skipped = text.len() - text.trim_start_matches([' ', '\t']).len();
    source.line_span(line).start + skipped
}

/// Where the line's bytes end, terminator excluded.
fn content_end(source: &SourceText, line: u32) -> usize {
    source.line_span(line).start + source.line_text(line).len()
}

fn skip_blanks(body: &str, from: usize, limit: usize) -> usize {
    let bytes = body.as_bytes();
    let mut at = from;
    while at < limit && matches!(bytes.get(at), Some(b' ' | b'\t')) {
        at += 1;
    }
    at
}
