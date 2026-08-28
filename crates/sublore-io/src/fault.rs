//! Debug-only crash injection, so the save path can be interrupted on demand. BACKLOG.md M1.4.
//!
//! Same idiom as `src-tauri/src/crash/force.rs`: one environment variable, read once, and no
//! trigger at all in a release binary. `abort()` rather than `exit()`, so nothing unwinds and
//! nothing is flushed, which is what a real kill looks like.

/// The environment variable that selects a trip point.
pub const ENV_VAR: &str = "SUBLORE_IO_FAULT";

/// Where a save can be interrupted. Each point leaves the disk in a different state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultPoint {
    /// Backup written, destination untouched.
    AfterBackup,
    /// An empty temp file exists.
    AfterTempCreated,
    /// Half the bytes are in the temp file.
    DuringWrite,
    /// All bytes written, not synced.
    AfterWrite,
    /// Synced, not renamed.
    AfterSync,
    /// Renamed, before the directory is synced.
    AfterRename,
}

impl FaultPoint {
    /// The environment-variable spelling of this point.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AfterBackup => "after-backup",
            Self::AfterTempCreated => "after-temp-created",
            Self::DuringWrite => "during-write",
            Self::AfterWrite => "after-write",
            Self::AfterSync => "after-sync",
            Self::AfterRename => "after-rename",
        }
    }

    /// Read a [`ENV_VAR`] value. Anything unrecognised selects nothing, so a typo can never
    /// interrupt a save.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "after-backup" => Some(Self::AfterBackup),
            "after-temp-created" => Some(Self::AfterTempCreated),
            "during-write" => Some(Self::DuringWrite),
            "after-write" => Some(Self::AfterWrite),
            "after-sync" => Some(Self::AfterSync),
            "after-rename" => Some(Self::AfterRename),
            _ => None,
        }
    }
}

/// End the process here if this point is the one the environment selected.
#[cfg(debug_assertions)]
pub fn trip(point: FaultPoint) {
    if armed(point) {
        std::process::abort();
    }
}

/// Release builds carry no trigger: the environment variable is never read.
#[cfg(not(debug_assertions))]
#[inline(always)]
pub fn trip(_point: FaultPoint) {}

/// True when this point is armed, so the writer can split its write in two.
#[cfg(debug_assertions)]
pub(crate) fn armed(point: FaultPoint) -> bool {
    use std::sync::OnceLock;

    static SELECTED: OnceLock<Option<FaultPoint>> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        std::env::var(ENV_VAR)
            .ok()
            .and_then(|value| FaultPoint::parse(&value))
    });
    *selected == Some(point)
}

#[cfg(test)]
mod tests {
    use super::FaultPoint;

    #[test]
    fn every_point_survives_its_own_spelling() {
        for point in [
            FaultPoint::AfterBackup,
            FaultPoint::AfterTempCreated,
            FaultPoint::DuringWrite,
            FaultPoint::AfterWrite,
            FaultPoint::AfterSync,
            FaultPoint::AfterRename,
        ] {
            assert_eq!(FaultPoint::parse(point.as_str()), Some(point));
        }
    }

    #[test]
    fn nothing_else_arms_a_point() {
        for value in ["", " ", "after backup", "AfterBackup", "during-write "] {
            assert_eq!(FaultPoint::parse(value), None, "{value:?} must arm nothing");
        }
    }
}
