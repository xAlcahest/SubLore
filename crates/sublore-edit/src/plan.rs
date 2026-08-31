//! Turning an edit request into a byte splice, per format. See BACKLOG.md M2.1.
//!
//! Every mutation is one byte-range replacement over the document body. The bytes the replacement
//! does not name are memcpy'd by `splice::apply`, so "every other byte of the file is identical" is
//! structural rather than careful; re-parsing the result re-runs the M1 coverage guard on every
//! edit, and `verify` holds the parse to what the plan predicted. Nothing here reaches into
//! `sublore-formats`: the parsers stay the only authority on grammar.

use sublore_formats::{
    Cue, CueDetail, Newline, Segment, SegmentKind, Span, SrtCue, SubtitleDocument, SubtitleFormat,
    MAX_TIMECODE_MS,
};

use crate::diff;
use crate::error::{EditError, EditErrorKind};
use crate::splice::{self, EditKind, EditLabel, Splice};
use crate::verify;

/// Re-emitted byte for byte when the document had one; `sublore_formats` keeps its copy private.
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// The SRT scanner reads at most 9 digits as an index line, so a wider number would become text.
const MAX_INDEX: u32 = 999_999_999;

/// What the caller asked for. `cue` is an index into [`SubtitleDocument::cues`], which includes ASS
/// `Comment:` events; it is NOT the displayed count the status line shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// `text` uses "\n" for every line break whatever the file uses.
    SetText {
        cue: usize,
        text: String,
    },
    SetTimes {
        cue: usize,
        start_ms: u32,
        end_ms: u32,
    },
    /// `before == cues().count()` appends.
    Insert {
        before: usize,
        start_ms: u32,
        end_ms: u32,
        text: String,
    },
    Delete {
        cue: usize,
    },
    /// `text_offset` is a byte offset into the normalized text of `cue`.
    Split {
        cue: usize,
        text_offset: usize,
        at_ms: u32,
    },
    /// Merges `cue` and `cue + 1`.
    Merge {
        cue: usize,
    },
}

/// The byte replacement, its label, and what the document must look like once the edited bytes are
/// parsed again.
#[derive(Clone, Debug)]
pub struct Planned {
    pub splice: Splice,
    pub label: EditLabel,
    pub expect: Expectation,
}

/// What [`verify::verify`] holds the re-parsed document to.
#[derive(Clone, Debug)]
pub struct Expectation {
    /// The run of cue indices replaced, in the document as it was.
    pub from: usize,
    pub removed: usize,
    /// What the cues that replace them must read back as, in order. Text is in FILE form: the
    /// document's own line terminator, not the normalized "\n" the caller passed in.
    pub cues: Vec<ExpectedCue>,
    /// The run of segment indices the splice covers, in the document as it was.
    pub segments_from: usize,
    pub segments_removed: usize,
    /// How many segments replace them.
    pub segments_inserted: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedCue {
    pub text_raw: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

#[derive(Debug)]
pub struct Edited {
    pub document: SubtitleDocument,
    pub splice: Splice,
    pub label: EditLabel,
    /// `after.cues().count() - before.cues().count()`, for the history's replay check.
    pub cue_delta: isize,
}

/// Turn a request into a plan. Reads the document, writes nothing.
pub fn plan(document: &SubtitleDocument, edit: &Edit) -> Result<Planned, EditError> {
    match edit {
        Edit::SetText { cue, text } => plan_set_text(document, *cue, text),
        Edit::SetTimes {
            cue,
            start_ms,
            end_ms,
        } => plan_set_times(document, *cue, *start_ms, *end_ms),
        Edit::Insert {
            before,
            start_ms,
            end_ms,
            text,
        } => plan_insert(document, *before, *start_ms, *end_ms, text),
        Edit::Delete { cue } => plan_delete(document, *cue),
        Edit::Split {
            cue,
            text_offset,
            at_ms,
        } => plan_split(document, *cue, *text_offset, *at_ms),
        Edit::Merge { cue } => plan_merge(document, *cue),
    }
}

/// Plan, splice, re-parse, verify. The only way a document is ever edited. On any failure the
/// caller still holds the document it passed in, untouched.
pub fn edit(document: &SubtitleDocument, edit: &Edit) -> Result<Edited, EditError> {
    let planned = plan(document, edit)?;
    let body = splice::apply(document.source().body(), &planned.splice)?;
    // The format is carried over, never re-detected: an edit to the first lines of a file must not
    // change which parser reads it. See BACKLOG.md M2.1.
    let after = sublore_formats::parse(document.format(), &assemble(document, &body))
        .map_err(|error| EditError::from_parse(EditErrorKind::Reparse, error))?;
    verify::verify(document, &after, &planned.expect)?;

    let cue_delta = delta(document.cues().count(), after.cues().count());
    Ok(Edited {
        document: after,
        splice: planned.splice,
        label: planned.label,
        cue_delta,
    })
}

/// Apply a splice the history produced (undo, redo). Re-parses and refuses anything that does not
/// come back as a document; `apply`'s `removed` check is what makes a stale entry safe.
/// `expect_cue_delta` is the delta the entry recorded, negated for an undo.
pub fn replay(
    document: &SubtitleDocument,
    splice: &Splice,
    expect_cue_delta: isize,
) -> Result<SubtitleDocument, EditError> {
    let body = splice::apply(document.source().body(), splice)?;
    let after = sublore_formats::parse(document.format(), &assemble(document, &body))
        .map_err(|error| EditError::from_parse(EditErrorKind::Reparse, error))?;

    let moved = delta(document.cues().count(), after.cues().count());
    if moved != expect_cue_delta {
        return Err(EditError::new(
            EditErrorKind::Unverified,
            format!(
                "the replay moved the cue count by {moved}, the entry recorded {expect_cue_delta}"
            ),
        ));
    }
    Ok(after)
}

/// The file the edited body spells: the document's byte-order mark, then the body.
fn assemble(document: &SubtitleDocument, body: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(body.len().saturating_add(UTF8_BOM.len()));
    if document.source().has_bom() {
        bytes.extend_from_slice(&UTF8_BOM);
    }
    bytes.extend_from_slice(body.as_bytes());
    bytes
}

fn delta(before: usize, after: usize) -> isize {
    let before = isize::try_from(before).unwrap_or(isize::MAX);
    let after = isize::try_from(after).unwrap_or(isize::MAX);
    after.saturating_sub(before)
}

// ---------------------------------------------------------------------------------------------
// Reading the document
// ---------------------------------------------------------------------------------------------

/// A cue, the segment that owns it, and where that segment sits.
struct Located<'a> {
    segment_index: usize,
    segment: &'a Segment,
    cue: &'a Cue,
}

fn locate(document: &SubtitleDocument, index: usize) -> Result<Located<'_>, EditError> {
    let mut seen = 0usize;
    for (segment_index, segment) in document.segments().iter().enumerate() {
        let SegmentKind::Cue(cue) = &segment.kind else {
            continue;
        };
        if seen == index {
            return Ok(Located {
                segment_index,
                segment,
                cue,
            });
        }
        seen += 1;
    }
    Err(EditError::new(
        EditErrorKind::NoSuchCue,
        format!("cue {index}: the document holds {seen}"),
    ))
}

/// The line terminator this segment writes: the first one inside it, or the file's own when the
/// segment holds none (the last block of a file with no final newline).
fn newline_of(document: &SubtitleDocument, segment: &Segment) -> &'static str {
    let text = document.slice(segment.span);
    match text.find('\n') {
        Some(at) if at > 0 && text.as_bytes().get(at - 1) == Some(&b'\r') => "\r\n",
        Some(_) => "\n",
        None => match document.source().newline() {
            Newline::Crlf => "\r\n",
            Newline::Lf | Newline::Mixed | Newline::None => "\n",
        },
    }
}

/// The terminator this cue's text lines are written with: the one just above the text, or the one
/// that ends the timing line when the text is empty, or the block's own.
fn text_newline(document: &SubtitleDocument, located: &Located<'_>) -> &'static str {
    let body = document.source().body();
    let span = located.cue.text;
    let local = if span.is_empty() {
        terminator_at(body, span.end)
    } else {
        terminator_before(body, span.start)
    };
    local.unwrap_or_else(|| newline_of(document, located.segment))
}

/// The line terminator ending `body[..at]`, when there is one.
fn terminator_before(body: &str, at: usize) -> Option<&'static str> {
    let head = body.get(..at)?;
    if head.ends_with("\r\n") {
        Some("\r\n")
    } else if head.ends_with('\n') {
        Some("\n")
    } else {
        None
    }
}

/// The line terminator starting at `at`, when one starts there.
fn terminator_at(body: &str, at: usize) -> Option<&'static str> {
    let tail = body.get(at..)?;
    if tail.starts_with("\r\n") {
        Some("\r\n")
    } else if tail.starts_with('\n') {
        Some("\n")
    } else {
        None
    }
}

/// How many bytes of line terminator `text` ends with.
fn terminator_len(text: &str) -> usize {
    if text.ends_with("\r\n") {
        2
    } else if text.ends_with('\n') {
        1
    } else {
        0
    }
}

/// The cue's two timestamps in the order the file wrote them: an ASS `Format:` line may put End
/// before Start.
fn ordered(cue: &Cue) -> (Span, Span) {
    let (start, end) = (cue.start.raw(), cue.end.raw());
    if start.start <= end.start {
        (start, end)
    } else {
        (end, start)
    }
}

fn srt_detail(cue: &Cue) -> Option<&SrtCue> {
    match &cue.detail {
        CueDetail::Srt(srt) => Some(srt),
        CueDetail::Vtt(_) | CueDetail::Ass(_) => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Writing text and timestamps
// ---------------------------------------------------------------------------------------------

/// A replacement the format cannot hold. SRT and VTT break blocks on blank lines, so a blank line
/// inside cue text would split the cue in two; an ASS event is one line.
fn validate_text(format: SubtitleFormat, text: &str) -> Result<(), EditError> {
    let unwritable = |detail: &str| EditError::new(EditErrorKind::UnwritableText, detail);
    match format {
        SubtitleFormat::Ass => {
            if text.contains(['\n', '\r']) {
                return Err(unwritable("an ASS event holds no line break; use \\N"));
            }
        }
        SubtitleFormat::Srt | SubtitleFormat::Vtt => {
            if text.is_empty() {
                return Ok(());
            }
            if text.starts_with('\n') || text.ends_with('\n') {
                return Err(unwritable(
                    "cue text may not start or end with a line break",
                ));
            }
            // A `\r` against a line break cannot be told from a terminator once written back.
            if text.ends_with('\r') || text.contains("\r\n") {
                return Err(unwritable(
                    "a carriage return may not end a line of cue text",
                ));
            }
            if text.split('\n').any(is_blank_line) {
                return Err(unwritable(
                    "a blank line inside cue text would split the cue",
                ));
            }
        }
    }
    Ok(())
}

/// Blank by the parsers' own rule: nothing a reader would see.
fn is_blank_line(line: &str) -> bool {
    line.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

/// Normalized text in file form: every `\n` becomes the terminator the block writes.
fn render_text(text: &str, newline: &str) -> String {
    if newline == "\n" {
        text.to_owned()
    } else {
        text.replace('\n', newline)
    }
}

/// How a timestamp was spelled, so a rewritten one is spelled the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimeShape {
    /// Digit width of the hours field; `None` for the VTT `mm:ss` short form.
    hours: Option<usize>,
    /// `,` or `.`, as the file wrote it.
    separator: char,
    /// 1 for tenths, 2 for centiseconds, 3 for milliseconds.
    fraction: usize,
}

fn shape_of(raw: &str) -> TimeShape {
    let separator = if raw.contains(',') { ',' } else { '.' };
    let (clock, fraction) = raw.rsplit_once(separator).unwrap_or((raw, ""));
    let hours = (clock.matches(':').count() >= 2)
        .then(|| clock.split(':').next().unwrap_or("").len().clamp(1, 9));
    TimeShape {
        hours,
        separator,
        fraction: fraction.len().clamp(1, 3),
    }
}

fn default_shape(format: SubtitleFormat) -> TimeShape {
    match format {
        SubtitleFormat::Srt => TimeShape {
            hours: Some(2),
            separator: ',',
            fraction: 3,
        },
        SubtitleFormat::Vtt => TimeShape {
            hours: Some(2),
            separator: '.',
            fraction: 3,
        },
        SubtitleFormat::Ass => TimeShape {
            hours: Some(1),
            separator: '.',
            fraction: 2,
        },
    }
}

/// Spell `millis` the way `shape` was spelled. Widening is allowed where it loses nothing; rounding
/// never is, because rounding a value the caller asked for is a silent change to the user's data.
fn render_timecode(millis: u32, shape: TimeShape) -> Result<String, EditError> {
    if millis > MAX_TIMECODE_MS {
        return Err(EditError::new(
            EditErrorKind::UnwritableTimecode,
            format!("{millis} ms is past the {MAX_TIMECODE_MS} ms ceiling"),
        ));
    }
    let width = shape.fraction.clamp(1, 3);
    let step: u32 = match width {
        1 => 100,
        2 => 10,
        _ => 1,
    };
    let fraction = millis % 1_000;
    if !fraction.is_multiple_of(step) {
        return Err(EditError::new(
            EditErrorKind::UnwritableTimecode,
            format!("{millis} ms needs more than {width} fraction digit(s)"),
        ));
    }

    let seconds = millis / 1_000;
    let clock = match shape.hours {
        Some(digits) => format!(
            "{:0width$}:{:02}:{:02}",
            seconds / 3_600,
            (seconds / 60) % 60,
            seconds % 60,
            width = digits.clamp(1, 9)
        ),
        None if seconds / 60 > 59 => {
            // The `mm:ss` short form cannot hold an hour, and WEBVTT spells hours with two digits
            // or more, so the promotion widens rather than writing `1:00:00.000`.
            return render_timecode(
                millis,
                TimeShape {
                    hours: Some(2),
                    ..shape
                },
            );
        }
        None => format!("{:02}:{:02}", seconds / 60, seconds % 60),
    };
    Ok(format!(
        "{clock}{}{:0width$}",
        shape.separator,
        fraction / step,
        width = width
    ))
}

// ---------------------------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------------------------

/// The shape one SRT or VTT block is written in. Everything here is bytes the file already had,
/// except the timestamps and the text.
#[derive(Clone, Copy, Debug)]
struct Block<'a> {
    newline: &'static str,
    /// Whatever sits between the two timestamps, `" --> "` in every file anyone has shipped.
    arrow: &'a str,
    start_shape: TimeShape,
    end_shape: TimeShape,
    /// SRT: the index line's number, when the block has one.
    number: Option<u32>,
    /// SRT: whatever followed the end timestamp, DVD coordinates included.
    trailer: Option<&'a str>,
    /// VTT: the cue identifier line, when the block has one.
    id: Option<&'a str>,
    /// VTT: the cue settings after the end timestamp.
    settings: Option<&'a str>,
}

impl Block<'_> {
    fn render(
        &self,
        format: SubtitleFormat,
        start_ms: u32,
        end_ms: u32,
        text: &str,
        terminate: bool,
    ) -> Result<String, EditError> {
        let newline = self.newline;
        let mut block = String::new();
        match format {
            SubtitleFormat::Srt => {
                if let Some(number) = self.number {
                    if number > MAX_INDEX {
                        return Err(EditError::new(
                            EditErrorKind::NotApplicable,
                            format!("index {number} is wider than an SRT index line"),
                        ));
                    }
                    block.push_str(&number.to_string());
                    block.push_str(newline);
                }
            }
            SubtitleFormat::Vtt => {
                if let Some(id) = self.id {
                    block.push_str(id);
                    block.push_str(newline);
                }
            }
            SubtitleFormat::Ass => {
                return Err(EditError::new(
                    EditErrorKind::NotApplicable,
                    "an ASS event is built from its own line, not from a block",
                ))
            }
        }

        block.push_str(&render_timecode(start_ms, self.start_shape)?);
        block.push_str(self.arrow);
        block.push_str(&render_timecode(end_ms, self.end_shape)?);
        match format {
            SubtitleFormat::Srt => block.push_str(self.trailer.unwrap_or("")),
            SubtitleFormat::Vtt => {
                if let Some(settings) = self.settings {
                    // The parser trims the settings span, so the separator is written back here.
                    block.push(' ');
                    block.push_str(settings);
                }
            }
            SubtitleFormat::Ass => {}
        }

        if !text.is_empty() {
            block.push_str(newline);
            block.push_str(&render_text(text, newline));
        }
        if terminate {
            block.push_str(newline);
        }
        Ok(block)
    }
}

/// The shape of an existing block, identity included: a split or a merge keeps the index line, the
/// identifier, the settings and the trailer the user already had.
fn block_of<'a>(
    document: &'a SubtitleDocument,
    located: &Located<'a>,
) -> Result<Block<'a>, EditError> {
    let cue = located.cue;
    let (number, trailer) = match srt_detail(cue) {
        Some(srt) => (
            srt.number,
            srt.timing_trailer.map(|span| document.slice(span)),
        ),
        None => (None, None),
    };
    let (id, settings) = match &cue.detail {
        CueDetail::Vtt(vtt) => (
            vtt.id.map(|span| document.slice(span)),
            vtt.settings.map(|span| document.slice(span)),
        ),
        CueDetail::Srt(_) | CueDetail::Ass(_) => (None, None),
    };
    Ok(Block {
        newline: newline_of(document, located.segment),
        arrow: arrow_of(document, cue)?,
        start_shape: shape_of(document.slice(cue.start.raw())),
        end_shape: shape_of(document.slice(cue.end.raw())),
        number,
        trailer,
        id,
        settings,
    })
}

fn arrow_of<'a>(document: &'a SubtitleDocument, cue: &Cue) -> Result<&'a str, EditError> {
    let (start, end) = (cue.start.raw(), cue.end.raw());
    if start.end > end.start {
        return Err(EditError::new(
            EditErrorKind::NotApplicable,
            "the cue's timestamps are not written in order",
        ));
    }
    Ok(document.slice(Span::new(start.end, end.start)))
}

/// One ASS event line built from an existing one: the two timing fields and the text field are
/// replaced, every other field is copied byte for byte, so the new line still satisfies the
/// section's `Format:` line.
fn ass_line(
    document: &SubtitleDocument,
    from: &Located<'_>,
    start_ms: u32,
    end_ms: u32,
    text: &str,
    as_dialogue: bool,
    terminate: bool,
) -> Result<String, EditError> {
    let CueDetail::Ass(event) = &from.cue.detail else {
        return Err(EditError::new(
            EditErrorKind::NotApplicable,
            "the cue is not an ASS event",
        ));
    };
    let line = document.slice(from.segment.span);
    let content_end = from
        .segment
        .span
        .end
        .saturating_sub(terminator_len(line))
        .max(from.segment.span.start);

    let Some(text_span) = event.fields.get(event.text_field).copied() else {
        return Err(EditError::new(
            EditErrorKind::NotApplicable,
            "the event declares no text field",
        ));
    };

    let mut fields = vec![
        (
            from.cue.start.raw(),
            render_timecode(start_ms, shape_of(document.slice(from.cue.start.raw())))?,
        ),
        (
            from.cue.end.raw(),
            render_timecode(end_ms, shape_of(document.slice(from.cue.end.raw())))?,
        ),
        (text_span, text.to_owned()),
    ];
    // A new event is a Dialogue even when the line it was copied from is a Comment.
    if as_dialogue && event.kind == sublore_formats::AssEventKind::Comment {
        fields.push((event.descriptor, "Dialogue".to_owned()));
    }
    fields.sort_by_key(|(span, _)| span.start);

    let mut out = String::with_capacity(line.len().saturating_add(text.len()));
    let mut at = from.segment.span.start;
    for (span, replacement) in fields {
        if span.start < at || span.end > content_end {
            return Err(EditError::new(
                EditErrorKind::NotApplicable,
                "the event's fields overlap or escape their line",
            ));
        }
        out.push_str(document.slice(Span::new(at, span.start)));
        out.push_str(&replacement);
        at = span.end;
    }
    out.push_str(document.slice(Span::new(at, content_end)));
    if terminate {
        out.push_str(newline_of(document, from.segment));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The mutations
// ---------------------------------------------------------------------------------------------

fn plan_set_text(
    document: &SubtitleDocument,
    index: usize,
    text: &str,
) -> Result<Planned, EditError> {
    let located = locate(document, index)?;
    let format = document.format();
    let text = diff::normalize(text);
    validate_text(format, &text)?;

    let body = document.source().body();
    let span = located.cue.text;
    let newline = text_newline(document, &located);
    let written = render_text(&text, newline);

    // An SRT or VTT cue with no text parks an empty span at the end of its timing line: writing
    // into it directly would grow a timing trailer instead of a text line.
    let (range, inserted) = match format {
        SubtitleFormat::Ass => (span, written.clone()),
        SubtitleFormat::Srt | SubtitleFormat::Vtt => match (span.is_empty(), text.is_empty()) {
            (false, false) => (span, written.clone()),
            (false, true) => {
                let lead = terminator_before(body, span.start).unwrap_or("").len();
                (
                    Span::new(span.start.saturating_sub(lead), span.end),
                    String::new(),
                )
            }
            (true, false) => (span, format!("{newline}{written}")),
            (true, true) => {
                return Err(EditError::new(
                    EditErrorKind::NotApplicable,
                    "the cue already has no text",
                ))
            }
        },
    };

    Ok(Planned {
        splice: Splice::new(range.start, document.slice(range).to_owned(), inserted),
        label: EditLabel {
            kind: EditKind::SetText,
            cue: index,
        },
        expect: Expectation {
            from: index,
            removed: 1,
            cues: vec![ExpectedCue {
                text_raw: written,
                start_ms: located.cue.start.millis(),
                end_ms: located.cue.end.millis(),
            }],
            segments_from: located.segment_index,
            segments_removed: 1,
            segments_inserted: 1,
        },
    })
}

fn plan_set_times(
    document: &SubtitleDocument,
    index: usize,
    start_ms: u32,
    end_ms: u32,
) -> Result<Planned, EditError> {
    let located = locate(document, index)?;
    let cue = located.cue;
    let (first_span, second_span) = ordered(cue);
    if first_span.end > second_span.start {
        return Err(EditError::new(
            EditErrorKind::NotApplicable,
            "the cue's timestamps overlap in the file",
        ));
    }
    let (first_ms, second_ms) = if cue.start.raw().start <= cue.end.raw().start {
        (start_ms, end_ms)
    } else {
        (end_ms, start_ms)
    };

    // Only the two timestamps are rewritten; whatever the file wrote between and around them --
    // the arrow, ASS fields, DVD coordinates, VTT settings -- is never looked at.
    let first = render_timecode(first_ms, shape_of(document.slice(first_span)))?;
    let second = render_timecode(second_ms, shape_of(document.slice(second_span)))?;
    let between = document.slice(Span::new(first_span.end, second_span.start));
    let region = Span::new(first_span.start, second_span.end);

    Ok(Planned {
        splice: Splice::new(
            region.start,
            document.slice(region).to_owned(),
            format!("{first}{between}{second}"),
        ),
        label: EditLabel {
            kind: EditKind::SetTimes,
            cue: index,
        },
        expect: Expectation {
            from: index,
            removed: 1,
            cues: vec![ExpectedCue {
                text_raw: document.slice(cue.text).to_owned(),
                start_ms,
                end_ms,
            }],
            segments_from: located.segment_index,
            segments_removed: 1,
            segments_inserted: 1,
        },
    })
}

fn plan_insert(
    document: &SubtitleDocument,
    before: usize,
    start_ms: u32,
    end_ms: u32,
    text: &str,
) -> Result<Planned, EditError> {
    let count = document.cues().count();
    if before > count {
        return Err(EditError::new(
            EditErrorKind::NoSuchCue,
            format!("cue {before}: the document holds {count}"),
        ));
    }
    let format = document.format();
    let text = diff::normalize(text);
    validate_text(format, &text)?;

    // The new cue mirrors a neighbour's spelling: the cue before it, or the one after it.
    let neighbour = match before
        .checked_sub(1)
        .and_then(|at| locate(document, at).ok())
    {
        Some(previous) => Some(previous),
        None => locate(document, before).ok(),
    };

    let (splice, expected_text, segments_from, segments_inserted) = match format {
        SubtitleFormat::Ass => insert_ass(
            document,
            before,
            count,
            start_ms,
            end_ms,
            &text,
            neighbour.as_ref(),
        )?,
        SubtitleFormat::Srt | SubtitleFormat::Vtt => insert_block(
            document,
            before,
            count,
            start_ms,
            end_ms,
            &text,
            neighbour.as_ref(),
        )?,
    };

    Ok(Planned {
        splice,
        label: EditLabel {
            kind: EditKind::Insert,
            cue: before,
        },
        expect: Expectation {
            from: before,
            removed: 0,
            cues: vec![ExpectedCue {
                text_raw: expected_text,
                start_ms,
                end_ms,
            }],
            segments_from,
            segments_removed: 0,
            segments_inserted,
        },
    })
}

fn insert_block(
    document: &SubtitleDocument,
    before: usize,
    count: usize,
    start_ms: u32,
    end_ms: u32,
    text: &str,
    neighbour: Option<&Located<'_>>,
) -> Result<(Splice, String, usize, usize), EditError> {
    let format = document.format();
    let block = match neighbour {
        Some(near) => Block {
            newline: newline_of(document, near.segment),
            arrow: arrow_of(document, near.cue)?,
            start_shape: shape_of(document.slice(near.cue.start.raw())),
            end_shape: shape_of(document.slice(near.cue.end.raw())),
            // A new block carries no identity of its own: no DVD trailer, no identifier, no
            // settings. Duplicating a neighbour's identifier would be worse than having none.
            number: insert_number(document, before, near),
            trailer: None,
            id: None,
            settings: None,
        },
        None => Block {
            newline: default_newline(document),
            arrow: " --> ",
            start_shape: default_shape(format),
            end_shape: default_shape(format),
            number: (format == SubtitleFormat::Srt).then_some(1),
            trailer: None,
            id: None,
            settings: None,
        },
    };
    let newline = block.newline;
    let body = document.source().body();

    if before < count {
        let target = locate(document, before)?;
        let block_text = block.render(format, start_ms, end_ms, text, true)?;
        let inserted = format!("{block_text}{newline}");
        return Ok((
            Splice::new(target.segment.span.start, String::new(), inserted),
            render_text(text, newline),
            target.segment_index,
            2,
        ));
    }

    if let Some(last) = count
        .checked_sub(1)
        .and_then(|at| locate(document, at).ok())
    {
        let terminated = terminator_len(document.slice(last.segment.span)) > 0;
        // A separator on the side facing existing content: one blank line, plus the terminator the
        // last block never got when the file ends without one.
        let separator = if terminated {
            newline.to_owned()
        } else {
            format!("{newline}{newline}")
        };
        let block_text = block.render(format, start_ms, end_ms, text, terminated)?;
        return Ok((
            Splice::new(
                last.segment.span.end,
                String::new(),
                separator + &block_text,
            ),
            render_text(text, newline),
            last.segment_index.saturating_add(1),
            2,
        ));
    }

    let segments = document.segments();
    let last_is_blank = segments
        .last()
        .is_some_and(|segment| matches!(segment.kind, SegmentKind::Blank));
    let mut prefix = String::new();
    if !body.is_empty() && terminator_len(body) == 0 {
        prefix.push_str(newline);
    }
    let blank_added = !body.is_empty() && !last_is_blank;
    if blank_added {
        prefix.push_str(newline);
    }
    let terminate = body.is_empty() || terminator_len(body) > 0;
    let block_text = block.render(format, start_ms, end_ms, text, terminate)?;
    Ok((
        Splice::new(body.len(), String::new(), prefix + &block_text),
        render_text(text, newline),
        segments.len(),
        1 + usize::from(blank_added),
    ))
}

fn insert_ass(
    document: &SubtitleDocument,
    before: usize,
    count: usize,
    start_ms: u32,
    end_ms: u32,
    text: &str,
    neighbour: Option<&Located<'_>>,
) -> Result<(Splice, String, usize, usize), EditError> {
    // Without an event to copy from there is no way to know the section's field list, and guessing
    // one would write a line the file's own `Format:` does not describe.
    let Some(near) = neighbour else {
        return Err(EditError::new(
            EditErrorKind::NotApplicable,
            "the file holds no event to copy a shape from",
        ));
    };

    if before < count {
        let target = locate(document, before)?;
        let line = ass_line(document, near, start_ms, end_ms, text, true, true)?;
        return Ok((
            Splice::new(target.segment.span.start, String::new(), line),
            text.to_owned(),
            target.segment_index,
            1,
        ));
    }

    let last = locate(document, count.saturating_sub(1))?;
    let terminated = terminator_len(document.slice(last.segment.span)) > 0;
    let mut inserted = String::new();
    if !terminated {
        inserted.push_str(newline_of(document, last.segment));
    }
    inserted.push_str(&ass_line(
        document, near, start_ms, end_ms, text, true, terminated,
    )?);
    Ok((
        Splice::new(last.segment.span.end, String::new(), inserted),
        text.to_owned(),
        last.segment_index.saturating_add(1),
        1,
    ))
}

/// The index line a new SRT block gets: only when its neighbour has one, and never renumbering any
/// cue the user did not edit. Duplicates and gaps are what every player already tolerates.
fn insert_number(
    document: &SubtitleDocument,
    before: usize,
    neighbour: &Located<'_>,
) -> Option<u32> {
    if document.format() != SubtitleFormat::Srt {
        return None;
    }
    srt_detail(neighbour.cue)?.number?;
    let previous = before
        .checked_sub(1)
        .and_then(|at| locate(document, at).ok())
        .and_then(|located| srt_detail(located.cue).and_then(|srt| srt.number));
    Some(previous.map_or(1, |number| number.saturating_add(1)))
}

fn default_newline(document: &SubtitleDocument) -> &'static str {
    match document.source().newline() {
        Newline::Crlf => "\r\n",
        Newline::Lf | Newline::Mixed | Newline::None => "\n",
    }
}

fn plan_delete(document: &SubtitleDocument, index: usize) -> Result<Planned, EditError> {
    let located = locate(document, index)?;
    let segments = document.segments();
    let at = located.segment_index;

    let follows = segments
        .get(at.saturating_add(1))
        .is_some_and(|segment| matches!(segment.kind, SegmentKind::Blank));
    let precedes = at > 0
        && segments
            .get(at.saturating_sub(1))
            .is_some_and(|segment| matches!(segment.kind, SegmentKind::Blank));

    // The blank line that separates blocks belongs to the block being removed. An ASS blank
    // separates sections, so it stays unless it would join the one on the other side. M2.1.
    let (from, to) = match document.format() {
        SubtitleFormat::Ass => {
            if follows && precedes {
                (at, at.saturating_add(1))
            } else {
                (at, at)
            }
        }
        SubtitleFormat::Srt | SubtitleFormat::Vtt => {
            if follows {
                (at, at.saturating_add(1))
            } else if precedes {
                (at.saturating_sub(1), at)
            } else {
                (at, at)
            }
        }
    };

    let (Some(first), Some(last)) = (segments.get(from), segments.get(to)) else {
        return Err(EditError::new(
            EditErrorKind::NotApplicable,
            "the cue's segments moved while the edit was planned",
        ));
    };
    let region = Span::new(first.span.start, last.span.end);

    Ok(Planned {
        splice: Splice::new(
            region.start,
            document.slice(region).to_owned(),
            String::new(),
        ),
        label: EditLabel {
            kind: EditKind::Delete,
            cue: index,
        },
        expect: Expectation {
            from: index,
            removed: 1,
            cues: Vec::new(),
            segments_from: from,
            segments_removed: to.saturating_sub(from).saturating_add(1),
            segments_inserted: 0,
        },
    })
}

fn plan_split(
    document: &SubtitleDocument,
    index: usize,
    text_offset: usize,
    at_ms: u32,
) -> Result<Planned, EditError> {
    let located = locate(document, index)?;
    let cue = located.cue;
    let format = document.format();
    let text = diff::normalize(document.slice(cue.text));

    if text_offset > text.len() || !text.is_char_boundary(text_offset) {
        return Err(EditError::new(
            EditErrorKind::NotApplicable,
            format!(
                "offset {text_offset} is not a character offset of {} bytes",
                text.len()
            ),
        ));
    }
    let (low, high) = (
        cue.start.millis().min(cue.end.millis()),
        cue.start.millis().max(cue.end.millis()),
    );
    if at_ms < low || at_ms > high {
        return Err(EditError::new(
            EditErrorKind::NotApplicable,
            format!("{at_ms} ms is outside the cue's {low}..{high} ms"),
        ));
    }

    let first = text.get(..text_offset).unwrap_or("").trim_matches('\n');
    let second = text.get(text_offset..).unwrap_or("").trim_matches('\n');
    if first.is_empty() || second.is_empty() {
        return Err(EditError::new(
            EditErrorKind::NotApplicable,
            "a split half would hold no text",
        ));
    }
    validate_text(format, first)?;
    validate_text(format, second)?;

    let span = located.segment.span;
    let terminated = terminator_len(document.slice(span)) > 0;
    let (start_ms, end_ms) = (cue.start.millis(), cue.end.millis());

    let (inserted, expected, segments_inserted) = match format {
        SubtitleFormat::Ass => {
            let head = ass_line(document, &located, start_ms, at_ms, first, false, true)?;
            let tail = ass_line(document, &located, at_ms, end_ms, second, false, terminated)?;
            (
                format!("{head}{tail}"),
                vec![first.to_owned(), second.to_owned()],
                2,
            )
        }
        SubtitleFormat::Srt | SubtitleFormat::Vtt => {
            let block = block_of(document, &located)?;
            let newline = block.newline;
            let head = block.render(format, start_ms, at_ms, first, true)?;
            // The second half keeps the block's shape but not its identity: a duplicated VTT
            // identifier would name two cues.
            let tail_block = Block {
                number: block.number.map(|number| number.saturating_add(1)),
                id: None,
                ..block
            };
            let tail = tail_block.render(format, at_ms, end_ms, second, terminated)?;
            (
                format!("{head}{newline}{tail}"),
                vec![render_text(first, newline), render_text(second, newline)],
                3,
            )
        }
    };

    let mut cues = Vec::with_capacity(2);
    for (offset, text_raw) in expected.into_iter().enumerate() {
        let (from_ms, to_ms) = if offset == 0 {
            (start_ms, at_ms)
        } else {
            (at_ms, end_ms)
        };
        cues.push(ExpectedCue {
            text_raw,
            start_ms: from_ms,
            end_ms: to_ms,
        });
    }

    Ok(Planned {
        splice: Splice::new(span.start, document.slice(span).to_owned(), inserted),
        label: EditLabel {
            kind: EditKind::Split,
            cue: index,
        },
        expect: Expectation {
            from: index,
            removed: 1,
            cues,
            segments_from: located.segment_index,
            segments_removed: 1,
            segments_inserted,
        },
    })
}

fn plan_merge(document: &SubtitleDocument, index: usize) -> Result<Planned, EditError> {
    let first = locate(document, index)?;
    let second = locate(document, index.saturating_add(1))?;
    let format = document.format();
    let segments = document.segments();

    // Merging two cues may only swallow the blank lines between them. A VTT NOTE or an ASS comment
    // line in the gap belongs to no cue and must not disappear with them. See CONTRIBUTING.md §3.
    for position in first.segment_index.saturating_add(1)..second.segment_index {
        let Some(segment) = segments.get(position) else {
            break;
        };
        if !matches!(segment.kind, SegmentKind::Blank) {
            return Err(EditError::new(
                EditErrorKind::NotApplicable,
                "the two cues are separated by content that is not blank lines",
            ));
        }
    }

    let head = diff::normalize(document.slice(first.cue.text));
    let tail = diff::normalize(document.slice(second.cue.text));
    let joined = if head.is_empty() || tail.is_empty() {
        format!("{head}{tail}")
    } else {
        match format {
            SubtitleFormat::Ass => format!("{head}\\N{tail}"),
            SubtitleFormat::Srt | SubtitleFormat::Vtt => format!("{head}\n{tail}"),
        }
    };
    validate_text(format, &joined)?;

    let region = Span::new(first.segment.span.start, second.segment.span.end);
    let terminated = terminator_len(document.slice(second.segment.span)) > 0;
    let (start_ms, end_ms) = (first.cue.start.millis(), second.cue.end.millis());

    // The second cue's own shape -- its index line, identifier or non-timing fields -- goes with
    // it: two lines becoming one can only keep one shape, and undo restores the bytes exactly.
    let (inserted, text_raw) = match format {
        SubtitleFormat::Ass => (
            ass_line(
                document, &first, start_ms, end_ms, &joined, false, terminated,
            )?,
            joined.clone(),
        ),
        SubtitleFormat::Srt | SubtitleFormat::Vtt => {
            let block = block_of(document, &first)?;
            (
                block.render(format, start_ms, end_ms, &joined, terminated)?,
                render_text(&joined, block.newline),
            )
        }
    };

    Ok(Planned {
        splice: Splice::new(region.start, document.slice(region).to_owned(), inserted),
        label: EditLabel {
            kind: EditKind::Merge,
            cue: index,
        },
        expect: Expectation {
            from: index,
            removed: 2,
            cues: vec![ExpectedCue {
                text_raw,
                start_ms,
                end_ms,
            }],
            segments_from: first.segment_index,
            segments_removed: second
                .segment_index
                .saturating_sub(first.segment_index)
                .saturating_add(1),
            segments_inserted: 1,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{
        default_shape, is_blank_line, render_text, render_timecode, shape_of, validate_text,
        TimeShape,
    };
    use sublore_formats::{timecode::parse_timecode, SubtitleFormat, MAX_TIMECODE_MS};

    /// Every rendered timestamp is proved by the scanner that will read it back.
    fn round_trip(millis: u32, shape: TimeShape) -> String {
        let written = render_timecode(millis, shape).expect("the shape can hold the value");
        let (timecode, end) = parse_timecode(&written, 0).expect("the scanner reads it back");
        assert_eq!(end, written.len(), "{written:?} must be consumed whole");
        assert_eq!(timecode.millis(), millis, "{written:?} must mean {millis}");
        written
    }

    #[test]
    fn mirrors_the_spelling_it_was_given() {
        assert_eq!(shape_of("00:00:01,000").separator, ',');
        assert_eq!(shape_of("00:00:01.000").separator, '.');
        assert_eq!(shape_of("00:00:01,5").fraction, 1);
        assert_eq!(shape_of("0:00:01.50").fraction, 2);
        assert_eq!(shape_of("0:00:01.50").hours, Some(1));
        assert_eq!(shape_of("00:01.000").hours, None);
    }

    #[test]
    fn writes_back_what_each_format_writes() {
        assert_eq!(
            round_trip(3_723_004, default_shape(SubtitleFormat::Srt)),
            "01:02:03,004"
        );
        assert_eq!(
            round_trip(3_723_004, default_shape(SubtitleFormat::Vtt)),
            "01:02:03.004"
        );
        assert_eq!(
            round_trip(3_723_000, default_shape(SubtitleFormat::Ass)),
            "1:02:03.00"
        );
        assert_eq!(
            round_trip(0, default_shape(SubtitleFormat::Srt)),
            "00:00:00,000"
        );
    }

    #[test]
    fn refuses_a_value_the_precision_cannot_hold_rather_than_rounding_it() {
        let centiseconds = shape_of("0:00:01.50");
        let error = render_timecode(1_234, centiseconds).expect_err("1234 ms needs 3 digits");
        assert_eq!(error.kind, crate::error::EditErrorKind::UnwritableTimecode);
        assert_eq!(round_trip(1_230, centiseconds), "0:00:01.23");
    }

    #[test]
    fn refuses_a_value_past_the_ceiling() {
        let error = render_timecode(MAX_TIMECODE_MS + 1, default_shape(SubtitleFormat::Srt))
            .expect_err("past the ceiling");
        assert_eq!(error.kind, crate::error::EditErrorKind::UnwritableTimecode);
        assert_eq!(
            round_trip(MAX_TIMECODE_MS, default_shape(SubtitleFormat::Srt)),
            "999:59:59,999"
        );
    }

    #[test]
    fn widens_only_where_widening_loses_nothing() {
        // Hours grow past the width the file used; the vtt short form promotes past its last hour.
        assert_eq!(
            round_trip(3_600_000, shape_of("00:00:01.000")),
            "01:00:00.000"
        );
        assert_eq!(round_trip(3_599_999, shape_of("00:01.000")), "59:59.999");
        assert_eq!(round_trip(3_600_000, shape_of("00:01.000")), "01:00:00.000");
    }

    #[test]
    fn a_blank_line_inside_cue_text_is_unwritable() {
        for text in ["one\n\ntwo", "one\n \ntwo", "\nleading", "trailing\n", "\r"] {
            assert!(
                validate_text(SubtitleFormat::Srt, text).is_err(),
                "{text:?} must be refused"
            );
        }
        assert!(validate_text(SubtitleFormat::Srt, "one\ntwo").is_ok());
        assert!(validate_text(SubtitleFormat::Vtt, "").is_ok());
    }

    #[test]
    fn an_ass_event_holds_no_line_break() {
        assert!(validate_text(SubtitleFormat::Ass, "one\ntwo").is_err());
        assert!(validate_text(SubtitleFormat::Ass, "one\rtwo").is_err());
        assert!(validate_text(SubtitleFormat::Ass, "one\\Ntwo").is_ok());
    }

    #[test]
    fn renders_text_with_the_terminator_the_block_uses() {
        assert_eq!(render_text("a\nb", "\r\n"), "a\r\nb");
        assert_eq!(render_text("a\nb", "\n"), "a\nb");
        assert!(is_blank_line(" \t\r"));
        assert!(!is_blank_line(" x"));
    }
}
