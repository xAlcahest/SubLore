//! Transcription over IPC: list models, download one, start a run, cancel it. The command names,
//! the event names and their payloads are a public interface (CONTRIBUTING.md §6). See BACKLOG.md M3.4.
//!
//! Everything heavy lives in `sublore-asr`; this module is the thin layer that owns the app's
//! directories, keeps the one in-flight run, and turns results into events. No transcription work
//! happens on the main thread or on the async runtime's poll thread: every call into the sidecar
//! goes through `spawn_blocking` (CONTRIBUTING.md §7).

pub mod error;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sublore_asr::cues;
use sublore_asr::model::{catalog, download, HttpFetcher, ModelState, ModelStore};
use sublore_asr::render;
use sublore_asr::scratch;
use sublore_asr::sidecar::{transcribe, Cancel, Compute, Language, Phase, TranscribeRequest};
use sublore_asr::tools::Tools;
use sublore_asr::transcript::{Backend, Transcript};
use sublore_formats::{parse, SubtitleFormat};
use tauri::{AppHandle, Emitter, Manager, State};

use error::{AsrError, AsrErrorCode};

/// Under the app data directory, beside `backups`. Models are the user's, kept between versions.
const MODELS_DIR: &str = "models";
/// Under the app data directory. Extracted audio and whisper's JSON, deleted after every run.
const SCRATCH_DIR: &str = "scratch";

const EVENT_PROGRESS: &str = "asr://progress";
const EVENT_DONE: &str = "asr://done";
const EVENT_ERROR: &str = "asr://error";
const EVENT_MODEL_PROGRESS: &str = "asr://model-progress";

/// Which binary the user asked for. The UI's "use GPU when available" checkbox, on the wire.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AsrCompute {
    Gpu,
    Cpu,
}

impl From<AsrCompute> for Compute {
    fn from(compute: AsrCompute) -> Self {
        match compute {
            AsrCompute::Gpu => Compute::Gpu,
            AsrCompute::Cpu => Compute::Cpu,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrModelStatus {
    /// What the UI sends back to start a run or a download, e.g. `tiny.en`.
    pub id: String,
    /// The whole file's length, from the catalog.
    pub bytes: u64,
    /// "missing" | "partial" | "ready" | "corrupt".
    pub state: &'static str,
    pub downloaded_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrRunStarted {
    pub run_id: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrProgress {
    pub run_id: u64,
    /// "extracting" | "transcribing".
    pub phase: &'static str,
    pub percent: u8,
}

/// One generated cue, as the UI lists it. `lines` never holds a line terminator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrCue {
    pub start_ms: u32,
    pub end_ms: u32,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrDone {
    pub run_id: u64,
    /// "gpu" | "cpu": which binary produced this. The two do not agree word for word, so a result
    /// is only reproducible against the backend that made it.
    pub backend: &'static str,
    /// The user asked for the GPU and did not get it. Said out loud, never silently.
    pub fell_back_to_cpu: bool,
    pub audio_duration_ms: u32,
    pub cues: Vec<AsrCue>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AsrRunFailed {
    run_id: u64,
    code: AsrErrorCode,
    detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AsrModelProgress {
    id: String,
    received_bytes: u64,
    /// What is left to fetch is `totalBytes - receivedBytes`; both come from the catalog row.
    total_bytes: u64,
}

/// One run and one download at a time. A second start is refused, never queued: two whisper
/// processes on one machine make both of them slower and neither of them cancellable by name.
#[derive(Debug, Default)]
pub struct AsrState {
    run: Mutex<Option<ActiveRun>>,
    download: Mutex<Option<ActiveDownload>>,
    /// What the last run that reached the end produced, until another run starts.
    finished: Mutex<Option<FinishedRun>>,
    next_run_id: AtomicU64,
}

/// A finished run's cues, as the SRT they render to. Kept so that adopting them builds the document
/// out of what the sidecar produced rather than out of a payload the webview sends back, and so a
/// refused adoption can be offered again. See BACKLOG.md M3.5.
#[derive(Debug)]
struct FinishedRun {
    id: u64,
    srt: Vec<u8>,
}

#[derive(Debug)]
struct ActiveRun {
    id: u64,
    cancel: Cancel,
}

#[derive(Debug)]
struct ActiveDownload {
    id: String,
    cancel: Cancel,
}

impl AsrState {
    /// Claim the one run slot. The id is fresh whether or not the claim succeeds, so no two runs
    /// can ever share one, and every event carries an id the UI can match against.
    fn begin_run(&self, cancel: Cancel) -> Result<u64, AsrError> {
        let id = self.next_run_id.fetch_add(1, Ordering::SeqCst) + 1;
        let mut slot = lock(&self.run);
        if slot.is_some() {
            return Err(AsrError::new(
                AsrErrorCode::Busy,
                "a transcription is already running",
            ));
        }
        // The run that is starting supersedes the one before it, and a run that ends any other way
        // than finishing leaves nothing behind (decision 24, C3).
        *lock(&self.finished) = None;
        *slot = Some(ActiveRun { id, cancel });
        Ok(id)
    }

    /// Hold what a run produced, for as long as it is the last one to have finished.
    fn keep_finished(&self, id: u64, srt: Vec<u8>) {
        *lock(&self.finished) = Some(FinishedRun { id, srt });
    }

    /// Free the slot if it still holds this run. A late finisher must never free a newer run.
    fn end_run(&self, id: u64) {
        let mut slot = lock(&self.run);
        if slot.as_ref().is_some_and(|active| active.id == id) {
            *slot = None;
        }
    }

    /// The canceller for `id`, or nothing when that run is already over.
    fn run_cancel(&self, id: u64) -> Option<Cancel> {
        lock(&self.run)
            .as_ref()
            .filter(|active| active.id == id)
            .map(|active| active.cancel.clone())
    }

    fn begin_download(&self, id: &str, cancel: Cancel) -> Result<(), AsrError> {
        let mut slot = lock(&self.download);
        if let Some(active) = slot.as_ref() {
            return Err(AsrError::new(
                AsrErrorCode::Busy,
                format!("{} is already downloading", active.id),
            ));
        }
        *slot = Some(ActiveDownload {
            id: id.to_owned(),
            cancel,
        });
        Ok(())
    }

    fn end_download(&self, id: &str) {
        let mut slot = lock(&self.download);
        if slot.as_ref().is_some_and(|active| active.id == id) {
            *slot = None;
        }
    }

    fn download_cancel(&self, id: &str) -> Option<Cancel> {
        lock(&self.download)
            .as_ref()
            .filter(|active| active.id == id)
            .map(|active| active.cancel.clone())
    }
}

/// Holds the one run slot for as long as a run is in flight. A guard rather than a pair of calls
/// because a panic inside the sidecar would otherwise leave the app refusing every later run.
struct RunSlot {
    app: AppHandle,
    run_id: u64,
}

impl RunSlot {
    /// Give the slot back now. Idempotent, and it never frees a slot a newer run has claimed.
    fn release(&self) {
        // Managed for the app's whole life, so this is always Some; asking rather than indexing
        // keeps a teardown race from turning into a panic on a worker thread.
        if let Some(state) = self.app.try_state::<AsrState>() {
            state.end_run(self.run_id);
        }
    }
}

impl Drop for RunSlot {
    fn drop(&mut self) {
        self.release();
    }
}

/// Stop whatever is in flight, on the way out. The child is killed rather than waited for: the
/// process is about to end, and a whisper run left behind would outlive the window that started
/// it. See BACKLOG.md M3.1.
pub fn shutdown(app: &AppHandle) {
    let Some(state) = app.try_state::<AsrState>() else {
        return;
    };
    // Taken out of the locks first: nothing is held while a child is being killed.
    let run = lock(&state.run)
        .as_ref()
        .map(|active| active.cancel.clone());
    let download = lock(&state.download)
        .as_ref()
        .map(|active| active.cancel.clone());
    for cancel in run.into_iter().chain(download) {
        cancel.cancel();
    }
}

/// The SRT the run `run_id` produced, or nothing once another run has replaced it. Cloned rather
/// than taken: an adoption the user cancels leaves the result where it was. See BACKLOG.md M3.5.
pub fn finished_srt(state: &AsrState, run_id: u64) -> Option<Vec<u8>> {
    lock(&state.finished)
        .as_ref()
        .filter(|finished| finished.id == run_id)
        .map(|finished| finished.srt.clone())
}

/// A poisoned lock still has to be able to cancel a child, so the poison is stepped over rather
/// than propagated: leaving a process running would be the worse failure.
fn lock<T>(slot: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    slot.lock().unwrap_or_else(|error| error.into_inner())
}

/// Remove run directories an earlier Sublore was killed before it could clean up. Age-based, so a
/// second copy running right now is never harmed. See `sublore_asr::scratch`.
pub fn sweep_scratch(app: &AppHandle) {
    let Ok(root) = scratch_root(app) else {
        return;
    };
    tauri::async_runtime::spawn_blocking(move || {
        let removed = scratch::sweep(&root, scratch::SWEEP_MAX_AGE);
        if removed > 0 {
            crate::log::info!("swept {removed} abandoned transcription directories");
        }
    });
}

#[tauri::command]
pub async fn asr_models(app: AppHandle) -> Result<Vec<AsrModelStatus>, AsrError> {
    let dir = models_dir(&app)?;
    // A directory listing, so it never runs on the poll thread. It also never opens a socket:
    // what Sublore knows about a model is the catalog plus what is on disk (CONTRIBUTING.md §1).
    blocking(move || Ok(statuses(&ModelStore::new(dir)))).await
}

/// The only command that can open a socket, and it opens one because the user pressed Download.
#[tauri::command]
pub async fn asr_model_download(
    app: AppHandle,
    state: State<'_, AsrState>,
    id: String,
) -> Result<(), AsrError> {
    let spec = catalog::find(&id).ok_or_else(|| {
        AsrError::new(
            AsrErrorCode::ModelMissing,
            format!("{id:?} is not a model Sublore knows"),
        )
    })?;
    let dir = models_dir(&app)?;
    let cancel = Cancel::new();
    state.begin_download(&id, cancel.clone())?;

    let handle = app.clone();
    let progress_id = id.clone();
    let outcome = blocking(move || {
        download(
            &ModelStore::new(dir),
            spec,
            catalog::BASE_URL,
            &HttpFetcher::new(),
            &cancel,
            &|received_bytes, total_bytes| {
                let _ = handle.emit(
                    EVENT_MODEL_PROGRESS,
                    AsrModelProgress {
                        id: progress_id.clone(),
                        received_bytes,
                        total_bytes,
                    },
                );
            },
        )
        .map(|_| ())
        .map_err(AsrError::from)
    })
    .await;

    // Whatever happened, the slot is free again: a failed download must not lock the button out.
    state.end_download(&id);
    outcome
}

#[tauri::command]
pub async fn asr_model_download_cancel(
    state: State<'_, AsrState>,
    id: String,
) -> Result<(), AsrError> {
    // A cancel for a download that already finished is not an error: the user clicked as it ended.
    if let Some(cancel) = state.download_cancel(&id) {
        cancel.cancel();
    }
    Ok(())
}

/// Start a run and return its id. The transcription itself happens on a blocking task and reports
/// through `asr://progress`, then exactly one `asr://done` or `asr://error`.
#[tauri::command]
pub async fn asr_transcribe_start(
    app: AppHandle,
    state: State<'_, AsrState>,
    media: String,
    model_id: String,
    compute: AsrCompute,
) -> Result<AsrRunStarted, AsrError> {
    let models = models_dir(&app)?;
    let scratch_root = scratch_root(&app)?;
    let resource_dir = app.path().resource_dir().ok();

    let cancel = Cancel::new();
    let run_id = state.begin_run(cancel.clone())?;
    // From here the slot is owned by a guard, so every way out of this command gives it back.
    let slot = RunSlot {
        app: app.clone(),
        run_id,
    };

    // Everything that can fail before a process exists is answered here, so the caller gets a
    // rejection it can show at once instead of a run that starts and dies.
    let media = PathBuf::from(media);
    let preflight = {
        let media = media.clone();
        blocking(move || {
            if !media.is_file() {
                return Err(AsrError::new(
                    AsrErrorCode::MediaUnreadable,
                    format!("{} is not a file", media.display()),
                ));
            }
            let tools = Tools::discover(resource_dir.as_deref(), scratch_root)?;
            let model = ModelStore::new(models).resolve(&model_id)?;
            Ok((tools, model))
        })
        .await
    };
    // `slot` is dropped with the error, which frees the run.
    let (tools, model) = preflight?;

    let request = TranscribeRequest::new(media, model, Language::Auto, compute.into());
    let handle = app.clone();
    // Blocking, not async: process handling in std blocks, and this must never sit on a poll
    // thread. The window keeps answering while it runs, which is what the E2E asserts.
    tauri::async_runtime::spawn_blocking(move || {
        // `slot` moves in with this closure, so the run is freed even if the sidecar panics:
        // a bug there cannot leave the app permanently refusing to start another transcription.
        let progress = |phase: Phase, percent: u8| {
            let _ = handle.emit(
                EVENT_PROGRESS,
                AsrProgress {
                    run_id,
                    phase: phase_name(phase),
                    percent,
                },
            );
        };
        let outcome = transcribe(&tools, &request, &cancel, &progress)
            .map_err(AsrError::from)
            .and_then(|transcript| done_payload(run_id, transcript));

        // Freed before the outcome is announced, so a user who clicks Transcribe the moment the
        // result appears is not told Sublore is busy.
        slot.release();
        match outcome {
            Ok(finished) => {
                crate::log::info!(
                    "transcription {run_id} produced {} cues on the {}",
                    finished.done.cues.len(),
                    finished.done.backend
                );
                // Kept before the event goes out: the UI answers it by asking for these cues, and
                // an event that arrives first would find nothing to adopt. See BACKLOG.md M3.5.
                if let Some(state) = handle.try_state::<AsrState>() {
                    state.keep_finished(run_id, finished.srt);
                }
                let _ = handle.emit(EVENT_DONE, finished.done);
            }
            Err(error) => {
                crate::log::warn!("transcription {run_id} ended: {error}");
                let _ = handle.emit(
                    EVENT_ERROR,
                    AsrRunFailed {
                        run_id,
                        code: error.code,
                        detail: error.detail,
                    },
                );
            }
        }
    });

    Ok(AsrRunStarted { run_id })
}

#[tauri::command]
pub async fn asr_transcribe_cancel(
    state: State<'_, AsrState>,
    run_id: u64,
) -> Result<(), AsrError> {
    // Cancelling a run that just finished is not an error, and cancelling an older one must never
    // reach into the current one, which is why the id is checked.
    if let Some(cancel) = state.run_cancel(run_id) {
        cancel.cancel();
    }
    Ok(())
}

/// What a run that reached the end leaves behind: the payload the UI is told, and the SRT those
/// same cues render to, which is what becomes the document. See BACKLOG.md M3.5.
pub struct Finished {
    pub done: AsrDone,
    pub srt: Vec<u8>,
}

/// Segment a transcript into cues and describe them the way the UI lists them.
///
/// The bytes go out through `sublore_formats::parse`, the same door a file the user opened comes
/// in by, so M1's coverage guard runs on generated subtitles too and the cues shown are the cues a
/// saved SRT would hold. See BACKLOG.md M3.3.
pub fn done_payload(run_id: u64, transcript: Transcript) -> Result<Finished, AsrError> {
    let generated = cues::segment(&transcript.words, transcript.audio_duration_ms);
    let bytes = render::srt(&generated);
    let document = parse(SubtitleFormat::Srt, &bytes).map_err(|error| {
        AsrError::new(
            AsrErrorCode::Internal,
            format!("the generated SRT did not parse: {error}"),
        )
    })?;
    let cues = document
        .cues()
        .map(|cue| AsrCue {
            start_ms: cue.start.millis(),
            end_ms: cue.end.millis(),
            lines: document
                .slice(cue.text)
                .split('\n')
                .map(str::to_owned)
                .collect(),
        })
        .collect();

    Ok(Finished {
        done: AsrDone {
            run_id,
            backend: backend_name(transcript.backend),
            fell_back_to_cpu: transcript.fell_back_to_cpu,
            audio_duration_ms: transcript.audio_duration_ms,
            cues,
        },
        srt: bytes,
    })
}

pub fn statuses(store: &ModelStore) -> Vec<AsrModelStatus> {
    store
        .statuses()
        .into_iter()
        .map(|status| AsrModelStatus {
            id: status.spec.id.to_owned(),
            bytes: status.spec.bytes,
            state: state_name(status.state),
            downloaded_bytes: status.downloaded_bytes,
        })
        .collect()
}

/// Run `work` off the async runtime's poll thread and flatten the join failure into a wire error.
async fn blocking<T, F>(work: F) -> Result<T, AsrError>
where
    F: FnOnce() -> Result<T, AsrError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            AsrError::new(
                AsrErrorCode::CommandFailed,
                format!("the transcription task failed: {error}"),
            )
        })?
}

fn models_dir(app: &AppHandle) -> Result<PathBuf, AsrError> {
    Ok(app_data_dir(app)?.join(MODELS_DIR))
}

fn scratch_root(app: &AppHandle) -> Result<PathBuf, AsrError> {
    Ok(app_data_dir(app)?.join(SCRATCH_DIR))
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, AsrError> {
    app.path().app_data_dir().map_err(|error| {
        AsrError::new(
            AsrErrorCode::CommandFailed,
            format!("no app data directory: {error}"),
        )
    })
}

/// The wire spelling of a model's state. Stable: the UI maps it to copy.
fn state_name(state: ModelState) -> &'static str {
    match state {
        ModelState::Missing => "missing",
        ModelState::Partial => "partial",
        ModelState::Ready => "ready",
        ModelState::Corrupt => "corrupt",
    }
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Extracting => "extracting",
        Phase::Transcribing => "transcribing",
    }
}

fn backend_name(backend: Backend) -> &'static str {
    match backend {
        Backend::Gpu => "gpu",
        Backend::Cpu => "cpu",
    }
}

#[cfg(test)]
mod tests {
    use super::error::AsrErrorCode;
    use super::{
        backend_name, done_payload, finished_srt, phase_name, state_name, AsrCompute, AsrState,
    };
    use sublore_asr::model::ModelState;
    use sublore_asr::sidecar::{Cancel, Compute, Phase};
    use sublore_asr::transcript::{Backend, Transcript, Word};

    fn word(text: &str, start_ms: u32, end_ms: u32) -> Word {
        Word {
            text: text.to_owned(),
            start_ms,
            end_ms,
        }
    }

    fn transcript(words: Vec<Word>) -> Transcript {
        Transcript {
            language: "en".to_owned(),
            words,
            backend: Backend::Cpu,
            fell_back_to_cpu: true,
            audio_duration_ms: 60_000,
        }
    }

    #[test]
    fn a_transcript_becomes_cues_the_ui_can_list() {
        let finished = done_payload(
            7,
            transcript(vec![
                word("Sublore", 200, 700),
                word("keeps", 700, 1000),
                word("your", 1000, 1200),
                word("terminology.", 1200, 1900),
            ]),
        )
        .expect("a short transcript segments and parses");
        let done = finished.done;
        assert!(
            String::from_utf8(finished.srt)
                .expect("the SRT is UTF-8")
                .contains("Sublore keeps your terminology."),
            "the bytes kept for the document are the cues the UI was shown"
        );

        assert_eq!(done.run_id, 7);
        assert_eq!(done.backend, "cpu");
        assert!(done.fell_back_to_cpu);
        assert_eq!(done.audio_duration_ms, 60_000);
        assert_eq!(done.cues.len(), 1);
        assert_eq!(done.cues[0].lines, ["Sublore keeps your terminology."]);
        assert_eq!(done.cues[0].start_ms, 200);
        assert!(done.cues[0].end_ms > done.cues[0].start_ms);
    }

    #[test]
    fn a_two_line_cue_arrives_as_two_lines_with_no_terminators_in_them() {
        // 84 characters is the cue width limit, so this splits across both lines of one cue.
        let words: Vec<_> = (0..12)
            .map(|index| word("terminology", index * 300, index * 300 + 250))
            .collect();
        let done = done_payload(1, transcript(words))
            .expect("it segments and parses")
            .done;

        for cue in &done.cues {
            assert!(!cue.lines.is_empty());
            assert!(
                cue.lines.len() <= 2,
                "a cue never draws more than two lines"
            );
            for line in &cue.lines {
                assert!(!line.contains('\n') && !line.contains('\r'), "{line:?}");
                assert!(!line.is_empty());
            }
        }
        assert!(
            done.cues.iter().any(|cue| cue.lines.len() == 2),
            "twelve long words have to wrap somewhere: {:?}",
            done.cues
        );
    }

    #[test]
    fn an_empty_transcript_produces_no_cues_rather_than_a_broken_document() {
        let finished = done_payload(2, transcript(Vec::new())).expect("an empty word list parses");
        assert!(finished.done.cues.is_empty());
        assert!(finished.srt.is_empty(), "no cues is an empty file");
    }

    #[test]
    fn the_wire_spellings_are_the_ones_the_typescript_expects() {
        assert_eq!(state_name(ModelState::Missing), "missing");
        assert_eq!(state_name(ModelState::Partial), "partial");
        assert_eq!(state_name(ModelState::Ready), "ready");
        assert_eq!(state_name(ModelState::Corrupt), "corrupt");
        assert_eq!(phase_name(Phase::Extracting), "extracting");
        assert_eq!(phase_name(Phase::Transcribing), "transcribing");
        assert_eq!(backend_name(Backend::Gpu), "gpu");
        assert_eq!(backend_name(Backend::Cpu), "cpu");
        assert_eq!(Compute::from(AsrCompute::Gpu), Compute::Gpu);
        assert_eq!(Compute::from(AsrCompute::Cpu), Compute::Cpu);
    }

    #[test]
    fn a_second_run_is_refused_while_one_is_going_and_allowed_once_it_ends() {
        let state = AsrState::default();
        let first = state.begin_run(Cancel::new()).expect("nothing is running");
        let refused = state
            .begin_run(Cancel::new())
            .expect_err("one run at a time");
        assert_eq!(refused.code, AsrErrorCode::Busy);

        state.end_run(first);
        let second = state.begin_run(Cancel::new()).expect("the slot is free");
        assert_ne!(second, first, "ids are never reused");
    }

    #[test]
    fn a_finished_run_never_frees_a_newer_ones_slot_or_cancels_it() {
        let state = AsrState::default();
        let first = state.begin_run(Cancel::new()).expect("nothing is running");
        state.end_run(first);
        let second = state.begin_run(Cancel::new()).expect("the slot is free");

        // The late tail of the first run tidying up must not touch the run that replaced it.
        state.end_run(first);
        assert!(state.run_cancel(second).is_some(), "the second run is live");
        assert!(state.run_cancel(first).is_none());

        // Cancelling by id reaches only that run.
        let cancel = state.run_cancel(second).expect("live");
        cancel.cancel();
        assert!(cancel.is_cancelled());
        state.end_run(second);
        assert!(state.run_cancel(second).is_none());
    }

    #[test]
    fn what_a_run_produced_is_kept_until_another_run_starts() {
        let state = AsrState::default();
        let first = state.begin_run(Cancel::new()).expect("nothing is running");
        state.keep_finished(first, b"1\n".to_vec());
        state.end_run(first);

        assert_eq!(finished_srt(&state, first), Some(b"1\n".to_vec()));
        // Only by its own id: a stale id must never be handed another run's cues.
        assert_eq!(finished_srt(&state, first + 1), None);

        // A run that starts supersedes it, and one that ends any other way leaves nothing behind.
        let second = state.begin_run(Cancel::new()).expect("the slot is free");
        assert_eq!(finished_srt(&state, first), None);
        assert_eq!(finished_srt(&state, second), None);
    }

    #[test]
    fn one_download_at_a_time_and_the_slot_is_free_again_afterwards() {
        let state = AsrState::default();
        state
            .begin_download("tiny.en", Cancel::new())
            .expect("nothing is downloading");
        let refused = state
            .begin_download("base.en", Cancel::new())
            .expect_err("one download at a time");
        assert_eq!(refused.code, AsrErrorCode::Busy);
        assert!(refused.detail.contains("tiny.en"), "{}", refused.detail);

        // A cancel names the model, so a click on the wrong row cannot stop the live download.
        assert!(state.download_cancel("base.en").is_none());
        assert!(state.download_cancel("tiny.en").is_some());

        state.end_download("base.en");
        assert!(
            state.download_cancel("tiny.en").is_some(),
            "another model finishing must not free this one's slot"
        );
        state.end_download("tiny.en");
        state
            .begin_download("base.en", Cancel::new())
            .expect("the slot is free");
    }
}
