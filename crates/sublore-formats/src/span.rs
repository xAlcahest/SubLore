//! Byte ranges into the one buffer that holds the file. Spans are the whole model: the parsers
//! never copy text, so the serializer can only ever write bytes the file already had.
//! See BACKLOG.md M1.1.

/// A byte range into [`crate::text::SourceText::body`]. `Copy`, so it travels without cloning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// A reversed range is a parser bug: debug builds trip, release builds clamp to empty rather
    /// than hand out garbage that would slice at the wrong place.
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "span start {start} is past its end {end}");
        Self {
            start,
            end: if end < start { start } else { end },
        }
    }

    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn range(self) -> std::ops::Range<usize> {
        self.start..self.end
    }
}

#[cfg(test)]
mod tests {
    use super::Span;

    #[test]
    fn measures_and_ranges() {
        let span = Span::new(4, 10);
        assert_eq!(span.len(), 6);
        assert!(!span.is_empty());
        assert_eq!(span.range(), 4..10);
    }

    #[test]
    fn an_equal_pair_is_empty() {
        let span = Span::new(7, 7);
        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
    }

    #[test]
    fn a_reversed_pair_never_measures_backwards() {
        // Built by hand, because Span::new would trip its debug assertion on this input.
        let reversed = Span { start: 9, end: 2 };
        assert_eq!(reversed.len(), 0);
        assert!(reversed.is_empty());
    }
}
