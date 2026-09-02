//! Licensed under the GNU GPL v3 or later, with the section 7 additional permission for modules
//! loaded through `sublore-module-api`. See LICENSE at the root of the repository.

//! Media in, waveform peaks out: one bucket per millisecond, holding the smallest and the largest
//! sample that fell in it.
//!
//! No Tauri and no async here, for the reason `sublore-asr` has none: process handling in std is
//! blocking, the crate is driven from a blocking task, and keeping the window out of it is what
//! makes every behaviour below testable with `cargo test -p sublore-audio`. Decoding is ffmpeg's
//! job in a child process, so a broken file cannot take the app with it. The shape of that child
//! is `sublore-asr`'s and none of its code (decision 12). See BACKLOG.md M2.4.

pub mod error;
pub mod extract;
pub mod peaks;

pub use error::{AudioError, AudioErrorKind};
pub use extract::{extract_peaks, Cancel, PeakRequest, STALL_TIMEOUT};
pub use peaks::{Bucket, Peaks, CHUNK_BUCKETS, SAMPLES_PER_BUCKET, SAMPLE_RATE};
