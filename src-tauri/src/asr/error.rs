//! Errors the transcription commands send to the UI: a stable code and a technical detail. Same
//! shape as `subtitle::error` and `video::error`. See BACKLOG.md M3.4.
//!
//! The UI maps every code through src/i18n/en.ts, so no English prose crosses the IPC boundary.
//! `detail` carries paths, exit codes and the tail of a child's stderr, for the log only.

use serde::Serialize;
use sublore_asr::{AsrError as SidecarError, AsrErrorKind};

/// The wire half of [`AsrErrorKind`], plus the one failure the command machinery can have on its
/// own. Exhaustive on purpose: a new kind in the crate must break this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AsrErrorCode {
    /// No whisper binary anywhere Sublore looks.
    BinaryMissing,
    /// It is there but it will not run: permissions, a missing library, the wrong architecture.
    BinaryUnrunnable,
    FfmpegMissing,
    /// ffmpeg refused the media, or it has no audio track.
    MediaUnreadable,
    ModelMissing,
    /// The file on disk is not the length the catalog says. A re-download fixes it.
    ModelCorrupt,
    /// whisper could not build a context from the model.
    ModelRejected,
    /// whisper could not open the audio we extracted. Always a Sublore bug.
    NoInput,
    /// whisper rejected one of our flags. Always a Sublore bug.
    BadArguments,
    /// The run ended with no readable JSON, whatever the exit code claimed.
    NoOutput,
    /// Valid output with no words in it. Not a defect: silence transcribes to nothing.
    EmptyTranscript,
    Stalled,
    /// The user cancelled. Never an error banner.
    Cancelled,
    ScratchFailed,
    /// Pipe or thread machinery failed. Always a Sublore bug.
    Internal,
    NetworkFailed,
    DownloadWriteFailed,
    SizeMismatch,
    /// The model's bytes do not hash to the catalog's sha256, so it never reached whisper.
    /// Downloading it again replaces it.
    ChecksumMismatch,
    /// A run or a download is already going. Nothing is queued.
    Busy,
    /// The command itself failed: no app data directory, or a task that did not finish.
    CommandFailed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrError {
    pub code: AsrErrorCode,
    /// Technical, never shown to the user, may be empty.
    pub detail: String,
}

impl AsrError {
    pub fn new(code: AsrErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.code == AsrErrorCode::Cancelled
    }
}

impl From<SidecarError> for AsrError {
    fn from(error: SidecarError) -> Self {
        let code = match error.kind {
            AsrErrorKind::BinaryMissing => AsrErrorCode::BinaryMissing,
            AsrErrorKind::BinaryUnrunnable => AsrErrorCode::BinaryUnrunnable,
            AsrErrorKind::FfmpegMissing => AsrErrorCode::FfmpegMissing,
            AsrErrorKind::MediaUnreadable => AsrErrorCode::MediaUnreadable,
            AsrErrorKind::ModelMissing => AsrErrorCode::ModelMissing,
            AsrErrorKind::ModelCorrupt => AsrErrorCode::ModelCorrupt,
            AsrErrorKind::ModelRejected => AsrErrorCode::ModelRejected,
            AsrErrorKind::NoInput => AsrErrorCode::NoInput,
            AsrErrorKind::BadArguments => AsrErrorCode::BadArguments,
            AsrErrorKind::NoOutput => AsrErrorCode::NoOutput,
            AsrErrorKind::EmptyTranscript => AsrErrorCode::EmptyTranscript,
            AsrErrorKind::Stalled => AsrErrorCode::Stalled,
            AsrErrorKind::Cancelled => AsrErrorCode::Cancelled,
            AsrErrorKind::ScratchFailed => AsrErrorCode::ScratchFailed,
            AsrErrorKind::Internal => AsrErrorCode::Internal,
            AsrErrorKind::NetworkFailed => AsrErrorCode::NetworkFailed,
            AsrErrorKind::DownloadWriteFailed => AsrErrorCode::DownloadWriteFailed,
            AsrErrorKind::SizeMismatch => AsrErrorCode::SizeMismatch,
            AsrErrorKind::ChecksumMismatch => AsrErrorCode::ChecksumMismatch,
            AsrErrorKind::Busy => AsrErrorCode::Busy,
        };
        Self::new(code, error.detail)
    }
}

impl std::fmt::Display for AsrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

impl std::error::Error for AsrError {}

#[cfg(test)]
mod tests {
    use super::{AsrError, AsrErrorCode};
    use sublore_asr::{AsrError as SidecarError, AsrErrorKind};

    /// Every kind the sidecar crate can produce, so the mapping below is provably total.
    const EVERY_KIND: [AsrErrorKind; 20] = [
        AsrErrorKind::BinaryMissing,
        AsrErrorKind::BinaryUnrunnable,
        AsrErrorKind::FfmpegMissing,
        AsrErrorKind::MediaUnreadable,
        AsrErrorKind::ModelMissing,
        AsrErrorKind::ModelCorrupt,
        AsrErrorKind::ModelRejected,
        AsrErrorKind::NoInput,
        AsrErrorKind::BadArguments,
        AsrErrorKind::NoOutput,
        AsrErrorKind::EmptyTranscript,
        AsrErrorKind::Stalled,
        AsrErrorKind::Cancelled,
        AsrErrorKind::ScratchFailed,
        AsrErrorKind::Internal,
        AsrErrorKind::NetworkFailed,
        AsrErrorKind::DownloadWriteFailed,
        AsrErrorKind::SizeMismatch,
        AsrErrorKind::ChecksumMismatch,
        AsrErrorKind::Busy,
    ];

    #[test]
    fn every_sidecar_kind_maps_to_its_own_code_and_keeps_its_detail() {
        let mut codes = Vec::new();
        for kind in EVERY_KIND {
            let error = AsrError::from(SidecarError::new(kind, "exit 3, no out.json"));
            assert_eq!(error.detail, "exit 3, no out.json", "{kind:?}");
            assert!(!codes.contains(&error.code), "{kind:?} shares a code");
            codes.push(error.code);
        }
        assert_eq!(codes.len(), EVERY_KIND.len());
    }

    #[test]
    fn a_code_reaches_the_ui_as_the_camel_case_name_the_typescript_expects() {
        let error = AsrError::new(AsrErrorCode::EmptyTranscript, "no words");
        let json = serde_json::to_string(&error).expect("the wire error serializes");
        assert_eq!(json, r#"{"code":"emptyTranscript","detail":"no words"}"#);
    }

    #[test]
    fn only_cancellation_reads_as_cancelled() {
        assert!(AsrError::from(SidecarError::new(AsrErrorKind::Cancelled, "")).is_cancelled());
        assert!(!AsrError::from(SidecarError::new(AsrErrorKind::NoOutput, "")).is_cancelled());
    }
}
