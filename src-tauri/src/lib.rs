pub mod asr;
pub mod crash;
pub mod dialog;
pub mod project;
pub mod strings;
pub mod subtitle;
pub mod video;

/// The `log` facade, re-exported by tauri-plugin-log, so the crate needs no direct dependency on it.
pub use tauri_plugin_log::log;

use std::sync::atomic::{AtomicBool, Ordering};

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

/// Sublore accepts paths as arguments: `sublore file.mkv file.srt`, in any order. Sorted by
/// extension rather than by position so neither has to come first.
///
/// This is also the only way automation reaches the app on a real desktop: synthetic keystrokes go
/// to whichever window holds the X focus, which under a compositor is not reliably ours
/// (WORKFLOW.md, and docs/reports/n2b-collaudo-reale.md for what that cost).
fn startup_files(args: impl Iterator<Item = String>) -> StartupFiles {
    let mut files = StartupFiles::default();
    // Only arguments that are actually files on disk. The driver chain and the packagers both pass
    // their own arguments through, and treating a stray value as a path made the app try to open
    // one at startup: every E2E spec that opens a file then failed.
    for arg in args
        .skip(1)
        .filter(|a| !a.starts_with('-') && std::path::Path::new(a).is_file())
    {
        let lower = arg.to_lowercase();
        if lower.ends_with(".srt")
            || lower.ends_with(".vtt")
            || lower.ends_with(".ass")
            || lower.ends_with(".ssa")
        {
            files.subtitle.get_or_insert(arg);
        } else {
            files.video.get_or_insert(arg);
        }
    }
    files
}

#[tauri::command]
fn startup_files_command(state: tauri::State<'_, StartupFiles>) -> StartupFiles {
    state.inner().clone()
}

/// Build and run the app. Startup errors propagate to `main` so a failed launch is reported.
pub fn run() -> tauri::Result<()> {
    crash::install();

    let app = tauri::Builder::default()
        // First in the chain, so anything logged during setup already lands in the file.
        .plugin(log_plugin())
        .plugin(tauri_plugin_dialog::init())
        .manage(project::ProjectState::default())
        .manage(startup_files(std::env::args()))
        .invoke_handler(tauri::generate_handler![
            asr::asr_models,
            asr::asr_model_download,
            asr::asr_model_download_cancel,
            asr::asr_transcribe_start,
            asr::asr_transcribe_cancel,
            project::project_add_episode,
            project::project_attach_file,
            project::project_choose_path,
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
            video::video_open,
            video::video_play,
            video::video_pause,
            video::video_seek,
            video::video_set_region,
            startup_files_command
        ])
        .setup(|app| {
            crash::attach(app);
            app.manage(subtitle::SubtitleState::default());
            log::info!(
                "Sublore {} starting on {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS
            );
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
            // An answered gate already decided this close, and the flag is consumed here so it
            // can wave through exactly one request and never a later one. See BACKLOG N1b.
            if CLOSING.swap(false, Ordering::SeqCst) {
                asr::shutdown(app_handle);
                shutdown_video(app_handle);
            }
            // Unsaved edits are the user's to keep or drop, never ours to discard silently
            // (CLAUDE.md §3). See BACKLOG N1.
            else if unsaved_work(app_handle) {
                api.prevent_close();
                // A gate already up owns this decision; raising a second one over it would let the
                // first answer destroy the window the second is still asking about.
                if !GATE_OPEN.swap(true, Ordering::SeqCst) {
                    ask_before_closing(app_handle.clone(), label.clone());
                }
            } else {
                asr::shutdown(app_handle);
                shutdown_video(app_handle);
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

/// One gate at a time. Without this a second close request while the dialog is up raises a second
/// dialog on the same document, and answering the first destroys the window the second is still
/// deciding about.
static GATE_OPEN: AtomicBool = AtomicBool::new(false);

/// Set once the gate has been answered with save or discard, so the close that follows goes through
/// without asking again. Consumed by the handler that reads it: a flag left standing would let a
/// later close skip the gate in silence, which is how the work it guards gets lost. See BACKLOG N1b.
static CLOSING: AtomicBool = AtomicBool::new(false);

/// True when the window must not close without asking. `Unknown` counts as unsaved: the gate is
/// there for the bad moment, and refusing to ask during one is how work gets lost (BACKLOG N1).
fn unsaved_work(app_handle: &AppHandle) -> bool {
    app_handle
        .try_state::<subtitle::SubtitleState>()
        .is_some_and(|state| {
            !matches!(
                subtitle::session_state(&state.slot()),
                subtitle::SessionState::Clean
            )
        })
}

/// Ask what to do with unsaved edits, then act on the answer.
///
/// The answer arrives on the main thread, off the close event that asked for it, which is what
/// keeps this clear of the deadlock `project::choose_path` documents. A save that succeeds marks
/// the session clean and a discard drops it, so the close that follows those two answers finds
/// nothing to ask about; a save that fails closes nothing and says why.
fn ask_before_closing(app: AppHandle, label: String) {
    let acted = app.clone();
    let acted_label = label.clone();
    let asked = dialog::ask_close(&app, &label, move |answer| {
        let close = match answer {
            dialog::CloseAnswer::Save => save_open_file(&acted),
            dialog::CloseAnswer::Discard => discard_open_file(&acted),
            dialog::CloseAnswer::Cancel => false,
        };
        if close {
            close_window(acted.clone(), acted_label.clone());
        } else {
            // Cancelled, or a save that failed: the window stays, so the next X must ask again.
            GATE_OPEN.store(false, Ordering::SeqCst);
        }
    });
    if let Err(error) = asked {
        // Nobody will be asked, so nobody can answer. Logged rather than shown, because reporting
        // it would need the same main thread that just refused to take the question; the window
        // stays and the next close tries again.
        log::error!("close gate: the dialog could not be raised: {error:?}");
        GATE_OPEN.store(false, Ordering::SeqCst);
    }
}

/// Save the open file. A failed save keeps the window open: closing anyway would lose exactly the
/// work the user asked us to keep. The refusal is shown, not only logged (CLAUDE.md §6).
fn save_open_file(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<subtitle::SubtitleState>() else {
        // Unreachable: no session state means nothing was dirty and the gate never opened. Keeping
        // the window is the safe direction for every other uncertainty here, so it is for this one.
        log::error!("close gate: asked to save with no subtitle state");
        return false;
    };
    let outcome = subtitle::backup_root(app)
        .and_then(|backups| subtitle::save_current(&state.slot(), backups));
    match outcome {
        // `None` is a session that turned out clean: the gate can open on a merely busy lock, and
        // there is nothing to write when it does.
        Ok(_) => true,
        Err(error) => {
            log::error!("close gate: save failed, staying open: {error:?}");
            report_save_failure(app, &error);
            false
        }
    }
}

/// Tell the user the save did not happen. Fire and forget: the window is staying open either way,
/// and blocking here would be the deadlock `ask_before_closing` exists to avoid.
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
fn discard_open_file(app: &AppHandle) -> bool {
    if let Some(state) = app.try_state::<subtitle::SubtitleState>() {
        if let Err(error) = subtitle::close_session(&state.slot(), true) {
            log::error!("close gate: discarding the session failed: {error:?}");
        }
    }
    true
}

/// Ask for the close on the main thread, and let the close event do the teardown. mpv draws into a
/// child of the window and has to be gone before the window is; `CloseRequested` is where that
/// happens for every other close, and this one now joins it instead of having its own order.
///
/// A failure here leaves a window that is open but no longer backed by its session, so it is said
/// out loud rather than logged: silently, the next edit would fail with no document and the next
/// close would find nothing dirty and exit without asking.
fn close_window(app: AppHandle, label: String) {
    let handle = app.clone();
    let posted = app.run_on_main_thread(move || {
        match handle.get_webview_window(&label) {
            Some(window) => {
                // `close`, not `destroy`: destroying the GTK window directly skips tao's close
                // sequence and the main loop dies in GDK's event queue. See BACKLOG N1b.
                CLOSING.store(true, Ordering::SeqCst);
                if let Err(error) = window.close() {
                    CLOSING.store(false, Ordering::SeqCst);
                    log::error!("close gate: closing the window failed: {error:?}");
                    report_close_failure(&handle, &error.to_string());
                }
            }
            // No window to destroy and no exit either: leaving the gate flag up here would make
            // every later close silently skip the question.
            None => {
                log::error!("close gate: window {label} was gone before it could be destroyed");
                GATE_OPEN.store(false, Ordering::SeqCst);
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
fn report_close_failure(app: &AppHandle, reason: &str) {
    GATE_OPEN.store(false, Ordering::SeqCst);
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
