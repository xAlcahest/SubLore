pub mod asr;
pub mod crash;
pub mod strings;
pub mod subtitle;
pub mod video;

/// The `log` facade, re-exported by tauri-plugin-log, so the crate needs no direct dependency on it.
pub use tauri_plugin_log::log;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_log::{Target, TargetKind, TimezoneStrategy};

use crash::force::ForcePoint;

/// Two archived files beside the active one, so the logs stay bounded without hiding history.
const LOG_ROTATION: tauri_plugin_log::RotationStrategy =
    tauri_plugin_log::RotationStrategy::KeepSome(3);
const LOG_MAX_BYTES: u128 = 2 * 1024 * 1024;

/// Build and run the app. Startup errors propagate to `main` so a failed launch is reported.
pub fn run() -> tauri::Result<()> {
    crash::install();

    let app = tauri::Builder::default()
        // First in the chain, so anything logged during setup already lands in the file.
        .plugin(log_plugin())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            asr::asr_models,
            asr::asr_model_download,
            asr::asr_model_download_cancel,
            asr::asr_transcribe_start,
            asr::asr_transcribe_cancel,
            subtitle::subtitle_open,
            subtitle::subtitle_save_as,
            video::video_open,
            video::video_play,
            video::video_pause,
            video::video_seek,
            video::video_set_region
        ])
        .setup(|app| {
            crash::attach(app);
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
            event: WindowEvent::CloseRequested { .. },
            ..
        } => {
            asr::shutdown(app_handle);
            shutdown_video(app_handle);
        }
        RunEvent::ExitRequested { .. } | RunEvent::Exit => {
            // A transcription outlives the window that started it unless it is stopped here.
            asr::shutdown(app_handle);
            shutdown_video(app_handle);
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

/// Idempotent: every one of the events above may fire, and only the first does the work.
fn shutdown_video(app_handle: &AppHandle) {
    if let Some(state) = app_handle.try_state::<video::VideoState>() {
        state.shutdown();
    }
}
