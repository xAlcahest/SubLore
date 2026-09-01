//! One open subtitle file and the commands that edit it. The session lives here, behind a mutex,
//! because the document is the authority on its own bytes: the frontend holds a list of rows and a
//! revision number, never a second model. The IPC names and payloads here are a public interface
//! (CONTRIBUTING.md section 6). See BACKLOG.md M2.3.

pub mod error;

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::Instant;

use serde::Serialize;
use sublore_edit::diff::{CuePatch, CueView};
use sublore_edit::history::Run;
use sublore_edit::plan::{self, Edit};
use sublore_edit::session::EditSession;
use sublore_formats::{parse, Newline, SubtitleDocument, SubtitleFormat};
use sublore_io::atomic::save_with_backup;
use sublore_io::backup::BackupStore;
use tauri::{AppHandle, Manager, State};

use crate::asr::AsrState;
use crate::dialog::CloseAnswer;
use error::{SubtitleError, SubtitleErrorCode};

/// Bigger than any subtitle file that exists. A user who points at a 4 GB video gets a sentence
/// rather than an out-of-memory kill.
pub const MAX_SUBTITLE_BYTES: u64 = 16 * 1024 * 1024;

/// Backups live under Sublore's own data directory, never beside the user's file (CONTRIBUTING.md §3.5).
const BACKUP_DIR: &str = "backups";

/// The one open file, or none. A plain `Mutex`: every command body runs inside `spawn_blocking`,
/// so the guard is never held across an await.
pub type SessionSlot = Mutex<Option<EditSession>>;

#[derive(Default)]
pub struct SubtitleState {
    session: Arc<SessionSlot>,
}

impl SubtitleState {
    /// A handle the blocking half of a command can own, as `VideoState` hands out its player.
    // TODO(M2.6): narrow back to private. Public only so the close gate in `lib.rs` can read the
    // session; M2.6 reshapes this signature for two documents anyway (owner ruling 2026-08-29).
    pub fn slot(&self) -> Arc<SessionSlot> {
        Arc::clone(&self.session)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSummary {
    /// Where the document came from, or none while it has never had a file (BACKLOG.md M3.5).
    pub path: Option<String>,
    /// "srt" | "vtt" | "ass".
    pub format: String,
    /// Cues a player would draw; ASS `Comment:` events are not among them.
    pub cue_count: usize,
    pub has_bom: bool,
    /// "lf" | "crlf" | "mixed" | "none".
    pub newline: String,
    pub byte_length: u64,
}

/// A row of the cue list. Its index is its position in the list, so a patch that moves rows can
/// never leave a stale index behind on the rows it did not resend.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CueRowDto {
    pub start_ms: u32,
    pub end_ms: u32,
    /// Line breaks are always "\n" here, whatever the file uses.
    pub text: String,
    /// An ASS `Comment:` event: listed and editable, but not a line a player draws.
    pub comment: bool,
    /// The cue's own number, when the file wrote one. Never renumbered.
    pub number: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleOpened {
    pub summary: SubtitleSummary,
    pub revision: u64,
    /// Every cue, in `cues()` order: ASS comments included, unlike `summary.cue_count`.
    pub cues: Vec<CueRowDto>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
    pub truncated: bool,
}

/// One contiguous run of rows replaced by another, plus the state that changed with it. Every
/// mutation, undo and redo answers with one of these, so the UI has a single reply shape.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CuePatchDto {
    pub revision: u64,
    pub from: usize,
    pub removed: usize,
    pub cues: Vec<CueRowDto>,
    /// For the status line: ASS `Comment:` events excluded, as at open.
    pub cue_count: usize,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSaved {
    pub path: String,
    pub bytes_written: u64,
    /// Absent when the destination did not exist before.
    pub backup_path: Option<String>,
    /// Whether the document still holds edits that are not on disk. A copy written elsewhere leaves
    /// a file-backed document unsaved and an untitled one saved, so the write reports it rather
    /// than the UI guessing. See BACKLOG.md M3.5.
    pub dirty: bool,
}

// -------------------------------------------------------------------------------------------
// Commands
// -------------------------------------------------------------------------------------------

#[tauri::command]
pub async fn subtitle_open(
    state: State<'_, SubtitleState>,
    path: String,
) -> Result<SubtitleOpened, SubtitleError> {
    let slot = state.slot();
    blocking(move || open_session(&slot, &path)).await
}

#[tauri::command]
pub async fn subtitle_close(
    state: State<'_, SubtitleState>,
    discard: bool,
) -> Result<(), SubtitleError> {
    let slot = state.slot();
    blocking(move || close_session(&slot, discard)).await
}

#[tauri::command]
pub async fn subtitle_set_text(
    state: State<'_, SubtitleState>,
    revision: u64,
    cue: usize,
    text: String,
) -> Result<CuePatchDto, SubtitleError> {
    let slot = state.slot();
    blocking(move || apply_edit(&slot, revision, Edit::SetText { cue, text })).await
}

#[tauri::command]
pub async fn subtitle_set_times(
    state: State<'_, SubtitleState>,
    revision: u64,
    cue: usize,
    start_ms: u32,
    end_ms: u32,
) -> Result<CuePatchDto, SubtitleError> {
    let slot = state.slot();
    blocking(move || {
        apply_edit(
            &slot,
            revision,
            Edit::SetTimes {
                cue,
                start_ms,
                end_ms,
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn subtitle_insert(
    state: State<'_, SubtitleState>,
    revision: u64,
    before: usize,
    start_ms: u32,
    end_ms: u32,
    text: String,
) -> Result<CuePatchDto, SubtitleError> {
    let slot = state.slot();
    blocking(move || {
        apply_edit(
            &slot,
            revision,
            Edit::Insert {
                before,
                start_ms,
                end_ms,
                text,
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn subtitle_delete(
    state: State<'_, SubtitleState>,
    revision: u64,
    cue: usize,
) -> Result<CuePatchDto, SubtitleError> {
    let slot = state.slot();
    blocking(move || apply_edit(&slot, revision, Edit::Delete { cue })).await
}

#[tauri::command]
pub async fn subtitle_split(
    state: State<'_, SubtitleState>,
    revision: u64,
    cue: usize,
    text_offset: usize,
    at_ms: u32,
) -> Result<CuePatchDto, SubtitleError> {
    let slot = state.slot();
    blocking(move || {
        apply_edit(
            &slot,
            revision,
            Edit::Split {
                cue,
                text_offset,
                at_ms,
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn subtitle_merge(
    state: State<'_, SubtitleState>,
    revision: u64,
    cue: usize,
) -> Result<CuePatchDto, SubtitleError> {
    let slot = state.slot();
    blocking(move || apply_edit(&slot, revision, Edit::Merge { cue })).await
}

#[tauri::command]
pub async fn subtitle_undo(
    state: State<'_, SubtitleState>,
    revision: u64,
) -> Result<CuePatchDto, SubtitleError> {
    let slot = state.slot();
    blocking(move || undo(&slot, revision)).await
}

#[tauri::command]
pub async fn subtitle_redo(
    state: State<'_, SubtitleState>,
    revision: u64,
) -> Result<CuePatchDto, SubtitleError> {
    let slot = state.slot();
    blocking(move || redo(&slot, revision)).await
}

#[tauri::command]
pub async fn subtitle_save(
    app: AppHandle,
    state: State<'_, SubtitleState>,
    revision: u64,
) -> Result<SubtitleSaved, SubtitleError> {
    let slot = state.slot();
    let backups = backup_root(&app)?;
    blocking(move || save(&slot, revision, backups)).await
}

#[tauri::command]
pub async fn subtitle_save_as(
    app: AppHandle,
    state: State<'_, SubtitleState>,
    revision: u64,
    destination: String,
) -> Result<SubtitleSaved, SubtitleError> {
    let slot = state.slot();
    let backups = backup_root(&app)?;
    blocking(move || save_as(&slot, revision, &destination, backups)).await
}

/// Make the cues a finished transcription produced the open document.
///
/// Nothing is written to disk here: the result lives in the session until the user saves it, and
/// the media file is never touched (CONTRIBUTING.md §3.1). `Ok(None)` is the user answering Cancel,
/// which leaves both the document that was open and the transcription result exactly as they were.
/// See BACKLOG.md M3.5.
#[tauri::command]
pub async fn subtitle_adopt_transcription(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, SubtitleState>,
    asr: State<'_, AsrState>,
    run_id: u64,
) -> Result<Option<SubtitleOpened>, SubtitleError> {
    let srt = crate::asr::finished_srt(&asr, run_id).ok_or_else(|| {
        SubtitleError::new(
            SubtitleErrorCode::TranscriptionGone,
            format!("run {run_id} is not the transcription that finished last"),
        )
    })?;
    let slot = state.slot();
    let backups = backup_root(&app)?;

    let adopted = {
        let (slot, srt) = (Arc::clone(&slot), srt.clone());
        blocking(move || adopt_if_clean(&slot, &srt)).await?
    };
    if let Some(opened) = adopted {
        return Ok(Some(opened));
    }

    // Unsaved work is in the way, so the user is asked the same three answers the close gate asks
    // before anything replaces it (decision 24, B1).
    let answer = ask_about_unsaved(&app, window.label()).await?;
    blocking(move || adopt_answered(&slot, &srt, answer, backups)).await
}

// -------------------------------------------------------------------------------------------
// The bodies, free of Tauri so the suite can drive them
// -------------------------------------------------------------------------------------------

/// Read `path`, parse it, and make it the open file. Refused while the open file has unsaved
/// edits: dropping the user's work is a decision only the user makes (CONTRIBUTING.md §3).
pub fn open_session(slot: &SessionSlot, path: &str) -> Result<SubtitleOpened, SubtitleError> {
    let mut guard = lock(slot)?;
    if guard.as_ref().is_some_and(EditSession::dirty) {
        return Err(SubtitleError::new(
            SubtitleErrorCode::UnsavedChanges,
            "the open file has edits that are not on disk",
        ));
    }

    // A file that did not open is not the file on screen either, and the one being replaced was
    // just proven saved, so closing it first loses nothing.
    *guard = None;
    let document = read_document(Path::new(path))?;
    let summary = summarize(Some(path), &document);
    let session = EditSession::open(PathBuf::from(path), document);
    let opened = opened_payload(&session, summary);
    // Said out loud because it is the one moment a document becomes the one on screen, and because
    // nothing else could observe it: the harness had to guess with fixed waits, and the guess was
    // calibrated on fast hardware (gate 2, the CI run of 2026-08-30).
    crate::log::info!(
        "subtitle: opened {path} — {} cues, {}",
        opened.cues.len(),
        if opened.truncated {
            "truncated"
        } else {
            "whole"
        }
    );
    *guard = Some(session);
    Ok(opened)
}

/// Close the open file. `discard` is the user having chosen to lose the edits; without it an
/// unsaved file stays open.
pub fn close_session(slot: &SessionSlot, discard: bool) -> Result<(), SubtitleError> {
    let mut guard = lock(slot)?;
    if !discard && guard.as_ref().is_some_and(EditSession::dirty) {
        return Err(SubtitleError::new(
            SubtitleErrorCode::UnsavedChanges,
            "the open file has edits that are not on disk",
        ));
    }
    *guard = None;
    Ok(())
}

/// Make `srt` the open document, unless unsaved edits are in the way. `Ok(None)` means the user
/// has to be asked before anything is replaced.
pub fn adopt_if_clean(
    slot: &SessionSlot,
    srt: &[u8],
) -> Result<Option<SubtitleOpened>, SubtitleError> {
    let mut guard = lock(slot)?;
    if guard.as_ref().is_some_and(EditSession::dirty) {
        return Ok(None);
    }
    adopt_locked(&mut guard, srt).map(Some)
}

/// Act on what the user answered about the unsaved document in the way. `Ok(None)` is Cancel.
///
/// The save and the replacement happen under one lock, so a save that fails replaces nothing and
/// nothing can be typed between the two (CONTRIBUTING.md §3).
pub fn adopt_answered(
    slot: &SessionSlot,
    srt: &[u8],
    answer: CloseAnswer,
    backup_root: PathBuf,
) -> Result<Option<SubtitleOpened>, SubtitleError> {
    if answer == CloseAnswer::Cancel {
        return Ok(None);
    }
    let mut guard = lock(slot)?;
    if answer == CloseAnswer::Save {
        // Nothing to save if the document was closed while the question was up, and a clean one
        // needs no write.
        if let Some(session) = guard.as_mut().filter(|session| session.dirty()) {
            save_locked(session, backup_root)?;
        }
    }
    adopt_locked(&mut guard, srt).map(Some)
}

/// The replacement itself: the generated SRT becomes a document with no file, unsaved from the
/// first moment because these bytes exist nowhere else. See BACKLOG.md M3.5.
fn adopt_locked(
    guard: &mut MutexGuard<'_, Option<EditSession>>,
    srt: &[u8],
) -> Result<SubtitleOpened, SubtitleError> {
    // A document Sublore could not open again must not be creatable either, so the same bound the
    // reader and the editor hold applies here.
    if srt.len() as u64 > MAX_SUBTITLE_BYTES {
        return Err(SubtitleError::new(
            SubtitleErrorCode::TooLarge,
            format!("{} bytes, limit {MAX_SUBTITLE_BYTES}", srt.len()),
        ));
    }
    let document = parse(SubtitleFormat::Srt, srt).map_err(SubtitleError::from_parse)?;
    let summary = summarize(None, &document);
    let session = EditSession::untitled(document);
    let opened = opened_payload(&session, summary);
    // The other moment a document becomes the one on screen, said out loud for the same reason
    // `open_session` says it: nothing else outside the window can observe it.
    crate::log::info!(
        "subtitle: adopted a transcription — {} cues, unsaved",
        opened.cues.len()
    );
    **guard = Some(session);
    Ok(opened)
}

/// Raise the unsaved-changes question and wait for its answer.
///
/// The answer is delivered on a thread of the dialog's own, and every way of losing the dialog
/// answers Cancel (`dialog::Delivery`), so the wait below always ends.
async fn ask_about_unsaved(app: &AppHandle, label: &str) -> Result<CloseAnswer, SubtitleError> {
    let (send, receive) = mpsc::channel();
    crate::dialog::ask_unsaved(
        app,
        label,
        crate::strings::REPLACE_UNSAVED_BODY,
        move |answer| {
            // The receiver is gone only if this command was dropped, and then the answer decides
            // nothing: the document has not been touched.
            let _ = send.send(answer);
        },
    )
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::CommandFailed,
            format!("the unsaved-changes question could not be raised: {error}"),
        )
    })?;
    // Off the poll thread: this waits for a person. A dropped sender means the answer thread died
    // holding the question, which is the one case nobody answers, and Cancel keeps both documents.
    blocking(move || Ok(receive.recv().unwrap_or(CloseAnswer::Cancel))).await
}

/// Apply one mutation. Nothing is written to disk here: a save is its own command.
pub fn apply_edit(
    slot: &SessionSlot,
    revision: u64,
    edit: Edit,
) -> Result<CuePatchDto, SubtitleError> {
    let mut guard = lock(slot)?;
    let session = current(&mut guard)?;
    check_revision(session, revision)?;
    guard_size(session, &edit)?;

    // Every command carries one finished edit: the editor sends a field when it is committed,
    // never a keystroke, so two of them are two undo steps. See BACKLOG.md M2.2.
    let patch = session
        .apply(&edit, Run::New, Instant::now())
        .map_err(SubtitleError::from_edit)?;
    // One line per committed edit, not per keystroke: the editor sends a field when it is finished.
    // It is the only outside evidence that an edit landed. The text length is here and the text is
    // not: a length is enough to tell a real edit from a field committed unchanged, and a subtitle
    // line is the user's own writing.
    crate::log::info!(
        "subtitle: edit committed, revision {}, {}, cue {} now {} chars",
        session.revision(),
        if session.dirty() { "dirty" } else { "clean" },
        patch.from,
        patch.cues.first().map_or(0, |cue| cue.text.chars().count())
    );
    Ok(describe(session, patch))
}

pub fn undo(slot: &SessionSlot, revision: u64) -> Result<CuePatchDto, SubtitleError> {
    let mut guard = lock(slot)?;
    let session = current(&mut guard)?;
    check_revision(session, revision)?;

    let patch = session
        .undo()
        .map_err(SubtitleError::from_edit)?
        .unwrap_or_else(nothing_changed);
    Ok(describe(session, patch))
}

pub fn redo(slot: &SessionSlot, revision: u64) -> Result<CuePatchDto, SubtitleError> {
    let mut guard = lock(slot)?;
    let session = current(&mut guard)?;
    check_revision(session, revision)?;

    let patch = session
        .redo()
        .map_err(SubtitleError::from_edit)?
        .unwrap_or_else(nothing_changed);
    Ok(describe(session, patch))
}

/// Write the document back where it came from, and call it saved.
pub fn save(
    slot: &SessionSlot,
    revision: u64,
    backup_root: PathBuf,
) -> Result<SubtitleSaved, SubtitleError> {
    let mut guard = lock(slot)?;
    let session = current(&mut guard)?;
    check_revision(session, revision)?;
    save_locked(session, backup_root)
}

/// The write itself, under a lock the caller already holds. Shared so the close gate cannot drift
/// from the command, and so neither of them takes the lock twice.
fn save_locked(
    session: &mut EditSession,
    backup_root: PathBuf,
) -> Result<SubtitleSaved, SubtitleError> {
    // A document that has never had a file has nowhere to be written back to; naming a destination
    // is what Save as is for. See BACKLOG.md M3.5.
    let path = session
        .path()
        .ok_or_else(|| {
            SubtitleError::new(
                SubtitleErrorCode::NoPath,
                "this document has never had a file, so there is nowhere to write it back to",
            )
        })?
        .to_path_buf();
    let bytes = session.to_bytes();
    let outcome = save_with_backup(&path, &bytes, &BackupStore::new(backup_root))
        .map_err(SubtitleError::from_io)?;
    session.mark_saved();
    // What was written and where, because "the save succeeded" and "the file on disk changed" are
    // different claims and CI has already shown them disagreeing (gate 2, run 33363671401).
    crate::log::info!("subtitle: saved {} — {} bytes", path.display(), bytes.len());
    Ok(saved(outcome, session.dirty()))
}

/// What the close gate needs before it decides whether to ask (BACKLOG N1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Clean,
    Dirty,
    /// The lock could not be taken: a command holds it, or one panicked holding it. Both mean the
    /// gate must ask, because a needless question costs a click and a skipped one costs the work.
    Unknown,
}

/// Never blocks. The gate runs on the main loop, and this mutex is held for the whole of
/// `read_document` and of `save_with_backup`, so waiting here would freeze the window mid-save
/// (CONTRIBUTING.md §7).
pub fn session_state(slot: &SessionSlot) -> SessionState {
    match slot.try_lock() {
        Ok(guard) => match guard.as_ref() {
            Some(session) if session.dirty() => SessionState::Dirty,
            _ => SessionState::Clean,
        },
        Err(_) => SessionState::Unknown,
    }
}

/// Save at whatever revision the session holds, recovering a poisoned lock. `Ok(None)` means there
/// was nothing to write.
///
/// The interactive commands refuse a poisoned session and that is right for them: a refused edit
/// costs a retry. The close gate is the user's last chance to keep the work, so it recovers
/// instead. Sound here because a mutation never edits the document in place: `plan::edit` builds a
/// whole new document, `EditSession::commit` assigns it in one move, and `history` is only touched
/// after the new document exists, so a panic leaves the session holding one whole document or the
/// other and never half of one. The poison flag is cleared once the guard is in hand, or every
/// later command would keep refusing a session this call just proved usable.
///
/// A clean session writes nothing. The gate can open on a session that is merely busy, and an
/// unasked-for write would change the mtime of a file the user only opened, and would overwrite
/// whatever another program put there in the meantime (CONTRIBUTING.md §3.1).
pub fn save_current(
    slot: &SessionSlot,
    backup_root: PathBuf,
) -> Result<Option<SubtitleSaved>, SubtitleError> {
    let mut guard = match slot.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            slot.clear_poison();
            poisoned.into_inner()
        }
    };
    let session = current(&mut guard)?;
    if !session.dirty() {
        return Ok(None);
    }
    save_locked(session, backup_root).map(Some)
}

/// Write the document somewhere else. The file being edited keeps its unsaved edits, and the
/// session keeps pointing at it: saying otherwise would be a lie the user pays for.
pub fn save_as(
    slot: &SessionSlot,
    revision: u64,
    destination: &str,
    backup_root: PathBuf,
) -> Result<SubtitleSaved, SubtitleError> {
    if destination.is_empty() {
        return Err(SubtitleError::new(
            SubtitleErrorCode::InvalidPath,
            "the destination path is empty",
        ));
    }

    let mut guard = lock(slot)?;
    let session = current(&mut guard)?;
    check_revision(session, revision)?;

    let bytes = session.to_bytes();
    let outcome = save_with_backup(
        Path::new(destination),
        &bytes,
        &BackupStore::new(backup_root),
    )
    .map_err(SubtitleError::from_io)?;
    // A document with no file of its own has nothing to be out of step with, so this write is the
    // one place its bytes now exist and it is not unsaved work any more. See BACKLOG.md M3.5.
    if session.path().is_none() {
        session.mark_saved();
    }
    Ok(saved(outcome, session.dirty()))
}

/// What the file is, in the order a translator reads it.
pub fn summarize(path: Option<&str>, document: &SubtitleDocument) -> SubtitleSummary {
    let source = document.source();
    SubtitleSummary {
        path: path.map(str::to_owned),
        format: document.format().as_str().to_owned(),
        cue_count: document.displayed_cue_count(),
        has_bom: source.has_bom(),
        newline: newline_str(source.newline()).to_owned(),
        byte_length: source.byte_len() as u64,
    }
}

// -------------------------------------------------------------------------------------------
// Plumbing
// -------------------------------------------------------------------------------------------

/// Reading, parsing and saving all block, so no command body runs on the async runtime's poll
/// thread (CONTRIBUTING.md §7).
async fn blocking<T, F>(work: F) -> Result<T, SubtitleError>
where
    F: FnOnce() -> Result<T, SubtitleError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            SubtitleError::new(
                SubtitleErrorCode::CommandFailed,
                format!("the subtitle task failed: {error}"),
            )
        })?
}

// TODO(M2.6): narrow back to private, together with `SubtitleState::slot`.
pub fn backup_root(app: &AppHandle) -> Result<PathBuf, SubtitleError> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| {
            SubtitleError::new(
                SubtitleErrorCode::BackupFailed,
                format!("no app data directory: {error}"),
            )
        })?
        .join(BACKUP_DIR))
}

/// A poisoned lock means a command panicked holding it, so the commands refuse rather than build on
/// it. [`save_current`] deliberately does not: see the reasoning there.
fn lock(slot: &SessionSlot) -> Result<MutexGuard<'_, Option<EditSession>>, SubtitleError> {
    slot.lock().map_err(|_| {
        SubtitleError::new(
            SubtitleErrorCode::CommandFailed,
            "the subtitle session lock is poisoned",
        )
    })
}

fn current<'a>(
    guard: &'a mut MutexGuard<'_, Option<EditSession>>,
) -> Result<&'a mut EditSession, SubtitleError> {
    guard.as_mut().ok_or_else(|| {
        SubtitleError::new(SubtitleErrorCode::NoDocument, "no subtitle file is open")
    })
}

/// The caller's cue indices describe the list at its revision. If the session has moved on, the
/// safe answer is a refusal and a refetch, never an edit at a guessed index.
fn check_revision(session: &EditSession, revision: u64) -> Result<(), SubtitleError> {
    if session.revision() == revision {
        return Ok(());
    }
    Err(SubtitleError::new(
        SubtitleErrorCode::StaleRevision,
        format!(
            "the caller is at revision {revision}, the session at {}",
            session.revision()
        ),
    ))
}

/// An edit that would grow the file past what Sublore re-opens is refused before it is applied:
/// a document that cannot be opened again must not be creatable.
fn guard_size(session: &EditSession, edit: &Edit) -> Result<(), SubtitleError> {
    let planned = plan::plan(session.document(), edit).map_err(SubtitleError::from_edit)?;
    let grown = session
        .document()
        .source()
        .byte_len()
        .saturating_sub(planned.splice.removed.len())
        .saturating_add(planned.splice.inserted.len());
    if u64::try_from(grown).unwrap_or(u64::MAX) > MAX_SUBTITLE_BYTES {
        return Err(SubtitleError::new(
            SubtitleErrorCode::TooLarge,
            format!("the edit would make the file {grown} bytes, limit {MAX_SUBTITLE_BYTES}"),
        ));
    }
    Ok(())
}

/// A call that replayed nothing: the bottom of the undo stack, or the top of the redo tail.
fn nothing_changed() -> CuePatch {
    CuePatch {
        from: 0,
        removed: 0,
        cues: Vec::new(),
    }
}

fn opened_payload(session: &EditSession, summary: SubtitleSummary) -> SubtitleOpened {
    SubtitleOpened {
        summary,
        revision: session.revision(),
        cues: rows(session.views()),
        can_undo: session.can_undo(),
        can_redo: session.can_redo(),
        dirty: session.dirty(),
        truncated: session.truncated(),
    }
}

fn describe(session: &EditSession, patch: CuePatch) -> CuePatchDto {
    CuePatchDto {
        revision: session.revision(),
        from: patch.from,
        removed: patch.removed,
        cues: rows(&patch.cues),
        cue_count: session.document().displayed_cue_count(),
        can_undo: session.can_undo(),
        can_redo: session.can_redo(),
        dirty: session.dirty(),
        truncated: session.truncated(),
    }
}

fn rows(views: &[CueView]) -> Vec<CueRowDto> {
    views
        .iter()
        .map(|view| CueRowDto {
            start_ms: view.start_ms,
            end_ms: view.end_ms,
            text: view.text.clone(),
            comment: view.comment,
            number: view.number,
        })
        .collect()
}

fn saved(outcome: sublore_io::atomic::SaveOutcome, dirty: bool) -> SubtitleSaved {
    SubtitleSaved {
        path: outcome.destination.to_string_lossy().into_owned(),
        bytes_written: outcome.bytes_written,
        backup_path: outcome
            .backup
            .map(|path| path.to_string_lossy().into_owned()),
        dirty,
    }
}

fn read_document(path: &Path) -> Result<SubtitleDocument, SubtitleError> {
    if path.as_os_str().is_empty() {
        return Err(SubtitleError::new(
            SubtitleErrorCode::InvalidPath,
            "the path is empty",
        ));
    }

    // Metadata before opening: a directory opens fine on Linux, and "that is not a file" is the
    // sentence the user needs on both platforms.
    let metadata =
        std::fs::metadata(path).map_err(|error| SubtitleError::from_read(&error, path))?;
    if !metadata.is_file() {
        return Err(SubtitleError::new(
            SubtitleErrorCode::NotAFile,
            format!("{} is not a regular file", path.display()),
        ));
    }
    if metadata.len() > MAX_SUBTITLE_BYTES {
        return Err(SubtitleError::new(
            SubtitleErrorCode::TooLarge,
            format!("{} bytes, limit {MAX_SUBTITLE_BYTES}", metadata.len()),
        ));
    }

    let file = File::open(path).map_err(|error| SubtitleError::from_read(&error, path))?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    // One byte past the limit, so a file that grew since it was measured is refused, not truncated.
    let read = file
        .take(MAX_SUBTITLE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| SubtitleError::from_read(&error, path))?;
    if read as u64 > MAX_SUBTITLE_BYTES {
        return Err(SubtitleError::new(
            SubtitleErrorCode::TooLarge,
            format!("more than {MAX_SUBTITLE_BYTES} bytes"),
        ));
    }

    let format = detect(path, &bytes).ok_or_else(|| {
        SubtitleError::new(
            SubtitleErrorCode::UnknownFormat,
            format!("{} is not an SRT, VTT or ASS file", path.display()),
        )
    })?;
    parse(format, &bytes).map_err(SubtitleError::from_parse)
}

/// Content decides, extension breaks ties. Undecodable bytes make the content say nothing, and the
/// extension then picks the parser that reports the encoding problem properly.
fn detect(path: &Path, bytes: &[u8]) -> Option<SubtitleFormat> {
    let extension = path.extension().and_then(|value| value.to_str());
    SubtitleFormat::detect(extension, &String::from_utf8_lossy(bytes))
}

/// The wire spelling of a line terminator. Stable: the UI maps it to copy.
fn newline_str(newline: Newline) -> &'static str {
    match newline {
        Newline::Lf => "lf",
        Newline::Crlf => "crlf",
        Newline::Mixed => "mixed",
        Newline::None => "none",
    }
}
