//! Licensed under the GNU GPL v3 or later, with the section 7 additional permission for modules
//! loaded through `sublore-module-api`. See LICENSE at the root of the repository.

//! The whisper.cpp sidecar: run it, watch it, cancel it, and turn what it wrote into cues.
//!
//! No Tauri and no async here. Process handling in std is blocking, the crate is driven from a
//! blocking task, and keeping the window out of it is what makes every behaviour below testable
//! with `cargo test -p sublore-asr`. Speech recognition itself is whisper.cpp's job in a child
//! process: nothing is linked in, so a whisper crash cannot take the app with it (CONTRIBUTING.md §2).
//! See BACKLOG.md M3.

pub mod cues;
pub mod error;
pub mod json;
pub mod model;
pub mod progress;
pub mod render;
pub mod scratch;
pub mod sidecar;
pub mod tools;
pub mod transcript;

pub use error::{AsrError, AsrErrorKind};
pub use sidecar::{transcribe, Cancel, Compute, Language, Phase, TranscribeRequest};
pub use tools::Tools;
pub use transcript::{Backend, Transcript, Word};
