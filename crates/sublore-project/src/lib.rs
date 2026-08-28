//! The project database: one SQLite file per project, holding a series, its episodes, and the
//! paths of the files attached to each episode.
//!
//! Paths only. This crate records *where* the user's media and subtitles are; it never copies,
//! moves, rewrites or deletes them. See CLAUDE.md §3 and BACKLOG.md M4.

pub mod db;
pub mod delete;
pub mod error;
pub mod layout;
pub mod migrate;
pub mod model;
pub mod records;

pub use db::Database;
pub use error::{ProjectError, ProjectErrorKind};
pub use layout::{database_path, APPLICATION_ID, CURRENT_VERSION, DATABASE_NAME, OWNED_FILES};
pub use migrate::{migrate, migrate_to};
pub use model::{Episode, EpisodeFile, FileRole, ProjectSummary};
