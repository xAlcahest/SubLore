//! Durable file writes: replace a file atomically, and keep a backup of what was there before.
//!
//! Separate from `sublore-formats` on purpose: this crate knows nothing about subtitles, and
//! subtitles know nothing about the disk. The public API lands with BACKLOG.md M1.4.

pub mod atomic;
pub mod backup;
pub mod error;
pub mod fault;
