pub mod video;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

/// Build and run the app. Startup errors propagate to `main` so a failed launch is reported.
pub fn run() -> tauri::Result<()> {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            video::video_open,
            video::video_play,
            video::video_pause,
            video::video_seek,
            video::video_set_region
        ])
        .setup(|app| {
            video::setup(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(|app_handle, event| match event {
        // mpv draws into a child of the window, so it has to be gone before the window is.
        // CloseRequested is the last event that still runs while the window exists.
        RunEvent::WindowEvent {
            event: WindowEvent::CloseRequested { .. },
            ..
        } => shutdown_video(app_handle),
        RunEvent::ExitRequested { .. } | RunEvent::Exit => shutdown_video(app_handle),
        _ => {}
    });

    Ok(())
}

/// Idempotent: every one of the events above may fire, and only the first does the work.
fn shutdown_video(app_handle: &AppHandle) {
    if let Some(state) = app_handle.try_state::<video::VideoState>() {
        state.shutdown();
    }
}
