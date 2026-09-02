//! Errors the waveform commands send to the UI: a stable code and a technical detail. Same shape
//! as `asr::error` and `video::error`. See BACKLOG.md M2.4, W4.
//!
//! The UI maps every code through src/i18n/en.ts, so no English prose crosses the IPC boundary.
//! `detail` carries paths and the head of ffmpeg's stderr, for the log only.

use serde::Serialize;
use sublore_audio::{AudioError as PeaksError, AudioErrorKind};

use crate::video::error::{VideoError, VideoErrorCode};

/// The wire half of [`AudioErrorKind`], plus what the command layer can fail at on its own.
/// Exhaustive on purpose: a new kind in `sublore-audio` or a new code in `video::error` must break
/// this build rather than slip past a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioErrorCode {
    /// No ffmpeg anywhere Sublore looks.
    FfmpegMissing,
    /// ffmpeg refused the media, or the named track held no audio.
    MediaUnreadable,
    /// ffmpeg wrote nothing for the stall timeout and was killed for it.
    Stalled,
    /// The media was closed or replaced, or the user switched track. Never an error banner.
    Cancelled,
    /// Pipe or thread machinery failed. Always a Sublore bug.
    Internal,
    /// Peaks for this media and this track are already being computed. Nothing is queued.
    Busy,
    /// No file is open, so there is no audio to peak.
    NotLoaded,
    /// The open media has no audio track with that index.
    NoSuchTrack,
    /// mpv is gone, or the app is shutting down.
    PlayerUnavailable,
    /// The command itself failed: a task that did not finish, or mpv refusing a property.
    CommandFailed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioError {
    pub code: AudioErrorCode,
    /// Technical, never shown to the user, may be empty.
    pub detail: String,
}

impl AudioError {
    pub fn new(code: AudioErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.code == AudioErrorCode::Cancelled
    }
}

impl From<PeaksError> for AudioError {
    fn from(error: PeaksError) -> Self {
        let code = match error.kind {
            AudioErrorKind::FfmpegMissing => AudioErrorCode::FfmpegMissing,
            AudioErrorKind::MediaUnreadable => AudioErrorCode::MediaUnreadable,
            AudioErrorKind::Stalled => AudioErrorCode::Stalled,
            AudioErrorKind::Cancelled => AudioErrorCode::Cancelled,
            AudioErrorKind::Internal => AudioErrorCode::Internal,
        };
        Self::new(code, error.detail)
    }
}

impl From<VideoError> for AudioError {
    fn from(error: VideoError) -> Self {
        let code = match error.code {
            VideoErrorCode::PlayerUnavailable => AudioErrorCode::PlayerUnavailable,
            VideoErrorCode::NotLoaded => AudioErrorCode::NotLoaded,
            // The player's own load failures reach the waveform as one code; the detail below
            // keeps which of them it was, so the log is not lossy.
            VideoErrorCode::InvalidPath
            | VideoErrorCode::OpenFailed
            | VideoErrorCode::OpenTimeout
            | VideoErrorCode::PlaybackStopped
            | VideoErrorCode::CommandFailed => AudioErrorCode::CommandFailed,
        };
        Self::new(code, format!("{:?}: {}", error.code, error.detail))
    }
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

impl std::error::Error for AudioError {}

#[cfg(test)]
mod tests {
    use super::{AudioError, AudioErrorCode};
    use crate::video::error::{VideoError, VideoErrorCode};
    use sublore_audio::{AudioError as PeaksError, AudioErrorKind};

    /// Every kind `sublore-audio` can produce, so the mapping above is provably total.
    const EVERY_KIND: [AudioErrorKind; 5] = [
        AudioErrorKind::FfmpegMissing,
        AudioErrorKind::MediaUnreadable,
        AudioErrorKind::Stalled,
        AudioErrorKind::Cancelled,
        AudioErrorKind::Internal,
    ];

    /// Every code the player can produce, for the same reason.
    const EVERY_VIDEO_CODE: [VideoErrorCode; 7] = [
        VideoErrorCode::PlayerUnavailable,
        VideoErrorCode::InvalidPath,
        VideoErrorCode::OpenFailed,
        VideoErrorCode::OpenTimeout,
        VideoErrorCode::NotLoaded,
        VideoErrorCode::CommandFailed,
        VideoErrorCode::PlaybackStopped,
    ];

    #[test]
    fn every_extraction_kind_maps_to_its_own_code_and_keeps_its_detail() {
        let mut codes = Vec::new();
        for kind in EVERY_KIND {
            let error = AudioError::from(PeaksError::new(kind, "ffmpeg exit status: 1"));
            assert_eq!(error.detail, "ffmpeg exit status: 1", "{kind:?}");
            assert!(!codes.contains(&error.code), "{kind:?} shares a code");
            codes.push(error.code);
        }
        assert_eq!(codes.len(), EVERY_KIND.len());
    }

    #[test]
    fn a_player_failure_keeps_its_own_code_in_the_detail_whatever_it_collapses_to() {
        for code in EVERY_VIDEO_CODE {
            let error = AudioError::from(VideoError::new(code, "mpv error 12 (track-list/count)"));
            assert!(
                error.detail.contains("mpv error 12 (track-list/count)"),
                "{code:?} lost the player's own detail: {}",
                error.detail
            );
            assert!(
                error.detail.contains(&format!("{code:?}")),
                "{code:?} is not named in the detail it collapsed into: {}",
                error.detail
            );
        }
    }

    #[test]
    fn the_two_player_codes_the_waveform_can_act_on_keep_their_own_meaning() {
        assert_eq!(
            AudioError::from(VideoError::new(
                VideoErrorCode::NotLoaded,
                "no file is open"
            ))
            .code,
            AudioErrorCode::NotLoaded
        );
        assert_eq!(
            AudioError::from(VideoError::player_unavailable("gone")).code,
            AudioErrorCode::PlayerUnavailable
        );
    }

    #[test]
    fn a_code_reaches_the_ui_as_the_camel_case_name_the_typescript_expects() {
        let error = AudioError::new(AudioErrorCode::NoSuchTrack, "track 9");
        let json = serde_json::to_string(&error).expect("the wire error serializes");
        assert_eq!(json, r#"{"code":"noSuchTrack","detail":"track 9"}"#);
    }

    #[test]
    fn only_cancellation_reads_as_cancelled() {
        assert!(AudioError::from(PeaksError::new(AudioErrorKind::Cancelled, "")).is_cancelled());
        assert!(!AudioError::from(PeaksError::new(AudioErrorKind::Stalled, "")).is_cancelled());
    }
}
