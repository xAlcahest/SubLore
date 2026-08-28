//! Decoding and the line index: the only place in the crate where bytes become text.
//!
//! Line rules, frozen for all three parsers:
//! - A line ends after `\n`. `\r\n` is one terminator; a lone `\r` is an ordinary character, not a
//!   break, because every format in scope is `\n`-terminated in practice and a bare `\r` is real
//!   content often enough to matter.
//! - A trailing terminator does not open a new line: `"a\n"` is one line, and `line_of(len)`
//!   saturates to the last line that holds bytes, so an error at EOF points at real text. An empty
//!   body is one empty line.
//!
//! See BACKLOG.md M1.1.

use crate::error::{snippet, ParseError, ParseErrorKind};
use crate::span::Span;

/// The UTF-8 byte-order mark. Remembered on parse and written back byte for byte.
pub(crate) const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// How much of the file is checked for NUL bytes before decoding.
const NUL_SCAN_BYTES: usize = 1024;

/// The dominant line terminator of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Newline {
    Lf,
    Crlf,
    Mixed,
    None,
}

/// A decoded file: the whole body in one buffer, plus the index every span is measured against.
#[derive(Clone, Debug)]
pub struct SourceText {
    bom: bool,
    body: String,
    line_starts: Vec<usize>,
    newline: Newline,
}

impl SourceText {
    /// Rejects UTF-16/32 byte-order marks, NUL-bearing files and invalid UTF-8. Nothing is
    /// transcoded and nothing is stripped: a file we might decode wrong is a file we refuse.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        if let Some(encoding) = wide_encoding(bytes) {
            return Err(ParseError::new(
                1,
                1,
                ParseErrorKind::UnsupportedEncoding,
                encoding.to_owned(),
            ));
        }

        let bom = bytes.starts_with(&UTF8_BOM);
        let rest = if bom {
            bytes.get(UTF8_BOM.len()..).unwrap_or(&[])
        } else {
            bytes
        };

        // BOM-less UTF-16 is valid UTF-8 full of NULs; failing here beats a grammar error 40 lines in.
        let head = rest.get(..NUL_SCAN_BYTES).unwrap_or(rest);
        if head.contains(&0) {
            return Err(ParseError::new(
                1,
                1,
                ParseErrorKind::UnsupportedEncoding,
                format!("NUL byte within the first {NUL_SCAN_BYTES} bytes"),
            ));
        }

        let body = match std::str::from_utf8(rest) {
            Ok(text) => text.to_owned(),
            Err(error) => return Err(invalid_utf8(rest, error.valid_up_to())),
        };

        let line_starts = index_lines(&body);
        let newline = detect_newline(&body);
        Ok(Self {
            bom,
            body,
            line_starts,
            newline,
        })
    }

    pub fn has_bom(&self) -> bool {
        self.bom
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    /// Total bytes of the original file, BOM included.
    pub fn byte_len(&self) -> usize {
        self.body.len() + if self.bom { UTF8_BOM.len() } else { 0 }
    }

    pub fn newline(&self) -> Newline {
        self.newline
    }

    /// 1-based line containing `offset`; saturates at the last line for `offset >= body.len()`.
    pub fn line_of(&self, offset: usize) -> u32 {
        let clamped = offset.min(self.body.len());
        let index = self.line_starts.partition_point(|&start| start <= clamped);
        u32::try_from(index.max(1)).unwrap_or(u32::MAX)
    }

    /// 1-based byte column of `offset` inside its line.
    pub fn column_of(&self, offset: usize) -> u32 {
        let clamped = offset.min(self.body.len());
        let index = (self.line_of(clamped) as usize).saturating_sub(1);
        let start = self.line_starts.get(index).copied().unwrap_or(0);
        u32::try_from(clamped.saturating_sub(start) + 1).unwrap_or(u32::MAX)
    }

    /// A 1-based line's text without its terminator. Empty for an out-of-range line.
    pub fn line_text(&self, line: u32) -> &str {
        let raw = self.body.get(self.line_span(line).range()).unwrap_or("");
        match raw.strip_suffix('\n') {
            Some(without_lf) => without_lf.strip_suffix('\r').unwrap_or(without_lf),
            None => raw,
        }
    }

    /// Span of a 1-based line including its terminator. Parsers scan through this, never by hand.
    pub fn line_span(&self, line: u32) -> Span {
        if line == 0 {
            return Span::new(0, 0);
        }
        let index = (line as usize).saturating_sub(1);
        match self.line_starts.get(index).copied() {
            Some(start) => {
                let end = self
                    .line_starts
                    .get(index + 1)
                    .copied()
                    .unwrap_or(self.body.len());
                Span::new(start, end)
            }
            None => Span::new(self.body.len(), self.body.len()),
        }
    }

    pub fn line_count(&self) -> u32 {
        u32::try_from(self.line_starts.len()).unwrap_or(u32::MAX)
    }
}

/// The wide byte-order marks we refuse, named for the error snippet. UTF-32LE shares its first two
/// bytes with UTF-16LE, so the wider marks are tested first.
fn wide_encoding(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        Some("UTF-32LE byte-order mark")
    } else if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        Some("UTF-32BE byte-order mark")
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Some("UTF-16LE byte-order mark")
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Some("UTF-16BE byte-order mark")
    } else {
        None
    }
}

/// Position an [`ParseErrorKind::InvalidUtf8`] failure without a line index, which cannot exist yet.
fn invalid_utf8(bytes: &[u8], offset: usize) -> ParseError {
    let valid = bytes.get(..offset).unwrap_or(bytes);
    let line_start = valid
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |at| at + 1);
    let line =
        u32::try_from(valid.iter().filter(|&&byte| byte == b'\n').count() + 1).unwrap_or(u32::MAX);
    let column = u32::try_from(offset.saturating_sub(line_start) + 1).unwrap_or(u32::MAX);
    let tail = bytes.get(line_start..).unwrap_or(&[]);
    let line_end = tail
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(bytes.len(), |at| line_start + at);
    let raw = bytes.get(line_start..line_end).unwrap_or(&[]);
    ParseError::new(line, column, ParseErrorKind::InvalidUtf8, snippet(raw))
}

fn index_lines(body: &str) -> Vec<usize> {
    let bytes = body.as_bytes();
    let mut starts = Vec::with_capacity(bytes.len() / 32 + 1);
    starts.push(0);
    for (index, &byte) in bytes.iter().enumerate() {
        // A trailing terminator opens no line, so an EOF error points at the last line with bytes.
        if byte == b'\n' && index + 1 < bytes.len() {
            starts.push(index + 1);
        }
    }
    starts
}

fn detect_newline(body: &str) -> Newline {
    let bytes = body.as_bytes();
    let mut crlf = false;
    let mut lf = false;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte != b'\n' {
            continue;
        }
        if index > 0 && bytes.get(index - 1) == Some(&b'\r') {
            crlf = true;
        } else {
            lf = true;
        }
    }
    match (crlf, lf) {
        (true, true) => Newline::Mixed,
        (true, false) => Newline::Crlf,
        (false, true) => Newline::Lf,
        (false, false) => Newline::None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Newline, SourceText, UTF8_BOM};
    use crate::error::ParseErrorKind;

    fn source(text: &str) -> SourceText {
        SourceText::from_bytes(text.as_bytes()).expect("valid utf-8 fixture")
    }

    #[test]
    fn keeps_the_bom_out_of_the_body_but_inside_the_length() {
        let mut bytes = UTF8_BOM.to_vec();
        bytes.extend_from_slice(b"WEBVTT\n");
        let source = SourceText::from_bytes(&bytes).expect("valid utf-8");
        assert!(source.has_bom());
        assert_eq!(source.body(), "WEBVTT\n");
        assert_eq!(source.byte_len(), 10);
    }

    #[test]
    fn a_file_without_a_bom_measures_its_body() {
        let source = source("WEBVTT\n");
        assert!(!source.has_bom());
        assert_eq!(source.byte_len(), 7);
    }

    #[test]
    fn refuses_wide_byte_order_marks() {
        for (bytes, name) in [
            (vec![0xFF, 0xFE, b'1', 0x00], "UTF-16LE byte-order mark"),
            (vec![0xFE, 0xFF, 0x00, b'1'], "UTF-16BE byte-order mark"),
            (vec![0xFF, 0xFE, 0x00, 0x00], "UTF-32LE byte-order mark"),
            (vec![0x00, 0x00, 0xFE, 0xFF], "UTF-32BE byte-order mark"),
        ] {
            let error = SourceText::from_bytes(&bytes).expect_err("wide encoding must be refused");
            assert_eq!(error.kind, ParseErrorKind::UnsupportedEncoding);
            assert_eq!((error.line, error.column), (1, 1));
            assert_eq!(error.snippet, name);
        }
    }

    #[test]
    fn refuses_nul_bytes_near_the_start() {
        let error = SourceText::from_bytes(b"1\x0000:00:00,000")
            .expect_err("NUL-bearing files must be refused");
        assert_eq!(error.kind, ParseErrorKind::UnsupportedEncoding);
        assert_eq!((error.line, error.column), (1, 1));
    }

    #[test]
    fn reports_invalid_utf8_where_it_starts() {
        let mut bytes = b"1\n00:00:01,000\n".to_vec();
        bytes.extend_from_slice(&[b'C', b'a', b'f', 0xE9, b'\n']);
        let error = SourceText::from_bytes(&bytes).expect_err("latin-1 bytes must be refused");
        assert_eq!(error.kind, ParseErrorKind::InvalidUtf8);
        assert_eq!((error.line, error.column), (3, 4));
        assert_eq!(error.snippet, "Caf\u{fffd}");
    }

    #[test]
    fn a_trailing_terminator_opens_no_line() {
        let source = source("hello\n");
        assert_eq!(source.line_count(), 1);
        assert_eq!(source.line_of(source.body().len()), 1);
        assert_eq!(source.line_text(1), "hello");
    }

    #[test]
    fn counts_a_blank_line_that_holds_a_terminator() {
        let source = source("a\n\n");
        assert_eq!(source.line_count(), 2);
        assert_eq!(source.line_text(2), "");
        assert_eq!(source.line_span(2).range(), 2..3);
    }

    #[test]
    fn an_empty_body_is_one_empty_line() {
        let source = source("");
        assert_eq!(source.line_count(), 1);
        assert_eq!(source.line_of(0), 1);
        assert_eq!(source.line_text(1), "");
        assert_eq!(source.newline(), Newline::None);
    }

    #[test]
    fn a_lone_carriage_return_is_content() {
        let source = source("one\rtwo\nthree\n");
        assert_eq!(source.line_count(), 2);
        assert_eq!(source.line_text(1), "one\rtwo");
        assert_eq!(source.newline(), Newline::Lf);
    }

    #[test]
    fn line_spans_include_their_terminator() {
        let source = source("ab\r\ncd\r\n");
        assert_eq!(source.line_span(1).range(), 0..4);
        assert_eq!(source.line_text(1), "ab");
        assert_eq!(source.line_span(2).range(), 4..8);
        assert_eq!(source.line_text(2), "cd");
        assert_eq!(source.newline(), Newline::Crlf);
    }

    #[test]
    fn an_out_of_range_line_is_empty() {
        let source = source("only\n");
        assert_eq!(source.line_text(0), "");
        assert_eq!(source.line_text(9), "");
        assert!(source.line_span(9).is_empty());
    }

    #[test]
    fn mixed_terminators_are_reported_as_mixed() {
        assert_eq!(source("a\r\nb\nc\n").newline(), Newline::Mixed);
        assert_eq!(source("no terminator").newline(), Newline::None);
    }

    #[test]
    fn columns_are_one_based_bytes_within_the_line() {
        let source = source("first\nsecond\n");
        let offset = source.body().find("cond").expect("fixture holds cond");
        assert_eq!(source.line_of(offset), 2);
        assert_eq!(source.column_of(offset), 3);
        assert_eq!(source.column_of(usize::MAX), 8);
    }
}
