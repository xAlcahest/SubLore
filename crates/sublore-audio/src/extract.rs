//! Running ffmpeg as a child process: spawn it, fold what it writes, cancel it, reap it.
//! See BACKLOG.md M2.4.
//!
//! The shape is `sublore-asr`'s and none of its code (decision 12). Two differences, both
//! deliberate: the decoded audio goes to a pipe instead of a file, because peaks are wanted as
//! they are computed and the samples are not kept afterwards, and the rate is the waveform's
//! rather than whisper's. And as there, a killed child that is not waited for stays a zombie a
//! process check still sees, so every path out of here reaps.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{AudioError, AudioErrorKind};
use crate::peaks::{Bucket, Peaks, SAMPLE_RATE};

/// How long ffmpeg may write nothing before it is killed. A stall timer, not a run timer:
/// decoding runs far faster than playback, and a fixed cap would kill legitimate work on a long
/// episode. Generous because opening a file on a cold or remote disk produces no output while it
/// works.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(60);
/// How often the child is checked for having exited. Also the worst-case delay after a cancel.
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How much of ffmpeg's stderr is kept as the technical detail of a failure. The head rather than
/// the tail: with `-loglevel error` the first line is the one that names the cause.
const STDERR_HEAD_BYTES: usize = 4096;
/// How much of the pipe is taken at a time. Two buckets short of 700 ms of audio.
const READ_BUFFER_BYTES: usize = 64 * 1024;

/// One extraction: which file, and which of its streams.
#[derive(Clone, Debug)]
pub struct PeakRequest {
    /// The user's file. Opened for reading and nothing else, ever (CONTRIBUTING.md §3.1).
    pub media: PathBuf,
    /// ffmpeg's own index for the audio stream, which is what mpv reports as `ff-index`. The
    /// track that is playing is the one the waveform draws (decision 24 E2), and mpv is the only
    /// thing in the process that knows which that is.
    pub ff_index: u32,
    /// Usually [`STALL_TIMEOUT`]. A field rather than a constant so the tests can prove the timer
    /// fires without waiting a minute for it.
    pub stall: Duration,
}

impl PeakRequest {
    pub fn new(media: PathBuf, ff_index: u32) -> Self {
        Self {
            media,
            ff_index,
            stall: STALL_TIMEOUT,
        }
    }
}

/// The handle the caller keeps while an extraction is in flight. Cheap to clone, safe from any
/// thread.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<CancelInner>);

#[derive(Debug, Default)]
struct CancelInner {
    flag: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the extraction to stop. Idempotent, returns immediately, and never waits on the child:
    /// the thread that calls it is the UI's, and it is not held up by a process teardown.
    pub fn cancel(&self) {
        self.0.flag.store(true, Ordering::SeqCst);
        if let Some(child) = self.slot().as_mut() {
            let _ = child.kill();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::SeqCst)
    }

    /// A poisoned lock still has to be able to kill a process, so the poison is stepped over
    /// rather than propagated: leaving a child running would be the worse failure.
    fn slot(&self) -> MutexGuard<'_, Option<Child>> {
        self.0
            .child
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Hand the freshly spawned child over, and close the window between `spawn` and this call by
    /// re-reading the flag while the slot is held.
    fn arm(&self, mut child: Child) {
        let mut slot = self.slot();
        if self.0.flag.load(Ordering::SeqCst) {
            let _ = child.kill();
        }
        *slot = Some(child);
    }

    /// Kill without claiming the caller asked. Used by the stall timer, which goes on polling
    /// afterwards and reaps there.
    fn kill_child(&self) {
        if let Some(child) = self.slot().as_mut() {
            let _ = child.kill();
        }
    }

    /// Kill and wait, for the one path that leaves the polling loop without a status of its own.
    fn kill_and_reap(&self) {
        if let Some(child) = self.slot().as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn poll(&self) -> std::io::Result<Option<ExitStatus>> {
        match self.slot().as_mut() {
            Some(child) => child.try_wait(),
            None => Ok(None),
        }
    }

    /// Drop the reaped child. Only ever called after `poll` returned a status, so no zombie is
    /// left behind by the drop.
    fn disarm(&self) {
        *self.slot() = None;
    }
}

/// Decode one audio track of `request.media` into millisecond peaks, handing `on_chunk` each chunk
/// as it is folded, and return how many buckets there were.
///
/// Blocking: the caller runs it on a blocking task, never on the main thread and never on an async
/// runtime's poll thread. `on_chunk` is called from the reader thread, with the millisecond its
/// first bucket starts at; chunks arrive in order and never overlap. An error means every chunk
/// already handed over is void: the caller keeps nothing from a run that did not finish.
///
/// `ffmpeg` is the program to run, discovered by the caller. A bare name is looked up on PATH.
pub fn extract_peaks(
    ffmpeg: &Path,
    request: &PeakRequest,
    cancel: &Cancel,
    on_chunk: &(dyn Fn(u32, &[Bucket]) + Sync),
) -> Result<u32, AudioError> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    // The path comes from the UI, so it is checked here rather than trusted.
    if !request.media.is_file() {
        return Err(AudioError::new(
            AudioErrorKind::MediaUnreadable,
            format!("{} is not a file", request.media.display()),
        ));
    }

    let map = format!("0:{}", request.ff_index);
    let rate = SAMPLE_RATE.to_string();
    let mut command = Command::new(ffmpeg);
    command
        .arg("-nostdin")
        .arg("-hide_banner")
        .args(["-loglevel", "error"])
        .arg("-i")
        .arg(&request.media)
        .args(["-vn", "-sn", "-dn"])
        .args(["-map", &map])
        // One lane is drawn, so one channel is decoded: a downmix, not the left channel.
        .args(["-ac", "1"])
        .args(["-ar", &rate])
        .args(["-c:a", "pcm_s16le"])
        .args(["-f", "s16le"])
        // The samples go to the pipe. The media appears exactly once, after `-i`, where it can
        // only be read: nothing is written beside the user's file (CONTRIBUTING.md §3.1).
        .arg("-");

    let outcome = run_child(&mut command, cancel, request.stall, on_chunk)?;
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    if outcome.stalled {
        return Err(AudioError::new(
            AudioErrorKind::Stalled,
            format!("ffmpeg said nothing for {:?}", request.stall),
        ));
    }
    if !outcome.status.success() {
        return Err(AudioError::new(
            AudioErrorKind::MediaUnreadable,
            format!("ffmpeg {}: {}", outcome.status, outcome.stderr_head),
        ));
    }
    let buckets = outcome.buckets?;
    if buckets == 0 {
        return Err(AudioError::new(
            AudioErrorKind::MediaUnreadable,
            format!(
                "ffmpeg read {} stream {} and produced no samples: {}",
                request.media.display(),
                request.ff_index,
                outcome.stderr_head
            ),
        ));
    }
    Ok(buckets)
}

struct ChildOutcome {
    status: ExitStatus,
    stderr_head: String,
    /// The stall timer fired and the child was killed for it.
    stalled: bool,
    /// How many buckets the fold produced, or why the pipe could not be read.
    buckets: Result<u32, AudioError>,
}

/// Spawn, fold stdout, drain stderr, watch for cancel and for silence, then reap.
///
/// Both pipes are read by their own thread. Reading one to the end before the other deadlocks the
/// moment the unread pipe's buffer fills, and this child fills its output pipe by design.
fn run_child(
    command: &mut Command,
    cancel: &Cancel,
    stall: Duration,
    on_chunk: &(dyn Fn(u32, &[Bucket]) + Sync),
) -> Result<ChildOutcome, AudioError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: without it a console flashes on screen on every run.
        command.creation_flags(0x0800_0000);
    }

    let mut child = command.spawn().map_err(|error| {
        AudioError::new(
            AudioErrorKind::FfmpegMissing,
            format!("cannot run {:?}: {error}", command.get_program()),
        )
    })?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        // Both were asked for above, so this is a Sublore bug rather than a run that failed.
        let _ = child.kill();
        let _ = child.wait();
        return Err(AudioError::new(
            AudioErrorKind::Internal,
            "ffmpeg was spawned without its pipes",
        ));
    };
    cancel.arm(child);

    let started = Instant::now();
    let activity_ms = AtomicU64::new(0);

    let result = thread::scope(|scope| {
        let reader = scope.spawn(|| fold_stdout(stdout, on_chunk, started, &activity_ms));
        let draining = scope.spawn(|| read_head(stderr, STDERR_HEAD_BYTES));

        let mut stalled = false;
        let status = loop {
            match cancel.poll() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    // Nothing else will collect this one: the loop that reaps is the loop being
                    // left. The scope still joins both readers on the way out.
                    cancel.kill_and_reap();
                    return Err(AudioError::new(
                        AudioErrorKind::Internal,
                        format!("cannot wait for the child: {error}"),
                    ));
                }
            }
            let silent_for = started
                .elapsed()
                .saturating_sub(Duration::from_millis(activity_ms.load(Ordering::Relaxed)));
            if !stalled && silent_for >= stall {
                stalled = true;
                cancel.kill_child();
            }
            thread::sleep(POLL_INTERVAL);
        };

        // Joined before returning, so no reader thread ever outlives the call. A thread that
        // panicked is this crate's bug, and it is reported as one rather than hidden.
        let buckets = reader.join().unwrap_or_else(|_| {
            Err(AudioError::new(
                AudioErrorKind::Internal,
                "the fold thread panicked",
            ))
        });
        let stderr_head = draining.join().unwrap_or_default();
        Ok((status, stalled, buckets, stderr_head))
    });

    cancel.disarm();
    let (status, stalled, buckets, stderr_head) = result?;
    Ok(ChildOutcome {
        status,
        stderr_head,
        stalled,
        buckets,
    })
}

/// Read the pipe to the end, folding as it goes. Every read is an activity stamp, so the stall
/// timer measures silence rather than duration.
fn fold_stdout(
    mut pipe: impl Read,
    on_chunk: &(dyn Fn(u32, &[Bucket]) + Sync),
    started: Instant,
    activity_ms: &AtomicU64,
) -> Result<u32, AudioError> {
    let mut peaks = Peaks::new();
    let mut buffer = vec![0u8; READ_BUFFER_BYTES];
    let mut emit = |first: u32, buckets: &[Bucket]| on_chunk(first, buckets);
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                activity_ms.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
                peaks.push(&buffer[..read], &mut emit);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(AudioError::new(
                    AudioErrorKind::Internal,
                    format!("cannot read ffmpeg's output: {error}"),
                ))
            }
        }
    }
    Ok(peaks.finish(&mut emit))
}

/// The first `limit` bytes of a pipe, as the technical detail of a failure. The rest is read and
/// dropped rather than left unread: an unread pipe stops the child.
fn read_head(mut pipe: impl Read, limit: usize) -> String {
    let mut kept: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let room = limit.saturating_sub(kept.len());
                kept.extend_from_slice(&buffer[..read.min(room)]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            // A pipe that cannot be read has no detail to give; the exit status is still the
            // verdict.
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&kept).trim().to_owned()
}

fn cancelled() -> AudioError {
    AudioError::new(AudioErrorKind::Cancelled, "the caller cancelled the run")
}

#[cfg(test)]
mod tests {
    use super::{extract_peaks, Cancel, PeakRequest};
    use crate::error::AudioErrorKind;
    use std::path::{Path, PathBuf};

    #[test]
    fn a_media_path_that_is_not_a_file_is_refused_before_anything_is_spawned() {
        let error = extract_peaks(
            Path::new("/nonexistent-sublore-ffmpeg"),
            &PeakRequest::new(PathBuf::from("/nonexistent-sublore-media.mkv"), 1),
            &Cancel::new(),
            &|_, _| {},
        )
        .expect_err("no media");
        assert_eq!(error.kind, AudioErrorKind::MediaUnreadable);
        assert!(
            error.detail.contains("nonexistent-sublore-media.mkv"),
            "the detail names the file: {}",
            error.detail
        );
    }

    #[test]
    fn a_cancel_set_before_the_start_spawns_nothing() {
        let cancel = Cancel::new();
        cancel.cancel();
        let error = extract_peaks(
            Path::new("/nonexistent-sublore-ffmpeg"),
            &PeakRequest::new(PathBuf::from("/nonexistent-sublore-media.mkv"), 1),
            &cancel,
            &|_, _| {},
        )
        .expect_err("cancelled");
        assert_eq!(error.kind, AudioErrorKind::Cancelled);
        assert!(error.is_cancelled());
    }
}
