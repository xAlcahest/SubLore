//! Running whisper.cpp as a child process: spawn it, stream it, cancel it, reap it.
//! See BACKLOG.md M3.1.
//!
//! Three measured facts shape everything here. whisper cannot put its JSON and its progress on
//! the same pipe, so the JSON goes to a file in our scratch directory and progress comes off
//! stderr. Its exit code is not a verdict: a truncated model, a non-audio input and an unknown
//! flag all exit 0, so the artifact is the verdict and the exit code is only a hint. And a killed
//! child that is not waited for stays a zombie, which a process check still sees, so every path
//! out of here reaps.

use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{AsrError, AsrErrorKind};
use crate::json::{self, ParsedTranscript};
use crate::progress::{self, Line, Stream, Tail};
use crate::scratch::ScratchDir;
use crate::tools::Tools;
use crate::transcript::{Backend, Transcript};

/// How long the child may say nothing before it is killed. A stall timer, not a run timer:
/// whisper's runtime scales with the media, and a fixed cap would kill legitimate work. Generous
/// because loading a large model from a cold disk produces no output while it works.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(300);
/// How often the child is checked for having exited. Also the worst-case delay after a cancel.
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How much of the child's stderr is kept as the technical detail of a failure.
const STDERR_TAIL_BYTES: usize = 4096;
/// Above this, more threads stop helping and start making runs less comparable.
const MAX_THREADS: u16 = 8;
/// How much of a WAV is read to find its duration. Chunks before `data` are a few hundred
/// bytes in anything ffmpeg writes; this is room to spare.
const WAV_HEADER_BYTES: usize = 64 * 1024;
/// whisper exits 3 when it cannot build a context from the model file.
const EXIT_MODEL: i32 = 3;
/// whisper exits 2 when it cannot open the audio file.
const EXIT_INPUT: i32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Language {
    /// `-l auto`. whisper's default is English, so this is never left implicit.
    Auto,
    /// An ISO code whisper knows, e.g. `en`.
    Code(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compute {
    Gpu,
    Cpu,
}

/// Which half of a run the progress belongs to. Extraction reports no percentage worth showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Extracting,
    Transcribing,
}

#[derive(Clone, Debug)]
pub struct TranscribeRequest {
    /// The user's file. Opened for reading and nothing else, ever (CLAUDE.md §3.1).
    pub media: PathBuf,
    /// A model the store has already checked.
    pub model: PathBuf,
    pub language: Language,
    /// Pinned by the caller, never left to whisper's default: measured, `-t 4` and `-t 8` produce
    /// different transcripts, so the thread count is part of what makes a run reproducible.
    pub threads: u16,
    pub compute: Compute,
    /// Usually [`STALL_TIMEOUT`]. A field rather than a constant so the tests can prove the timer
    /// fires without waiting five minutes for it.
    pub stall: Duration,
}

impl TranscribeRequest {
    /// A request with the app's defaults for everything that is not a user choice.
    pub fn new(media: PathBuf, model: PathBuf, language: Language, compute: Compute) -> Self {
        Self {
            media,
            model,
            language,
            threads: default_threads(),
            compute,
            stall: STALL_TIMEOUT,
        }
    }
}

/// The thread count every run uses unless the caller says otherwise.
pub fn default_threads() -> u16 {
    std::thread::available_parallelism()
        .map(|count| (count.get() as u64).clamp(1, MAX_THREADS as u64) as u16)
        .unwrap_or(4)
}

/// The handle the UI keeps while a run is in flight. Cheap to clone, safe from any thread.
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

    /// Ask the run to stop. Idempotent, returns immediately, and never waits on the child: the
    /// thread that called it is the UI's, and it is not held up by a process teardown.
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

    /// Kill without claiming the user asked. Used by the stall timer.
    fn kill_child(&self) {
        if let Some(child) = self.slot().as_mut() {
            let _ = child.kill();
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

/// Transcribe `request.media`. Blocking: the caller runs it on a blocking task, never on the main
/// thread and never on an async runtime's poll thread.
///
/// `on_progress` is called from the reader threads, with a percentage that never goes backwards.
pub fn transcribe(
    tools: &Tools,
    request: &TranscribeRequest,
    cancel: &Cancel,
    on_progress: &(dyn Fn(Phase, u8) + Sync),
) -> Result<Transcript, AsrError> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    validate(request)?;

    let scratch = ScratchDir::create(&tools.scratch_root)?;
    on_progress(Phase::Extracting, 0);
    extract_audio(tools, request, &scratch, cancel)?;
    let audio_duration_ms = wav_duration_ms(&scratch.audio())?;
    if cancel.is_cancelled() {
        return Err(cancelled());
    }

    let (_, missing_gpu) = tools.whisper(request.compute);
    let attempted = match request.compute {
        Compute::Cpu => Backend::Cpu,
        Compute::Gpu if missing_gpu => Backend::Cpu,
        Compute::Gpu => Backend::Gpu,
    };
    on_progress(Phase::Transcribing, 0);

    let first = run_whisper(
        tools,
        request,
        &scratch,
        attempted,
        audio_duration_ms,
        cancel,
        on_progress,
    );
    let (parsed, backend, fell_back_to_cpu) = match first {
        Ok(parsed) => (parsed, attempted, missing_gpu),
        Err(error) if attempted == Backend::Cpu || error.is_cancelled() => return Err(error),
        // A Vulkan run can fail for reasons the CPU one will not have: a broken driver, a device
        // lost mid-run, a missing loader. One retry, and the UI is told which backend produced
        // the result.
        Err(_) => {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            // A second run starts its own count, so the bar is reset rather than appearing to
            // fall back from whatever the first attempt reached.
            on_progress(Phase::Transcribing, 0);
            let parsed = run_whisper(
                tools,
                request,
                &scratch,
                Backend::Cpu,
                audio_duration_ms,
                cancel,
                on_progress,
            )?;
            (parsed, Backend::Cpu, true)
        }
    };

    if parsed.words.is_empty() {
        return Err(AsrError::new(
            AsrErrorKind::EmptyTranscript,
            "the run produced no words",
        ));
    }
    Ok(Transcript {
        language: parsed.language,
        words: parsed.words,
        backend,
        fell_back_to_cpu,
        audio_duration_ms,
    })
}

/// Everything that is checked before a process is started. The paths come from the UI, so they
/// are checked here rather than trusted.
fn validate(request: &TranscribeRequest) -> Result<(), AsrError> {
    if !request.media.is_file() {
        return Err(AsrError::new(
            AsrErrorKind::MediaUnreadable,
            format!("{} is not a file", request.media.display()),
        ));
    }
    if !request.model.is_file() {
        return Err(AsrError::new(
            AsrErrorKind::ModelMissing,
            format!("{} is not a file", request.model.display()),
        ));
    }
    if let Language::Code(code) = &request.language {
        let plausible = (2..=5).contains(&code.len())
            && code.chars().all(|character| character.is_ascii_lowercase());
        if !plausible {
            return Err(AsrError::new(
                AsrErrorKind::BadArguments,
                format!("{code:?} is not a language code"),
            ));
        }
    }
    Ok(())
}

/// Decode the user's media into the 16 kHz mono WAV whisper wants.
///
/// The only path ffmpeg is given to write is inside the scratch directory. The media appears once,
/// after `-i`, where it can only be read: no remux, no in-place conversion, nothing written beside
/// the user's file (CLAUDE.md §3.1).
fn extract_audio(
    tools: &Tools,
    request: &TranscribeRequest,
    scratch: &ScratchDir,
    cancel: &Cancel,
) -> Result<(), AsrError> {
    let mut command = Command::new(&tools.ffmpeg);
    command
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(&request.media)
        .args(["-vn", "-sn", "-dn"])
        .args(["-map", "0:a:0"])
        .args(["-ac", "1"])
        .args(["-ar", "16000"])
        .args(["-c:a", "pcm_s16le"])
        .args(["-f", "wav"])
        .arg(scratch.audio());

    let outcome =
        run_child(&mut command, cancel, request.stall, &|_, _| {}).map_err(
            |error| match error.kind {
                AsrErrorKind::BinaryUnrunnable => {
                    AsrError::new(AsrErrorKind::FfmpegMissing, error.detail)
                }
                _ => error,
            },
        )?;
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    if outcome.stalled {
        return Err(AsrError::new(
            AsrErrorKind::Stalled,
            format!("ffmpeg said nothing for {:?}", request.stall),
        ));
    }
    if !outcome.status.success() {
        return Err(AsrError::new(
            AsrErrorKind::MediaUnreadable,
            format!("ffmpeg {}: {}", outcome.status, outcome.stderr_tail),
        ));
    }
    Ok(())
}

/// One whisper run against the already extracted audio.
#[allow(clippy::too_many_arguments)]
fn run_whisper(
    tools: &Tools,
    request: &TranscribeRequest,
    scratch: &ScratchDir,
    backend: Backend,
    audio_duration_ms: u32,
    cancel: &Cancel,
    on_progress: &(dyn Fn(Phase, u8) + Sync),
) -> Result<ParsedTranscript, AsrError> {
    let compute = match backend {
        Backend::Gpu => Compute::Gpu,
        Backend::Cpu => Compute::Cpu,
    };
    let (binary, _) = tools.whisper(compute);
    // The output of an earlier attempt must never be read as this one's result.
    let _ = fs::remove_file(scratch.json());

    let mut command = Command::new(binary);
    command
        .arg("-m")
        .arg(&request.model)
        .arg("-f")
        .arg(scratch.audio())
        .arg("-ojf")
        .arg("-of")
        .arg(scratch.output_stem())
        .arg("-t")
        .arg(request.threads.clamp(1, 64).to_string())
        .arg("-pp")
        .arg("-l")
        .arg(match &request.language {
            Language::Auto => "auto",
            Language::Code(code) => code.as_str(),
        });
    if compute == Compute::Cpu {
        command.arg("-ng");
    }

    // Reported percentage, and the lock that keeps it monotone as the sink sees it: whisper's own
    // number moves in steps of five, the segment timestamps move between them, and a progress bar
    // that goes backwards is worse than one that jumps.
    let reported = Mutex::new(0u8);
    let bad_arguments = AtomicBool::new(false);
    let sink = |_stream: Stream, line: &str| {
        let candidate = match progress::parse_line(line) {
            Line::Progress(percent) => Some(percent),
            Line::Segment { end_ms } => Some(percent_of(end_ms, audio_duration_ms)),
            Line::BadArguments => {
                bad_arguments.store(true, Ordering::SeqCst);
                None
            }
            Line::Other => None,
        };
        if let Some(candidate) = candidate {
            let mut current = reported.lock().unwrap_or_else(|error| error.into_inner());
            if candidate > *current {
                *current = candidate;
                on_progress(Phase::Transcribing, candidate);
            }
        }
    };

    let outcome = run_child(&mut command, cancel, request.stall, &sink)?;
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    if outcome.stalled {
        return Err(AsrError::new(
            AsrErrorKind::Stalled,
            format!("whisper said nothing for {:?}", request.stall),
        ));
    }
    if bad_arguments.load(Ordering::SeqCst) {
        return Err(AsrError::new(
            AsrErrorKind::BadArguments,
            format!("whisper rejected an argument: {}", outcome.stderr_tail),
        ));
    }
    match outcome.status.code() {
        Some(EXIT_MODEL) => {
            return Err(AsrError::new(
                AsrErrorKind::ModelRejected,
                format!("whisper could not load the model: {}", outcome.stderr_tail),
            ))
        }
        Some(EXIT_INPUT) => {
            return Err(AsrError::new(
                AsrErrorKind::NoInput,
                format!("whisper could not open the audio: {}", outcome.stderr_tail),
            ))
        }
        _ => {}
    }

    // The verdict, whatever the exit code claimed.
    let bytes = fs::read(scratch.json()).map_err(|error| {
        AsrError::new(
            AsrErrorKind::NoOutput,
            format!(
                "whisper {} left no readable {}: {error}; {}",
                outcome.status,
                scratch.json().display(),
                outcome.stderr_tail
            ),
        )
    })?;
    json::parse_transcript(&bytes)
}

fn percent_of(position_ms: u32, duration_ms: u32) -> u8 {
    if duration_ms == 0 {
        return 0;
    }
    let percent = u64::from(position_ms) * 100 / u64::from(duration_ms);
    percent.min(100) as u8
}

struct ChildOutcome {
    status: ExitStatus,
    stderr_tail: String,
    /// The stall timer fired and the child was killed for it.
    stalled: bool,
}

/// Spawn, drain both pipes, watch for cancel and for silence, then reap.
///
/// Both pipes are drained by their own thread. Reading one to EOF before the other deadlocks the
/// moment the unread pipe's buffer fills, which a chatty whisper run does.
fn run_child(
    command: &mut Command,
    cancel: &Cancel,
    stall: Duration,
    sink: &(dyn Fn(Stream, &str) + Sync),
) -> Result<ChildOutcome, AsrError> {
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
        AsrError::new(
            AsrErrorKind::BinaryUnrunnable,
            format!("cannot run {:?}: {error}", command.get_program()),
        )
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    cancel.arm(child);

    let started = Instant::now();
    let activity_ms = AtomicU64::new(0);
    let tail = Mutex::new(Tail::new(STDERR_TAIL_BYTES));

    // Declared out here rather than inside the scope: the reader threads borrow it, so it has to
    // outlive them.
    let note = |stream: Stream, line: &str| {
        activity_ms.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        if stream == Stream::Err {
            let mut tail = tail.lock().unwrap_or_else(|error| error.into_inner());
            tail.push(line);
        }
        sink(stream, line);
    };

    let result = thread::scope(|scope| {
        let out_reader = stdout.map(|pipe| scope.spawn(|| drain(pipe, Stream::Out, &note)));
        let err_reader = stderr.map(|pipe| scope.spawn(|| drain(pipe, Stream::Err, &note)));

        let mut stalled = false;
        let status = loop {
            match cancel.poll() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    cancel.kill_child();
                    return Err(AsrError::new(
                        AsrErrorKind::Internal,
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

        // Joined before returning, so no reader thread ever outlives the call. A read error is
        // not the verdict: the exit status and the JSON are.
        for reader in [out_reader, err_reader].into_iter().flatten() {
            let _ = reader.join();
        }
        Ok((status, stalled))
    });

    cancel.disarm();
    let (status, stalled) = result?;
    Ok(ChildOutcome {
        status,
        stderr_tail: tail
            .into_inner()
            .unwrap_or_else(|error| error.into_inner())
            .into_string(),
        stalled,
    })
}

fn drain(pipe: impl Read, stream: Stream, note: &(dyn Fn(Stream, &str) + Sync)) {
    let _ = progress::for_each_line(BufReader::new(pipe), |line| note(stream, line));
}

/// The audio's length, from the WAV header ffmpeg just wrote. Free, and it is what cue times are
/// clamped against later.
///
/// Only the header is read: a 45-minute episode is 86 MB of samples, and none of them say
/// anything the `data` chunk's length does not (CLAUDE.md §7).
fn wav_duration_ms(path: &Path) -> Result<u32, AsrError> {
    let unreadable = |what: &str| {
        AsrError::new(
            AsrErrorKind::MediaUnreadable,
            format!("the extracted audio {what}"),
        )
    };
    let mut file = fs::File::open(path).map_err(|error| {
        AsrError::new(
            AsrErrorKind::MediaUnreadable,
            format!("cannot read the extracted audio: {error}"),
        )
    })?;
    let file_len = file
        .metadata()
        .map_err(|error| {
            AsrError::new(
                AsrErrorKind::MediaUnreadable,
                format!("cannot measure the extracted audio: {error}"),
            )
        })?
        .len();

    let mut bytes = vec![0u8; WAV_HEADER_BYTES];
    let read = read_at_most(&mut file, &mut bytes)?;
    bytes.truncate(read);
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(unreadable("is not a WAV file"));
    }

    let mut offset = 12;
    let mut byte_rate = 0u32;
    let mut data_len = 0u64;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as u64;
        let body = offset + 8;
        match id {
            b"fmt " if body + 16 <= bytes.len() => {
                byte_rate = u32::from_le_bytes([
                    bytes[body + 8],
                    bytes[body + 9],
                    bytes[body + 10],
                    bytes[body + 11],
                ]);
            }
            b"data" => {
                // A streamed WAV carries a placeholder length; what is on disk is the truth.
                let remaining = file_len.saturating_sub(body as u64);
                data_len = size.min(remaining);
                break;
            }
            _ => {}
        }
        // Chunks are word aligned. A declared size larger than the header window ends the walk
        // rather than overflowing the offset.
        let Some(next) = body
            .checked_add(size as usize)
            .and_then(|next| next.checked_add((size % 2) as usize))
        else {
            break;
        };
        offset = next;
    }

    if byte_rate == 0 {
        return Err(unreadable("has no format chunk"));
    }
    if data_len == 0 {
        return Err(unreadable("has no audio in it"));
    }
    Ok((data_len * 1000 / u64::from(byte_rate)).min(u32::MAX as u64) as u32)
}

/// Fill as much of `buffer` as the file has, without failing on a short read.
fn read_at_most(file: &mut fs::File, buffer: &mut [u8]) -> Result<usize, AsrError> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(AsrError::new(
                    AsrErrorKind::MediaUnreadable,
                    format!("cannot read the extracted audio: {error}"),
                ))
            }
        }
    }
    Ok(filled)
}

fn cancelled() -> AsrError {
    AsrError::new(AsrErrorKind::Cancelled, "the user cancelled the run")
}

#[cfg(test)]
mod tests {
    use super::{
        default_threads, percent_of, validate, wav_duration_ms, Compute, Language,
        TranscribeRequest,
    };
    use crate::error::AsrErrorKind;
    use std::fs;
    use std::path::PathBuf;

    fn wav(byte_rate: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // pcm
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&16_000u32.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("sublore-wav-{}-{name}", std::process::id()));
        fs::write(&path, bytes).expect("the temp file should be writable");
        path
    }

    #[test]
    fn a_wav_header_gives_the_duration_in_milliseconds() {
        let path = temp_file("ok.wav", &wav(32_000, &vec![0u8; 32_000]));
        assert_eq!(wav_duration_ms(&path).expect("a valid header"), 1000);
        fs::remove_file(path).ok();
    }

    #[test]
    fn a_wav_with_no_samples_is_unreadable_media() {
        let path = temp_file("empty.wav", &wav(32_000, &[]));
        assert_eq!(
            wav_duration_ms(&path).expect_err("no audio").kind,
            AsrErrorKind::MediaUnreadable
        );
        fs::remove_file(path).ok();
    }

    #[test]
    fn a_truncated_data_chunk_is_measured_by_what_is_on_disk() {
        // A header that claims a second of audio over a file holding half of it.
        let mut bytes = wav(32_000, &vec![0u8; 32_000]);
        bytes.truncate(bytes.len() - 16_000);
        let path = temp_file("short.wav", &bytes);
        assert_eq!(wav_duration_ms(&path).expect("still readable"), 500);
        fs::remove_file(path).ok();
    }

    #[test]
    fn a_file_that_is_not_a_wav_is_refused_rather_than_guessed_at() {
        for (name, bytes) in [
            ("tiny.wav", &b"RIF"[..]),
            ("mp3.wav", &b"ID3\x04\x00\x00\x00\x00\x00\x00"[..]),
        ] {
            let path = temp_file(name, bytes);
            assert_eq!(
                wav_duration_ms(&path).expect_err(name).kind,
                AsrErrorKind::MediaUnreadable
            );
            fs::remove_file(path).ok();
        }
    }

    #[test]
    fn a_missing_wav_is_an_error_not_a_panic() {
        assert_eq!(
            wav_duration_ms(&PathBuf::from("/nonexistent-sublore.wav"))
                .expect_err("missing")
                .kind,
            AsrErrorKind::MediaUnreadable
        );
    }

    #[test]
    fn the_progress_share_of_a_position_is_clamped_to_a_hundred() {
        assert_eq!(percent_of(0, 1000), 0);
        assert_eq!(percent_of(500, 1000), 50);
        assert_eq!(percent_of(4000, 1000), 100);
        assert_eq!(percent_of(1, 0), 0, "an unknown duration cannot divide");
    }

    #[test]
    fn the_thread_count_is_pinned_inside_a_range_that_stays_comparable() {
        let threads = default_threads();
        assert!((1..=8).contains(&threads), "got {threads}");
    }

    #[test]
    fn a_language_that_is_not_a_code_is_refused_before_anything_is_spawned() {
        let media = temp_file("media.wav", &wav(32_000, &[0u8; 32]));
        let model = temp_file("model.bin", b"not really a model");
        for bad in ["", "e", "english-please", "EN", "-ng"] {
            let request = TranscribeRequest::new(
                media.clone(),
                model.clone(),
                Language::Code(bad.to_owned()),
                Compute::Cpu,
            );
            assert_eq!(
                validate(&request).expect_err(bad).kind,
                AsrErrorKind::BadArguments,
                "{bad:?}"
            );
        }
        let good =
            TranscribeRequest::new(media.clone(), model.clone(), Language::Auto, Compute::Cpu);
        assert!(validate(&good).is_ok());
        fs::remove_file(media).ok();
        fs::remove_file(model).ok();
    }

    #[test]
    fn a_missing_media_file_and_a_missing_model_are_told_apart() {
        let model = temp_file("model2.bin", b"model");
        let request = TranscribeRequest::new(
            PathBuf::from("/nonexistent-sublore-media.mkv"),
            model.clone(),
            Language::Auto,
            Compute::Cpu,
        );
        assert_eq!(
            validate(&request).expect_err("no media").kind,
            AsrErrorKind::MediaUnreadable
        );

        let media = temp_file("media2.wav", &wav(32_000, &[0u8; 32]));
        let request = TranscribeRequest::new(
            media.clone(),
            PathBuf::from("/nonexistent-sublore-model.bin"),
            Language::Auto,
            Compute::Cpu,
        );
        assert_eq!(
            validate(&request).expect_err("no model").kind,
            AsrErrorKind::ModelMissing
        );
        fs::remove_file(media).ok();
        fs::remove_file(model).ok();
    }
}
