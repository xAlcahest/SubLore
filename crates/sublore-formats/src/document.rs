//! The document model: one immutable body, plus segments that tile it exactly.
//!
//! Every byte of the file belongs to exactly one segment, in order, with no gaps and no overlaps.
//! That makes byte-identical serialization a structural property instead of a hope:
//! [`SubtitleDocument::to_bytes`] can only re-emit slices of the original, and
//! [`SubtitleDocument::check_coverage`] proves the slices add up to the whole file.
//! See BACKLOG.md M1.1.

use crate::cue::{AssEventKind, Cue, CueDetail};
use crate::span::Span;
use crate::text::{SourceText, UTF8_BOM};

/// Section names that identify an ASS/SSA file by content alone.
const ASS_SECTIONS: [&str; 6] = [
    "script info",
    "v4 styles",
    "v4+ styles",
    "events",
    "fonts",
    "graphics",
];

/// How far into a file [`SubtitleFormat::detect`] looks for an ASS section header.
const DETECT_LINE_BUDGET: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleFormat {
    Srt,
    Vtt,
    Ass,
}

impl SubtitleFormat {
    /// Content decides, extension breaks ties. `None` means "not one of ours".
    pub fn detect(extension: Option<&str>, body: &str) -> Option<Self> {
        // A caller that has not decoded through SourceText still has its BOM in hand.
        let body = body.strip_prefix('\u{feff}').unwrap_or(body);
        if is_vtt_header(body) {
            return Some(Self::Vtt);
        }

        for line in body
            .split('\n')
            .filter(|line| !line.trim().is_empty())
            .take(DETECT_LINE_BUDGET)
        {
            let Some(name) = section_name(line.trim()) else {
                continue;
            };
            if ASS_SECTIONS.contains(&name.trim().to_ascii_lowercase().as_str()) {
                return Some(Self::Ass);
            }
        }

        match extension.map(str::to_ascii_lowercase).as_deref() {
            Some("srt") => Some(Self::Srt),
            Some("vtt") => Some(Self::Vtt),
            Some("ass" | "ssa") => Some(Self::Ass),
            _ => None,
        }
    }

    /// Stable: it is the IPC wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Ass => "ass",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Segment {
    /// The bytes this segment owns, including its line terminator(s).
    pub span: Span,
    pub kind: SegmentKind,
}

#[derive(Clone, Debug)]
pub enum SegmentKind {
    /// Format header the file must start with (the VTT `WEBVTT` block; in ASS everything before the
    /// first event line, section by section). SRT never produces one.
    Header,
    /// A run of blank lines between blocks.
    Blank,
    /// Metadata kept verbatim: VTT `NOTE`/`STYLE`/`REGION`, ASS section headers, `Format:`,
    /// `Style:`, `;` comments, key/value lines, and any line inside a section we do not interpret.
    Meta,
    /// A cue or an ASS event.
    Cue(Cue),
}

/// A parsed file: the bytes it came from, and the ordered segments that tile them.
#[derive(Clone, Debug)]
pub struct SubtitleDocument {
    format: SubtitleFormat,
    source: SourceText,
    segments: Vec<Segment>,
}

impl SubtitleDocument {
    /// Built by the format parsers. Coverage is checked by [`crate::parse`], not here, so a parser
    /// can assemble its segments in whatever order its grammar needs.
    pub fn new(format: SubtitleFormat, source: SourceText, segments: Vec<Segment>) -> Self {
        Self {
            format,
            source,
            segments,
        }
    }

    pub fn format(&self) -> SubtitleFormat {
        self.format
    }

    pub fn source(&self) -> &SourceText {
        &self.source
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Every cue in file order, ASS `Comment:` events included.
    pub fn cues(&self) -> impl Iterator<Item = &Cue> + '_ {
        self.segments
            .iter()
            .filter_map(|segment| match &segment.kind {
                SegmentKind::Cue(cue) => Some(cue),
                SegmentKind::Header | SegmentKind::Blank | SegmentKind::Meta => None,
            })
    }

    /// Every cue a player would draw: ASS `Comment:` events excluded. This is the number the UI
    /// shows.
    pub fn displayed_cue_count(&self) -> usize {
        self.cues()
            .filter(|cue| {
                !matches!(&cue.detail, CueDetail::Ass(event) if event.kind == AssEventKind::Comment)
            })
            .count()
    }

    /// The text a span spells. Spans must come from this document; a foreign span yields "".
    pub fn slice(&self, span: Span) -> &str {
        let text = self.source.body().get(span.range());
        debug_assert!(text.is_some(), "{span:?} does not belong to this document");
        text.unwrap_or("")
    }

    /// Rebuild the file from the segments. Byte-identical to the input for an unedited document.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.source.byte_len());
        if self.source.has_bom() {
            bytes.extend_from_slice(&UTF8_BOM);
        }
        for segment in &self.segments {
            bytes.extend_from_slice(self.slice(segment.span).as_bytes());
        }
        bytes
    }

    /// Segments are non-empty, ascending, contiguous, cut on character boundaries, and cover the
    /// whole body. The first violation wins; an uncovered tail is reported as a violation one past
    /// the last segment.
    ///
    /// The boundary rule is part of the guarantee: a segment cut inside a multi-byte character
    /// cannot be sliced, and [`Self::to_bytes`] would drop it silently. See CONTRIBUTING.md §3.
    pub fn check_coverage(&self) -> Result<(), CoverageViolation> {
        let body = self.source.body();
        let total = body.len();
        let mut expected = 0usize;
        for (index, segment) in self.segments.iter().enumerate() {
            if segment.span.start != expected
                || segment.span.is_empty()
                || segment.span.end > total
                || !body.is_char_boundary(segment.span.start)
            {
                return Err(CoverageViolation {
                    segment: index,
                    expected_start: expected,
                    found: segment.span,
                });
            }
            expected = segment.span.end;
        }
        if expected != total {
            return Err(CoverageViolation {
                segment: self.segments.len(),
                expected_start: expected,
                found: Span::new(expected, total),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverageViolation {
    pub segment: usize,
    pub expected_start: usize,
    pub found: Span,
}

fn is_vtt_header(body: &str) -> bool {
    match body.strip_prefix("WEBVTT") {
        Some(rest) => rest.is_empty() || rest.starts_with([' ', '\t', '\n', '\r']),
        None => false,
    }
}

/// The name inside a `[Section]` header, or `None` for any other line.
fn section_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('[')?;
    let end = rest.find(']')?;
    rest.get(..end)
}

#[cfg(test)]
mod tests {
    use super::{Segment, SegmentKind, SubtitleDocument, SubtitleFormat};
    use crate::cue::{AssEvent, AssEventKind, Cue, CueDetail, SrtCue};
    use crate::span::Span;
    use crate::text::SourceText;
    use crate::timecode::Timecode;

    fn document(text: &str, spans: &[(usize, usize)]) -> SubtitleDocument {
        let source = SourceText::from_bytes(text.as_bytes()).expect("valid utf-8 fixture");
        let segments = spans
            .iter()
            .map(|&(start, end)| Segment {
                span: Span::new(start, end),
                kind: SegmentKind::Meta,
            })
            .collect();
        SubtitleDocument::new(SubtitleFormat::Srt, source, segments)
    }

    fn srt_cue(start: usize, end: usize) -> Cue {
        Cue {
            start: Timecode::new(0, Span::new(start, start)),
            end: Timecode::new(1_000, Span::new(end, end)),
            text: Span::new(start, end),
            detail: CueDetail::Srt(SrtCue {
                number: None,
                number_span: None,
                timing_trailer: None,
            }),
        }
    }

    fn ass_cue(kind: AssEventKind) -> Cue {
        Cue {
            start: Timecode::new(0, Span::new(0, 0)),
            end: Timecode::new(1_000, Span::new(0, 0)),
            text: Span::new(0, 0),
            detail: CueDetail::Ass(AssEvent {
                kind,
                descriptor: Span::new(0, 0),
                fields: Vec::new(),
                text_field: 0,
                style_field: None,
                name_field: None,
            }),
        }
    }

    #[test]
    fn rebuilds_the_file_from_its_segments() {
        let document = document("one\ntwo\n", &[(0, 4), (4, 8)]);
        assert_eq!(document.to_bytes(), b"one\ntwo\n");
        assert!(document.check_coverage().is_ok());
    }

    #[test]
    fn rebuilds_a_bom_prefixed_file() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"one\n");
        let source = SourceText::from_bytes(&bytes).expect("valid utf-8");
        let segments = vec![Segment {
            span: Span::new(0, 4),
            kind: SegmentKind::Meta,
        }];
        let document = SubtitleDocument::new(SubtitleFormat::Srt, source, segments);
        assert_eq!(document.to_bytes(), bytes);
    }

    #[test]
    fn a_messy_file_tiled_line_by_line_comes_back_byte_for_byte() {
        // The invariant every parser inherits: spans that tile the body reproduce the file exactly.
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            "1\r\n00:00:01,000 --> 00:00:02,000\r\n\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\r\n\r\n2\n00:00:03,000 --> 00:00:04,000\nbare\rreturn\ttab \nno final newline"
                .as_bytes(),
        );
        let source = SourceText::from_bytes(&bytes).expect("valid utf-8");

        let segments = (1..=source.line_count())
            .map(|line| Segment {
                span: source.line_span(line),
                kind: SegmentKind::Meta,
            })
            .collect();
        let document = SubtitleDocument::new(SubtitleFormat::Srt, source, segments);

        assert_eq!(document.check_coverage(), Ok(()));
        assert_eq!(document.to_bytes(), bytes);
        assert_eq!(document.segments().len(), 8);
    }

    #[test]
    fn an_empty_body_is_covered_by_no_segments() {
        let document = document("", &[]);
        assert!(document.check_coverage().is_ok());
        assert!(document.to_bytes().is_empty());
    }

    #[test]
    fn a_gap_is_a_coverage_violation() {
        let document = document("one\ntwo\n", &[(0, 4), (5, 8)]);
        let violation = document
            .check_coverage()
            .expect_err("the gap must be caught");
        assert_eq!(violation.segment, 1);
        assert_eq!(violation.expected_start, 4);
    }

    #[test]
    fn an_overlap_is_a_coverage_violation() {
        let document = document("one\ntwo\n", &[(0, 4), (3, 8)]);
        let violation = document
            .check_coverage()
            .expect_err("the overlap must be caught");
        assert_eq!(violation.segment, 1);
    }

    #[test]
    fn a_segment_cut_inside_a_character_is_a_coverage_violation() {
        // "é" is two bytes: tiling by bytes alone would slice to nothing and lose the character.
        let document = document("é\n", &[(0, 1), (1, 3)]);
        let violation = document
            .check_coverage()
            .expect_err("the split character must be caught");
        assert_eq!(violation.segment, 1);
        assert_eq!(violation.expected_start, 1);
    }

    #[test]
    fn an_empty_segment_is_a_coverage_violation() {
        let document = document("one\ntwo\n", &[(0, 4), (4, 4), (4, 8)]);
        let violation = document
            .check_coverage()
            .expect_err("the empty segment must be caught");
        assert_eq!(violation.segment, 1);
    }

    #[test]
    fn an_uncovered_tail_is_a_coverage_violation() {
        let document = document("one\ntwo\n", &[(0, 4)]);
        let violation = document
            .check_coverage()
            .expect_err("the tail must be caught");
        assert_eq!(violation.segment, 1);
        assert_eq!(violation.expected_start, 4);
        assert_eq!(violation.found.range(), 4..8);
    }

    #[test]
    fn counts_cues_and_leaves_ass_comments_out_of_the_displayed_count() {
        let source = SourceText::from_bytes(b"ignored").expect("valid utf-8");
        let segments = vec![
            Segment {
                span: Span::new(0, 3),
                kind: SegmentKind::Cue(ass_cue(AssEventKind::Dialogue)),
            },
            Segment {
                span: Span::new(3, 5),
                kind: SegmentKind::Cue(ass_cue(AssEventKind::Comment)),
            },
            Segment {
                span: Span::new(5, 7),
                kind: SegmentKind::Blank,
            },
        ];
        let document = SubtitleDocument::new(SubtitleFormat::Ass, source, segments);
        assert_eq!(document.cues().count(), 2);
        assert_eq!(document.displayed_cue_count(), 1);
    }

    #[test]
    fn every_srt_cue_counts_as_displayed() {
        let source = SourceText::from_bytes(b"one\ntwo\n").expect("valid utf-8");
        let segments = vec![
            Segment {
                span: Span::new(0, 4),
                kind: SegmentKind::Cue(srt_cue(0, 3)),
            },
            Segment {
                span: Span::new(4, 8),
                kind: SegmentKind::Cue(srt_cue(4, 7)),
            },
        ];
        let document = SubtitleDocument::new(SubtitleFormat::Srt, source, segments);
        assert_eq!(document.displayed_cue_count(), 2);
        assert_eq!(document.slice(Span::new(4, 7)), "two");
    }

    #[test]
    fn detects_vtt_from_its_header_whatever_the_extension_says() {
        assert_eq!(
            SubtitleFormat::detect(Some("srt"), "WEBVTT\n\n"),
            Some(SubtitleFormat::Vtt)
        );
        assert_eq!(
            SubtitleFormat::detect(None, "WEBVTT - Episode 1\r\n"),
            Some(SubtitleFormat::Vtt)
        );
        assert_eq!(
            SubtitleFormat::detect(Some("srt"), "\u{feff}WEBVTT\n"),
            Some(SubtitleFormat::Vtt)
        );
        assert_eq!(
            SubtitleFormat::detect(Some("srt"), "WEBVTTX\n"),
            Some(SubtitleFormat::Srt)
        );
    }

    #[test]
    fn detects_ass_from_a_section_header() {
        assert_eq!(
            SubtitleFormat::detect(Some("txt"), "\n; a comment\n[Script Info]\nTitle: x\n"),
            Some(SubtitleFormat::Ass)
        );
        assert_eq!(
            SubtitleFormat::detect(None, "[V4+ Styles]\n"),
            Some(SubtitleFormat::Ass)
        );
        assert_eq!(
            SubtitleFormat::detect(None, "[Whatever]\n"),
            None,
            "an unknown section is not enough to claim the file"
        );
    }

    #[test]
    fn falls_back_to_the_extension_then_gives_up() {
        assert_eq!(
            SubtitleFormat::detect(Some("SRT"), "1\n00:00:01,000 --> 00:00:02,000\nhi\n"),
            Some(SubtitleFormat::Srt)
        );
        assert_eq!(
            SubtitleFormat::detect(Some("ssa"), "Dialogue: 0,0:00:01.00\n"),
            Some(SubtitleFormat::Ass)
        );
        assert_eq!(SubtitleFormat::detect(Some("mkv"), "binary junk"), None);
        assert_eq!(SubtitleFormat::detect(None, ""), None);
    }

    #[test]
    fn wire_values_are_stable() {
        assert_eq!(SubtitleFormat::Srt.as_str(), "srt");
        assert_eq!(SubtitleFormat::Vtt.as_str(), "vtt");
        assert_eq!(SubtitleFormat::Ass.as_str(), "ass");
    }
}
