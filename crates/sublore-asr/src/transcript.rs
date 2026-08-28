//! What a transcription run produces. Frozen before M3.1 and M3.3 were implemented in parallel:
//! the sidecar fills a `Transcript`, the cue builder reads one, and neither knows the other.
//! See BACKLOG.md M3.1.

/// One spoken word with the timestamps whisper reported for it, in milliseconds from the start of
/// the audio. Sub-word tokens are already joined (see `json.rs`), so this is a word a reader would
/// recognise, not a model token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

/// Which binary ran. Recorded because the two backends do not produce identical text, so a result
/// is only reproducible against the backend that made it (BACKLOG.md M3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Gpu,
    Cpu,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transcript {
    /// The language whisper reported, e.g. `en`. Its own word, not ours.
    pub language: String,
    pub words: Vec<Word>,
    pub backend: Backend,
    /// The user asked for the GPU, the GPU run failed, and the CPU binary produced this instead.
    /// The UI says so: a silent fallback would misreport what the machine did.
    pub fell_back_to_cpu: bool,
    /// Length of the extracted audio, from the WAV header. Cue times are clamped to it.
    pub audio_duration_ms: u32,
}
