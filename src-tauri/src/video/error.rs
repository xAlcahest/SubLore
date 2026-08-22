//! Errors the video player sends to the UI: a stable code plus a technical detail.
//! The UI maps the code through src/i18n/en.ts, so no English prose crosses the IPC boundary.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoErrorCode {
    /// mpv did not start, or the app is shutting down.
    PlayerUnavailable,
    /// Empty path, or not an existing readable file.
    InvalidPath,
    /// mpv refused the file.
    OpenFailed,
    /// No verdict from mpv within the open timeout.
    OpenTimeout,
    /// play/pause/seek with no file open.
    NotLoaded,
    /// mpv rejected a command.
    CommandFailed,
    /// Playback ended in error after a successful open.
    PlaybackStopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoError {
    pub code: VideoErrorCode,
    /// Technical, never shown to the user, may be empty.
    pub detail: String,
}

impl VideoError {
    pub fn new(code: VideoErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub fn player_unavailable(detail: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::PlayerUnavailable, detail)
    }

    pub fn invalid_path(detail: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::InvalidPath, detail)
    }

    pub fn open_failed(detail: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::OpenFailed, detail)
    }

    pub fn command_failed(detail: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::CommandFailed, detail)
    }
}

impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

impl std::error::Error for VideoError {}

/// Map a libmpv error onto the IPC contract. libmpv2::Error never reaches the frontend:
/// its Display is `{:?}` and its raw codes are meaningless to a translator.
pub fn from_mpv(error: libmpv2::Error, context: &str) -> VideoError {
    use libmpv2::mpv_error;

    match error {
        libmpv2::Error::VersionMismatch { loaded, .. } => VideoError::player_unavailable(format!(
            "libmpv client API {loaded}, expected major {}",
            libmpv2::MPV_CLIENT_API_MAJOR
        )),
        libmpv2::Error::Null => VideoError::player_unavailable("mpv returned a null handle"),
        libmpv2::Error::InvalidUtf8 => {
            VideoError::command_failed(format!("non-UTF-8 value from mpv ({context})"))
        }
        libmpv2::Error::Raw(code) if code == mpv_error::PropertyUnavailable => {
            VideoError::new(VideoErrorCode::NotLoaded, context.to_owned())
        }
        libmpv2::Error::Raw(code) if code == mpv_error::LoadingFailed => {
            VideoError::open_failed(context.to_owned())
        }
        libmpv2::Error::Raw(code) => {
            VideoError::command_failed(format!("mpv error {code} ({context})"))
        }
    }
}
