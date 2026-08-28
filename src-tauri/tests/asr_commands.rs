//! What the transcription commands hand the UI, driven through their bodies rather than through
//! IPC: the async wrappers add a `spawn_blocking` and nothing else. The E2E spec drives the app;
//! this covers what a GUI test cannot see cheaply. See BACKLOG.md M3.4.
//!
//! Nothing here spawns whisper, opens a socket or needs a model: the transcript is the committed
//! capture of a real whisper run, which is the same one `e2e/tools/whisper-stub.mjs` replays, so
//! the two layers are asserting against the same bytes.

use std::fs;
use std::path::{Path, PathBuf};

use sublore_asr::json::parse_transcript;
use sublore_asr::model::{catalog, ModelStore};
use sublore_asr::transcript::{Backend, Transcript};
use sublore_lib::asr::{done_payload, statuses, AsrCue};

/// The fixture the E2E spec transcribes is 60 s long; cue times are clamped to it.
const AUDIO_MS: u32 = 60_000;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

/// A scratch directory that removes itself, so a failed assertion never leaves litter behind.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sublore-m34-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch directory");
        Self { path }
    }

    /// A file of exactly `bytes` length, sparse: the model listing reads lengths and nothing else.
    fn sized(&self, name: &str, bytes: u64) {
        let file = fs::File::create(self.path.join(name)).expect("creatable");
        file.set_len(bytes).expect("sizeable");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn captured_transcript() -> Transcript {
    let path = repo_root().join("fixtures/asr/whisper-tiny-en.json");
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let parsed = parse_transcript(&bytes).expect("the committed capture parses");
    Transcript {
        language: parsed.language,
        words: parsed.words,
        backend: Backend::Cpu,
        fell_back_to_cpu: false,
        audio_duration_ms: AUDIO_MS,
    }
}

fn spoken(cues: &[AsrCue]) -> String {
    cues.iter()
        .flat_map(|cue| cue.lines.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_real_whisper_capture_becomes_cues_that_lose_no_words() {
    let transcript = captured_transcript();
    let spoken_words = transcript
        .words
        .iter()
        .map(|word| word.text.clone())
        .collect::<Vec<_>>()
        .join(" ");

    let done = done_payload(1, transcript).expect("the capture segments and parses");

    assert!(done.cues.len() > 1, "8.5 s of speech is more than one cue");
    // Every word, in order, exactly once: segmentation splits, it never rewrites or drops.
    assert_eq!(spoken(&done.cues), spoken_words);
    assert!(spoken(&done.cues).contains("terminology"));
}

#[test]
fn every_generated_cue_is_ordered_and_inside_the_audio() {
    let done = done_payload(1, captured_transcript()).expect("the capture segments and parses");

    let mut previous_end = 0;
    for cue in &done.cues {
        assert!(
            cue.start_ms >= previous_end,
            "{cue:?} overlaps its neighbour"
        );
        assert!(cue.end_ms > cue.start_ms, "{cue:?} is empty or inverted");
        assert!(cue.end_ms <= AUDIO_MS, "{cue:?} runs past the audio");
        assert!(!cue.lines.is_empty());
        previous_end = cue.end_ms;
    }
}

#[test]
fn the_same_transcript_always_produces_the_same_cues() {
    let first = done_payload(1, captured_transcript()).expect("segments");
    let second = done_payload(2, captured_transcript()).expect("segments");
    assert_eq!(first.cues, second.cues);
}

#[test]
fn the_model_list_says_what_is_on_disk_and_never_looks_further() {
    let scratch = Scratch::new("models");
    let tiny = catalog::find("tiny.en").expect("tiny.en is in the catalog");
    let base = catalog::find("base.en").expect("base.en is in the catalog");
    let small = catalog::find("small.en").expect("small.en is in the catalog");

    scratch.sized(tiny.file, tiny.bytes);
    // Half a file: whisper exits 0 on a truncated model, so the length is what refuses it.
    scratch.sized(base.file, base.bytes / 2);
    scratch.sized(&format!("{}.part", small.file), 4096);

    let listed = statuses(&ModelStore::new(scratch.path.clone()));
    assert_eq!(listed.len(), catalog::CATALOG.len(), "the whole catalog");

    let row = |id: &str| {
        listed
            .iter()
            .find(|status| status.id == id)
            .unwrap_or_else(|| panic!("{id} is missing from the listing"))
    };
    assert_eq!(row("tiny.en").state, "ready");
    assert_eq!(row("tiny.en").downloaded_bytes, tiny.bytes);
    assert_eq!(row("tiny.en").bytes, tiny.bytes);
    assert_eq!(row("base.en").state, "corrupt");
    assert_eq!(row("small.en").state, "partial");
    assert_eq!(row("small.en").downloaded_bytes, 4096);
    assert_eq!(row("large-v3").state, "missing");
    assert_eq!(row("large-v3").downloaded_bytes, 0);
}
