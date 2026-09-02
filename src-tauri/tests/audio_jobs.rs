//! What a waveform job does over its life, driven through the job layer rather than through IPC:
//! the async command wrappers add a `spawn_blocking` and nothing else. See BACKLOG.md M2.4, W4.
//!
//! A stand-in stands where ffmpeg does, for the reason `crates/sublore-audio/tests/child.rs` uses
//! one: these are assertions about the job and the process, not about decoding, so they must fail
//! for their own reason on a machine where ffmpeg is missing rather than be skipped by it. The
//! stand-in writes bytes the test chose, so the samples that come back out of the payload are
//! checkable against a number.
//!
//! Linux only, and deliberately: the survivor assertions read `/proc`, and Linux is where this
//! project's behaviour is proved (CONTRIBUTING.md §5.5). The waveform criteria that need a real
//! window belong to W5's E2E; these are the same behaviours at the layer that exists today.
#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use sublore_audio::{Cancel, PeakRequest, CHUNK_BUCKETS, SAMPLES_PER_BUCKET};
use sublore_lib::audio::error::AudioErrorCode;
use sublore_lib::audio::{run_job, AudioEvent, AudioState, JobTarget};

/// One test at a time. Writing a stand-in while another thread forks hands that thread's write
/// handle to the child, and the exec that follows fails with ETXTBSY; the tests are milliseconds
/// long, so serialising them costs nothing. The shape `sublore-audio`'s own child tests use.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn alone() -> MutexGuard<'static, ()> {
    // A test that panicked poisons the lock; the ones after it still have their own work to do.
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// A directory of this test file's own, cleaned between runs.
fn workspace(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("sublore-w4-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("the test directory should be creatable");
    root
}

/// An executable shell script standing in for ffmpeg. It ignores the arguments it is given: what
/// is under test is what Sublore does with the child, not what ffmpeg does with a file.
fn stand_in(root: &Path, body: &str) -> PathBuf {
    let path = root.join("ffmpeg-stand-in");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("the stand-in should be writable");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .expect("the stand-in should be executable");
    path
}

/// A file for the request to point at. Its contents never reach the stand-in.
fn media(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, b"not really media").expect("the placeholder should be writable");
    path
}

/// The low sample of every pair the stand-in writes.
const QUIET: i16 = 0;
/// The high one. Not full scale and not symmetric with `QUIET`, so a payload that swapped the two
/// lanes, or read the bytes the other way round, reads a different pair of numbers.
const LOUD: i16 = 16_384;

/// Exactly `buckets` milliseconds of `s16le`, alternating [`QUIET`] and [`LOUD`], on disk for the
/// stand-in to write to its pipe. Deterministic in length and in content, which is what lets an
/// assertion name both.
fn samples(root: &Path, buckets: usize) -> PathBuf {
    let mut bytes = Vec::with_capacity(buckets * SAMPLES_PER_BUCKET * 2);
    for index in 0..buckets * SAMPLES_PER_BUCKET {
        let value = if index % 2 == 0 { QUIET } else { LOUD };
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let path = root.join("samples.raw");
    fs::write(&path, &bytes).expect("the sample file should be writable");
    path
}

/// A stand-in that writes for long enough to be cancelled mid-run, and then stops on its own.
/// 96 MB of `s16le` is a thousand seconds of audio, which takes seconds to fold on any machine and
/// finishes on every one of them: a cancel that never arrives ends this job in `done` and fails
/// the test that expected `cancelled`, rather than hanging the run. `exec` keeps the pid the shell
/// reported, so the process a test looks for afterwards is the one Sublore spawned.
const LONG_WRITER: &str = "exec head -c 96000000 /dev/zero";

/// The pid the stand-in wrote before it exec'd, once it exists.
fn wait_for_pid(path: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(text) = fs::read_to_string(path) {
            if let Ok(pid) = text.trim().parse() {
                return pid;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the stand-in never reported its pid");
}

/// True while `pid` is anything at all to the kernel: running, sleeping, or a zombie nobody
/// waited for. A pid already handed to another process reads as present, which is the answer that
/// fails this check honestly rather than the one that passes it.
fn still_there(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// Everything one job handed back, in order.
#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<AudioEvent>>,
}

impl Recorder {
    fn record(&self, event: AudioEvent) {
        self.events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(event);
    }

    fn taken(self) -> Vec<AudioEvent> {
        self.events
            .into_inner()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn chunks_seen(&self) -> usize {
        self.events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|event| matches!(event, AudioEvent::Peaks(_)))
            .count()
    }

    /// Block until the job has streamed something, so what happens next happens mid-run rather
    /// than at a moment the machine chose. A job that only hands its peaks over at the end never
    /// gets here, which is the point: that is the regression the streaming exists to prevent.
    fn wait_for_a_chunk(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.chunks_seen() == 0 {
            assert!(
                Instant::now() < deadline,
                "the job never streamed a chunk, so nothing below happens while it is running"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn recorder_chunks(events: &[AudioEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, AudioEvent::Peaks(_)))
        .count()
}

/// One `audio://peaks` payload, unwrapped.
struct Chunk {
    first_ms: u32,
    min: Vec<i16>,
    max: Vec<i16>,
}

/// The chunks, and the one terminal event. Panics unless there is exactly one terminal event and
/// it is last, which is the shape every job promises.
fn split(events: Vec<AudioEvent>) -> (Vec<Chunk>, AudioEvent) {
    let terminal: Vec<_> = events
        .iter()
        .filter(|event| !matches!(event, AudioEvent::Peaks(_)))
        .cloned()
        .collect();
    assert_eq!(
        terminal.len(),
        1,
        "a job ends in exactly one terminal event, got {terminal:?}"
    );
    assert!(
        matches!(events.last(), Some(event) if !matches!(event, AudioEvent::Peaks(_))),
        "the terminal event is the last one a job hands over"
    );
    let chunks = events
        .into_iter()
        .filter_map(|event| match event {
            AudioEvent::Peaks(peaks) => Some(Chunk {
                first_ms: peaks.first_ms,
                min: peaks.min,
                max: peaks.max,
            }),
            _ => None,
        })
        .collect();
    (
        chunks,
        terminal.into_iter().next().expect("one terminal event"),
    )
}

/// How many buckets the chunks carry, after proving they tile the timeline: each one starts where
/// the last one ended, so there is no gap and no overlap anywhere in the run.
fn covered(chunks: &[Chunk], job_id: u64, events: &[AudioEvent]) -> u32 {
    let mut next = 0u32;
    for chunk in chunks {
        assert_eq!(
            chunk.first_ms, next,
            "a chunk started at {} ms with {next} ms already covered: gap or overlap",
            chunk.first_ms
        );
        assert_eq!(
            chunk.min.len(),
            chunk.max.len(),
            "a chunk's two lanes are the same length"
        );
        assert!(!chunk.min.is_empty(), "an empty chunk is not a chunk");
        next += chunk.min.len() as u32;
    }
    for event in events {
        if let AudioEvent::Peaks(peaks) = event {
            assert_eq!(peaks.job_id, job_id, "every chunk carries its own job's id");
        }
    }
    next
}

#[test]
fn a_job_streams_its_chunks_and_then_exactly_one_done_that_counts_them() {
    let _alone = alone();
    let root = workspace("streamed");
    // Not a multiple of a chunk, so the last one is short and the tail is covered too.
    const BUCKETS: usize = CHUNK_BUCKETS * 3 + 137;
    let ffmpeg = stand_in(&root, &format!("cat {}", samples(&root, BUCKETS).display()));

    let recorder = Recorder::default();
    run_job(
        &ffmpeg,
        7,
        &PeakRequest::new(media(&root, "ep01.mkv"), 1),
        &Cancel::new(),
        &|event| recorder.record(event),
    );

    let events = recorder.taken();
    let (chunks, terminal) = split(events.clone());
    assert_eq!(
        covered(&chunks, 7, &events),
        BUCKETS as u32,
        "the chunks cover every millisecond the stand-in wrote"
    );
    match terminal {
        AudioEvent::Done(done) => {
            assert_eq!(done.job_id, 7);
            assert_eq!(done.buckets, BUCKETS as u32, "done counts what went out");
        }
        other => panic!("a job that read every byte ends in done, not {other:?}"),
    }

    // The samples the stand-in wrote, as the payload carries them: the quiet one in `min` and the
    // loud one in `max`, so a swapped lane or a byte order slip is a different pair of numbers.
    for chunk in &chunks {
        assert!(
            chunk.min.iter().all(|value| *value == QUIET),
            "the low lane holds the smallest sample of the millisecond"
        );
        assert!(
            chunk.max.iter().all(|value| *value == LOUD),
            "the high lane holds the largest"
        );
    }
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_cancelled_job_ends_in_cancelled_never_in_done_and_leaves_no_child_behind() {
    let _alone = alone();
    let root = workspace("cancelled");
    let pid_file = root.join("pid");
    let ffmpeg = stand_in(
        &root,
        &format!("echo $$ > {}\n{LONG_WRITER}", pid_file.display()),
    );

    let cancel = Cancel::new();
    let recorder = Recorder::default();
    let pid = std::thread::scope(|scope| {
        let stopper = scope.spawn(|| {
            let pid = wait_for_pid(&pid_file);
            // Cancel while it is writing, not before it starts, and not after a duration this
            // machine happens to take: the wait is on the job's own first chunk.
            recorder.wait_for_a_chunk();
            cancel.cancel();
            pid
        });
        run_job(
            &ffmpeg,
            3,
            &PeakRequest::new(media(&root, "ep01.mkv"), 1),
            &cancel,
            &|event| recorder.record(event),
        );
        stopper.join().expect("the stopper thread cannot panic")
    });

    let events = recorder.taken();
    let (chunks, terminal) = split(events.clone());
    assert!(
        !chunks.is_empty(),
        "the job should have been streaming when it was cancelled, or this proves nothing"
    );
    covered(&chunks, 3, &events);
    match terminal {
        AudioEvent::Failed(failed) => {
            assert_eq!(failed.job_id, 3);
            assert_eq!(failed.code, AudioErrorCode::Cancelled);
        }
        other => panic!("a cancelled job never reports done: {other:?}"),
    }
    assert!(
        !still_there(pid),
        "pid {pid} outlived the job it was spawned for"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn opening_a_second_media_stops_the_first_job_with_no_done_and_no_child_left() {
    let _alone = alone();
    let root = workspace("superseded");
    let pid_file = root.join("pid");
    let ffmpeg = stand_in(
        &root,
        &format!("echo $$ > {}\n{LONG_WRITER}", pid_file.display()),
    );

    let state = AudioState::default();
    let first_cancel = Cancel::new();
    let first = state
        .begin(
            JobTarget {
                media: media(&root, "ep01.mkv"),
                ff_index: 1,
            },
            first_cancel.clone(),
        )
        .expect("nothing is peaking");

    let recorder = Recorder::default();
    let pid = std::thread::scope(|scope| {
        let opener = scope.spawn(|| {
            let pid = wait_for_pid(&pid_file);
            recorder.wait_for_a_chunk();
            // What `video_open` does when a second file replaces the first (decision 12).
            state
                .begin(
                    JobTarget {
                        media: root.join("ep02.mkv"),
                        ff_index: 1,
                    },
                    Cancel::new(),
                )
                .expect("a different media takes the slot");
            pid
        });
        run_job(
            &ffmpeg,
            first,
            &PeakRequest::new(root.join("ep01.mkv"), 1),
            &first_cancel,
            &|event| recorder.record(event),
        );
        opener.join().expect("the opening thread cannot panic")
    });

    let events = recorder.taken();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AudioEvent::Done(_))),
        // Counted rather than printed: a job that was not stopped hands over a million buckets,
        // and a megabyte of them in the failure message buries the sentence that matters.
        "the media that was replaced must never report a finished waveform, and it reported one \
         after {} chunks",
        recorder_chunks(&events)
    );
    let (chunks, terminal) = split(events.clone());
    assert!(
        !chunks.is_empty(),
        "the first job should have been streaming when the second media arrived"
    );
    match terminal {
        AudioEvent::Failed(failed) => assert_eq!(failed.code, AudioErrorCode::Cancelled),
        other => panic!("a superseded job ends cancelled: {other:?}"),
    }
    assert!(
        !still_there(pid),
        "pid {pid} was peaking the media that was replaced and outlived it"
    );

    // The job that took the slot is the one a later cancel reaches, and the old id is inert.
    let live = state.live().expect("the second job holds the slot");
    assert_ne!(live.0, first);
    assert_eq!(live.1.media, root.join("ep02.mkv"));
    fs::remove_dir_all(&root).ok();
}

/// The assertion `e2e/scripts/shutdown-check.js` makes about the whisper sidecar, at the layer
/// that exists today: `audio::shutdown` is `AudioState::cancel_all` once it has the state out of
/// the app handle, and `lib.rs`'s own test holds `shutdown_all` to calling it.
#[test]
fn a_shutdown_mid_job_leaves_no_ffmpeg_process_behind() {
    let _alone = alone();
    let root = workspace("shutdown");
    let pid_file = root.join("pid");
    let ffmpeg = stand_in(
        &root,
        &format!("echo $$ > {}\n{LONG_WRITER}", pid_file.display()),
    );

    let state = AudioState::default();
    let cancel = Cancel::new();
    let job = state
        .begin(
            JobTarget {
                media: media(&root, "ep01.mkv"),
                ff_index: 1,
            },
            cancel.clone(),
        )
        .expect("nothing is peaking");

    let recorder = Recorder::default();
    let pid = std::thread::scope(|scope| {
        let closer = scope.spawn(|| {
            let pid = wait_for_pid(&pid_file);
            recorder.wait_for_a_chunk();
            state.cancel_all();
            pid
        });
        run_job(
            &ffmpeg,
            job,
            &PeakRequest::new(root.join("ep01.mkv"), 1),
            &cancel,
            &|event| recorder.record(event),
        );
        closer.join().expect("the closing thread cannot panic")
    });

    assert!(
        !still_there(pid),
        "pid {pid} survived the shutdown that cancelled its job"
    );
    let (chunks, terminal) = split(recorder.taken());
    assert!(
        !chunks.is_empty(),
        "the job should have been running when the app was told to stop"
    );
    assert!(
        matches!(terminal, AudioEvent::Failed(failed) if failed.code == AudioErrorCode::Cancelled),
        "a job stopped on the way out ends cancelled"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_second_start_for_the_same_media_and_track_is_refused_while_the_first_keeps_running() {
    let _alone = alone();
    let root = workspace("refused");
    let pid_file = root.join("pid");
    let ffmpeg = stand_in(
        &root,
        &format!("echo $$ > {}\n{LONG_WRITER}", pid_file.display()),
    );
    let media = media(&root, "ep01.mkv");

    let state = AudioState::default();
    let cancel = Cancel::new();
    let first = state
        .begin(
            JobTarget {
                media: media.clone(),
                ff_index: 1,
            },
            cancel.clone(),
        )
        .expect("nothing is peaking");

    let recorder = Recorder::default();
    let pid = std::thread::scope(|scope| {
        let asker = scope.spawn(|| {
            let pid = wait_for_pid(&pid_file);
            let refused = state
                .begin(
                    JobTarget {
                        media: media.clone(),
                        ff_index: 1,
                    },
                    Cancel::new(),
                )
                .expect_err("the same media and track twice is a refusal, never a queue");
            assert_eq!(refused.code, AudioErrorCode::Busy);
            assert!(
                refused.detail.contains("ep01.mkv") && refused.detail.contains("track 1"),
                "the refusal is a sentence naming the file and the track: {}",
                refused.detail
            );
            // The refusal changed nothing: the job that holds the slot is still the first one.
            assert_eq!(state.live().map(|(id, _)| id), Some(first));
            assert!(
                still_there(pid),
                "the refused start stopped the child of the job that was already running"
            );
            recorder.wait_for_a_chunk();
            cancel.cancel();
            pid
        });
        run_job(
            &ffmpeg,
            first,
            &PeakRequest::new(root.join("ep01.mkv"), 1),
            &cancel,
            &|event| recorder.record(event),
        );
        asker.join().expect("the asking thread cannot panic")
    });

    let (chunks, _) = split(recorder.taken());
    assert!(
        !chunks.is_empty(),
        "the first job kept streaming through the refusal"
    );
    assert!(!still_there(pid), "pid {pid} outlived its job");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_child_that_fails_ends_in_one_error_carrying_its_own_words_and_never_in_done() {
    let _alone = alone();
    let root = workspace("failed");
    let ffmpeg = stand_in(
        &root,
        "echo 'ep01.mkv: Invalid data found when processing input' >&2\nexit 1",
    );

    let recorder = Recorder::default();
    run_job(
        &ffmpeg,
        11,
        &PeakRequest::new(media(&root, "ep01.mkv"), 1),
        &Cancel::new(),
        &|event| recorder.record(event),
    );

    let (chunks, terminal) = split(recorder.taken());
    assert!(
        chunks.is_empty(),
        "a child that wrote nothing sent no chunk"
    );
    match terminal {
        AudioEvent::Failed(failed) => {
            assert_eq!(failed.job_id, 11);
            assert_eq!(failed.code, AudioErrorCode::MediaUnreadable);
            assert!(
                failed.detail.contains("Invalid data found"),
                "the detail carries the child's own sentence for the log: {}",
                failed.detail
            );
        }
        other => panic!("a failed child never reports done: {other:?}"),
    }
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_missing_ffmpeg_ends_in_one_error_that_says_which_program_could_not_be_run() {
    let _alone = alone();
    let root = workspace("no-ffmpeg");

    let recorder = Recorder::default();
    run_job(
        &root.join("there-is-no-ffmpeg-here"),
        2,
        &PeakRequest::new(media(&root, "ep01.mkv"), 1),
        &Cancel::new(),
        &|event| recorder.record(event),
    );

    let (chunks, terminal) = split(recorder.taken());
    assert!(chunks.is_empty());
    match terminal {
        AudioEvent::Failed(failed) => {
            assert_eq!(failed.code, AudioErrorCode::FfmpegMissing);
            assert!(
                failed.detail.contains("there-is-no-ffmpeg-here"),
                "the detail names what could not be run: {}",
                failed.detail
            );
        }
        other => panic!("nothing to run is an error, not {other:?}"),
    }
    fs::remove_dir_all(&root).ok();
}
