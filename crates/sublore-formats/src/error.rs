//! Parse failures: a position, a stable kind, and a technical snippet. The kind is the entire
//! vocabulary this crate speaks; every word the user reads is picked by the UI from the kind.
//! See BACKLOG.md M1.1.

use crate::text::SourceText;

/// Longest snippet kept for logs. Truncation lands on a char boundary, never mid-character.
const SNIPPET_MAX_BYTES: usize = 120;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line. The BOM belongs to line 1.
    pub line: u32,
    /// 1-based byte column inside that line.
    pub column: u32,
    pub kind: ParseErrorKind,
    /// The offending line, terminator stripped, truncated to 120 bytes on a char boundary.
    /// For logs and the technical detail field. Never rendered as UI copy.
    pub snippet: String,
}

impl ParseError {
    pub fn new(line: u32, column: u32, kind: ParseErrorKind, snippet: String) -> Self {
        Self {
            line,
            column,
            kind,
            snippet,
        }
    }

    /// The failure at `offset` in `source`. Position and snippet come from the line index, so a
    /// parser reports where it stopped without doing arithmetic of its own.
    pub fn at(source: &SourceText, offset: usize, kind: ParseErrorKind) -> Self {
        let line = source.line_of(offset);
        Self::new(
            line,
            source.column_of(offset),
            kind,
            snippet(source.line_text(line).as_bytes()),
        )
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}, column {}: {:?}",
            self.line, self.column, self.kind
        )
    }
}

impl std::error::Error for ParseError {}

/// Exhaustive on purpose: adding a variant must break the UI mapping in src-tauri, not slip past
/// a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// UTF-16/UTF-32 byte-order mark, or NUL bytes in the first 1 KiB.
    UnsupportedEncoding,
    InvalidUtf8,
    UnknownFormat,
    /// The parser's segments do not tile the file, so it could not be written back byte for byte.
    /// A Sublore bug, never a bad file, and a refusal rather than a corrupted save.
    SegmentCoverage,
    /// A block did not start with an index plus timing, or with a timing line.
    ExpectedTiming,
    BadTimecode,
    TimecodeOutOfRange,
    MissingVttHeader,
    /// ASS: an event line before its section's `Format:` line.
    MissingFormatLine,
    /// ASS: the `Format:` line declares no Start or no End field.
    MissingTimingFields,
    /// ASS: fewer fields than the `Format:` line declares.
    FieldCountMismatch,
    /// ASS: a section header with no closing bracket.
    BadSectionHeader,
    UnexpectedEndOfFile,
}

/// A one-line technical excerpt of `raw`: line terminator stripped, invalid bytes replaced, cut to
/// [`SNIPPET_MAX_BYTES`] on a char boundary.
pub(crate) fn snippet(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let trimmed = text.trim_end_matches(['\r', '\n']);
    let mut end = trimmed.len().min(SNIPPET_MAX_BYTES);
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed.get(..end).unwrap_or("").to_owned()
}

#[cfg(test)]
mod tests {
    use super::{snippet, ParseError, ParseErrorKind};
    use crate::text::SourceText;

    #[test]
    fn displays_position_and_kind() {
        let error = ParseError::new(7, 3, ParseErrorKind::BadTimecode, String::new());
        assert_eq!(error.to_string(), "line 7, column 3: BadTimecode");
    }

    #[test]
    fn positions_itself_from_the_line_index() {
        let source = SourceText::from_bytes(b"1\r\n00:00 --> junk\r\nhello\r\n").expect("utf-8");
        let offset = source.body().find("junk").expect("fixture holds junk");
        let error = ParseError::at(&source, offset, ParseErrorKind::BadTimecode);
        assert_eq!(error.line, 2);
        assert_eq!(error.column, 11);
        assert_eq!(error.snippet, "00:00 --> junk");
    }

    #[test]
    fn snippets_drop_the_terminator_and_survive_bad_bytes() {
        assert_eq!(snippet(b"a line\r\n"), "a line");
        assert_eq!(snippet(&[b'a', 0xFF, b'b']), "a\u{fffd}b");
    }

    #[test]
    fn snippets_cut_on_a_char_boundary() {
        let long = "é".repeat(200);
        let cut = snippet(long.as_bytes());
        assert!(cut.len() <= 120);
        assert_eq!(cut.chars().count(), 60);
    }
}
