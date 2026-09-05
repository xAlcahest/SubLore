//! ASS/SSA. A line-oriented format: every line of the file becomes exactly one segment, except
//! runs of blank lines, which share one.
//!
//! Sections are opened by a `[Name]` line and each keeps its own `Format:` field list. Only
//! `Dialogue:` and `Comment:` inside `[Events]` become cues; everything else travels through as
//! metadata, unread and unchanged, which is what lets unknown sections and `[Fonts]` blobs survive
//! a save. See BACKLOG.md M1.3.

use crate::cue::{AssEvent, AssEventKind, Cue, CueDetail};
use crate::document::{Segment, SegmentKind, SubtitleDocument, SubtitleFormat};
use crate::error::{ParseError, ParseErrorKind};
use crate::span::Span;
use crate::text::SourceText;
use crate::timecode::{parse_timecode, Timecode};

/// The section the scanner is inside.
struct Section {
    is_events: bool,
    format: Option<FieldFormat>,
}

/// What a section's `Format:` line declared, plus where it sits: a Format line without timing
/// fields is reported at itself, not at the event that tripped over it.
struct FieldFormat {
    count: usize,
    start_index: Option<usize>,
    end_index: Option<usize>,
    style_index: Option<usize>,
    name_index: Option<usize>,
    offset: usize,
}

/// Parse a decoded ASS/SSA body into a tiling document. Frozen seam: [`crate::parse`] dispatches
/// here.
pub(crate) fn parse(source: SourceText) -> Result<SubtitleDocument, ParseError> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut section = Section {
        is_events: false,
        format: None,
    };
    let mut blank: Option<Span> = None;

    for line in 1..=source.line_count() {
        let span = source.line_span(line);
        // Only an empty body reaches this, and an empty segment would break the tiling invariant.
        if span.is_empty() {
            continue;
        }
        let text = source.line_text(line);
        if is_blank(text) {
            blank = Some(match blank {
                Some(run) => Span::new(run.start, span.end),
                None => span,
            });
            continue;
        }
        if let Some(run) = blank.take() {
            segments.push(Segment {
                span: run,
                kind: SegmentKind::Blank,
            });
        }

        let indent = text.len() - text.trim_start_matches([' ', '\t']).len();
        let head = text.get(indent..).unwrap_or("");
        let at = span.start + indent;

        if let Some(rest) = head.strip_prefix('[') {
            let Some(close) = rest.find(']') else {
                return Err(ParseError::at(
                    &source,
                    at,
                    ParseErrorKind::BadSectionHeader,
                ));
            };
            let name = rest.get(..close).unwrap_or("").trim_matches([' ', '\t']);
            section = Section {
                is_events: name.eq_ignore_ascii_case("events"),
                format: None,
            };
            segments.push(Segment {
                span,
                kind: SegmentKind::Meta,
            });
            continue;
        }

        // A line with no descriptor cannot be an event; it belongs to whatever section holds it.
        let Some(colon) = head.find(':') else {
            segments.push(Segment {
                span,
                kind: SegmentKind::Meta,
            });
            continue;
        };
        let descriptor = head
            .get(..colon)
            .unwrap_or("")
            .trim_end_matches([' ', '\t']);
        let descriptor_span = Span::new(at, at + descriptor.len());
        let remainder = Span::new(at + colon + 1, span.start + text.len());

        let event_kind = if descriptor.eq_ignore_ascii_case("dialogue") {
            Some(AssEventKind::Dialogue)
        } else if descriptor.eq_ignore_ascii_case("comment") {
            Some(AssEventKind::Comment)
        } else {
            None
        };

        match event_kind {
            // Events outside their section are ordinary lines: only `[Events]` holds cues.
            Some(kind) if section.is_events => {
                let Some(format) = section.format.as_ref() else {
                    return Err(ParseError::at(
                        &source,
                        at,
                        ParseErrorKind::MissingFormatLine,
                    ));
                };
                let cue = event_cue(&source, format, kind, descriptor_span, remainder)?;
                segments.push(Segment {
                    span,
                    kind: SegmentKind::Cue(cue),
                });
            }
            _ => {
                if descriptor.eq_ignore_ascii_case("format") {
                    let names = source.body().get(remainder.range()).unwrap_or("");
                    section.format = Some(field_format(names, at));
                }
                segments.push(Segment {
                    span,
                    kind: SegmentKind::Meta,
                });
            }
        }
    }

    if let Some(run) = blank {
        segments.push(Segment {
            span: run,
            kind: SegmentKind::Blank,
        });
    }
    Ok(SubtitleDocument::new(SubtitleFormat::Ass, source, segments))
}

/// A line that holds nothing a reader would see. A lone `\r` is content elsewhere, but a line made
/// only of one is blank.
fn is_blank(text: &str) -> bool {
    text.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

/// Read a `Format:` field list: how many fields events in this section carry, which two hold the
/// timing, and which two the grid names a line by. The first match of each name wins.
fn field_format(names: &str, offset: usize) -> FieldFormat {
    let mut count = 0;
    let mut start_index = None;
    let mut end_index = None;
    let mut style_index = None;
    let mut name_index = None;
    for (index, name) in names.split(',').enumerate() {
        count = index + 1;
        let name = name.trim_matches([' ', '\t', '\r']);
        if start_index.is_none() && name.eq_ignore_ascii_case("start") {
            start_index = Some(index);
        } else if end_index.is_none() && name.eq_ignore_ascii_case("end") {
            end_index = Some(index);
        } else if style_index.is_none() && name.eq_ignore_ascii_case("style") {
            style_index = Some(index);
        // The specification calls the speaker `Name`; tools that follow the column label write
        // `Actor`. One condition, so the first field named either wins. See grid-columns-tasks G1.
        } else if name_index.is_none()
            && (name.eq_ignore_ascii_case("name") || name.eq_ignore_ascii_case("actor"))
        {
            name_index = Some(index);
        }
    }
    FieldFormat {
        count,
        start_index,
        end_index,
        style_index,
        name_index,
        offset,
    }
}

/// Split one event line into the fields its section declared. The last field takes the rest of the
/// line, commas and all, which is the whole reason dialogue text may hold commas.
fn event_cue(
    source: &SourceText,
    format: &FieldFormat,
    kind: AssEventKind,
    descriptor: Span,
    remainder: Span,
) -> Result<Cue, ParseError> {
    let (Some(start_index), Some(end_index)) = (format.start_index, format.end_index) else {
        return Err(ParseError::at(
            source,
            format.offset,
            ParseErrorKind::MissingTimingFields,
        ));
    };

    let mut fields = Vec::with_capacity(format.count);
    let mut field_start = remainder.start;
    for _ in 0..format.count.saturating_sub(1) {
        let Some(comma) = find_comma(source.body(), field_start, remainder.end) else {
            return Err(ParseError::at(
                source,
                descriptor.start,
                ParseErrorKind::FieldCountMismatch,
            ));
        };
        fields.push(Span::new(field_start, comma));
        field_start = comma + 1;
    }
    fields.push(Span::new(field_start, remainder.end));

    // Both indices come from the Format line that fixed `count`, so they are in range; reading
    // through `get` keeps a parser bug an error rather than an index panic.
    let empty = Span::new(remainder.end, remainder.end);
    let start = timecode_of(source, fields.get(start_index).copied().unwrap_or(empty))?;
    let end = timecode_of(source, fields.get(end_index).copied().unwrap_or(empty))?;

    let text_field = fields.len().saturating_sub(1);
    let text = fields.get(text_field).copied().unwrap_or(empty);
    Ok(Cue {
        start,
        end,
        text,
        detail: CueDetail::Ass(AssEvent {
            kind,
            descriptor,
            fields,
            text_field,
            style_field: before_text(format.style_index, text_field),
            name_field: before_text(format.name_index, text_field),
        }),
    })
}

/// A declared field only names something readable while it sits before the text, which takes the
/// rest of the line: anything at or past it is inside the dialogue. See grid-columns-tasks.md G1.
fn before_text(index: Option<usize>, text_field: usize) -> Option<usize> {
    index.filter(|at| *at < text_field)
}

/// The first comma in `body[from..to]`, as an absolute offset. Commas are ASCII, so the offset is
/// always a char boundary.
fn find_comma(body: &str, from: usize, to: usize) -> Option<usize> {
    let slice = body.as_bytes().get(from..to)?;
    slice
        .iter()
        .position(|&byte| byte == b',')
        .map(|at| from + at)
}

/// Read a timing field, spaces trimmed. Anything left over after the timestamp is a broken file,
/// not a quirk we could write back faithfully.
fn timecode_of(source: &SourceText, span: Span) -> Result<Timecode, ParseError> {
    let body = source.body();
    let raw = body.get(span.range()).unwrap_or("");
    let lead = raw.len() - raw.trim_start_matches([' ', '\t']).len();
    let trimmed = raw
        .get(lead..)
        .unwrap_or("")
        .trim_end_matches([' ', '\t', '\r']);
    let at = span.start + lead;

    let (timecode, end) =
        parse_timecode(body, at).map_err(|kind| ParseError::at(source, at, kind))?;
    if end != at + trimmed.len() {
        return Err(ParseError::at(source, end, ParseErrorKind::BadTimecode));
    }
    Ok(timecode)
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::cue::CueDetail;
    use crate::document::SegmentKind;
    use crate::error::ParseErrorKind;
    use crate::text::SourceText;

    fn document(text: &str) -> crate::document::SubtitleDocument {
        let source = SourceText::from_bytes(text.as_bytes()).expect("valid utf-8 fixture");
        let document = parse(source).expect("the fixture parses");
        assert_eq!(document.check_coverage(), Ok(()));
        assert_eq!(document.to_bytes(), text.as_bytes());
        document
    }

    fn kind(text: &str) -> ParseErrorKind {
        let source = SourceText::from_bytes(text.as_bytes()).expect("valid utf-8 fixture");
        parse(source).expect_err("the fixture must be refused").kind
    }

    /// The style and the name the first event declares, as the parser hands them: the file's own
    /// bytes, so a field written first on the line still carries the space after the descriptor.
    /// `None` where the section's `Format:` line names no such field before the text.
    fn named_fields(text: &str) -> (Option<String>, Option<String>) {
        let document = document(text);
        let cue = document.cues().next().expect("the fixture holds one event");
        let CueDetail::Ass(event) = &cue.detail else {
            panic!("an ASS cue carries an ASS event");
        };
        let read = |at: Option<usize>| {
            at.map(|at| {
                let span = *event.fields.get(at).expect("a declared field is in range");
                document.slice(span).to_owned()
            })
        };
        (read(event.style_field), read(event.name_field))
    }

    fn one_event(format_line: &str, event_line: &str) -> String {
        format!("[Events]\nFormat: {format_line}\nDialogue: {event_line}\n")
    }

    #[test]
    fn an_empty_body_holds_no_segments() {
        let document = document("");
        assert!(document.segments().is_empty());
        assert_eq!(document.cues().count(), 0);
    }

    #[test]
    fn a_file_without_events_opens_with_no_cues() {
        let document = document("[Script Info]\nTitle: Nothing timed yet\n");
        assert_eq!(document.segments().len(), 2);
        assert_eq!(document.cues().count(), 0);
    }

    #[test]
    fn an_event_line_outside_its_section_is_metadata() {
        let document = document(
            "[Script Info]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Not an event here\n",
        );
        assert_eq!(document.cues().count(), 0);
        assert!(document
            .segments()
            .iter()
            .all(|segment| matches!(segment.kind, SegmentKind::Meta)));
    }

    #[test]
    fn a_format_line_belongs_to_the_section_that_declared_it() {
        // The styles Format line must not arm the events section. See BACKLOG.md M1.3.
        assert_eq!(
            kind(concat!(
                "[V4+ Styles]\n",
                "Format: Name, Start, End, Text\n",
                "[Events]\n",
                "Dialogue: 0,0:00:01.00,0:00:02.00,Hello\n",
            )),
            ParseErrorKind::MissingFormatLine
        );
    }

    #[test]
    fn a_timing_field_with_trailing_junk_is_refused() {
        assert_eq!(
            kind(concat!(
                "[Events]\n",
                "Format: Layer, Start, End, Text\n",
                "Dialogue: 0,0:00:01.00abc,0:00:02.00,Hello\n",
            )),
            ParseErrorKind::BadTimecode
        );
    }

    #[test]
    fn the_specifications_own_field_order_names_the_style_and_the_speaker() {
        assert_eq!(
            named_fields(&one_event(
                "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text",
                "0,0:00:01.00,0:00:02.00,Sign,Ingrid,0,0,0,,Hello",
            )),
            (Some("Sign".to_owned()), Some("Ingrid".to_owned()))
        );
    }

    #[test]
    fn a_shuffled_field_order_names_them_where_it_put_them() {
        // The check that fails the moment either index is read as a position (G1). The speaker is
        // the first field on the line, so its raw span still holds the descriptor's own space.
        assert_eq!(
            named_fields(&one_event(
                "Name, Layer, Start, End, MarginL, MarginR, MarginV, Effect, Style, Text",
                "Ingrid,0,0:00:01.00,0:00:02.00,0,0,0,,Sign,Hello",
            )),
            (Some("Sign".to_owned()), Some(" Ingrid".to_owned()))
        );
    }

    #[test]
    fn the_speaker_field_spelled_actor_is_read_too() {
        assert_eq!(
            named_fields(&one_event(
                "Layer, Start, End, Style, Actor, MarginL, MarginR, MarginV, Effect, Text",
                "0,0:00:01.00,0:00:02.00,Sign,Ingrid,0,0,0,,Hello",
            )),
            (Some("Sign".to_owned()), Some("Ingrid".to_owned()))
        );
    }

    #[test]
    fn a_format_line_declaring_both_spellings_takes_whichever_comes_first() {
        // One condition covers the two spellings, so order on the line decides and nothing else
        // does. Written down because a split into two branches would change it silently (G1).
        assert_eq!(
            named_fields(&one_event(
                "Layer, Start, End, Style, Name, Actor, Text",
                "0,0:00:01.00,0:00:02.00,Sign,Ingrid,Marek,Hello",
            )),
            (Some("Sign".to_owned()), Some("Ingrid".to_owned()))
        );
        assert_eq!(
            named_fields(&one_event(
                "Layer, Start, End, Style, Actor, Name, Text",
                "0,0:00:01.00,0:00:02.00,Sign,Ingrid,Marek,Hello",
            )),
            (Some("Sign".to_owned()), Some("Ingrid".to_owned()))
        );
    }

    #[test]
    fn a_format_line_declaring_neither_names_neither() {
        assert_eq!(
            named_fields(&one_event(
                "Layer, Start, End, Text",
                "0,0:00:01.00,0:00:02.00,Hello",
            )),
            (None, None)
        );
    }

    #[test]
    fn a_field_declared_after_the_text_names_nothing() {
        // The last declared field takes the rest of the line, so an index that reaches it is
        // pointing into the dialogue and must not be handed to a column.
        assert_eq!(
            named_fields(&one_event(
                "Layer, Start, End, Text, Style",
                "0,0:00:01.00,0:00:02.00,Hello and, in passing,Default",
            )),
            (None, None)
        );
        assert_eq!(
            named_fields(&one_event(
                "Layer, Start, End, Text, Name",
                "0,0:00:01.00,0:00:02.00,Hello and, in passing,Ingrid",
            )),
            (None, None)
        );
    }
}
