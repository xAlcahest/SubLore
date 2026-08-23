//! The video player module: mpv lifecycle, the native surface, and the five IPC commands.
//! The IPC names and payloads here are a public interface (CLAUDE.md section 6).

pub mod error;
pub mod player;
mod surface;

use std::cell::RefCell;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::crash::force::{trip, ForcePoint};
use error::VideoError;
use player::{Player, PlayerConfig, VideoOpened};
use surface::{SurfaceRegion, VideoSurface};

/// How long a surface change waits for the main thread to apply it.
const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(2);

thread_local! {
    /// The surface is a GTK or Win32 handle, so it never enters shared state: it lives on the
    /// main thread and is only reached from inside `run_on_main_thread`.
    static SURFACE: RefCell<Option<VideoSurface>> = const { RefCell::new(None) };
}

/// A rectangle measured by the frontend with `getBoundingClientRect`, in CSS pixels.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub struct VideoState {
    player: Arc<Player>,
}

impl VideoState {
    fn player(&self) -> Arc<Player> {
        Arc::clone(&self.player)
    }

    /// Main thread only. Destroy the surface only once mpv has stopped drawing into it.
    pub fn shutdown(&self) {
        // A core that outlived shutdown keeps its surface: GTK and Win32 tear the child window
        // down with the parent anyway, and doing it here under a live mpv is the unsafe order.
        if self.player.shutdown() {
            take_surface();
        }
    }
}

/// Build the surface, then the mpv core that draws into it. Main thread, during Tauri setup.
pub fn setup(app: &tauri::App) -> Result<(), VideoError> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| VideoError::player_unavailable("the main window is missing"))?;

    let surface = VideoSurface::create(&window)?;
    let wid = surface.wid();
    SURFACE.with(|slot| *slot.borrow_mut() = Some(surface));

    match Player::new(PlayerConfig::embedded(wid), Some(app.handle().clone())) {
        Ok(player) => {
            app.manage(VideoState {
                player: Arc::new(player),
            });
            Ok(())
        }
        Err(error) => {
            take_surface();
            Err(error)
        }
    }
}

fn take_surface() {
    SURFACE.with(|slot| {
        if let Some(surface) = slot.borrow_mut().take() {
            let _ = surface.destroy();
        }
    });
}

#[tauri::command]
pub async fn video_open(
    app: AppHandle,
    state: State<'_, VideoState>,
    path: String,
) -> Result<VideoOpened, VideoError> {
    // Debug builds only: the worker-thread crash path the M0.4 acceptance criteria exercise.
    trip(ForcePoint::Open);

    let player = state.player();
    // mpv builds its own window inside the surface during the load and leaves it unmapped if the
    // surface is hidden, so the surface has to be visible first. See BACKLOG.md M0.2.
    on_main_thread(&app, || {
        // Debug builds only: the main-thread crash path, where no dialog can appear. See M0.4.
        trip(ForcePoint::MainThread);
        with_surface(|surface| surface.show())
    })
    .await?;

    // `open` blocks until mpv reports a verdict, so it never runs on the async runtime's poll.
    let opened = tauri::async_runtime::spawn_blocking(move || player.open(&path))
        .await
        .map_err(|error| VideoError::player_unavailable(format!("open task failed: {error}")))?;

    if opened.is_err() {
        let _ = on_main_thread(&app, || with_surface(|surface| surface.hide())).await;
    }
    opened
}

#[tauri::command]
pub async fn video_play(state: State<'_, VideoState>) -> Result<(), VideoError> {
    state.player().play()
}

#[tauri::command]
pub async fn video_pause(state: State<'_, VideoState>) -> Result<(), VideoError> {
    state.player().pause()
}

#[tauri::command]
pub async fn video_seek(state: State<'_, VideoState>, position: f64) -> Result<(), VideoError> {
    state.player().seek(position)
}

#[tauri::command]
pub async fn video_set_region(
    app: AppHandle,
    window: WebviewWindow,
    region: VideoRegion,
) -> Result<(), VideoError> {
    if !(region.x.is_finite() && region.y.is_finite()) {
        return Err(VideoError::command_failed(
            "region position is not a number",
        ));
    }

    on_main_thread(&app, move || apply_region(&window, region)).await
}

/// Run `action` on the main thread and wait for its result. `run_on_main_thread` only queues the
/// closure, so the channel is what makes the caller see the outcome.
async fn on_main_thread<F>(app: &AppHandle, action: F) -> Result<(), VideoError>
where
    F: FnOnce() -> Result<(), VideoError> + Send + 'static,
{
    let (sender, receiver) = channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(action());
    })
    .map_err(|error| VideoError::command_failed(format!("main thread dispatch: {error}")))?;

    tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(MAIN_THREAD_TIMEOUT))
        .await
        .map_err(|error| VideoError::command_failed(format!("main thread task failed: {error}")))?
        .map_err(|_| VideoError::command_failed("the main thread did not answer"))?
}

/// Main thread only: called from inside `run_on_main_thread`.
fn with_surface(
    action: impl FnOnce(&VideoSurface) -> Result<(), VideoError>,
) -> Result<(), VideoError> {
    SURFACE.with(|slot| {
        let slot = slot.borrow();
        let surface = slot
            .as_ref()
            .ok_or_else(|| VideoError::player_unavailable("the video surface is gone"))?;
        action(surface)
    })
}

/// Main thread only: called from inside `run_on_main_thread`.
fn apply_region(window: &WebviewWindow, region: VideoRegion) -> Result<(), VideoError> {
    let scale = window
        .scale_factor()
        .map_err(|error| VideoError::command_failed(format!("scale factor: {error}")))?;
    let region = SurfaceRegion {
        x: region.x,
        y: region.y,
        width: region.width,
        height: region.height,
        scale,
    };

    with_surface(|surface| {
        if region.is_empty() {
            surface.hide()
        } else {
            surface.set_region(region)
        }
    })
}
