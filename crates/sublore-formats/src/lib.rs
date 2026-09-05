//! Licensed under the GNU GPL v3 or later, with the section 7 additional permission for modules
//! loaded through `sublore-module-api`. See LICENSE at the root of the repository.

//! Lossless subtitle parsing and serialization for SRT, WEBVTT and ASS/SSA.
//!
//! The crate holds no I/O: it takes bytes and returns bytes, so the rule that a user's file is
//! read-only stays enforceable in one place, in the app. Parsing keeps the original file whole in
//! one buffer and describes it with spans, so serializing an unedited document can only ever
//! reproduce the bytes it was given. See BACKLOG.md M1.

pub mod ass;
pub mod cue;
pub mod document;
pub mod error;
pub mod span;
pub mod srt;
pub mod text;
pub mod timecode;
pub mod vtt;

pub use cue::{AssEvent, AssEventKind, AssField, Cue, CueDetail, SrtCue, VttCue};
pub use document::{
    AssStyle, CoverageViolation, Segment, SegmentKind, SubtitleDocument, SubtitleFormat,
};
pub use error::{ParseError, ParseErrorKind};
pub use span::Span;
pub use text::{Newline, SourceText};
pub use timecode::{Timecode, MAX_TIMECODE_MS};

/// Parse `bytes` as `format`. The app's only entry point: it validates the encoding, dispatches to
/// the grammar, and proves the result can be written back byte for byte.
///
/// Parsing stops at the first thing it cannot represent, and reports where. Nothing is repaired
/// silently: a tolerated quirk is recorded and written back unchanged, and anything else is an
/// error the user gets to see.
pub fn parse(format: SubtitleFormat, bytes: &[u8]) -> Result<SubtitleDocument, ParseError> {
    let source = SourceText::from_bytes(bytes)?;
    let document = match format {
        SubtitleFormat::Srt => srt::parse(source)?,
        SubtitleFormat::Vtt => vtt::parse(source)?,
        SubtitleFormat::Ass => ass::parse(source)?,
    };
    ensure_tiled(document)
}

/// Segments that do not tile the body are a parser bug, never a bad file, and serializing them
/// would write bytes the file never had. A plain check, not a debug assertion: release builds are
/// the ones that save the user's work (CONTRIBUTING.md §3). See BACKLOG.md M1.1.
fn ensure_tiled(document: SubtitleDocument) -> Result<SubtitleDocument, ParseError> {
    let violation = match document.check_coverage() {
        Ok(()) => return Ok(document),
        Err(violation) => violation,
    };

    let detail = match document.segments().get(violation.segment) {
        Some(_) => format!(
            "segment {} spans {}..{}, expected it to start at {}",
            violation.segment, violation.found.start, violation.found.end, violation.expected_start
        ),
        // One past the last segment: the tail no segment ever claimed.
        None => format!(
            "bytes {}..{} belong to no segment",
            violation.found.start, violation.found.end
        ),
    };

    let source = document.source();
    Err(ParseError::new(
        source.line_of(violation.expected_start),
        source.column_of(violation.expected_start),
        ParseErrorKind::SegmentCoverage,
        detail,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_tiled, ParseErrorKind, Segment, SegmentKind, SourceText, Span, SubtitleDocument,
        SubtitleFormat,
    };

    /// A document tiled exactly as `spans` says, however wrong that is.
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

    #[test]
    fn a_document_that_tiles_its_body_passes_through() {
        let document = ensure_tiled(document("one\ntwo\n", &[(0, 4), (4, 8)]))
            .expect("tiled segments are the normal case");
        assert_eq!(document.to_bytes(), b"one\ntwo\n");
    }

    #[test]
    fn a_document_that_does_not_tile_its_body_is_refused_in_every_build() {
        // Not a debug assertion: an overlap must be refused in the release build that ships.
        let error = ensure_tiled(document("one\ntwo\n", &[(0, 4), (3, 8)]))
            .expect_err("an overlap must never reach the save path");
        assert_eq!(error.kind, ParseErrorKind::SegmentCoverage);
        assert_eq!(error.line, 2);
        assert_eq!(error.column, 1);
        assert!(
            error.snippet.contains("segment 1"),
            "the detail names the segment: {}",
            error.snippet
        );
    }

    #[test]
    fn an_uncovered_tail_is_refused_too() {
        let error = ensure_tiled(document("one\ntwo\n", &[(0, 4)]))
            .expect_err("a dropped tail must never reach the save path");
        assert_eq!(error.kind, ParseErrorKind::SegmentCoverage);
        assert_eq!(
            error.snippet, "bytes 4..8 belong to no segment",
            "the detail names the tail rather than a segment that does not exist"
        );
    }
}
