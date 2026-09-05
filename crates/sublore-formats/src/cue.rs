//! What a cue is in each format. Every field is a span into the source, so nothing here can hold a
//! rewritten version of the user's text. See BACKLOG.md M1.1.

use crate::span::Span;
use crate::timecode::Timecode;

/// One timed line of a subtitle file, plus whatever its format wrote around it.
#[derive(Clone, Debug)]
pub struct Cue {
    pub start: Timecode,
    pub end: Timecode,
    /// The payload as written: inline tags, entities, `\N`, and its internal line breaks. An empty
    /// span for a cue with no text.
    pub text: Span,
    pub detail: CueDetail,
}

#[derive(Clone, Debug)]
pub enum CueDetail {
    Srt(SrtCue),
    Vtt(VttCue),
    Ass(AssEvent),
}

#[derive(Clone, Debug)]
pub struct SrtCue {
    /// The number as written, when the block had an index line. Leading zeros live in `number_span`.
    pub number: Option<u32>,
    /// The index line without its terminator.
    pub number_span: Option<Span>,
    /// Whatever followed the end timestamp on the timing line (`X1:040 X2:600 ...`), when present.
    pub timing_trailer: Option<Span>,
}

#[derive(Clone, Debug)]
pub struct VttCue {
    /// The cue identifier line without its terminator, when present.
    pub id: Option<Span>,
    /// Cue settings after the end timestamp (`align:start line:90%`), when present.
    pub settings: Option<Span>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssEventKind {
    Dialogue,
    Comment,
}

#[derive(Clone, Debug)]
pub struct AssEvent {
    pub kind: AssEventKind,
    /// The descriptor before the colon: `Dialogue` or `Comment`, as written.
    pub descriptor: Span,
    /// Every field in the order the section's `Format:` line declares. The last field is the text
    /// and keeps every comma inside it.
    pub fields: Vec<Span>,
    /// Index into `fields` of the text field.
    pub text_field: usize,
    /// Index into `fields` of the declared `Style`, when the section declared one before the text.
    pub style_field: Option<usize>,
    /// Index into `fields` of the declared `Name` (or `Actor`), under the same rule.
    pub name_field: Option<usize>,
}
