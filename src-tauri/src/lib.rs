//! Licensed under the GNU GPL v3 or later, with the section 7 additional permission for modules
//! loaded through `sublore-module-api`. See LICENSE at the root of the repository.

pub mod asr;
pub mod chooser;
pub mod crash;
pub mod dialog;
pub mod project;
pub mod strings;
pub mod subtitle;
pub mod video;

/// The `log` facade, re-exported by tauri-plugin-log, so the crate needs no direct dependency on it.
pub use tauri_plugin_log::log;

use std::ffi::OsString;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_log::{Target, TargetKind, TimezoneStrategy};

use crash::force::ForcePoint;

/// Two archived files beside the active one, so the logs stay bounded without hiding history.
const LOG_ROTATION: tauri_plugin_log::RotationStrategy =
    tauri_plugin_log::RotationStrategy::KeepSome(3);
const LOG_MAX_BYTES: u128 = 2 * 1024 * 1024;

/// Files named on the command line, for the frontend to open once it is up.
#[derive(Default, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StartupFiles {
    pub video: Option<String>,
    pub subtitle: Option<String>,
}

/// What the command line asked for, plus the arguments dropped on the way there. The dropped ones
/// are handed back rather than logged here: this runs before the log plugin has a logger.
struct StartupArgs {
    files: StartupFiles,
    ignored: Vec<String>,
}

/// Sublore accepts paths as arguments: `sublore file.mkv file.srt`, in any order. Sorted by
/// extension rather than by position so neither has to come first.
///
/// This is also the only way automation reaches the app on a real desktop: synthetic keystrokes go
/// to whichever window holds the X focus, which under a compositor is not reliably ours.
///
/// `OsString`, not `String`: a Linux filename is a byte string, and `std::env::args()` panics on one
/// that is not UTF-8, which killed the app before its window existed (gate 2, `lib.rs:75`).
fn startup_files(args: impl Iterator<Item = OsString>) -> StartupArgs {
    let mut files = StartupFiles::default();
    let mut ignored = Vec::new();
    for arg in args.skip(1) {
        // The IPC payload cannot carry a name that is not UTF-8 either, so it costs that argument
        // and is named in the log, never the launch.
        let Some(arg) = arg.to_str() else {
            ignored.push(format!("{} (not valid Unicode)", arg.to_string_lossy()));
            continue;
        };
        // Only arguments that are actually files on disk. The driver chain and the packagers both
        // pass their own arguments through, and treating a stray value as a path made the app try
        // to open one at startup: every E2E spec that opens a file then failed.
        if !std::path::Path::new(arg).is_file() {
            // A switch is not a file and never was; anything else was meant to be one, so it is
            // named rather than dropped in silence.
            if !arg.starts_with('-') {
                ignored.push(format!("{arg} (not a file on disk)"));
            }
            continue;
        }
        let lower = arg.to_lowercase();
        let (slot, kind) = if lower.ends_with(".srt")
            || lower.ends_with(".vtt")
            || lower.ends_with(".ass")
            || lower.ends_with(".ssa")
        {
            (&mut files.subtitle, "subtitle")
        } else {
            (&mut files.video, "video")
        };
        // One of each kind is opened and the rest are named: `sublore ep01.srt ep02.srt` used to
        // drop the second one without a word anywhere (gate 2b, `lib.rs:76`).
        match slot {
            Some(_) => ignored.push(format!("{arg} (a {kind} was already named)")),
            None => *slot = Some(arg.to_owned()),
        }
    }
    StartupArgs { files, ignored }
}

#[tauri::command]
fn startup_files_command(state: tauri::State<'_, StartupFiles>) -> StartupFiles {
    state.inner().clone()
}

/// Build and run the app. Startup errors propagate to `main` so a failed launch is reported.
pub fn run() -> tauri::Result<()> {
    crash::install();

    let StartupArgs { files, ignored } = startup_files(std::env::args_os());
    let taken = files.clone();

    let app = tauri::Builder::default()
        // First in the chain, so anything logged during setup already lands in the file.
        .plugin(log_plugin())
        .plugin(tauri_plugin_dialog::init())
        .manage(project::ProjectState::default())
        .manage(files)
        .invoke_handler(tauri::generate_handler![
            asr::asr_models,
            asr::asr_model_download,
            asr::asr_model_download_cancel,
            asr::asr_transcribe_start,
            asr::asr_transcribe_cancel,
            chooser::choose_path,
            project::project_add_episode,
            project::project_attach_file,
            project::project_create,
            project::project_delete,
            project::project_open,
            subtitle::subtitle_open,
            subtitle::subtitle_close,
            subtitle::subtitle_set_text,
            subtitle::subtitle_set_times,
            subtitle::subtitle_insert,
            subtitle::subtitle_delete,
            subtitle::subtitle_split,
            subtitle::subtitle_merge,
            subtitle::subtitle_undo,
            subtitle::subtitle_redo,
            subtitle::subtitle_save,
            subtitle::subtitle_save_as,
            subtitle::subtitle_adopt_transcription,
            video::video_open,
            video::video_play,
            video::video_pause,
            video::video_seek,
            video::video_set_region,
            startup_files_command
        ])
        .setup(move |app| {
            crash::attach(app);
            app.manage(subtitle::SubtitleState::default());
            log::info!(
                "Sublore {} starting on {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS
            );
            // The first line the log can carry: `startup_files` runs before this plugin has a
            // logger, and an argument dropped in silence is an argument the user cannot find.
            log::info!(
                "command line: video={:?}, subtitle={:?}",
                taken.video,
                taken.subtitle
            );
            for argument in &ignored {
                log::warn!("command line: ignored {argument}");
            }
            crash::force::trip(ForcePoint::Startup);
            app.manage(asr::AsrState::default());
            // A killed process cannot run its own cleanup, so abandoned run directories are swept
            // here, off the main thread. See BACKLOG.md M3.1.
            asr::sweep_scratch(app.handle());
            if let Err(error) = video::setup(app) {
                log::error!("video setup failed: {error}");
                return Err(error.into());
            }
            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(|app_handle, event| match event {
        // mpv draws into a child of the window, so it has to be gone before the window is.
        // CloseRequested is the last event that still runs while the window exists.
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            // Read for every close, an answered one included: the window stays alive and editable
            // until this event, so work committed after the answer has never been asked about
            // (CONTRIBUTING.md §3). See BACKLOG N1, N1b and gate 2, `lib.rs:138`.
            let session = session_now(app_handle);
            // Decided through a call that drops the guard before it returns: a guard built in the
            // scrutinee lives for every arm, and two of them re-enter the gate (gate 2b, `lib.rs:176`).
            match decide_close_now(&label, session) {
                CloseAction::Close => {
                    asr::shutdown(app_handle);
                    shutdown_video(app_handle);
                }
                CloseAction::Ask => {
                    api.prevent_close();
                    ask_before_closing(app_handle.clone(), label.clone());
                }
                CloseAction::AskAgain => {
                    api.prevent_close();
                    log::info!(
                        "close gate: {label} was edited after its gate was answered, asking again"
                    );
                    ask_before_closing(app_handle.clone(), label.clone());
                }
                // A silent `prevent_close` and a hung app look the same from outside, and these are
                // the branches that hold a window shut with no dialog on screen.
                CloseAction::Wait(held) => {
                    api.prevent_close();
                    log::warn!("close gate: {label} held closed, {}", held.reason());
                }
            }
        }
        RunEvent::ExitRequested { .. } | RunEvent::Exit => {
            // A transcription outlives the window that started it unless it is stopped here.
            asr::shutdown(app_handle);
            shutdown_video(app_handle);
            shutdown_project(app_handle);
        }
        _ => {}
    });

    Ok(())
}

/// File logging only in release: stdout is added in debug builds so `pnpm tauri dev` stays useful.
fn log_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    // The plugin's defaults are Stdout + LogDir and a 40 KB cap, so both are replaced here.
    let builder = tauri_plugin_log::Builder::new()
        .clear_targets()
        .target(Target::new(TargetKind::LogDir {
            file_name: Some("sublore".to_owned()),
        }))
        .rotation_strategy(LOG_ROTATION)
        .max_file_size(LOG_MAX_BYTES)
        .timezone_strategy(TimezoneStrategy::UseUtc)
        .level(log::LevelFilter::Info);

    #[cfg(debug_assertions)]
    let builder = builder
        .target(Target::new(TargetKind::Stdout))
        .level(log::LevelFilter::Debug);

    builder.build()
}

/// The close decision in flight, if there is one. One gate at a time: a second dialog over the
/// first would let the first answer destroy the window the second is still asking about.
static GATE: Mutex<Option<Gate>> = Mutex::new(None);

/// A close decision, from the moment the gate goes up until the close it decided has passed or the
/// window has stayed. It replaces a pair of process-wide flags that could say neither which of the
/// three phases below it was in, nor which window it belonged to.
#[derive(Debug, PartialEq, Eq)]
struct Gate {
    /// The window that was asked about: an answer for one window is not an answer for another.
    label: String,
    phase: Phase,
}

/// How far a gate has got. `Asking` and `Acting` both hold the window shut, and only the first of
/// them has anything on screen to explain why (gate 2, `lib.rs:192`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The dialog is up and unanswered. Unbounded, and rightly so: the user is reading it.
    Asking,
    /// The answer is being acted on. The dialog is already destroyed, so the window looks
    /// answerable and is not.
    Acting,
    /// The answer has been acted on, and the close it asks for has not arrived yet.
    Acted(SessionAfter),
}

/// What an answer, once acted on, leaves behind for the close it asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionAfter {
    /// Nothing of the user's is unsaved any more, so anything unsaved at the close is newer work.
    Clean,
    /// A discard whose drop failed on a poisoned lock. The session may still hold the edits the
    /// user chose to lose, and `subtitle::lock` refuses that lock to every editing command, so a
    /// dirty session at the close is that same abandoned work and never something newer.
    Unproven,
}

/// What one close request does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloseAction {
    /// Let the window go, after tearing down what has to die before it.
    Close,
    /// Keep the window and ask.
    Ask,
    /// Keep the window and ask again: work was committed after the last answer.
    AskAgain,
    /// Keep the window: something else is already deciding it. Carries which case, because a held
    /// window with no dialog on it is the one state the log has to be able to explain.
    Wait(Held),
}

/// Why a close request was held instead of answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    /// This window's own dialog is up and unanswered.
    Dialog,
    /// This window's answer is being acted on: a file is being written, or a session dropped.
    Answer,
    /// Another window owns the gate.
    OtherWindow,
}

impl Held {
    fn reason(self) -> &'static str {
        match self {
            Self::Dialog => "its dialog is on screen and unanswered",
            Self::Answer => "its answer is still being acted on",
            Self::OtherWindow => "another window's close decision is in flight",
        }
    }
}

/// The gate state. A poisoned lock still answers: a panic somewhere else must never leave a window
/// that cannot be closed.
fn gate() -> std::sync::MutexGuard<'static, Option<Gate>> {
    GATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Decide one close request against the process gate, and drop the guard before returning.
///
/// The caller acts on the decision, and acting on it re-enters this lock on the same thread:
/// `ask_before_closing` reaches `clear_gate` when the dialog cannot be raised, and `mark_acting`
/// when the answer thread is refused and the question is cancelled inline. `std::sync::Mutex` is
/// not reentrant, and the deadlock is the whole close path (gate 2b, `lib.rs:176`).
fn decide_close_now(label: &str, session: subtitle::SessionState) -> CloseAction {
    decide_close(&mut gate(), label, session)
}

/// Decide one close request, and leave the state the next one will read. Takes the state as an
/// argument rather than reading the static, so the tests below can drive every combination.
///
/// A session whose lock is held reads as `Unknown`, and that counts as unsaved wherever `Dirty`
/// does, the answered gate included: it is the instant an edit is being committed, and closing
/// there is the loss N1 was about. The price is a second dialog for a document that may be clean.
fn decide_close(
    gate: &mut Option<Gate>,
    label: &str,
    session: subtitle::SessionState,
) -> CloseAction {
    let unsaved = session != subtitle::SessionState::Clean;
    match gate.as_ref() {
        // Unsaved edits are the user's to keep or drop, never ours to discard silently
        // (CONTRIBUTING.md §3). See BACKLOG N1.
        None if unsaved => {
            *gate = Some(Gate::raised(label));
            CloseAction::Ask
        }
        None => CloseAction::Close,
        // Another window's decision is not an answer for this one.
        Some(open) if open.label != label => CloseAction::Wait(Held::OtherWindow),
        Some(Gate {
            phase: Phase::Asking,
            ..
        }) => CloseAction::Wait(Held::Dialog),
        Some(Gate {
            phase: Phase::Acting,
            ..
        }) => CloseAction::Wait(Held::Answer),
        // The window stayed alive and editable until this event, so anything unsaved now was
        // committed after the answer and has never been asked about. See gate 2, `lib.rs:138`.
        Some(Gate {
            phase: Phase::Acted(SessionAfter::Clean),
            ..
        }) if unsaved => {
            *gate = Some(Gate::raised(label));
            CloseAction::AskAgain
        }
        // Consumed here, so one answer can wave through exactly one close and never a later one.
        Some(Gate {
            phase: Phase::Acted(_),
            ..
        }) => {
            *gate = None;
            CloseAction::Close
        }
    }
}

impl Gate {
    fn raised(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            phase: Phase::Asking,
        }
    }
}

/// The dialog has been answered and the answer is being acted on. From here the gate holds the
/// window with nothing on screen to say so, which is why the phase is recorded rather than implied.
fn mark_acting(label: &str) {
    match gate().as_mut() {
        Some(open) if open.label == label => open.phase = Phase::Acting,
        _ => log::warn!("close gate: no gate was open for {label} when its dialog was answered"),
    }
}

/// Record that the answer for `label` has been acted on, so the close it asks for is not asked
/// about again for the work that answer already covered.
fn mark_acted(label: &str, after: SessionAfter) {
    match gate().as_mut() {
        Some(open) if open.label == label => open.phase = Phase::Acted(after),
        // The gate went away under the answer. The close still goes ahead, and the dirty check is
        // what decides whether it asks first.
        _ => log::warn!("close gate: no gate was open for {label} when its answer was acted on"),
    }
}

/// Drop the decision: the window is staying, so the next close request asks from scratch.
fn clear_gate() {
    *gate() = None;
}

/// What the session says about unsaved work, as the gate reads it. No session state at all is
/// `Clean`: it is managed in `setup`, before any window exists to be closed.
fn session_now(app_handle: &AppHandle) -> subtitle::SessionState {
    app_handle
        .try_state::<subtitle::SubtitleState>()
        .map_or(subtitle::SessionState::Clean, |state| {
            subtitle::session_state(&state.slot())
        })
}

/// Ask what to do with unsaved edits, then act on the answer.
///
/// `dialog::ask_close` builds the dialog on the main thread and delivers its answer on a thread of
/// its own, so the file this writes never runs on the main loop. The close the answer asks for is
/// still decided from scratch when it arrives: an answer covers the work it was given, never work
/// committed after it (gate 2, `lib.rs:138`). A save that fails closes nothing and says why.
fn ask_before_closing(app: AppHandle, label: String) {
    let acted = app.clone();
    let acted_label = label.clone();
    let asked = dialog::ask_close(&app, &label, move |answer| {
        // The dialog destroys itself before this runs, so from here the gate holds the window with
        // nothing on screen. See gate 2, `lib.rs:192`.
        mark_acting(&acted_label);
        let answered = match answer {
            dialog::CloseAnswer::Save => save_open_file(&acted),
            dialog::CloseAnswer::Discard => discard_open_file(&acted),
            dialog::CloseAnswer::Cancel => Answered::Stay,
        };
        match answered {
            Answered::Close(after) => {
                stall_after_answer();
                close_window(acted.clone(), acted_label.clone(), after);
            }
            // Cancelled, or a save that failed: the window stays, so the next X must ask again.
            Answered::Stay => clear_gate(),
        }
    });
    if let Err(error) = asked {
        // Unreachable from this caller: `ask_close` posts to the main thread it is already on, and
        // that runs inline. Kept because the signature is fallible and nobody would be asked.
        log::error!("close gate: the dialog could not be raised: {error:?}");
        clear_gate();
    }
}

/// What the answer decided, and what it leaves behind for the close request that follows it.
enum Answered {
    Close(SessionAfter),
    Stay,
}

/// Test hook: hold the answer between acting on it and the close it asks for, which is the interval
/// `e2e/scripts/close-gate-late-edit-check.js` commits an edit inside. Debug builds only, like
/// `crash::force`.
#[cfg(debug_assertions)]
fn stall_after_answer() {
    const ENV_VAR: &str = "SUBLORE_CLOSE_ANSWER_DELAY_MS";

    // Anything unreadable selects nothing, and a minute is the ceiling: a typo must not be able to
    // hold a close gate open for the life of the process.
    let Some(ms) = std::env::var(ENV_VAR)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|ms| ms.min(60_000))
    else {
        return;
    };
    log::warn!("close gate: {ENV_VAR}={ms}, holding the answer before the close it asks for");
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

/// Release builds carry no hook: the environment variable is never read.
#[cfg(not(debug_assertions))]
#[inline(always)]
fn stall_after_answer() {}

/// Save the open file. A failed save keeps the window open: closing anyway would lose exactly the
/// work the user asked us to keep. The refusal is shown, not only logged (CONTRIBUTING.md §6).
fn save_open_file(app: &AppHandle) -> Answered {
    let Some(state) = app.try_state::<subtitle::SubtitleState>() else {
        // Unreachable: no session state means nothing was dirty and the gate never opened. Keeping
        // the window is the safe direction for every other uncertainty here, so it is for this one.
        log::error!("close gate: asked to save with no subtitle state");
        return Answered::Stay;
    };
    let outcome = subtitle::backup_root(app)
        .and_then(|backups| subtitle::save_current(&state.slot(), backups));
    match after_save(outcome) {
        Ok(after) => Answered::Close(after),
        Err(error) => {
            log::error!("close gate: save failed, staying open: {error:?}");
            report_save_failure(app, &error);
            Answered::Stay
        }
    }
}

/// What the gate's save means for the close that asked for it. Split out of the dialog it would
/// otherwise raise so that the classification the user's work depends on has a test.
fn after_save(
    outcome: Result<Option<subtitle::SubtitleSaved>, subtitle::error::SubtitleError>,
) -> Result<SessionAfter, subtitle::error::SubtitleError> {
    match outcome {
        // `None` is a session that turned out clean: the gate can open on a merely busy lock, and
        // there is nothing to write when it does.
        Ok(_) => Ok(SessionAfter::Clean),
        // Nothing to save is not a failed save: the document was closed while the gate was up, and
        // nothing of the user's is at risk. See gate 2, `lib.rs:258`.
        Err(error) if error.code == subtitle::error::SubtitleErrorCode::NoDocument => {
            log::info!("close gate: nothing to save, the document was already closed");
            Ok(SessionAfter::Clean)
        }
        Err(error) => Err(error),
    }
}

/// Tell the user the save did not happen. Fire and forget: the window is staying open either way,
/// and a dialog that waits would hold the answer thread for as long as the user takes to read it.
fn report_save_failure(app: &AppHandle, error: &subtitle::error::SubtitleError) {
    if let Err(posting) = dialog::report_error(
        app,
        strings::CLOSE_SAVE_FAILED_TITLE,
        strings::close_save_failed(&error.to_string()),
    ) {
        log::error!("close gate: could not report the failed save: {posting:?}");
    }
}

/// Drop the session the user chose to abandon. A failed drop still closes: the user said the edits
/// go, and the window is about to take them with it either way.
fn discard_open_file(app: &AppHandle) -> Answered {
    let Some(state) = app.try_state::<subtitle::SubtitleState>() else {
        return Answered::Close(SessionAfter::Clean);
    };
    match subtitle::close_session(&state.slot(), true) {
        Ok(()) => Answered::Close(SessionAfter::Clean),
        // The session may still be holding the edits, and they are the ones the user just chose to
        // lose, so the close must not stop to ask about them again.
        Err(error) => {
            log::error!("close gate: discarding the session failed: {error:?}");
            Answered::Close(SessionAfter::Unproven)
        }
    }
}

/// Ask for the close on the main thread, and let the close event do the teardown. mpv draws into a
/// child of the window and has to be gone before the window is; `CloseRequested` is where that
/// happens for every other close, and this one now joins it instead of having its own order.
///
/// A failure here leaves a window that is open but no longer backed by its session, so it is said
/// out loud rather than logged: silently, the next edit would fail with no document and the next
/// close would find nothing dirty and exit without asking.
fn close_window(app: AppHandle, label: String, after: SessionAfter) {
    let handle = app.clone();
    let posted = app.run_on_main_thread(move || {
        match handle.get_webview_window(&label) {
            Some(window) => {
                // `close`, not `destroy`: destroying the GTK window directly skips tao's close
                // sequence and the main loop dies in GDK's event queue. See BACKLOG N1b.
                mark_acted(&label, after);
                // Unlike `destroy`, this close can be prevented: the frontend must never register a
                // `tauri://close-requested` listener, or the window survives its own teardown.
                if let Err(error) = window.close() {
                    log::error!("close gate: closing the window failed: {error:?}");
                    report_close_failure(&handle, &error.to_string());
                }
            }
            // No window to destroy and no exit either: leaving the decision standing here would make
            // every later close silently skip the question.
            None => {
                log::error!("close gate: window {label} was gone before it could be destroyed");
                clear_gate();
            }
        }
    });
    if let Err(error) = posted {
        log::error!("close gate: could not reach the main thread: {error:?}");
        report_close_failure(&app, &error.to_string());
    }
}

/// The window would not close after the user answered. Whatever they chose, they need to know the
/// app is still holding their file.
///
/// Reached from the main thread as well as from the answer thread: `dialog::report_error` posts to
/// the main thread, and a post from the main thread runs inline rather than waiting.
fn report_close_failure(app: &AppHandle, reason: &str) {
    clear_gate();
    if let Err(posting) = dialog::report_error(
        app,
        strings::CLOSE_FAILED_TITLE,
        strings::close_failed(reason),
    ) {
        log::error!("close gate: could not report the failed close: {posting:?}");
    }
}

/// Idempotent: every one of the events above may fire, and only the first does the work.
fn shutdown_video(app_handle: &AppHandle) {
    if let Some(state) = app_handle.try_state::<video::VideoState>() {
        state.shutdown();
    }
}

/// Close the project database on the way out, so a normal quit checkpoints its WAL. Idempotent
/// for the same reason as above.
fn shutdown_project(app_handle: &AppHandle) {
    if let Some(state) = app_handle.try_state::<project::ProjectState>() {
        state.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtitle::SessionState;

    /// `GATE` is process-wide, so the tests that drive the static one instead of their own state
    /// take this first. A poisoned lock still hands it over: the gate tests must not chain-fail.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn gate_for(label: &str, phase: Phase) -> Option<Gate> {
        Some(Gate {
            label: label.to_owned(),
            phase,
        })
    }

    #[test]
    fn a_clean_window_closes_and_a_dirty_one_is_asked_about() {
        let mut gate = None;
        assert_eq!(
            decide_close(&mut gate, "main", SessionState::Clean),
            CloseAction::Close
        );
        assert_eq!(gate, None);
        assert_eq!(
            decide_close(&mut gate, "main", SessionState::Dirty),
            CloseAction::Ask
        );
        assert_eq!(gate, gate_for("main", Phase::Asking));
    }

    /// A session whose lock a command is holding cannot be read, and that is the instant an edit is
    /// being committed. The bool this used to take could not tell the two apart (gate 2b,
    /// `lib.rs:175`).
    #[test]
    fn a_session_that_cannot_be_read_is_asked_about_like_a_dirty_one() {
        let mut fresh = None;
        assert_eq!(
            decide_close(&mut fresh, "main", SessionState::Unknown),
            CloseAction::Ask
        );
        assert_eq!(fresh, gate_for("main", Phase::Asking));

        let mut answered = gate_for("main", Phase::Acted(SessionAfter::Clean));
        assert_eq!(
            decide_close(&mut answered, "main", SessionState::Unknown),
            CloseAction::AskAgain
        );
        assert_eq!(answered, gate_for("main", Phase::Asking));
    }

    #[test]
    fn an_answer_waves_through_one_close_and_never_a_later_one() {
        let mut gate = gate_for("main", Phase::Acted(SessionAfter::Clean));
        assert_eq!(
            decide_close(&mut gate, "main", SessionState::Clean),
            CloseAction::Close
        );
        assert_eq!(gate, None);
        assert_eq!(
            decide_close(&mut gate, "main", SessionState::Dirty),
            CloseAction::Ask
        );
    }

    #[test]
    fn work_committed_after_the_answer_is_asked_about_again() {
        let mut gate = gate_for("main", Phase::Acted(SessionAfter::Clean));
        assert_eq!(
            decide_close(&mut gate, "main", SessionState::Dirty),
            CloseAction::AskAgain
        );
        assert_eq!(gate, gate_for("main", Phase::Asking));
    }

    #[test]
    fn a_discard_that_could_not_drop_the_session_still_closes() {
        let mut gate = gate_for("main", Phase::Acted(SessionAfter::Unproven));
        assert_eq!(
            decide_close(&mut gate, "main", SessionState::Dirty),
            CloseAction::Close
        );
        assert_eq!(gate, None);
    }

    #[test]
    fn a_gate_still_on_screen_holds_the_window_without_raising_a_second_one() {
        let mut gate = gate_for("main", Phase::Asking);
        assert_eq!(
            decide_close(&mut gate, "main", SessionState::Dirty),
            CloseAction::Wait(Held::Dialog)
        );
        assert_eq!(gate, gate_for("main", Phase::Asking));
    }

    /// The decision has to come back with the gate released: the arms that act on it re-enter the
    /// same lock on the same thread, and `std::sync::Mutex` hangs there rather than failing.
    #[test]
    fn a_close_decision_leaves_the_gate_unlocked_for_the_arm_that_acts_on_it() {
        let _serial = serial();
        clear_gate();

        let action = decide_close_now("main", SessionState::Dirty);

        assert_eq!(action, CloseAction::Ask);
        assert!(
            GATE.try_lock().is_ok(),
            "the decision came back still holding the gate: the Ask arm would deadlock on it"
        );
        // What `ask_before_closing` does on this same thread when the answer is cancelled inline.
        mark_acting("main");
        clear_gate();
        assert_eq!(*gate(), None);
    }

    /// The guard cannot be created in the `match` scrutinee again: a temporary there lives for
    /// every arm, and two of them re-enter the gate (gate 2b, `lib.rs:176`).
    #[test]
    fn the_close_handler_takes_no_gate_guard_of_its_own() {
        let source = include_str!("lib.rs");
        let start = source
            .find("RunEvent::WindowEvent")
            .expect("the close handler is in this file");
        let end = start
            + source[start..]
                .find("RunEvent::ExitRequested")
                .expect("the exit handler follows it");
        let handler = &source[start..end];

        assert!(
            handler.contains("decide_close_now("),
            "the close handler no longer decides through the call that drops the guard:\n{handler}"
        );
        let held: Vec<_> = handler
            .match_indices("gate()")
            .filter(|(at, _)| {
                !handler[..*at]
                    .ends_with(|character: char| character.is_alphanumeric() || character == '_')
            })
            .map(|(at, _)| handler[..at].lines().count())
            .collect();
        assert!(
            held.is_empty(),
            "a gate guard taken in the close handler is held for every arm, and the arms re-enter \
             it: line {held:?} of the handler"
        );
    }

    /// The interval the dialog no longer covers: it is off screen, the answer is still being acted
    /// on, and the window is held shut by a gate the user cannot see.
    #[test]
    fn an_answer_being_acted_on_holds_the_window_and_says_which_case_it_is() {
        let mut gate = gate_for("main", Phase::Acting);
        assert_eq!(
            decide_close(&mut gate, "main", SessionState::Dirty),
            CloseAction::Wait(Held::Answer)
        );
        assert_eq!(gate, gate_for("main", Phase::Acting));
        assert_ne!(Held::Answer.reason(), Held::Dialog.reason());
    }

    #[test]
    fn one_windows_answer_never_closes_another_window() {
        let mut gate = gate_for("main", Phase::Acted(SessionAfter::Clean));
        assert_eq!(
            decide_close(&mut gate, "second", SessionState::Dirty),
            CloseAction::Wait(Held::OtherWindow)
        );
        assert_eq!(gate, gate_for("main", Phase::Acted(SessionAfter::Clean)));
    }

    fn subtitle_error(code: subtitle::error::SubtitleErrorCode) -> subtitle::error::SubtitleError {
        subtitle::error::SubtitleError::new(code, "from the test")
    }

    #[test]
    fn a_document_closed_under_the_gate_is_nothing_to_save_and_not_a_failed_save() {
        let after = after_save(Err(subtitle_error(
            subtitle::error::SubtitleErrorCode::NoDocument,
        )));
        assert_eq!(after, Ok(SessionAfter::Clean));
        assert_eq!(after_save(Ok(None)), Ok(SessionAfter::Clean));
    }

    #[test]
    fn a_write_that_failed_keeps_the_window_and_carries_its_reason() {
        let failure = subtitle_error(subtitle::error::SubtitleErrorCode::WriteFailed);
        assert_eq!(after_save(Err(failure.clone())), Err(failure));
    }

    /// A transcription that has never been saved has nowhere to be written back to. Reading that
    /// as nothing to save would close the window over the only copy of it. See BACKLOG.md M3.5.
    #[test]
    fn a_document_with_no_file_is_a_failed_save_and_never_nothing_to_save() {
        let failure = subtitle_error(subtitle::error::SubtitleErrorCode::NoPath);
        assert_eq!(after_save(Err(failure.clone())), Err(failure));
    }

    /// `close_window` uses `close`, which the webview can prevent where `destroy` could not: one
    /// `tauri://close-requested` listener would leave the app unclosable. See gate 2, `lib.rs:304`.
    #[test]
    fn the_frontend_registers_no_close_requested_listener() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri has a parent")
            .join("src");
        let mut offenders = Vec::new();
        let mut pending = vec![src.clone()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).expect("the frontend source directory is readable")
            {
                let path = entry.expect("a readable directory entry").path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                if text.contains("close-requested") || text.contains("onCloseRequested") {
                    offenders.push(path);
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "a close-requested listener would make the gate's own close preventable: {offenders:?}"
        );
    }

    /// A directory this test owns, so `is_file` decides on files this test made.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sublore-startup-files-{name}"));
        std::fs::create_dir_all(&dir).expect("the temp directory has to be creatable");
        dir
    }

    fn touch(dir: &std::path::Path, name: &str) -> OsString {
        let file = dir.join(name);
        std::fs::write(&file, b"1\n").expect("the fixture file has to be writable");
        file.into_os_string()
    }

    #[test]
    #[cfg(unix)]
    fn an_argument_that_is_not_unicode_costs_that_argument_and_is_named() {
        use std::os::unix::ffi::OsStringExt;

        let dir = scratch("not-unicode");
        let subtitle = touch(&dir, "episode.srt");
        let mut bytes = dir.join("").into_os_string().into_vec();
        bytes.extend_from_slice(b"s\xe9rie.srt");
        let args = vec![
            OsString::from("sublore"),
            OsString::from_vec(bytes),
            subtitle.clone(),
        ];

        let taken = startup_files(args.into_iter());
        assert_eq!(taken.files.subtitle, subtitle.into_string().ok());
        assert_eq!(taken.ignored.len(), 1, "ignored: {:?}", taken.ignored);
        assert!(
            taken.ignored[0].contains("not valid Unicode"),
            "the dropped argument was not named: {:?}",
            taken.ignored
        );
    }

    #[test]
    fn a_file_whose_name_starts_with_a_dash_is_still_the_file_the_user_named() {
        let dir = scratch("dash");
        let subtitle = touch(&dir, "-export.srt");

        let taken = startup_files(vec![OsString::from("sublore"), subtitle.clone()].into_iter());
        assert_eq!(taken.files.subtitle, subtitle.into_string().ok());
        assert!(taken.ignored.is_empty(), "ignored: {:?}", taken.ignored);
    }

    /// The commonest command line there is, and the one that used to lose an argument in silence:
    /// two files of the same kind (gate 2b, `lib.rs:76`).
    #[test]
    fn a_second_file_of_the_same_kind_is_named_rather_than_dropped() {
        let dir = scratch("second-of-a-kind");
        let first = touch(&dir, "ep01.srt");
        let second = touch(&dir, "ep02.srt");
        let video = touch(&dir, "ep01.mkv");
        let extra_video = touch(&dir, "ep02.mkv");

        let taken = startup_files(
            vec![
                OsString::from("sublore"),
                first.clone(),
                second.clone(),
                video.clone(),
                extra_video.clone(),
            ]
            .into_iter(),
        );

        assert_eq!(taken.files.subtitle, first.into_string().ok());
        assert_eq!(taken.files.video, video.into_string().ok());
        assert_eq!(taken.ignored.len(), 2, "ignored: {:?}", taken.ignored);
        assert_eq!(
            taken.ignored[0],
            format!(
                "{} (a subtitle was already named)",
                second.to_string_lossy()
            )
        );
        assert_eq!(
            taken.ignored[1],
            format!(
                "{} (a video was already named)",
                extra_video.to_string_lossy()
            )
        );
    }

    #[test]
    fn a_switch_goes_quietly_and_a_path_that_is_not_there_is_named() {
        let dir = scratch("missing");
        let missing = dir.join("epsiode.srt").into_os_string();

        let taken = startup_files(
            vec![
                OsString::from("sublore"),
                OsString::from("--webdriver"),
                missing,
            ]
            .into_iter(),
        );
        assert_eq!(taken.files.subtitle, None);
        assert_eq!(taken.ignored.len(), 1, "ignored: {:?}", taken.ignored);
        assert!(
            taken.ignored[0].contains("epsiode.srt") && taken.ignored[0].contains("not a file"),
            "the dropped path was not named: {:?}",
            taken.ignored
        );
    }
}
