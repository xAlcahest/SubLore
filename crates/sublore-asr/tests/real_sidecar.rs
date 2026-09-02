//! The half of M3.1 and M3.2 that only a real whisper build and a real model can prove.
//!
//! Off by default, so on a machine without them there is no test to skip: there is no test
//! compiled. With `--features sublore-asr/real-asr` the prerequisites are failures with an
//! actionable message, never skips.
//!
//!     sh scripts/build-whisper.sh --cpu-only
//!     cargo test --workspace --features sublore-asr/real-asr
//!
//! See BACKLOG.md M3.1.

/// The announcement. It appears in every default run's output, so the gated suite cannot be
/// forgotten about, and its name is the instruction.
#[cfg(not(feature = "real-asr"))]
#[test]
fn real_asr_suite_not_compiled_run_cargo_test_features_sublore_asr_real_asr() {}

#[cfg(feature = "real-asr")]
mod common;

#[cfg(feature = "real-asr")]
mod real {
    use super::common::{comm_prefix, entries_in, snapshot, Sandbox};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use sublore_asr::error::AsrErrorKind;
    use sublore_asr::model::catalog;
    use sublore_asr::model::http::HttpFetcher;
    use sublore_asr::model::{download, ModelState, ModelStore};
    use sublore_asr::sidecar::{transcribe, Cancel, Compute, Language, Phase, TranscribeRequest};
    use sublore_asr::tools::Tools;
    use sublore_asr::transcript::{Backend, Transcript};

    /// The model the gated suite uses. Small enough to download once and cache.
    const MODEL_ID: &str = "tiny.en";

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("the crate lives two levels under the repo root")
            .to_path_buf()
    }

    /// Where models are cached between runs. The same directory CI keys its cache on.
    fn model_dir() -> PathBuf {
        match std::env::var_os("SUBLORE_TEST_MODEL_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => dirs_cache().join("sublore").join("models"),
        }
    }

    fn dirs_cache() -> PathBuf {
        if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
            return PathBuf::from(dir);
        }
        #[cfg(windows)]
        if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(dir);
        }
        let home = std::env::var_os("HOME").expect("HOME should be set");
        PathBuf::from(home).join(".cache")
    }

    fn whisper_binary() -> PathBuf {
        if let Some(path) = std::env::var_os("SUBLORE_WHISPER_BIN") {
            let path = PathBuf::from(path);
            assert!(
                path.is_file(),
                "SUBLORE_WHISPER_BIN={} is not a file",
                path.display()
            );
            return path;
        }
        let built = repo_root()
            .join(".whisper")
            .join("bin")
            .join(format!("whisper-cli-cpu{}", std::env::consts::EXE_SUFFIX));
        assert!(
            built.is_file(),
            "no whisper binary at {}: run `sh scripts/build-whisper.sh --cpu-only`, or set SUBLORE_WHISPER_BIN",
            built.display()
        );
        built
    }

    /// The model, downloaded through the app's own code the first time and cached after that.
    /// This is also the only test that talks to the internet, and it does so because running it
    /// is the explicit act of asking.
    fn model() -> PathBuf {
        let store = ModelStore::new(model_dir());
        let spec = catalog::find(MODEL_ID).expect("tiny.en is in the catalog");
        let path = download(
            &store,
            spec,
            catalog::BASE_URL,
            &HttpFetcher::new(),
            &Cancel::new(),
            &|_, _| {},
        )
        .unwrap_or_else(|error| {
            panic!(
                "cannot obtain {} in {}: {error}. Put the file there by hand to run offline.",
                spec.file,
                store.dir().display()
            )
        });
        store
            .verify(spec)
            .expect("the downloaded model should hash to its catalogued sha256");
        // The gate a run goes through, not only the download's own check: a genuine model resolves.
        assert_eq!(
            store.resolve(MODEL_ID).expect("a genuine model resolves"),
            path
        );
        path
    }

    /// M3.2's acceptance criterion against a real model: the same file, one bit different, is the
    /// catalogued length and is refused anyway. Measured here, whisper loads a model corrupted this
    /// way without a word of complaint and transcribes nonsense from it, which is why the check
    /// belongs in front of the run rather than after it.
    #[test]
    fn a_real_model_damaged_in_place_is_refused_by_its_checksum() {
        let sandbox = Sandbox::new("real-model-damaged");
        let store = ModelStore::new(sandbox.path("models"));
        let spec = catalog::find(MODEL_ID).expect("tiny.en is in the catalog");
        fs::create_dir_all(store.dir()).expect("the models directory should be creatable");
        fs::copy(model(), store.path(spec)).expect("the model should be copyable");
        assert_eq!(
            store.resolve(MODEL_ID).expect("a genuine copy resolves"),
            store.path(spec)
        );

        let mut bytes = fs::read(store.path(spec)).expect("the copy should be readable");
        bytes[40_000_000] ^= 0x01;
        fs::write(store.path(spec), &bytes).expect("the copy should be writable");

        assert_eq!(
            store.status(spec).state,
            ModelState::Ready,
            "the length is untouched, which is what makes this case need a hash"
        );
        let error = store
            .resolve(MODEL_ID)
            .expect_err("one flipped bit is not the model");
        assert_eq!(error.kind, AsrErrorKind::ChecksumMismatch);
    }

    fn speech_fixture() -> PathBuf {
        let path = repo_root()
            .join("fixtures")
            .join("audio")
            .join("speech-en.wav");
        assert!(
            path.is_file(),
            "no speech fixture at {}: run `sh fixtures/audio/make-speech.sh`",
            path.display()
        );
        path
    }

    fn tools(sandbox: &Sandbox) -> Tools {
        Tools {
            whisper_gpu: None,
            whisper_cpu: whisper_binary(),
            ffmpeg: super::common::ffmpeg(),
            scratch_root: sandbox.path("scratch"),
        }
    }

    fn request(media: PathBuf, model: PathBuf) -> TranscribeRequest {
        let mut request =
            TranscribeRequest::new(media, model, Language::Code("en".to_owned()), Compute::Cpu);
        // Pinned rather than machine-dependent, so two runs here are comparable.
        request.threads = 4;
        request
    }

    fn run(sandbox: &Sandbox, media: PathBuf) -> Result<Transcript, sublore_asr::AsrError> {
        transcribe(
            &tools(sandbox),
            &request(media, model()),
            &Cancel::new(),
            &|_, _| {},
        )
    }

    #[test]
    fn a_real_run_transcribes_the_speech_fixture_end_to_end() {
        let sandbox = Sandbox::new("real-speech");
        let media = speech_fixture();
        let before = snapshot(&media);
        let started = Instant::now();

        let transcript = run(&sandbox, media.clone()).expect("a real transcription");

        let text = transcript
            .words
            .iter()
            .map(|word| word.text.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        // Anchored on words tiny.en gets right; espeak-ng differs between versions and the model
        // heard "Subloor" for the product name, so the whole sentence is not the assertion.
        for anchor in ["terminology", "consistent", "episode", "subtitle", "memory"] {
            assert!(text.contains(anchor), "{anchor:?} missing from {text:?}");
        }
        assert_eq!(transcript.language, "en");
        assert_eq!(transcript.backend, Backend::Cpu);
        assert!(!transcript.fell_back_to_cpu);
        assert!(
            (8_500..9_500).contains(&transcript.audio_duration_ms),
            "the fixture is about nine seconds, got {}",
            transcript.audio_duration_ms
        );

        let mut previous = 0;
        for word in &transcript.words {
            assert!(word.start_ms >= previous, "{word:?} goes backwards");
            assert!(
                word.end_ms >= word.start_ms,
                "{word:?} ends before it starts"
            );
            assert!(
                word.end_ms <= transcript.audio_duration_ms + 1000,
                "{word:?} is past the end of the audio"
            );
            previous = word.end_ms;
        }

        assert_eq!(snapshot(&media), before, "the fixture must be read only");
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "nine seconds of audio should not take two minutes"
        );
    }

    #[test]
    fn the_same_input_twice_produces_the_same_words() {
        let sandbox = Sandbox::new("real-determinism");
        let first = run(&sandbox, speech_fixture()).expect("first run");
        let second = run(&sandbox, speech_fixture()).expect("second run");
        assert_eq!(
            first.words, second.words,
            "same binary, same backend, same model, same audio, same thread count"
        );
    }

    #[test]
    fn progress_arrives_from_the_real_binary() {
        let sandbox = Sandbox::new("real-progress");
        let seen = AtomicU8::new(0);
        let transcript = transcribe(
            &tools(&sandbox),
            &request(speech_fixture(), model()),
            &Cancel::new(),
            &|phase, percent| {
                if phase == Phase::Transcribing {
                    seen.fetch_max(percent, Ordering::SeqCst);
                }
            },
        )
        .expect("a real transcription");
        assert!(!transcript.words.is_empty());
        assert_eq!(
            seen.load(Ordering::SeqCst),
            100,
            "whisper reports 100% at the end of even a short clip"
        );
    }

    #[test]
    fn cancelling_a_real_run_kills_the_real_binary_and_leaves_nothing_behind() {
        let sandbox = Sandbox::new("real-cancel");
        // Three minutes of speech, so there is a run long enough to interrupt.
        let media = repeat_wav(&sandbox, &speech_fixture(), 20);
        let cancel = Cancel::new();
        let tools = tools(&sandbox);

        let watcher = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(1500));
                cancel.cancel();
            })
        };
        let started = Instant::now();
        let error = transcribe(&tools, &request(media, model()), &cancel, &|_, _| {})
            .expect_err("a cancelled run is not a transcript");
        watcher.join().expect("the watcher should finish");

        assert_eq!(error.kind, AsrErrorKind::Cancelled);
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the cancel should end the run promptly"
        );
        assert!(
            !any_whisper_left(&tools),
            "a whisper process outlived the run that owns it"
        );
        assert_eq!(
            entries_in(&tools.scratch_root),
            0,
            "the scratch directory should be gone"
        );
    }

    /// True while any process whose name matches the binary is still in the table. Coarse on
    /// purpose: an orphan from this run and an orphan from a previous one are both failures.
    fn any_whisper_left(tools: &Tools) -> bool {
        let name = tools
            .whisper_cpu
            .file_name()
            .and_then(|name| name.to_str())
            .expect("the binary has a name");
        #[cfg(unix)]
        {
            // `pgrep -x` matches the kernel's truncated `comm`, so the pattern is truncated the
            // same way; a longer binary name would otherwise match nothing. See BACKLOG.md N9, S14.
            let output = std::process::Command::new("pgrep")
                .args(["-x", comm_prefix(name)])
                .output()
                .expect("pgrep should be runnable");
            !String::from_utf8_lossy(&output.stdout).trim().is_empty()
        }
        #[cfg(not(unix))]
        {
            let output = std::process::Command::new("tasklist")
                .args(["/FI", &format!("IMAGENAME eq {name}"), "/NH"])
                .output()
                .expect("tasklist should be runnable on Windows");
            String::from_utf8_lossy(&output.stdout).contains(name)
        }
    }

    #[test]
    fn a_tone_with_no_speech_in_it_runs_and_produces_nothing_to_show() {
        let sample = repo_root()
            .join("fixtures")
            .join("video")
            .join("sample.mkv");
        assert!(
            sample.is_file(),
            "no video fixture at {}: run `sh fixtures/video/make-sample.sh`",
            sample.display()
        );
        let sandbox = Sandbox::new("real-tone");

        // The fixture is a 440 Hz tone, so this proves the media path, never the text.
        match run(&sandbox, sample) {
            Ok(transcript) => assert!(
                transcript.words.len() < 40,
                "a pure tone should not produce a page of words: {:?}",
                transcript.words
            ),
            Err(error) => assert_eq!(error.kind, AsrErrorKind::EmptyTranscript),
        }
    }

    #[test]
    fn a_truncated_model_is_refused_by_length_before_whisper_ever_sees_it() {
        let sandbox = Sandbox::new("real-truncated");
        let spec = catalog::find(MODEL_ID).expect("in the catalog");
        let store = ModelStore::new(sandbox.path("models"));
        fs::create_dir_all(store.dir()).expect("creatable");
        let whole = fs::read(model()).expect("the cached model is readable");
        fs::write(store.path(spec), &whole[..whole.len() / 2]).expect("writable");

        let error = store
            .resolve(MODEL_ID)
            .expect_err("half a model is not a model");
        assert_eq!(error.kind, AsrErrorKind::ModelCorrupt);
    }

    /// Concatenate the fixture's samples `times` over, in the sandbox. A test's own file, never
    /// beside anything of the user's.
    fn repeat_wav(sandbox: &Sandbox, source: &Path, times: usize) -> PathBuf {
        let bytes = fs::read(source).expect("the fixture is readable");
        let header = &bytes[..44];
        let data = &bytes[44..];
        let mut out = header.to_vec();
        for _ in 0..times {
            out.extend_from_slice(data);
        }
        let data_len = (data.len() * times) as u32;
        out[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
        out[40..44].copy_from_slice(&data_len.to_le_bytes());
        let path = sandbox.path("long.wav");
        fs::write(&path, out).expect("the sandbox is writable");
        path
    }
}
