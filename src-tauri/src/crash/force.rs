//! Forced panic trip points, so the crash path can be exercised on demand instead of by accident.
//! Debug builds only: a release binary contains no trigger at all. See BACKLOG.md M0.4.

/// The environment variable that selects a trip point.
pub const ENV_VAR: &str = "SUBLORE_FORCE_PANIC";

/// Where a forced panic is raised. Each point covers a different thread context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForcePoint {
    /// Tauri setup: the main thread, before the window is usable.
    Startup,
    /// The `video_open` command: an async worker thread, which is the path a user reaches.
    Open,
    /// Inside a closure dispatched to the main thread, where no dialog can appear.
    MainThread,
}

impl ForcePoint {
    /// The environment-variable spelling of this point.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Open => "open",
            Self::MainThread => "main-thread",
        }
    }

    /// Read a `SUBLORE_FORCE_PANIC` value. Anything unrecognised selects nothing, so a typo can
    /// never crash the app.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "startup" => Some(Self::Startup),
            "open" => Some(Self::Open),
            "main-thread" => Some(Self::MainThread),
            _ => None,
        }
    }
}

/// Panic if this point is the one the environment selected.
#[cfg(debug_assertions)]
pub fn trip(point: ForcePoint) {
    use std::sync::OnceLock;

    static SELECTED: OnceLock<Option<ForcePoint>> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        std::env::var(ENV_VAR)
            .ok()
            .and_then(|value| ForcePoint::parse(&value))
    });

    if *selected == Some(point) {
        panic!("forced panic: {ENV_VAR}={}", point.as_str());
    }
}

/// Release builds carry no trigger: the environment variable is never read.
#[cfg(not(debug_assertions))]
#[inline(always)]
pub fn trip(_point: ForcePoint) {}
