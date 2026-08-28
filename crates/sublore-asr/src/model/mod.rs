//! Whisper models: what exists, where it lives, and the one download that may open a socket.
//! See BACKLOG.md M3.2.

pub mod catalog;
pub mod download;
pub mod http;
pub mod store;

pub use catalog::{ModelSpec, CATALOG};
pub use download::{download, Fetched, RangeFetcher};
pub use http::HttpFetcher;
pub use store::{ModelState, ModelStatus, ModelStore};
