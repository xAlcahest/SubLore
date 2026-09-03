//! Waveform peaks over IPC: list the media's audio tracks, start a peak job, cancel one. The
//! command names, the event names and their payloads are a public interface (CONTRIBUTING.md §6).
//! See BACKLOG.md M2.4, W4.
//!
//! Everything heavy lives in `sublore-audio`; this module is the thin layer that keeps the one
//! in-flight job, decides when it starts and stops, and turns what it produces into events. No
//! decoding happens on the main thread or on the async runtime's poll thread: every call into
//! ffmpeg goes through `spawn_blocking` (CONTRIBUTING.md §7).
//!
//! The job's lifetime is the open media, not a transcription run (decision 12): opening a video
//! starts peaking the track mpv is playing, closing or replacing it cancels the job, and a second
//! start for the same media and track is refused rather than queued.

pub mod error;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use sublore_audio::{extract_peaks, Bucket, Cancel, PeakRequest};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::log;
use crate::video::player::{AudioTrack, Player};
use crate::video::VideoState;
use error::{AudioError, AudioErrorCode};

const EVENT_STARTED: &str = "audio://started";
const EVENT_PEAKS: &str = "audio://peaks";
const EVENT_DONE: &str = "audio://done";
const EVENT_ERROR: &str = "audio://error";

/// Which media and which of its streams a job is peaking. Two starts that name the same pair are
/// the same job, which is what makes the second one a refusal rather than a queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobTarget {
    pub media: PathBuf,
    /// ffmpeg's own index for the stream, which is what mpv reports as `ff-index`.
    pub ff_index: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioJobStarted {
    pub job_id: u64,
}

/// One chunk of peaks, as the canvas wants them: two flat arrays rather than a list of pairs, so
/// the webview can read them straight into typed arrays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPeaks {
    pub job_id: u64,
    /// The millisecond the first bucket of this chunk starts at. Chunks arrive in order and never
    /// overlap, so this is also how many buckets came before it.
    pub first_ms: u32,
    pub min: Vec<i16>,
    pub max: Vec<i16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDone {
    pub job_id: u64,
    /// How many milliseconds of audio were peaked, which is how many buckets went out in total.
    pub buckets: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFailed {
    pub job_id: u64,
    pub code: AudioErrorCode,
    pub detail: String,
}

/// What a job hands back as it runs: any number of chunks, then exactly one terminal event. The
/// commands turn these into `audio://` events; the tests record them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioEvent {
    Peaks(AudioPeaks),
    Done(AudioDone),
    Failed(AudioFailed),
}

/// One peak job at a time. A second start for the same media and track is refused; one for a
/// different media or a different track supersedes, because the panel only ever draws one.
#[derive(Debug, Default)]
pub struct AudioState {
    job: Mutex<Option<ActiveJob>>,
    next_job_id: AtomicU64,
}

#[derive(Debug)]
struct ActiveJob {
    id: u64,
    target: JobTarget,
    cancel: Cancel,
}

impl AudioState {
    /// Claim the one job slot for `target`. The id is fresh whether or not the claim succeeds, so
    /// no two jobs ever share one and every event carries an id the UI can match against.
    ///
    /// A job already running for a different media or track is cancelled and its slot taken: the
    /// waveform draws one track of one file, so the older one has nothing left to draw into.
    pub fn begin(&self, target: JobTarget, cancel: Cancel) -> Result<u64, AudioError> {
        let id = self.next_job_id.fetch_add(1, Ordering::SeqCst) + 1;
        let mut slot = lock(&self.job);
        if let Some(active) = slot.as_ref() {
            // A cancelled occupant is not a reason to refuse: it is on its way out and only its own
            // thread frees the slot, so a re-open of the same file inside that window would be told
            // the file is already being peaked and would get no waveform at all. See W4, N12.
            if active.target == target && !active.cancel.is_cancelled() {
                return Err(AudioError::new(
                    AudioErrorCode::Busy,
                    format!(
                        "track {} of {} is already being peaked by job {}",
                        target.ff_index,
                        target.media.display(),
                        active.id
                    ),
                ));
            }
        }
        let superseded = slot.replace(ActiveJob { id, target, cancel });
        // Out of the lock before a child is killed: nothing waits on a process teardown here.
        drop(slot);
        if let Some(old) = superseded {
            old.cancel.cancel();
        }
        Ok(id)
    }

    /// Free the slot if it still holds this job. A late finisher must never free a newer job.
    pub fn end(&self, id: u64) {
        let mut slot = lock(&self.job);
        if slot.as_ref().is_some_and(|active| active.id == id) {
            *slot = None;
        }
    }

    /// Stop `id`, if that is still the job in flight. Cancelling one that already ended is not an
    /// error, and an old id must never reach into the job that replaced it.
    pub fn cancel_job(&self, id: u64) {
        let cancel = lock(&self.job)
            .as_ref()
            .filter(|active| active.id == id)
            .map(|active| active.cancel.clone());
        if let Some(cancel) = cancel {
            cancel.cancel();
        }
    }

    /// Stop whatever is in flight. What a media change and a shutdown both do.
    pub fn cancel_all(&self) {
        let cancel = lock(&self.job).as_ref().map(|active| active.cancel.clone());
        if let Some(cancel) = cancel {
            cancel.cancel();
        }
    }

    /// The id and target of the job in flight, or nothing when the slot is free.
    pub fn live(&self) -> Option<(u64, JobTarget)> {
        lock(&self.job)
            .as_ref()
            .map(|active| (active.id, active.target.clone()))
    }
}

/// A poisoned lock still has to be able to cancel a child, so the poison is stepped over rather
/// than propagated: leaving a process running would be the worse failure.
fn lock<T>(slot: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    slot.lock().unwrap_or_else(|error| error.into_inner())
}

/// Holds the one job slot while a job runs. A guard rather than a pair of calls: a panic inside
/// the extraction must not leave the app refusing every later job.
struct JobSlot {
    app: AppHandle,
    job_id: u64,
    cancel: Cancel,
}

impl JobSlot {
    /// Give the slot back now. Idempotent, and it never frees a slot a newer job has claimed.
    fn release(&self) {
        // Managed for the app's whole life, so this is always Some; asking rather than indexing
        // keeps a teardown race from turning into a panic on a worker thread.
        if let Some(state) = self.app.try_state::<AudioState>() {
            state.end(self.job_id);
        }
    }
}

impl Drop for JobSlot {
    fn drop(&mut self) {
        // The extraction reaps its own child on every path it can return from; a panic is the one
        // that skips them, and a killed ffmpeg is better than one that outlives the job. See W4.
        if std::thread::panicking() {
            self.cancel.cancel();
        }
        self.release();
    }
}

/// Run one job to its end: every chunk to `emit`, then exactly one terminal event, whatever
/// happened. A cancelled job ends in [`AudioEvent::Failed`] with [`AudioErrorCode::Cancelled`] and
/// never in [`AudioEvent::Done`], so nothing downstream mistakes a superseded job for a finished
/// one. See BACKLOG.md M2.4, W4.
///
/// Blocking: the caller runs it on a blocking task, never on the main thread.
pub fn run_job(
    ffmpeg: &Path,
    job_id: u64,
    request: &PeakRequest,
    cancel: &Cancel,
    emit: &(dyn Fn(AudioEvent) + Sync),
) {
    let outcome = extract_peaks(ffmpeg, request, cancel, &|first_ms, buckets| {
        emit(AudioEvent::Peaks(chunk(job_id, first_ms, buckets)));
    });
    let terminal = match outcome {
        Ok(buckets) => AudioEvent::Done(AudioDone { job_id, buckets }),
        Err(error) => {
            let error = AudioError::from(error);
            AudioEvent::Failed(AudioFailed {
                job_id,
                code: error.code,
                detail: error.detail,
            })
        }
    };
    emit(terminal);
}

fn chunk(job_id: u64, first_ms: u32, buckets: &[Bucket]) -> AudioPeaks {
    AudioPeaks {
        job_id,
        first_ms,
        min: buckets.iter().map(|bucket| bucket.min).collect(),
        max: buckets.iter().map(|bucket| bucket.max).collect(),
    }
}

/// Which ffmpeg to run, given what the override holds: the path it names, or the bare name, which
/// the operating system resolves on PATH.
///
/// Pure so it can be tested without touching the process environment, the shape
/// `video::player::gpu_context_from` already uses for the same reason.
fn ffmpeg_from(value: Option<&std::ffi::OsStr>) -> PathBuf {
    match value {
        // A set variable that is empty is not a path; treat it as unset rather than as `.`.
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from("ffmpeg"),
    }
}

/// The override `sublore-asr` already reads, so a machine that points transcription at a
/// particular ffmpeg points the waveform at the same one. One variable rather than a second one.
/// W2 moves discovery into `sublore-io`; this reads the name it will.
fn ffmpeg_binary() -> PathBuf {
    ffmpeg_from(std::env::var_os(sublore_asr::tools::FFMPEG_BIN_ENV).as_deref())
}

/// The audio tracks of the open media, as mpv sees them, with the playing one marked. mpv is the
/// authority on which track that is (decision 24 E2), so nothing else is asked and no second
/// dependency is added to answer it.
#[tauri::command]
pub async fn audio_tracks(video: State<'_, VideoState>) -> Result<Vec<AudioTrack>, AudioError> {
    let player = video.player();
    // A loop of mpv property reads, so it goes off the poll thread like every other heavy call.
    blocking(move || player.audio_tracks().map_err(AudioError::from)).await
}

/// Start peaking `ff_index` of the open media and return the job's id. The peaks arrive as
/// `audio://peaks` chunks, then exactly one `audio://done` or `audio://error`.
#[tauri::command]
pub async fn audio_peaks_start(
    app: AppHandle,
    video: State<'_, VideoState>,
    ff_index: u32,
) -> Result<AudioJobStarted, AudioError> {
    let player = video.player();
    // Everything that can fail before a process exists is answered here, so the caller gets a
    // rejection it can show at once instead of a job that starts and dies.
    let media = blocking(move || {
        let media = player
            .loaded_path()
            .ok_or_else(|| AudioError::new(AudioErrorCode::NotLoaded, "no file is open to peak"))?;
        let tracks = player.audio_tracks().map_err(AudioError::from)?;
        if !tracks.iter().any(|track| track.ff_index == ff_index) {
            return Err(AudioError::new(
                AudioErrorCode::NoSuchTrack,
                format!("{media} has no audio stream {ff_index}"),
            ));
        }
        Ok(PathBuf::from(media))
    })
    .await?;

    let job_id = start_job(&app, media, ff_index)?;
    Ok(AudioJobStarted { job_id })
}

#[tauri::command]
pub async fn audio_peaks_cancel(
    state: State<'_, AudioState>,
    job_id: u64,
) -> Result<(), AudioError> {
    // Cancelling a job that just finished is not an error: the user closed the media as it ended.
    state.cancel_job(job_id);
    Ok(())
}

/// Start peaking the track mpv is playing, for the media that just finished loading. Called from
/// `video_open` on the same blocking task that loaded the file, so no second open can slip in
/// between the load and this read of the track list.
///
/// Failures are logged rather than returned: the open succeeded, and the user has a video. The
/// failures a translator can act on come from the job itself, as `audio://error`.
pub fn start_for_playing_track(app: &AppHandle, player: &Player, media: &str) {
    // The path comes from the caller, which has just opened it, rather than from mpv. Asking mpv
    // gave None on a slow runner right after a successful open, and that branch returned in
    // silence, so the waveform simply never happened and no line said why. See W5.
    let tracks = match player.audio_tracks() {
        Ok(tracks) => tracks,
        Err(error) => {
            log::warn!("waveform: mpv would not list the audio tracks of {media}: {error}");
            return;
        }
    };
    // Decision 24 E3: media with no audio spawns no child, on any route into the panel. That is the
    // empty list below and not the fallback inside `track_to_peak`.
    let Some(track) = track_to_peak(&tracks).cloned() else {
        log::info!("waveform: {media} carries no audio track, so nothing is peaked");
        return;
    };
    if !track.playing {
        // Said out loud rather than passed over: the panel is right either way, and this is the one
        // line that shows mpv had not marked a track yet. See BACKLOG.md N14.
        let listed = tracks
            .iter()
            .map(|other| format!("stream {} selected={}", other.ff_index, other.playing))
            .collect::<Vec<_>>()
            .join(", ");
        log::info!(
            "waveform: mpv has marked no audio track of {media} as playing, so stream {} is peaked; it listed {listed}",
            track.ff_index
        );
    }
    if let Err(error) = start_job(app, PathBuf::from(&media), track.ff_index) {
        log::warn!(
            "waveform: no peaks for stream {} of {media}: {error}",
            track.ff_index
        );
    }
}

/// The track to peak: the one mpv is playing, or the first audio track when mpv has marked none.
///
/// `selected` is read on the thread that has just loaded the file, and mpv does not always have it
/// set by then on a loaded machine. W5 found the same shape one field over, where asking mpv for
/// the path right after a successful open answered `None`. A file that carries audio has a
/// waveform to draw whichever track mpv has got round to choosing, so the panel no longer depends
/// on that timing; a file that carries none still spawns no child, which is the empty list here.
fn track_to_peak(tracks: &[AudioTrack]) -> Option<&AudioTrack> {
    tracks
        .iter()
        .find(|track| track.playing)
        .or_else(|| tracks.first())
}

/// Claim the slot and run the job on a blocking task. What both entry points share.
fn start_job(app: &AppHandle, media: PathBuf, ff_index: u32) -> Result<u64, AudioError> {
    let state = app.try_state::<AudioState>().ok_or_else(|| {
        AudioError::new(
            AudioErrorCode::CommandFailed,
            "the waveform state is not managed",
        )
    })?;
    let cancel = Cancel::new();
    let target = JobTarget {
        media: media.clone(),
        ff_index,
    };
    let job_id = state.begin(target, cancel.clone())?;
    // Announced before the first chunk: a job started by `video_open` is nobody's return value, so
    // without this the page sees chunks carrying an id it has never been told about and cannot say
    // which job is the current one. See W5.
    let _ = app.emit(EVENT_STARTED, AudioJobStarted { job_id });

    let ffmpeg = ffmpeg_binary();
    let request = PeakRequest::new(media, ff_index);
    let handle = app.clone();
    // Blocking, not async: process handling in std blocks, and this must never sit on a poll
    // thread. The window keeps answering while it runs, which is what the E2E asserts.
    tauri::async_runtime::spawn_blocking(move || {
        // The guard gives the slot back on every way out of this closure, a panic included.
        let slot = JobSlot {
            app: handle.clone(),
            job_id,
            cancel: cancel.clone(),
        };
        run_job(&ffmpeg, job_id, &request, &cancel, &|event| {
            emit_event(&handle, event);
        });
        slot.release();
    });
    Ok(job_id)
}

fn emit_event(app: &AppHandle, event: AudioEvent) {
    match event {
        AudioEvent::Peaks(peaks) => {
            let _ = app.emit(EVENT_PEAKS, peaks);
        }
        AudioEvent::Done(done) => {
            log::info!("waveform: job {} peaked {} ms", done.job_id, done.buckets);
            let _ = app.emit(EVENT_DONE, done);
        }
        AudioEvent::Failed(failed) => {
            // A cancel is what every media change does, so it is not a warning: a log full of
            // them would bury the failures a translator can act on.
            if AudioError::new(failed.code, "").is_cancelled() {
                log::info!("waveform: job {} was cancelled", failed.job_id);
            } else {
                log::warn!(
                    "waveform: job {} ended {:?}: {}",
                    failed.job_id,
                    failed.code,
                    failed.detail
                );
            }
            let _ = app.emit(EVENT_ERROR, failed);
        }
    }
}

/// The media is being replaced or closed, so the peaks of the one before it are void. Called by
/// `video_open` before the new file loads (decision 12).
pub fn cancel_for_media_change(app: &AppHandle) {
    if let Some(state) = app.try_state::<AudioState>() {
        state.cancel_all();
    }
}

/// Stop the job on the way out. The child is killed rather than waited for: the process is about
/// to end, and an ffmpeg left behind would outlive the window that started it. See BACKLOG.md M3.1
/// for the same rule on the transcription side.
pub fn shutdown(app: &AppHandle) {
    cancel_for_media_change(app);
}

/// Run `work` off the async runtime's poll thread and flatten the join failure into a wire error.
async fn blocking<T, F>(work: F) -> Result<T, AudioError>
where
    F: FnOnce() -> Result<T, AudioError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            AudioError::new(
                AudioErrorCode::CommandFailed,
                format!("the waveform task failed: {error}"),
            )
        })?
}

#[cfg(test)]
mod tests {
    use super::{chunk, ffmpeg_from, track_to_peak, AudioState, JobTarget};
    use crate::audio::error::AudioErrorCode;
    use crate::video::player::AudioTrack;
    use std::ffi::OsStr;
    use std::path::PathBuf;
    use sublore_audio::{Bucket, Cancel};

    fn target(media: &str, ff_index: u32) -> JobTarget {
        JobTarget {
            media: PathBuf::from(media),
            ff_index,
        }
    }

    #[test]
    fn a_second_start_for_the_same_media_and_track_is_refused_with_a_sentence() {
        let state = AudioState::default();
        let first = state
            .begin(target("/media/ep01.mkv", 1), Cancel::new())
            .expect("nothing is peaking");

        let cancel = Cancel::new();
        let refused = state
            .begin(target("/media/ep01.mkv", 1), cancel.clone())
            .expect_err("the same track twice is a refusal");
        assert_eq!(refused.code, AudioErrorCode::Busy);
        assert!(
            refused.detail.contains("/media/ep01.mkv") && refused.detail.contains("track 1"),
            "the refusal names the file and the track: {}",
            refused.detail
        );
        assert!(
            !cancel.is_cancelled(),
            "a refused start must not stop the job that is running"
        );
        assert_eq!(
            state.live().map(|(id, _)| id),
            Some(first),
            "the running job keeps the slot"
        );
    }

    #[test]
    fn another_track_of_the_same_media_supersedes_rather_than_queues() {
        let state = AudioState::default();
        let first_cancel = Cancel::new();
        let first = state
            .begin(target("/media/ep01.mkv", 1), first_cancel.clone())
            .expect("nothing is peaking");

        let second = state
            .begin(target("/media/ep01.mkv", 2), Cancel::new())
            .expect("a different track takes the slot");
        assert_ne!(second, first, "ids are never reused");
        assert!(
            first_cancel.is_cancelled(),
            "the track that is no longer drawn stops being computed"
        );
        assert_eq!(state.live().map(|(id, _)| id), Some(second));
    }

    #[test]
    fn another_media_supersedes_and_the_slot_names_what_is_being_peaked() {
        let state = AudioState::default();
        let first_cancel = Cancel::new();
        state
            .begin(target("/media/ep01.mkv", 1), first_cancel.clone())
            .expect("nothing is peaking");

        state
            .begin(target("/media/ep02.mkv", 1), Cancel::new())
            .expect("a different media takes the slot");
        assert!(first_cancel.is_cancelled());
        assert_eq!(
            state.live().map(|(_, target)| target),
            Some(target("/media/ep02.mkv", 1))
        );
    }

    #[test]
    fn a_finished_job_never_frees_a_newer_ones_slot_or_cancels_it() {
        let state = AudioState::default();
        let first = state
            .begin(target("/media/ep01.mkv", 1), Cancel::new())
            .expect("nothing is peaking");
        state.end(first);
        assert_eq!(state.live(), None, "the slot is free once a job ends");

        let second_cancel = Cancel::new();
        let second = state
            .begin(target("/media/ep02.mkv", 1), second_cancel.clone())
            .expect("the slot is free");

        // The late tail of the first job tidying up must not touch the job that replaced it.
        state.end(first);
        assert_eq!(state.live().map(|(id, _)| id), Some(second));
        state.cancel_job(first);
        assert!(
            !second_cancel.is_cancelled(),
            "a stale id must never reach into the live job"
        );

        state.cancel_job(second);
        assert!(second_cancel.is_cancelled());
    }

    #[test]
    fn cancelling_everything_stops_the_job_in_flight_and_is_safe_with_none() {
        let state = AudioState::default();
        state.cancel_all();

        let cancel = Cancel::new();
        state
            .begin(target("/media/ep01.mkv", 1), cancel.clone())
            .expect("nothing is peaking");
        state.cancel_all();
        assert!(cancel.is_cancelled());
        // The slot stays claimed until the job's own thread gives it back, so a shutdown does not
        // open the door to a job started while the app is going away.
        assert!(state.live().is_some());
    }

    #[test]
    fn a_chunk_carries_the_buckets_as_two_flat_arrays_in_order() {
        let buckets = [
            Bucket {
                min: -32_768,
                max: 32_767,
            },
            Bucket { min: 0, max: 0 },
            Bucket { min: -5, max: 9 },
        ];
        let chunk = chunk(7, 2_000, &buckets);
        assert_eq!(chunk.job_id, 7);
        assert_eq!(chunk.first_ms, 2_000);
        assert_eq!(chunk.min, vec![-32_768, 0, -5]);
        assert_eq!(chunk.max, vec![32_767, 0, 9]);
    }

    /// The variable is `sublore-asr`'s own, so one setting cannot point the two paths at two
    /// different binaries. Named here rather than only read: reading it proves nothing on a machine
    /// where it is unset, which is every machine that has ffmpeg on PATH.
    #[test]
    fn the_ffmpeg_override_is_the_one_the_transcription_path_already_reads() {
        assert_eq!(sublore_asr::tools::FFMPEG_BIN_ENV, "SUBLORE_FFMPEG_BIN");
    }

    #[test]
    fn a_named_ffmpeg_is_run_and_anything_else_falls_back_to_the_bare_name() {
        assert_eq!(
            ffmpeg_from(Some(OsStr::new("/opt/ffmpeg/bin/ffmpeg"))),
            PathBuf::from("/opt/ffmpeg/bin/ffmpeg")
        );
        // A set variable that is empty is not a path, and must not become the current directory.
        assert_eq!(ffmpeg_from(Some(OsStr::new(""))), PathBuf::from("ffmpeg"));
        assert_eq!(ffmpeg_from(None), PathBuf::from("ffmpeg"));
    }

    fn track(ff_index: u32, playing: bool) -> AudioTrack {
        AudioTrack {
            id: i64::from(ff_index),
            ff_index,
            lang: None,
            title: None,
            playing,
        }
    }

    #[test]
    fn the_track_mpv_is_playing_is_the_one_peaked_even_when_it_is_not_the_first() {
        let tracks = [track(1, false), track(2, true), track(3, false)];
        assert_eq!(track_to_peak(&tracks).expect("a track").ff_index, 2);
    }

    #[test]
    fn a_file_whose_tracks_mpv_has_not_marked_yet_peaks_the_first_rather_than_nothing() {
        // The panel used to depend on mpv having chosen by the time this ran, which it has not
        // always done on a loaded machine. See BACKLOG.md N14.
        let tracks = [track(1, false), track(2, false)];
        assert_eq!(track_to_peak(&tracks).expect("a track").ff_index, 1);
    }

    #[test]
    fn a_file_that_carries_no_audio_peaks_nothing_at_all() {
        // Decision 24 E3, which the fallback above must not swallow.
        assert!(track_to_peak(&[]).is_none());
    }
}
