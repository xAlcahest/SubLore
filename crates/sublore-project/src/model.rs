//! What a project holds: a series, its episodes, and the files attached to each one. BACKLOG.md M4.
//!
//! No serde here. The IPC payloads live beside the Tauri commands, the way `SubtitleSummary` does.

use std::path::PathBuf;

/// What an attached file is to the episode. The three the editor and the player already need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileRole {
    Media,
    Source,
    Target,
}

impl FileRole {
    /// The column and wire spelling. Stable: it is written into every user's database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Source => "source",
            Self::Target => "target",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "media" => Some(Self::Media),
            "source" => Some(Self::Source),
            "target" => Some(Self::Target),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Episode {
    pub id: i64,
    /// Position in the series, 1-based.
    pub ordinal: u32,
    pub title: String,
    /// Unix seconds, UTC.
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodeFile {
    pub id: i64,
    pub episode_id: i64,
    pub role: FileRole,
    /// Absolute, exactly as the user's OS spells it. Never canonicalised, never rewritten.
    pub path: PathBuf,
    /// `None` when the file could not be read when it was attached.
    pub byte_length: Option<u64>,
    /// Unix seconds, UTC. `None` when unknown.
    pub modified_at: Option<i64>,
    pub added_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSummary {
    pub folder: PathBuf,
    pub database: PathBuf,
    pub title: String,
    pub schema_version: u32,
    pub episode_count: usize,
}

#[cfg(test)]
mod tests {
    use super::FileRole;

    #[test]
    fn every_role_survives_a_round_trip_through_its_column_spelling() {
        for role in [FileRole::Media, FileRole::Source, FileRole::Target] {
            assert_eq!(FileRole::parse(role.as_str()), Some(role));
        }
        assert_eq!(FileRole::parse("reference"), None);
        assert_eq!(FileRole::parse("Media"), None);
        assert_eq!(FileRole::parse(""), None);
    }
}
