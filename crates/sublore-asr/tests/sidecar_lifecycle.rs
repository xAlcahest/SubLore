//! M3.1 acceptance, against a fake binary: a run streams progress and produces words; cancelling
//! mid-run kills the child and leaves no process behind; a missing or unrunnable binary is a
//! readable error and never a crash; the user's media is never written to.
//!
//! No whisper build, no model, no network, no display. See BACKLOG.md M3.1.

mod common;

use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use common::{fake_whisper, process_present, snapshot, Sandbox};
use sublore_asr::error::AsrErrorKind;
use sublore_asr::sidecar::{transcribe, Cancel, Compute, Language, Phase, TranscribeRequest};
use sublore_asr::tools::Tools;
use sublore_asr::transcript::Backend;

/// Two words, one segment, in the shape whisper's `-ojf` actually writes.
const TRANSCRIPT_JSON: &str = r#"{
  "result": { "language": "en" },
  "transcription": [
    {
      "offsets": { "from": 0, "to": 900 },
      "text": " hello there",
      "tokens": [
        { "text": "[_BEG_]", "offsets": { "from": 0, "to": 0 } },
        { "text": " hel", "offsets": { "from": 20, "to": 300 } },
        { "text": "lo", "offsets": { "from": 300, "to": 420 } },
        { "text": " there", "offsets": { "from": 500, "to": 900 } }
      ]
    }
  ]
}"#;

fn request(media: std::path::PathBuf, script: std::path::PathBuf) -> TranscribeRequest {
    let mut request =
        TranscribeRequest::new(media, script, Language::Code("en".to_owned()), Compute::Cpu);
    // The tests must not wait five minutes to see the real timer fire.
    request.stall = Duration::from_millis(600);
    request
}

/// Collects what the UI would have been told.
#[derive(Default)]
struct Reported {
    steps: Mutex<Vec<(Phase, u8)>>,
}

impl Reported {
    fn sink(&self) -> impl Fn(Phase, u8) + Sync + '_ {
        |phase, percent| {
            self.steps
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push((phase, percent))
        }
    }

    fn percentages(&self) -> Vec<u8> {
        self.steps
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|(phase, _)| *phase == Phase::Transcribing)
            .map(|(_, percent)| *percent)
            .collect()
    }
}

#[test]
fn a_run_streams_progress_and_produces_the_words_whisper_wrote() {
    let sandbox = Sandbox::new("happy");
    let media = sandbox.silence("media.wav", 1000);
    let json = sandbox.path("transcript.json");
    fs::write(&json, TRANSCRIPT_JSON).expect("writable");
    let script = sandbox.script(
        "script",
        &[
            "progress 25",
            "segment 250 hello",
            "progress 60",
            "progress 100",
            &format!("json {}", json.display()),
            "exit 0",
        ],
    );
    let reported = Reported::default();

    let transcript = transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &reported.sink(),
    )
    .expect("the run should produce a transcript");

    assert_eq!(transcript.language, "en");
    assert_eq!(
        transcript
            .words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>(),
        vec!["hello", "there"]
    );
    assert_eq!(transcript.words[0].start_ms, 20);
    assert_eq!(transcript.words[1].end_ms, 900);
    assert_eq!(transcript.backend, Backend::Cpu);
    assert!(!transcript.fell_back_to_cpu);
    assert_eq!(transcript.audio_duration_ms, 1000);
    assert_eq!(reported.percentages(), vec![0, 25, 60, 100]);
}

#[test]
fn progress_never_goes_backwards_even_when_the_two_pipes_disagree() {
    let sandbox = Sandbox::new("monotone");
    let media = sandbox.silence("media.wav", 1000);
    let json = sandbox.path("transcript.json");
    fs::write(&json, TRANSCRIPT_JSON).expect("writable");
    // 800 ms of a 1000 ms clip is 80%, reported before whisper's own counter catches up; the
    // counter then reports less, and the UI must not be told to go back.
    let script = sandbox.script(
        "script",
        &[
            "progress 20",
            "sleep 20",
            "segment 800 most of it",
            "sleep 20",
            "progress 40",
            "sleep 20",
            "progress 100",
            &format!("json {}", json.display()),
        ],
    );
    let reported = Reported::default();

    transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &reported.sink(),
    )
    .expect("the run should succeed");

    let percentages = reported.percentages();
    assert_eq!(percentages, vec![0, 20, 80, 100], "got {percentages:?}");
}

#[test]
fn cancelling_mid_run_kills_the_child_and_leaves_no_process_behind() {
    let sandbox = Sandbox::new("cancel");
    let media = sandbox.silence("media.wav", 1000);
    let pid_file = sandbox.path("child.pid");
    let script = sandbox.script(
        "script",
        &[
            &format!("pid {}", pid_file.display()),
            "progress 5",
            "sleep-forever",
        ],
    );
    let cancel = Cancel::new();
    let started = Arc::new(AtomicU32::new(0));

    let watcher = {
        let cancel = cancel.clone();
        let started = Arc::clone(&started);
        let pid_file = pid_file.clone();
        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if let Ok(text) = fs::read_to_string(&pid_file) {
                    if let Ok(pid) = text.trim().parse::<u32>() {
                        started.store(pid, Ordering::SeqCst);
                        cancel.cancel();
                        return;
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
        })
    };

    let mut request = request(media, script);
    // A run this test would otherwise sit in forever: the stall timer must not be what ends it.
    request.stall = Duration::from_secs(60);
    let error = transcribe(&sandbox.tools(), &request, &cancel, &|_, _| {})
        .expect_err("a cancelled run is not a transcript");
    watcher.join().expect("the watcher thread should finish");

    assert_eq!(error.kind, AsrErrorKind::Cancelled);
    let pid = started.load(Ordering::SeqCst);
    assert_ne!(pid, 0, "the child should have written its pid");
    assert!(
        !process_present(pid, "fake_whisper"),
        "pid {pid} is still in the process table: killed but not reaped is still a survivor"
    );
}

#[test]
fn cancelling_before_the_run_starts_spawns_nothing() {
    let sandbox = Sandbox::new("cancel-early");
    let media = sandbox.silence("media.wav", 1000);
    let marker = sandbox.path("ran");
    let script = sandbox.script("script", &[&format!("pid {}", marker.display())]);
    let cancel = Cancel::new();
    cancel.cancel();

    let error = transcribe(
        &sandbox.tools(),
        &request(media, script),
        &cancel,
        &|_, _| {},
    )
    .expect_err("a cancelled run is not a transcript");

    assert_eq!(error.kind, AsrErrorKind::Cancelled);
    assert!(!marker.exists(), "no child should have been started");
}

#[test]
fn a_child_that_says_nothing_is_killed_by_the_stall_timer() {
    let sandbox = Sandbox::new("stall");
    let media = sandbox.silence("media.wav", 1000);
    let pid_file = sandbox.path("child.pid");
    let script = sandbox.script(
        "script",
        &[&format!("pid {}", pid_file.display()), "sleep-forever"],
    );

    let started = Instant::now();
    let error = transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("a silent child is not a transcript");

    assert_eq!(error.kind, AsrErrorKind::Stalled);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the timer should fire on its own"
    );
    let pid: u32 = fs::read_to_string(&pid_file)
        .expect("the child wrote its pid")
        .trim()
        .parse()
        .expect("a pid");
    assert!(
        !process_present(pid, "fake_whisper"),
        "pid {pid} outlived the run"
    );
}

#[test]
fn exit_zero_with_no_json_is_a_failure_because_the_exit_code_is_not_the_verdict() {
    let sandbox = Sandbox::new("no-json");
    let media = sandbox.silence("media.wav", 1000);
    let script = sandbox.script("script", &["progress 100", "exit 0"]);

    let error = transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("no JSON means no result, whatever the exit code says");

    assert_eq!(error.kind, AsrErrorKind::NoOutput);
    assert!(error.detail.contains("out.json"), "{}", error.detail);
}

#[test]
fn a_model_whisper_will_not_load_is_told_apart_from_a_missing_one() {
    let sandbox = Sandbox::new("model-rejected");
    let media = sandbox.silence("media.wav", 1000);
    let script = sandbox.script(
        "script",
        &[
            "noise error: failed to initialize whisper context",
            "exit 3",
        ],
    );

    let error = transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("exit 3 is a rejected model");

    assert_eq!(error.kind, AsrErrorKind::ModelRejected);
    assert!(error.detail.contains("initialize"), "{}", error.detail);
}

#[test]
fn an_unopenable_audio_file_is_told_apart_from_a_rejected_model() {
    let sandbox = Sandbox::new("no-input");
    let media = sandbox.silence("media.wav", 1000);
    let script = sandbox.script("script", &["exit 2"]);

    let error = transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("exit 2 is an unreadable input");

    assert_eq!(error.kind, AsrErrorKind::NoInput);
}

#[test]
fn a_flag_whisper_does_not_know_is_caught_even_though_it_exits_zero() {
    let sandbox = Sandbox::new("bad-argument");
    let media = sandbox.silence("media.wav", 1000);
    let script = sandbox.script("script", &["unknown-argument", "exit 0"]);

    let error = transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("our own bad flag is not a transcript");

    assert_eq!(error.kind, AsrErrorKind::BadArguments);
}

#[test]
fn five_megabytes_of_stderr_do_not_deadlock_the_run() {
    let sandbox = Sandbox::new("flood");
    let media = sandbox.silence("media.wav", 1000);
    let json = sandbox.path("transcript.json");
    fs::write(&json, TRANSCRIPT_JSON).expect("writable");
    let script = sandbox.script(
        "script",
        &[
            "stderr-flood 5242880",
            "progress 100",
            &format!("json {}", json.display()),
        ],
    );

    let mut request = request(media, script);
    request.stall = Duration::from_secs(60);
    let transcript = transcribe(&sandbox.tools(), &request, &Cancel::new(), &|_, _| {})
        .expect("a chatty child still finishes");

    assert_eq!(transcript.words.len(), 2);
}

#[test]
fn silence_is_reported_as_an_empty_transcript_rather_than_a_broken_run() {
    let sandbox = Sandbox::new("empty");
    let media = sandbox.silence("media.wav", 1000);
    let json = sandbox.path("transcript.json");
    fs::write(&json, r#"{"result":{"language":"en"},"transcription":[]}"#).expect("writable");
    let script = sandbox.script("script", &[&format!("json {}", json.display())]);

    let error = transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("no words is its own outcome");

    assert_eq!(error.kind, AsrErrorKind::EmptyTranscript);
}

#[test]
fn the_scratch_directory_is_gone_on_every_path_out_of_a_run() {
    let sandbox = Sandbox::new("scratch");
    let media = sandbox.silence("media.wav", 1000);
    let json = sandbox.path("transcript.json");
    fs::write(&json, TRANSCRIPT_JSON).expect("writable");
    let tools = sandbox.tools();

    let good = sandbox.script("good", &[&format!("json {}", json.display())]);
    transcribe(
        &tools,
        &request(media.clone(), good),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect("a run");
    assert_eq!(scratch_children(&tools), 0, "after a success");

    let bad = sandbox.script("bad", &["exit 0"]);
    transcribe(
        &tools,
        &request(media.clone(), bad),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("no JSON");
    assert_eq!(scratch_children(&tools), 0, "after a failure");

    let cancel = Cancel::new();
    cancel.cancel();
    let stuck = sandbox.script("stuck", &["sleep-forever"]);
    transcribe(&tools, &request(media, stuck), &cancel, &|_, _| {}).expect_err("cancelled");
    assert_eq!(scratch_children(&tools), 0, "after a cancel");
}

fn scratch_children(tools: &Tools) -> usize {
    fs::read_dir(&tools.scratch_root)
        .map(|entries| entries.count())
        .unwrap_or(0)
}

#[test]
fn the_users_media_is_read_and_never_written() {
    let sandbox = Sandbox::new("read-only");
    // In its own directory, so the assertion is the strong one: nothing at all is created beside
    // the user's file, not even a temporary.
    fs::create_dir_all(sandbox.path("library")).expect("creatable");
    let media = sandbox.silence("library/episode.mkv", 1000);
    let json = sandbox.path("transcript.json");
    fs::write(&json, TRANSCRIPT_JSON).expect("writable");
    let script = sandbox.script("script", &[&format!("json {}", json.display())]);
    let before = snapshot(&media);

    transcribe(
        &sandbox.tools(),
        &request(media.clone(), script),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect("a run");

    assert_eq!(
        snapshot(&media),
        before,
        "the media file and its directory must come out of a run untouched"
    );
}

#[test]
fn a_missing_binary_is_a_readable_error_and_never_a_crash() {
    let sandbox = Sandbox::new("missing-binary");
    let media = sandbox.silence("media.wav", 1000);
    let script = sandbox.script("script", &["exit 0"]);
    let tools = Tools {
        whisper_gpu: None,
        whisper_cpu: sandbox.path("not-installed"),
        ffmpeg: common::ffmpeg(),
        scratch_root: sandbox.path("scratch"),
    };

    let error = transcribe(&tools, &request(media, script), &Cancel::new(), &|_, _| {})
        .expect_err("there is no binary to run");

    assert_eq!(error.kind, AsrErrorKind::BinaryUnrunnable);
    assert!(error.detail.contains("not-installed"), "{}", error.detail);
}

#[test]
fn a_missing_ffmpeg_is_named_as_such_rather_than_as_a_whisper_failure() {
    let sandbox = Sandbox::new("missing-ffmpeg");
    let media = sandbox.silence("media.wav", 1000);
    let script = sandbox.script("script", &["exit 0"]);
    let tools = Tools {
        whisper_gpu: None,
        whisper_cpu: fake_whisper(),
        ffmpeg: sandbox.path("no-ffmpeg-here"),
        scratch_root: sandbox.path("scratch"),
    };

    let error = transcribe(&tools, &request(media, script), &Cancel::new(), &|_, _| {})
        .expect_err("there is no ffmpeg to run");

    assert_eq!(error.kind, AsrErrorKind::FfmpegMissing);
}

#[test]
fn a_media_file_ffmpeg_cannot_decode_is_unreadable_media() {
    let sandbox = Sandbox::new("bad-media");
    let media = sandbox.path("episode.mkv");
    fs::write(&media, b"this is not a container").expect("writable");
    let script = sandbox.script("script", &["exit 0"]);

    let error = transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("ffmpeg cannot decode it");

    assert_eq!(error.kind, AsrErrorKind::MediaUnreadable);
}

#[test]
fn a_vulkan_binary_that_will_not_run_falls_back_to_the_cpu_once() {
    let sandbox = Sandbox::new("fallback");
    let media = sandbox.silence("media.wav", 1000);
    let json = sandbox.path("transcript.json");
    fs::write(&json, TRANSCRIPT_JSON).expect("writable");
    let script = sandbox.script("script", &[&format!("json {}", json.display())]);
    // What a missing libvulkan.so.1 looks like from here: the GPU binary is there in the config
    // and cannot be started.
    let tools = sandbox.tools_with(sandbox.path("broken-vulkan"), fake_whisper());

    let mut request = request(media, script);
    request.compute = Compute::Gpu;
    let transcript = transcribe(&tools, &request, &Cancel::new(), &|_, _| {})
        .expect("the CPU retry should carry the run");

    assert_eq!(transcript.backend, Backend::Cpu);
    assert!(
        transcript.fell_back_to_cpu,
        "the UI has to be able to say so"
    );
    assert_eq!(transcript.words.len(), 2);
}

#[test]
fn a_cancelled_gpu_run_is_not_retried_on_the_cpu() {
    let sandbox = Sandbox::new("no-retry-on-cancel");
    let media = sandbox.silence("media.wav", 1000);
    let pid_file = sandbox.path("child.pid");
    let script = sandbox.script(
        "script",
        &[&format!("pid {}", pid_file.display()), "sleep-forever"],
    );
    let cancel = Cancel::new();
    let watcher = {
        let cancel = cancel.clone();
        let pid_file = pid_file.clone();
        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if pid_file.exists() {
                    cancel.cancel();
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
        })
    };

    let mut request = request(media, script);
    request.compute = Compute::Gpu;
    request.stall = Duration::from_secs(60);
    let started = Instant::now();
    let error = transcribe(&sandbox.tools(), &request, &cancel, &|_, _| {})
        .expect_err("the user asked for it to stop");
    watcher.join().expect("the watcher thread should finish");

    assert_eq!(error.kind, AsrErrorKind::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "a cancel must not be followed by a second run the user did not ask for"
    );
}

#[test]
fn asking_for_the_gpu_without_a_vulkan_build_runs_on_the_cpu_and_admits_it() {
    let sandbox = Sandbox::new("no-vulkan");
    let media = sandbox.silence("media.wav", 1000);
    let json = sandbox.path("transcript.json");
    fs::write(&json, TRANSCRIPT_JSON).expect("writable");
    let script = sandbox.script("script", &[&format!("json {}", json.display())]);
    let tools = Tools {
        whisper_gpu: None,
        whisper_cpu: fake_whisper(),
        ffmpeg: common::ffmpeg(),
        scratch_root: sandbox.path("scratch"),
    };

    let mut request = request(media, script);
    request.compute = Compute::Gpu;
    let transcript = transcribe(&tools, &request, &Cancel::new(), &|_, _| {}).expect("a run");

    assert_eq!(transcript.backend, Backend::Cpu);
    assert!(
        transcript.fell_back_to_cpu,
        "a silent fallback would misreport what the machine did"
    );
}

#[test]
fn the_cpu_run_passes_no_gpu_so_the_choice_is_not_only_a_file_name() {
    let sandbox = Sandbox::new("flags");
    let media = sandbox.silence("media.wav", 1000);
    let argv = sandbox.path("argv.txt");
    let script = sandbox.script("script", &[&format!("argv {}", argv.display())]);

    transcribe(
        &sandbox.tools(),
        &request(media, script),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("the script writes no JSON");

    let recorded = fs::read_to_string(&argv).expect("the child recorded its arguments");
    let arguments: Vec<&str> = recorded.lines().collect();
    assert!(arguments.contains(&"-ng"), "{arguments:?}");
    assert!(arguments.contains(&"-ojf"), "{arguments:?}");
    assert!(arguments.contains(&"-pp"), "{arguments:?}");
    assert!(arguments.contains(&"-l"), "{arguments:?}");
    assert!(
        !arguments.iter().any(|argument| argument.contains("-dtw")),
        "forced alignment is post-1.0: {arguments:?}"
    );
}
