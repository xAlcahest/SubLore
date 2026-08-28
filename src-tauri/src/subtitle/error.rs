//! Errors the subtitle commands send to the UI: a stable code, and for a grammar failure the line
//! and the reason. The UI maps them through src/i18n/en.ts, so no English prose crosses the IPC
//! boundary. Same shape as `video::error`. See BACKLOG.md M1.5.

use std::io;
use std::path::Path;

use serde::Serialize;
use sublore_edit::error::{EditError, EditErrorKind};
use sublore_formats::{ParseError, ParseErrorKind};
use sublore_io::error::{IoError, IoErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SubtitleErrorCode {
    /// Empty path.
    InvalidPath,
    /// Nothing there, or there but not a regular file.
    NotAFile,
    /// Past [`super::MAX_SUBTITLE_BYTES`].
    TooLarge,
    ReadFailed,
    /// Not UTF-8, or a wide byte-order mark.
    UnsupportedEncoding,
    /// Neither the content nor the extension names a format Sublore parses.
    UnknownFormat,
    /// The grammar stopped somewhere, or the file could not be reproduced byte for byte. `line`
    /// and `reason` say where and why whenever the failure is about one line.
    ParseFailed,
    WriteFailed,
    BackupFailed,
    PermissionDenied,
    /// A mutation, undo, redo or save arrived with no file open.
    NoDocument,
    /// The caller's revision is not the session's, so its cue indices describe a list that has
    /// moved. The UI refetches rather than editing the wrong cue.
    StaleRevision,
    /// No cue with that index in the open document.
    InvalidCue,
    /// The text cannot be written in this format without changing the file's structure.
    UnwritableText,
    /// Sublore's own guard refused the edit and nothing was changed. Six internal kinds collapse
    /// here because the user's next move is the same for all of them; `detail` keeps the
    /// difference for the log.
    EditRefused,
    /// Opening or closing would drop edits the user has not saved.
    UnsavedChanges,
    /// The command machinery itself failed. Never a situation the user created.
    CommandFailed,
}

/// Why a parse stopped. The wire half of [`ParseErrorKind`], minus the kinds that are about the
/// whole file rather than one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SubtitleReason {
    ExpectedTiming,
    BadTimecode,
    TimecodeOutOfRange,
    MissingVttHeader,
    MissingFormatLine,
    MissingTimingFields,
    FieldCountMismatch,
    BadSectionHeader,
    UnexpectedEndOfFile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleError {
    pub code: SubtitleErrorCode,
    /// 1-based, and present exactly when `reason` is: an encoding failure is about the file, not a
    /// line the user can go and look at.
    pub line: Option<u32>,
    pub reason: Option<SubtitleReason>,
    /// Technical, never shown to the user, may be empty.
    pub detail: String,
}

impl SubtitleError {
    pub fn new(code: SubtitleErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            line: None,
            reason: None,
            detail: detail.into(),
        }
    }

    /// Map a grammar failure onto the IPC contract. Exhaustive on purpose: a new
    /// [`ParseErrorKind`] must break this build rather than fall into a wildcard.
    pub fn from_parse(error: ParseError) -> Self {
        let (code, reason) = match error.kind {
            ParseErrorKind::UnsupportedEncoding | ParseErrorKind::InvalidUtf8 => {
                (SubtitleErrorCode::UnsupportedEncoding, None)
            }
            ParseErrorKind::UnknownFormat => (SubtitleErrorCode::UnknownFormat, None),
            // Our bug, not the user's line: the file is refused, and the detail carries the rest.
            ParseErrorKind::SegmentCoverage => (SubtitleErrorCode::ParseFailed, None),
            ParseErrorKind::ExpectedTiming => (
                SubtitleErrorCode::ParseFailed,
                Some(SubtitleReason::ExpectedTiming),
            ),
            ParseErrorKind::BadTimecode => (
                SubtitleErrorCode::ParseFailed,
                Some(SubtitleReason::BadTimecode),
            ),
            ParseErrorKind::TimecodeOutOfRange => (
                SubtitleErrorCode::ParseFailed,
                Some(SubtitleReason::TimecodeOutOfRange),
            ),
            ParseErrorKind::MissingVttHeader => (
                SubtitleErrorCode::ParseFailed,
                Some(SubtitleReason::MissingVttHeader),
            ),
            ParseErrorKind::MissingFormatLine => (
                SubtitleErrorCode::ParseFailed,
                Some(SubtitleReason::MissingFormatLine),
            ),
            ParseErrorKind::MissingTimingFields => (
                SubtitleErrorCode::ParseFailed,
                Some(SubtitleReason::MissingTimingFields),
            ),
            ParseErrorKind::FieldCountMismatch => (
                SubtitleErrorCode::ParseFailed,
                Some(SubtitleReason::FieldCountMismatch),
            ),
            ParseErrorKind::BadSectionHeader => (
                SubtitleErrorCode::ParseFailed,
                Some(SubtitleReason::BadSectionHeader),
            ),
            ParseErrorKind::UnexpectedEndOfFile => (
                SubtitleErrorCode::ParseFailed,
                Some(SubtitleReason::UnexpectedEndOfFile),
            ),
        };

        Self {
            code,
            line: reason.is_some().then_some(error.line),
            reason,
            detail: format!("{error}: {}", error.snippet),
        }
    }

    /// Map a write failure onto the IPC contract. Every step that can only mean "the bytes did not
    /// land" collapses to one code: the user's next move is the same for all of them.
    pub fn from_io(error: IoError) -> Self {
        let code = match error.kind {
            IoErrorKind::InvalidPath => SubtitleErrorCode::InvalidPath,
            IoErrorKind::NotAFile => SubtitleErrorCode::NotAFile,
            IoErrorKind::ReadFailed => SubtitleErrorCode::ReadFailed,
            IoErrorKind::TempCreateFailed
            | IoErrorKind::WriteFailed
            | IoErrorKind::SyncFailed
            | IoErrorKind::RenameFailed => SubtitleErrorCode::WriteFailed,
            IoErrorKind::PermissionDenied => SubtitleErrorCode::PermissionDenied,
            IoErrorKind::BackupFailed => SubtitleErrorCode::BackupFailed,
        };
        Self::new(code, error.to_string())
    }

    /// Map a refused edit onto the IPC contract. Exhaustive on purpose: a new [`EditErrorKind`]
    /// must break this build rather than fall into a wildcard.
    ///
    /// No `reason` travels with these: [`SubtitleReason`] describes grammar failures in a file
    /// being read, which is a different thing from an edit Sublore declined to make.
    pub fn from_edit(error: EditError) -> Self {
        let code = match error.kind {
            EditErrorKind::NoSuchCue => SubtitleErrorCode::InvalidCue,
            EditErrorKind::UnwritableText => SubtitleErrorCode::UnwritableText,
            EditErrorKind::BadRange
            | EditErrorKind::StaleSplice
            | EditErrorKind::UnwritableTimecode
            | EditErrorKind::NotApplicable
            | EditErrorKind::Reparse
            | EditErrorKind::Unverified => SubtitleErrorCode::EditRefused,
        };
        Self::new(code, error.to_string())
    }

    /// Map an operating-system failure from opening or reading the source file.
    pub(crate) fn from_read(error: &io::Error, path: &Path) -> Self {
        let code = match error.kind() {
            io::ErrorKind::NotFound => SubtitleErrorCode::NotAFile,
            io::ErrorKind::PermissionDenied => SubtitleErrorCode::PermissionDenied,
            _ => SubtitleErrorCode::ReadFailed,
        };
        Self::new(code, format!("{}: {error}", path.display()))
    }
}

impl std::fmt::Display for SubtitleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

impl std::error::Error for SubtitleError {}

#[cfg(test)]
mod tests {
    use super::{SubtitleError, SubtitleErrorCode, SubtitleReason};
    use std::io;
    use std::path::Path;
    use sublore_edit::error::{EditError, EditErrorKind};
    use sublore_formats::{ParseError, ParseErrorKind};
    use sublore_io::error::{IoError, IoErrorKind};

    fn parse_error(kind: ParseErrorKind) -> ParseError {
        ParseError::new(7, 3, kind, "00:00:04,000 00:00:06,000".to_owned())
    }

    #[test]
    fn a_grammar_failure_carries_its_line_and_reason() {
        let error = SubtitleError::from_parse(parse_error(ParseErrorKind::ExpectedTiming));
        assert_eq!(error.code, SubtitleErrorCode::ParseFailed);
        assert_eq!(error.line, Some(7));
        assert_eq!(error.reason, Some(SubtitleReason::ExpectedTiming));
        assert!(
            error.detail.contains("00:00:04,000"),
            "the snippet stays in the technical detail: {}",
            error.detail
        );
    }

    #[test]
    fn every_grammar_kind_maps_to_its_own_reason() {
        for (kind, reason) in [
            (
                ParseErrorKind::ExpectedTiming,
                SubtitleReason::ExpectedTiming,
            ),
            (ParseErrorKind::BadTimecode, SubtitleReason::BadTimecode),
            (
                ParseErrorKind::TimecodeOutOfRange,
                SubtitleReason::TimecodeOutOfRange,
            ),
            (
                ParseErrorKind::MissingVttHeader,
                SubtitleReason::MissingVttHeader,
            ),
            (
                ParseErrorKind::MissingFormatLine,
                SubtitleReason::MissingFormatLine,
            ),
            (
                ParseErrorKind::MissingTimingFields,
                SubtitleReason::MissingTimingFields,
            ),
            (
                ParseErrorKind::FieldCountMismatch,
                SubtitleReason::FieldCountMismatch,
            ),
            (
                ParseErrorKind::BadSectionHeader,
                SubtitleReason::BadSectionHeader,
            ),
            (
                ParseErrorKind::UnexpectedEndOfFile,
                SubtitleReason::UnexpectedEndOfFile,
            ),
        ] {
            let error = SubtitleError::from_parse(parse_error(kind));
            assert_eq!(error.code, SubtitleErrorCode::ParseFailed, "{kind:?}");
            assert_eq!(error.reason, Some(reason), "{kind:?}");
        }
    }

    #[test]
    fn an_encoding_failure_is_about_the_file_not_a_line() {
        for kind in [
            ParseErrorKind::UnsupportedEncoding,
            ParseErrorKind::InvalidUtf8,
        ] {
            let error = SubtitleError::from_parse(parse_error(kind));
            assert_eq!(
                error.code,
                SubtitleErrorCode::UnsupportedEncoding,
                "{kind:?}"
            );
            assert_eq!(error.line, None, "{kind:?}");
            assert_eq!(error.reason, None, "{kind:?}");
        }

        let error = SubtitleError::from_parse(parse_error(ParseErrorKind::UnknownFormat));
        assert_eq!(error.code, SubtitleErrorCode::UnknownFormat);
        assert_eq!(error.line, None);
    }

    #[test]
    fn a_file_the_parser_cannot_reproduce_is_refused_without_blaming_a_line() {
        let error = SubtitleError::from_parse(parse_error(ParseErrorKind::SegmentCoverage));
        assert_eq!(error.code, SubtitleErrorCode::ParseFailed);
        assert_eq!(error.line, None);
        assert_eq!(error.reason, None);
    }

    #[test]
    fn every_write_failure_maps_to_a_code_the_user_can_act_on() {
        for (kind, code) in [
            (IoErrorKind::InvalidPath, SubtitleErrorCode::InvalidPath),
            (IoErrorKind::NotAFile, SubtitleErrorCode::NotAFile),
            (IoErrorKind::ReadFailed, SubtitleErrorCode::ReadFailed),
            (
                IoErrorKind::TempCreateFailed,
                SubtitleErrorCode::WriteFailed,
            ),
            (IoErrorKind::WriteFailed, SubtitleErrorCode::WriteFailed),
            (IoErrorKind::SyncFailed, SubtitleErrorCode::WriteFailed),
            (IoErrorKind::RenameFailed, SubtitleErrorCode::WriteFailed),
            (
                IoErrorKind::PermissionDenied,
                SubtitleErrorCode::PermissionDenied,
            ),
            (IoErrorKind::BackupFailed, SubtitleErrorCode::BackupFailed),
        ] {
            let io_error = IoError {
                kind,
                path: Path::new("/tmp/ep01.srt").to_path_buf(),
                detail: "disk on fire".to_owned(),
            };
            let error = SubtitleError::from_io(io_error);
            assert_eq!(error.code, code, "{kind:?}");
            assert_eq!(error.line, None, "{kind:?}");
            assert!(error.detail.contains("ep01.srt"), "{kind:?}");
        }
    }

    #[test]
    fn every_refused_edit_maps_to_a_code_and_none_of_them_blames_a_line() {
        for (kind, code) in [
            (EditErrorKind::NoSuchCue, SubtitleErrorCode::InvalidCue),
            (EditErrorKind::BadRange, SubtitleErrorCode::EditRefused),
            (EditErrorKind::StaleSplice, SubtitleErrorCode::EditRefused),
            (
                EditErrorKind::UnwritableText,
                SubtitleErrorCode::UnwritableText,
            ),
            (
                EditErrorKind::UnwritableTimecode,
                SubtitleErrorCode::EditRefused,
            ),
            (EditErrorKind::NotApplicable, SubtitleErrorCode::EditRefused),
            (EditErrorKind::Reparse, SubtitleErrorCode::EditRefused),
            (EditErrorKind::Unverified, SubtitleErrorCode::EditRefused),
        ] {
            let error = SubtitleError::from_edit(EditError::new(kind, "cue 4 of 3"));
            assert_eq!(error.code, code, "{kind:?}");
            // A refused edit is not a grammar failure in a file being read, so it names no line.
            assert_eq!(error.line, None, "{kind:?}");
            assert_eq!(error.reason, None, "{kind:?}");
            assert!(error.detail.contains("cue 4 of 3"), "{kind:?}");
        }
    }

    #[test]
    fn a_missing_file_reads_as_not_a_file_and_a_denied_one_keeps_its_kind() {
        let path = Path::new("/tmp/ep01.srt");
        assert_eq!(
            SubtitleError::from_read(&io::Error::from(io::ErrorKind::NotFound), path).code,
            SubtitleErrorCode::NotAFile
        );
        assert_eq!(
            SubtitleError::from_read(&io::Error::from(io::ErrorKind::PermissionDenied), path).code,
            SubtitleErrorCode::PermissionDenied
        );
        assert_eq!(
            SubtitleError::from_read(&io::Error::from(io::ErrorKind::Other), path).code,
            SubtitleErrorCode::ReadFailed
        );
    }
}
