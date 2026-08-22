//! The mpv core: options, lifecycle, the single event thread, and the blocking command surface.
//! Nothing here touches Tauri windows; the native surface lives in `super::surface`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use libmpv2::events::{Event, PropertyData};
use libmpv2::{Format, Mpv};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::error::{from_mpv, VideoError, VideoErrorCode};

pub const EVENT_POSITION: &str = "video://position";
pub const EVENT_STATE: &str = "video://state";
pub const EVENT_ERROR: &str = "video://error";

const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const POSITION_EVENT_INTERVAL: Duration = Duration::from_millis(100);
const EVENT_POLL_SECONDS: f64 = 0.1;
/// How long shutdown waits for in-flight commands to release their mpv handle.
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

const OBSERVE_TIME_POS: u64 = 1;
const OBSERVE_PAUSE: u64 = 2;

/// mpv defaults that would write files, read config, follow references or grab input are all
/// turned off explicitly rather than assumed. See CLAUDE.md section 3 and the M0.2 design.
const SAFE_OPTIONS: &[(&str, &str)] = &[
    ("config", "no"),
    ("load-scripts", "no"),
    ("terminal", "no"),
    ("ytdl", "no"),
    ("save-position-on-quit", "no"),
    ("resume-playback", "no"),
    ("watch-later-options", ""),
    ("sub-auto", "no"),
    ("audio-file-auto", "no"),
    ("access-references", "no"),
    ("input-default-bindings", "no"),
    ("input-vo-keyboard", "no"),
    ("input-cursor", "no"),
    ("osd-level", "0"),
    ("keep-open", "yes"),
    ("idle", "yes"),
    ("pause", "yes"),
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoOpened {
    pub path: String,
    pub duration: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayerStatus {
    Idle,
    Loading,
    Ready,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPlayerState {
    pub status: PlayerStatus,
    pub path: Option<String>,
    pub duration: Option<f64>,
    pub paused: bool,
}

impl VideoPlayerState {
    fn idle() -> Self {
        Self {
            status: PlayerStatus::Idle,
            path: None,
            duration: None,
            paused: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionPayload {
    position: f64,
}

/// How the mpv core is wired to its output. `headless` exists for the integration tests;
/// it is never reachable over IPC.
pub struct PlayerConfig {
    pub wid: Option<i64>,
    pub headless: bool,
}

impl PlayerConfig {
    pub fn headless() -> Self {
        Self {
            wid: None,
            headless: true,
        }
    }

    pub fn embedded(wid: i64) -> Self {
        Self {
            wid: Some(wid),
            headless: false,
        }
    }
}

/// State the command threads and the event thread both reach.
struct Shared {
    app: Option<AppHandle>,
    state: Mutex<VideoPlayerState>,
    pending_open: Mutex<Option<SyncSender<Result<f64, VideoError>>>>,
}

impl Shared {
    fn emit_state(&self) {
        let Ok(state) = self.state.lock() else {
            return;
        };
        let payload = state.clone();
        drop(state);
        if let Some(app) = &self.app {
            let _ = app.emit(EVENT_STATE, payload);
        }
    }

    fn emit_position(&self, position: f64) {
        if let Some(app) = &self.app {
            let _ = app.emit(EVENT_POSITION, PositionPayload { position });
        }
    }

    fn emit_error(&self, error: &VideoError) {
        if let Some(app) = &self.app {
            let _ = app.emit(EVENT_ERROR, error.clone());
        }
    }

    /// Hand the outcome of a load to whoever is blocked in `Player::open`.
    fn resolve_open(&self, outcome: Result<f64, VideoError>) -> bool {
        let Ok(mut pending) = self.pending_open.lock() else {
            return false;
        };
        match pending.take() {
            Some(sender) => {
                let _ = sender.send(outcome);
                true
            }
            None => false,
        }
    }
}

pub struct Player {
    /// `None` once shut down, which is what makes every later command fail fast.
    mpv: Mutex<Option<Arc<Mpv>>>,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    event_thread: Mutex<Option<JoinHandle<()>>>,
    /// Set once mpv_destroy has really run, so a second `shutdown` reports the same verdict.
    core_destroyed: AtomicBool,
}

impl Player {
    pub fn new(config: PlayerConfig, app: Option<AppHandle>) -> Result<Self, VideoError> {
        force_c_numeric_locale()?;

        let mpv = Mpv::with_initializer(|init| {
            for (name, value) in SAFE_OPTIONS {
                init.set_option(name, *value)?;
            }
            if config.headless {
                init.set_option("vo", "null")?;
                init.set_option("ao", "null")?;
            }
            if let Some(wid) = config.wid {
                init.set_option("wid", wid)?;
            }
            Ok(())
        })
        .map_err(|error| from_mpv(error, "mpv initialisation"))?;

        mpv.observe_property("time-pos", Format::Double, OBSERVE_TIME_POS)
            .map_err(|error| from_mpv(error, "observe time-pos"))?;
        mpv.observe_property("pause", Format::Flag, OBSERVE_PAUSE)
            .map_err(|error| from_mpv(error, "observe pause"))?;

        let mpv = Arc::new(mpv);
        let shared = Arc::new(Shared {
            app,
            state: Mutex::new(VideoPlayerState::idle()),
            pending_open: Mutex::new(None),
        });
        let stop = Arc::new(AtomicBool::new(false));

        let event_thread = std::thread::Builder::new()
            .name("sublore-mpv-events".to_owned())
            .spawn({
                let mpv = Arc::clone(&mpv);
                let shared = Arc::clone(&shared);
                let stop = Arc::clone(&stop);
                move || event_loop(&mpv, &shared, &stop)
            })
            .map_err(|error| {
                VideoError::player_unavailable(format!("could not start the event thread: {error}"))
            })?;

        Ok(Self {
            mpv: Mutex::new(Some(mpv)),
            shared,
            stop,
            event_thread: Mutex::new(Some(event_thread)),
            core_destroyed: AtomicBool::new(false),
        })
    }

    /// Load a file, paused at position 0, and wait for mpv's verdict.
    pub fn open(&self, path: &str) -> Result<VideoOpened, VideoError> {
        let target = validate_path(path)?;

        // Taking the handle and arming the channel under one lock is what stops `shutdown` from
        // slipping between them and leaving this call waiting on a stopped event thread. See M0.2.
        let (mpv, receiver) = {
            let guard = self
                .mpv
                .lock()
                .map_err(|_| VideoError::player_unavailable("player lock poisoned"))?;
            let mpv = guard
                .clone()
                .ok_or_else(|| VideoError::player_unavailable("the player is not running"))?;
            let mut pending = self
                .shared
                .pending_open
                .lock()
                .map_err(|_| VideoError::player_unavailable("pending open lock poisoned"))?;
            if pending.is_some() {
                return Err(VideoError::command_failed(
                    "another open is already in progress",
                ));
            }
            let (sender, receiver) = sync_channel(1);
            *pending = Some(sender);
            (mpv, receiver)
        };

        self.set_state(|state| {
            state.status = PlayerStatus::Loading;
            state.path = Some(target.clone());
            state.duration = None;
            state.paused = true;
        })?;
        self.shared.emit_state();

        let issued = mpv
            .set_property("pause", true)
            .and_then(|()| mpv.command("loadfile", &[&target]));
        if let Err(error) = issued {
            self.shared
                .resolve_open(Err(VideoError::command_failed("loadfile")));
            self.reset_to_idle();
            return Err(from_mpv(error, "loadfile"));
        }

        // loadfile returns Ok even for a file mpv cannot play; the verdict arrives on the event queue.
        let outcome = receiver.recv_timeout(OPEN_TIMEOUT);
        let _ = self
            .shared
            .pending_open
            .lock()
            .map(|mut pending| pending.take());

        match outcome {
            Ok(Ok(duration)) => {
                self.set_state(|state| {
                    state.status = PlayerStatus::Ready;
                    state.duration = Some(duration);
                    state.paused = true;
                })?;
                self.shared.emit_state();
                Ok(VideoOpened {
                    path: target,
                    duration,
                })
            }
            Ok(Err(error)) => {
                self.reset_to_idle();
                Err(error)
            }
            Err(RecvTimeoutError::Timeout) => {
                self.reset_to_idle();
                Err(VideoError::new(VideoErrorCode::OpenTimeout, target))
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.reset_to_idle();
                Err(VideoError::player_unavailable(
                    "the player stopped while opening",
                ))
            }
        }
    }

    pub fn play(&self) -> Result<(), VideoError> {
        self.set_pause(false)
    }

    pub fn pause(&self) -> Result<(), VideoError> {
        self.set_pause(true)
    }

    /// Absolute seconds from the start, clamped into the file's range.
    pub fn seek(&self, position: f64) -> Result<(), VideoError> {
        let mpv = self.handle()?;
        let duration = self.loaded_duration()?;
        if !position.is_finite() {
            return Err(VideoError::command_failed("seek position is not a number"));
        }
        let target = position.clamp(0.0, duration);
        mpv.command("seek", &[&format!("{target}"), "absolute"])
            .map_err(|error| from_mpv(error, "seek"))
    }

    pub fn position(&self) -> Result<f64, VideoError> {
        let mpv = self.handle()?;
        mpv.get_property::<f64>("time-pos")
            .map_err(|error| from_mpv(error, "time-pos"))
    }

    pub fn paused(&self) -> Result<bool, VideoError> {
        let mpv = self.handle()?;
        mpv.get_property::<bool>("pause")
            .map_err(|error| from_mpv(error, "pause"))
    }

    /// Deterministic and idempotent. Order matters: no new handles, then stop the event thread,
    /// then destroy the core. Reports whether the core is actually gone, which is what callers
    /// need before they destroy the native surface it was drawing into.
    #[must_use]
    pub fn shutdown(&self) -> bool {
        // Same lock order as `open`, so an open racing this either finds a live player or never
        // arms its channel. Dropping the sender releases anyone already blocked there. See M0.2.
        let mpv = {
            let mut guard = match self.mpv.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let taken = guard.take();
            match self.shared.pending_open.lock() {
                Ok(mut pending) => drop(pending.take()),
                Err(poisoned) => drop(poisoned.into_inner().take()),
            }
            taken
        };

        self.stop.store(true, Ordering::Relaxed);
        let thread = match self.event_thread.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(thread) = thread {
            let _ = thread.join();
        }

        if let Some(mpv) = mpv {
            // mpv_destroy must run here, so wait for in-flight commands to drop their clones.
            let deadline = Instant::now() + SHUTDOWN_DRAIN_TIMEOUT;
            while Arc::strong_count(&mpv) > 1 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            // Only dropping the last clone runs mpv_destroy; a straggler means the core outlives
            // this call, so the surface has to stay. See M0.2.
            let destroyed = Arc::strong_count(&mpv) == 1;
            drop(mpv);
            self.core_destroyed.store(destroyed, Ordering::Release);
        }
        self.core_destroyed.load(Ordering::Acquire)
    }

    fn handle(&self) -> Result<Arc<Mpv>, VideoError> {
        let guard = self
            .mpv
            .lock()
            .map_err(|_| VideoError::player_unavailable("player lock poisoned"))?;
        guard
            .clone()
            .ok_or_else(|| VideoError::player_unavailable("the player is not running"))
    }

    fn state(&self) -> Result<MutexGuard<'_, VideoPlayerState>, VideoError> {
        self.shared
            .state
            .lock()
            .map_err(|_| VideoError::player_unavailable("state lock poisoned"))
    }

    fn set_state(&self, update: impl FnOnce(&mut VideoPlayerState)) -> Result<(), VideoError> {
        let mut state = self.state()?;
        update(&mut state);
        Ok(())
    }

    fn reset_to_idle(&self) {
        if self
            .set_state(|state| *state = VideoPlayerState::idle())
            .is_ok()
        {
            self.shared.emit_state();
        }
    }

    fn loaded_duration(&self) -> Result<f64, VideoError> {
        let state = self.state()?;
        match (state.status, state.duration) {
            (PlayerStatus::Ready, Some(duration)) => Ok(duration),
            _ => Err(VideoError::new(
                VideoErrorCode::NotLoaded,
                "no file is open",
            )),
        }
    }

    fn set_pause(&self, paused: bool) -> Result<(), VideoError> {
        let mpv = self.handle()?;
        self.loaded_duration()?;
        mpv.set_property("pause", paused)
            .map_err(|error| from_mpv(error, "pause"))
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // Nothing owns the surface here, so the verdict has no caller to act on it.
        let _ = self.shutdown();
    }
}

/// libmpv refuses to start unless LC_NUMERIC is exactly "C", and GTK sets the user's locale during
/// its own init, so this has to run after GTK and immediately before mpv_create. See BACKLOG.md M0.2.
fn force_c_numeric_locale() -> Result<(), VideoError> {
    // SAFETY: setlocale with a valid category and a NUL-terminated string.
    let applied = unsafe { libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr()) };
    if applied.is_null() {
        return Err(VideoError::player_unavailable(
            "could not set LC_NUMERIC to C, which libmpv requires",
        ));
    }
    Ok(())
}

/// Reject anything that is not an existing regular file before mpv sees it. This is what keeps a
/// crafted path away from mpv's protocol handlers. See CLAUDE.md section 3.
fn validate_path(path: &str) -> Result<String, VideoError> {
    if path.trim().is_empty() {
        return Err(VideoError::invalid_path("empty path"));
    }
    let candidate = Path::new(path);
    if !candidate.is_file() {
        return Err(VideoError::invalid_path(path.to_owned()));
    }
    let resolved = candidate
        .canonicalize()
        .map_err(|error| VideoError::invalid_path(format!("{path}: {error}")))?;
    Ok(mpv_path(&resolved))
}

/// Windows canonicalisation yields a `\\?\` verbatim path, which mpv does not accept. See M0.2.
fn mpv_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.chars().nth(1) == Some(':') => rest.to_owned(),
        _ => text.into_owned(),
    }
}

/// The only caller of `wait_event` in the codebase, so a second mpv client handle is never needed
/// and libmpv2's unchecked `create_client` is never reached. See the M0.2 design, section 2.1.
fn event_loop(mpv: &Mpv, shared: &Shared, stop: &AtomicBool) {
    let mut last_position = Instant::now() - POSITION_EVENT_INTERVAL;

    while !stop.load(Ordering::Relaxed) {
        match mpv.wait_event(EVENT_POLL_SECONDS) {
            Some(Ok(Event::PropertyChange {
                name: "time-pos",
                change: PropertyData::Double(position),
                ..
            })) => {
                // mpv reports time-pos at frame rate; the UI gets at most 10 updates per second.
                if last_position.elapsed() >= POSITION_EVENT_INTERVAL {
                    last_position = Instant::now();
                    shared.emit_position(position);
                }
            }
            Some(Ok(Event::PropertyChange {
                name: "pause",
                change: PropertyData::Flag(paused),
                ..
            })) => {
                if let Ok(mut state) = shared.state.lock() {
                    state.paused = paused;
                }
                shared.emit_state();
            }
            Some(Ok(Event::FileLoaded)) => {
                let outcome = match mpv.get_property::<f64>("duration") {
                    Ok(duration) if duration > 0.0 => Ok(duration),
                    Ok(duration) => Err(VideoError::open_failed(format!(
                        "mpv reported duration {duration}"
                    ))),
                    Err(error) => Err(from_mpv(error, "duration")),
                };
                shared.resolve_open(outcome);
            }
            Some(Ok(Event::Shutdown)) => break,
            // libmpv2 turns a failed load or a failed playback into Err rather than an EndFile
            // reason, so this arm is the only place a load failure can be observed.
            Some(Err(error)) => {
                let mapped = from_mpv(error, "playback");
                let mapped = VideoError::new(VideoErrorCode::OpenFailed, mapped.detail);
                if !shared.resolve_open(Err(mapped.clone())) {
                    let stopped = VideoError::new(VideoErrorCode::PlaybackStopped, mapped.detail);
                    shared.emit_error(&stopped);
                }
            }
            _ => {}
        }
    }
}
