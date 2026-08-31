//! Why an edit was refused. The kind is the whole vocabulary this crate speaks; the words the user
//! reads are picked from it by the UI, exactly as `ParseError` works. See BACKLOG.md M2.1.

use sublore_formats::ParseError;

/// Exhaustive on purpose: adding a variant must break the mapping in src-tauri, not slip past a
/// wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditErrorKind {
    /// No cue with that index in this document.
    NoSuchCue,
    /// The splice range is outside the body, or cuts a character.
    BadRange,
    /// The bytes at the range are not the ones the splice expects.
    StaleSplice,
    /// The replacement cannot be written in this format without changing the file's structure:
    /// a blank line inside an SRT/VTT payload, any line break inside an ASS field.
    UnwritableText,
    /// A timecode the format cannot spell at this cue's precision, or past MAX_TIMECODE_MS.
    UnwritableTimecode,
    /// The mutation does not apply here: split at an offset outside the text, merge past the last
    /// cue, insert into an ASS [Events] section that holds no event to copy a shape from.
    NotApplicable,
    /// The edited bytes did not parse. A Sublore bug: refuse, never write (CONTRIBUTING.md §3).
    Reparse,
    /// The edited bytes parsed into a document the plan did not predict. Same rule.
    Unverified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditError {
    pub kind: EditErrorKind,
    /// Technical, for logs and the IPC `detail` field. Never rendered as UI copy.
    pub detail: String,
    /// 1-based, only when the failure came from a `ParseError` that had one.
    pub line: Option<u32>,
}

impl EditError {
    pub fn new(kind: EditErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            line: None,
        }
    }

    /// A refusal the parser handed back: keep its line so the failure can point into the file.
    pub fn from_parse(kind: EditErrorKind, error: ParseError) -> Self {
        Self {
            kind,
            detail: format!("{error}: {}", error.snippet),
            line: Some(error.line),
        }
    }
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {:?}: {}", self.kind, self.detail),
            None => write!(f, "{:?}: {}", self.kind, self.detail),
        }
    }
}

impl std::error::Error for EditError {}
