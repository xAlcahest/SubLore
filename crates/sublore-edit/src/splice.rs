//! One byte-level replacement over a document body, and its algebra. Every mutation, undo and redo
//! in Sublore is one of these. Offsets are body offsets, BOM excluded, exactly like
//! `sublore_formats::Span`. See BACKLOG.md M2.1.

use crate::error::{EditError, EditErrorKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Splice {
    /// Where the replaced range starts.
    pub at: usize,
    /// The exact bytes that must be there now. Checked on apply, which is what makes a stale
    /// undo entry a refusal instead of a corrupted file.
    pub removed: String,
    pub inserted: String,
}

impl Splice {
    pub fn new(at: usize, removed: String, inserted: String) -> Self {
        Self {
            at,
            removed,
            inserted,
        }
    }

    /// Exact inverse: applying a splice and then its inverse restores the original bytes.
    pub fn inverse(&self) -> Splice {
        Splice::new(self.at, self.inserted.clone(), self.removed.clone())
    }

    /// One past the last replaced byte. Saturates rather than wraps; `apply` refuses the range.
    pub fn end(&self) -> usize {
        self.at.saturating_add(self.removed.len())
    }

    /// Bytes this splice keeps alive, for the history's memory bound.
    pub fn weight(&self) -> usize {
        self.removed.len().saturating_add(self.inserted.len())
    }

    pub fn is_noop(&self) -> bool {
        self.removed == self.inserted
    }
}

/// What a splice touches, so the history knows what may coalesce with what.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditLabel {
    pub kind: EditKind,
    /// Index into `SubtitleDocument::cues()` of the first cue the edit touches.
    pub cue: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    SetText,
    /// Its own kind, not `SetText` over more rows: the history coalesces only same-label edits, and
    /// a replace that rewrote forty cues must never merge into the keystroke before it.
    SetTexts,
    SetTimes,
    Insert,
    Delete,
    Split,
    Merge,
}

/// Apply `splice` to `body`. Fails when the range is outside the body, cuts a character, or does
/// not hold exactly `splice.removed`. A refusal never produces bytes.
pub fn apply(body: &str, splice: &Splice) -> Result<String, EditError> {
    let end = splice.at.checked_add(splice.removed.len()).ok_or_else(|| {
        EditError::new(
            EditErrorKind::BadRange,
            format!(
                "the splice range at {} for {} bytes overflows",
                splice.at,
                splice.removed.len()
            ),
        )
    })?;
    if end > body.len() || !body.is_char_boundary(splice.at) || !body.is_char_boundary(end) {
        return Err(EditError::new(
            EditErrorKind::BadRange,
            format!(
                "{}..{end} is not a character range of a {}-byte body",
                splice.at,
                body.len()
            ),
        ));
    }
    if body.get(splice.at..end) != Some(splice.removed.as_str()) {
        return Err(EditError::new(
            EditErrorKind::StaleSplice,
            format!(
                "{}..{end} no longer holds the {} bytes the splice replaces",
                splice.at,
                splice.removed.len()
            ),
        ));
    }

    let grown = body.len().saturating_add(splice.inserted.len());
    let mut out = String::with_capacity(grown.saturating_sub(splice.removed.len()));
    // Three memcpys: the bytes outside the range are copied, never rebuilt. See CONTRIBUTING.md §3.
    out.push_str(body.get(..splice.at).unwrap_or(""));
    out.push_str(&splice.inserted);
    out.push_str(body.get(end..).unwrap_or(""));
    Ok(out)
}
