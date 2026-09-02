//! Extraction failures: a stable kind plus a technical detail. Shaped like `sublore_io::IoError`
//! and `sublore_asr::AsrError` on purpose. See BACKLOG.md M2.4.
//!
//! The kind is the entire vocabulary this crate speaks; every sentence the user reads is picked
//! from the kind by the layer above. `detail` carries paths and operating-system messages for the
//! log and is never rendered as UI copy.

use std::fmt;

/// Exhaustive on purpose: adding a variant must break the mapping in the app, not slip past a
/// wildcard arm.
///
/// The ordering is the run's own order: find ffmpeg, read the media, watch the child, stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioErrorKind {
    /// ffmpeg could not be started: not there, not executable, wrong architecture.
    FfmpegMissing,
    /// ffmpeg refused the media, or the named track held no audio. Includes "this is not media"
    /// and "that stream index is not an audio stream".
    MediaUnreadable,
    /// ffmpeg wrote nothing for the stall timeout and was killed for it.
    Stalled,
    /// The caller cancelled. Never an error banner.
    Cancelled,
    /// Pipe or thread machinery failed. Always a Sublore bug.
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioError {
    pub kind: AudioErrorKind,
    /// Paths, exit codes, the head of ffmpeg's stderr. For logs. Never rendered as UI copy.
    pub detail: String,
}

impl AudioError {
    pub fn new(kind: AudioErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.kind == AudioErrorKind::Cancelled
    }
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for AudioError {}

#[cfg(test)]
mod tests {
    use super::{AudioError, AudioErrorKind};

    #[test]
    fn displays_the_kind_and_the_detail() {
        let error = AudioError::new(AudioErrorKind::MediaUnreadable, "ffmpeg exit status: 1");
        assert_eq!(error.to_string(), "MediaUnreadable: ffmpeg exit status: 1");
        assert!(!error.is_cancelled());
        assert!(AudioError::new(AudioErrorKind::Cancelled, "").is_cancelled());
    }
}
