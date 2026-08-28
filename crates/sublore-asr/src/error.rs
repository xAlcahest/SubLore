//! Transcription and model failures: a stable kind plus a technical detail. Shaped like
//! `sublore_io::IoError` on purpose. See BACKLOG.md M3.1.
//!
//! The kind is the entire vocabulary this crate speaks; every sentence the user reads is picked
//! from the kind by the UI. `detail` carries paths and operating-system messages for the log and
//! is never rendered as UI copy.

use std::fmt;

/// Exhaustive on purpose: adding a variant must break the mapping in the app, not slip past a
/// wildcard arm.
///
/// The ordering is the run's own order: find the tools, read the media, check the model, run the
/// child, read what it wrote. The last group belongs to the model download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsrErrorKind {
    /// No whisper binary at any search location.
    BinaryMissing,
    /// Spawning it failed: permissions, a missing shared library, the wrong architecture.
    BinaryUnrunnable,
    /// No ffmpeg at any search location.
    FfmpegMissing,
    /// ffmpeg refused the media, or produced an empty WAV. Includes "there is no audio track".
    MediaUnreadable,
    /// The model file is not in the store.
    ModelMissing,
    /// The model file's length disagrees with the catalog: a truncated or half-copied download.
    ModelCorrupt,
    /// whisper could not initialise a context from the model.
    ModelRejected,
    /// whisper could not open the audio we handed it. Our bug: the path vanished under us.
    NoInput,
    /// whisper rejected an argument. Our bug, and it exits 0 while doing it.
    BadArguments,
    /// The run ended without readable JSON. Whatever the exit code said.
    NoOutput,
    /// Valid output, no usable words. Not a defect: silence transcribes to nothing.
    EmptyTranscript,
    /// Nothing came out of the child for the stall timeout.
    Stalled,
    /// The user cancelled. Never an error banner.
    Cancelled,
    /// The scratch directory could not be created.
    ScratchFailed,
    /// Pipe or thread machinery failed. Always a Sublore bug.
    Internal,
    /// The transfer failed or ended early. The partial file is kept so the next attempt resumes.
    NetworkFailed,
    /// The download could not be written to the models directory: no space, no permission.
    DownloadWriteFailed,
    /// The server sent a different number of bytes than the catalog declares.
    SizeMismatch,
    /// The file hashes to something other than the catalog's sha256: a download that arrived wrong,
    /// or a model that changed under Sublore afterwards. Refused before whisper is spawned.
    ChecksumMismatch,
    /// A run or a download is already going. Nothing is queued.
    Busy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsrError {
    pub kind: AsrErrorKind,
    /// Paths, exit codes, the tail of the child's stderr. For logs. Never rendered as UI copy.
    pub detail: String,
}

impl AsrError {
    pub fn new(kind: AsrErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.kind == AsrErrorKind::Cancelled
    }
}

impl fmt::Display for AsrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for AsrError {}

#[cfg(test)]
mod tests {
    use super::{AsrError, AsrErrorKind};

    #[test]
    fn displays_the_kind_and_the_detail() {
        let error = AsrError::new(AsrErrorKind::NoOutput, "exit 0, no out.json");
        assert_eq!(error.to_string(), "NoOutput: exit 0, no out.json");
        assert!(!error.is_cancelled());
        assert!(AsrError::new(AsrErrorKind::Cancelled, "").is_cancelled());
    }
}
