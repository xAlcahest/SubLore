//! The child process half of an extraction, driven with stand-in programs instead of ffmpeg:
//! what happens when it cannot be started, exits badly, writes nothing, stops writing, or is
//! cancelled while writing. See BACKLOG.md M2.4.
//!
//! Linux only, and deliberately: the reaping assertion reads `/proc`, and Linux is where this
//! project's behaviour is proved (CONTRIBUTING.md §5.5). A stand-in is used because these are
//! assertions about the process, not about decoding: they must fail for their own reason on a
//! machine where ffmpeg is missing, not be skipped by it.
#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use sublore_audio::{extract_peaks, AudioErrorKind, Cancel, PeakRequest, CHUNK_BUCKETS};

/// One test at a time. Writing a stand-in while another thread forks hands that thread's write
/// handle to the child, and the exec that follows fails with ETXTBSY; the tests are milliseconds
/// long, so serialising them costs nothing.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn alone() -> MutexGuard<'static, ()> {
    // A test that panicked poisons the lock; the ones after it still have their own work to do.
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// A directory of this test file's own, cleaned between runs.
fn workspace(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("sublore-audio-child-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("the test directory should be creatable");
    root
}

/// An executable shell script standing in for ffmpeg. It ignores the arguments it is given, the
/// way every stand-in here does: what is under test is what Sublore does with the child.
fn stand_in(root: &Path, body: &str) -> PathBuf {
    let path = root.join("ffmpeg-stand-in");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("the stand-in should be writable");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .expect("the stand-in should be executable");
    path
}

/// A file for the request to point at. Its contents never reach the stand-in.
fn media(root: &Path) -> PathBuf {
    let path = root.join("media.mkv");
    fs::write(&path, b"not really media").expect("the placeholder should be writable");
    path
}

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

/// True when `pid` is a process that ended and was never waited for. A pid the kernel has already
/// handed to something else reads as alive, which is the answer that fails this check honestly.
fn left_as_zombie(pid: u32) -> bool {
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // `pid (comm) S ...`, and comm may hold spaces and brackets, so the state is what follows the
    // last bracket.
    stat.rsplit_once(')')
        .map(|(_, rest)| rest.trim_start().starts_with('Z'))
        .unwrap_or(false)
}

#[test]
fn a_missing_ffmpeg_says_so_rather_than_failing_as_unreadable_media() {
    let _alone = alone();
    let root = workspace("no-ffmpeg");
    let error = extract_peaks(
        &root.join("there-is-no-ffmpeg-here"),
        &PeakRequest::new(media(&root), 1),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("nothing to run");
    assert_eq!(error.kind, AudioErrorKind::FfmpegMissing);
    assert!(
        error.detail.contains("there-is-no-ffmpeg-here"),
        "the detail names what could not be run: {}",
        error.detail
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_child_that_fails_carries_its_own_words_into_the_error() {
    let _alone = alone();
    let root = workspace("bad-exit");
    let ffmpeg = stand_in(
        &root,
        "echo 'media.mkv: Invalid data found when processing input' >&2\nexit 1",
    );
    let error = extract_peaks(
        &ffmpeg,
        &PeakRequest::new(media(&root), 1),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("exit 1");
    assert_eq!(error.kind, AudioErrorKind::MediaUnreadable);
    assert!(
        error.detail.contains("Invalid data found"),
        "the detail carries the child's own sentence: {}",
        error.detail
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_child_that_succeeds_without_writing_a_sample_is_an_error_not_an_empty_waveform() {
    let _alone = alone();
    let root = workspace("no-samples");
    let ffmpeg = stand_in(&root, "exit 0");
    let error = extract_peaks(
        &ffmpeg,
        &PeakRequest::new(media(&root), 1),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("no samples");
    assert_eq!(error.kind, AudioErrorKind::MediaUnreadable);
    assert!(
        error.detail.contains("no samples"),
        "the detail says what was missing: {}",
        error.detail
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn samples_on_the_pipe_reach_the_caller_as_chunks_in_order() {
    let _alone = alone();
    let root = workspace("pipe");
    // Two seconds of digital silence: 48000 samples a second, two bytes each.
    let ffmpeg = stand_in(&root, "head -c 192000 /dev/zero");
    let seen = std::sync::Mutex::new(Vec::new());
    let total = extract_peaks(
        &ffmpeg,
        &PeakRequest::new(media(&root), 1),
        &Cancel::new(),
        &|first, buckets| {
            seen.lock().expect("the test sink is never poisoned").push((
                first,
                buckets.len(),
                buckets[0],
            ));
        },
    )
    .expect("the stand-in writes a whole number of milliseconds");
    assert_eq!(total, 2000);
    let seen = seen.into_inner().expect("the test sink is never poisoned");
    assert_eq!(
        seen.iter()
            .map(|(first, len, _)| (*first, *len))
            .collect::<Vec<_>>(),
        vec![(0, CHUNK_BUCKETS), (CHUNK_BUCKETS as u32, CHUNK_BUCKETS)],
        "chunks arrive a second at a time, in order, without a gap"
    );
    assert_eq!(seen[0].2.min, 0, "silence in, silence out");
    assert_eq!(seen[0].2.max, 0);
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_cancel_stops_a_running_child_at_once_and_leaves_no_zombie_behind() {
    let _alone = alone();
    let root = workspace("cancel");
    let pid_file = root.join("pid");
    // Writes for as long as it is allowed to. `exec` keeps the pid the shell reported, so the
    // process the test looks for afterwards is the one Sublore spawned.
    let ffmpeg = stand_in(
        &root,
        &format!("echo $$ > {}\nexec cat /dev/zero", pid_file.display()),
    );
    let cancel = Cancel::new();
    let chunks = AtomicU32::new(0);

    let elapsed = std::thread::scope(|scope| {
        let stopper = scope.spawn(|| {
            let pid = wait_for_pid(&pid_file);
            // Cancel while it is writing, not before it starts.
            std::thread::sleep(Duration::from_millis(50));
            let started = Instant::now();
            cancel.cancel();
            (pid, started)
        });
        let error = extract_peaks(
            &ffmpeg,
            &PeakRequest::new(media(&root), 1),
            &cancel,
            &|_, _| {
                chunks.fetch_add(1, Ordering::Relaxed);
            },
        )
        .expect_err("cancelled");
        let returned = Instant::now();
        let (pid, cancelled_at) = stopper.join().expect("the stopper thread cannot panic");
        assert_eq!(error.kind, AudioErrorKind::Cancelled);
        assert!(
            !left_as_zombie(pid),
            "pid {pid} was killed and never waited for"
        );
        returned.duration_since(cancelled_at)
    });

    assert!(
        chunks.load(Ordering::Relaxed) > 0,
        "the run should have been under way when it was cancelled"
    );
    assert!(
        elapsed < Duration::from_millis(100),
        "a cancel returned after {elapsed:?}"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_child_that_stops_writing_is_killed_and_reaped_rather_than_waited_for_forever() {
    let _alone = alone();
    let root = workspace("stall");
    let pid_file = root.join("pid");
    let ffmpeg = stand_in(
        &root,
        &format!("echo $$ > {}\nexec sleep 30", pid_file.display()),
    );
    let mut request = PeakRequest::new(media(&root), 1);
    request.stall = Duration::from_millis(150);

    let started = Instant::now();
    let error =
        extract_peaks(&ffmpeg, &request, &Cancel::new(), &|_, _| {}).expect_err("nothing arrived");
    let elapsed = started.elapsed();

    assert_eq!(error.kind, AudioErrorKind::Stalled);
    assert!(
        elapsed < Duration::from_secs(5),
        "the stall timer fired after {elapsed:?}, not after the sleep"
    );
    let pid = wait_for_pid(&pid_file);
    assert!(
        !left_as_zombie(pid),
        "pid {pid} was killed by the stall timer and never waited for"
    );
    fs::remove_dir_all(&root).ok();
}
