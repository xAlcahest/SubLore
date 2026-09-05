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
    /// Where each [`AssField`] sits in `fields`, in that enum's own order, or `None` where the
    /// section's `Format:` line did not declare it before the text. Read through
    /// [`AssEvent::field_index`], never by position.
    ///
    /// One narrow array rather than seven `Option<usize>` fields: a `Cue` rides inside every cue
    /// segment of the file, and seven of those would add eighty bytes to each one. `u32` holds any
    /// index a 16 MB file can spell, and one that does not fit names nothing, which is the same
    /// answer as a field the `Format:` line never declared. See ass-field-write-tasks.md W1.
    pub(crate) named: [Option<u32>; AssField::COUNT],
}

/// A field of an ASS event that something outside the parser may name.
///
/// Closed on purpose, and there is deliberately no variant for the text: the text field takes the
/// rest of the line, so a caller able to name it could hand a field write the user's own writing.
/// `Actor` is the `Name` field under the word the column uses. See ass-field-write-tasks.md W2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssField {
    Style,
    Actor,
    Effect,
    Layer,
    MarginL,
    MarginR,
    MarginV,
}

impl AssField {
    /// Every field a write can name, in the order [`AssEvent::named`] stores them.
    pub const ALL: [AssField; AssField::COUNT] = [
        AssField::Style,
        AssField::Actor,
        AssField::Effect,
        AssField::Layer,
        AssField::MarginL,
        AssField::MarginR,
        AssField::MarginV,
    ];

    /// How many there are. The array length and the enum cannot drift apart.
    pub const COUNT: usize = 7;

    /// Its slot in [`AssEvent::named`]. Private: a caller holding a slot is a caller holding an
    /// index, which is what [`AssField`] exists to stop.
    fn slot(self) -> usize {
        match self {
            AssField::Style => 0,
            AssField::Actor => 1,
            AssField::Effect => 2,
            AssField::Layer => 3,
            AssField::MarginL => 4,
            AssField::MarginR => 5,
            AssField::MarginV => 6,
        }
    }

    /// The name the `Format:` line spells it with, for error detail. Never user-facing copy.
    pub fn as_str(self) -> &'static str {
        match self {
            AssField::Style => "Style",
            AssField::Actor => "Name",
            AssField::Effect => "Effect",
            AssField::Layer => "Layer",
            AssField::MarginL => "MarginL",
            AssField::MarginR => "MarginR",
            AssField::MarginV => "MarginV",
        }
    }
}

impl AssEvent {
    /// Where `field` sits in [`Self::fields`], or `None` when this section's `Format:` line did not
    /// declare it before the text. The one place a name becomes an index.
    pub fn field_index(&self, field: AssField) -> Option<usize> {
        self.named
            .get(field.slot())
            .copied()
            .flatten()
            .and_then(|at| usize::try_from(at).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::{AssEvent, AssEventKind, AssField, Span};

    /// `event_cue` fills `named` by walking `ALL`, and `field_index` reads it by `slot`. The two
    /// orders agreeing is what makes a style read a style. See ass-field-write-tasks.md W2.
    #[test]
    fn every_field_stores_and_reads_at_the_same_slot() {
        for (slot, field) in AssField::ALL.into_iter().enumerate() {
            assert_eq!(field.slot(), slot, "{field:?} is stored at {slot}");
        }
        assert_eq!(AssField::ALL.len(), AssField::COUNT);
    }

    #[test]
    fn a_field_the_format_line_did_not_declare_resolves_to_nothing() {
        let mut named = [None; AssField::COUNT];
        named[AssField::Effect.slot()] = Some(8);
        let event = AssEvent {
            kind: AssEventKind::Dialogue,
            descriptor: Span::new(0, 0),
            fields: Vec::new(),
            text_field: 9,
            named,
        };
        assert_eq!(event.field_index(AssField::Effect), Some(8));
        for field in AssField::ALL {
            if field != AssField::Effect {
                assert_eq!(event.field_index(field), None, "{field:?}");
            }
        }
    }
}
