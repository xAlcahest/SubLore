//! Crash safety: a panic hook that writes a readable report next to the log file, shows a native
//! dialog when one can still appear, and ends the process with the panic exit code so the user can
//! simply start Sublore again. See BACKLOG.md M0.4.
//!
//! The hook never uses the `log` macros, `println!` or anything else that takes a shared lock: a
//! panic raised while the logger's mutex is held by this same thread would deadlock the handler and
//! the user would get nothing. The report is written through its own file handle for that reason.

pub mod force;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::OnceLock;
use std::time::Duration;

use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_dialog::{Dialog, DialogExt, MessageDialogKind};

use crate::log;
use crate::strings;

/// The exit code the default Rust panic runtime uses.
const EXIT_PANIC: i32 = 101;
/// How long the hook waits for proof that the main loop can still run a closure.
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
/// How long the hook waits for the dialog to be dismissed before exiting anyway.
const DIALOG_TIMEOUT: Duration = Duration::from_secs(120);
/// Past this the report is rotated to `crash.log.1`. The only automatic deletion in the app, and it
/// only ever touches Sublore's own file. See CLAUDE.md section 3.
pub const MAX_REPORT_BYTES: u64 = 256 * 1024;
const REPORT_FILE: &str = "crash.log";
const FALLBACK_REPORT_FILE: &str = "sublore-crash.log";

/// The hook `install` displaced, kept so debug builds still print the familiar panic line.
type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send>;

static IN_HANDLER: AtomicBool = AtomicBool::new(false);
static REPORT_PATH: OnceLock<PathBuf> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();
static PREVIOUS_HOOK: OnceLock<PanicHook> = OnceLock::new();

/// Take over panic handling. Call this before anything else in `run`.
pub fn install() {
    let previous = std::panic::take_hook();
    // Only the first install records a previous hook; a second call would otherwise store ours.
    let _ = PREVIOUS_HOOK.set(previous);
    std::panic::set_hook(Box::new(on_panic));
}

/// Give the hook the app handle it needs for the dialog and the real report path. Call this at the
/// top of Tauri's `setup`, before anything that can fail.
pub fn attach(app: &tauri::App) {
    let _ = APP.set(app.handle().clone());
    match app.path().app_log_dir() {
        Ok(dir) => set_report_path(dir.join(REPORT_FILE)),
        Err(error) => {
            log::error!("no log directory, crash reports fall back to the temp dir: {error}");
        }
    }
}

/// Fix where crash reports are written. The first call wins; in the app that caller is `attach`.
pub fn set_report_path(path: PathBuf) {
    let _ = REPORT_PATH.set(path);
}

/// Where the next crash report goes. Before `attach` runs, that is Sublore's own file in the OS
/// temp dir, so an early crash is still recorded somewhere.
pub fn report_path() -> PathBuf {
    REPORT_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join(FALLBACK_REPORT_FILE))
}

fn on_panic(info: &PanicHookInfo<'_>) {
    // A second panic, here or on another thread, must be a no-op rather than a recursive handler.
    if IN_HANDLER.swap(true, Ordering::SeqCst) {
        return;
    }

    let path = report_path();
    let written = append_report(&path, &format_report(info)).is_ok();

    // Debug builds keep the familiar `thread ... panicked` line on stderr. Release stays silent.
    #[cfg(debug_assertions)]
    if let Some(previous) = PREVIOUS_HOOK.get() {
        previous(info);
    }

    show_dialog(&path, written);

    // A panic on a worker thread would otherwise leave the app alive but broken, and the acceptance
    // criterion is that the user can start it again.
    std::process::exit(EXIT_PANIC);
}

fn format_report(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-text panic payload>");
    let location = match info.location() {
        Some(location) => format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        ),
        None => "<unknown>".to_owned(),
    };
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("<unnamed>").to_owned();
    let now = tauri_plugin_log::TimezoneStrategy::UseUtc.get_now();
    let backtrace = std::backtrace::Backtrace::force_capture();

    format!(
        "==== Sublore {version} crash ====\n\
         time:     {now}\n\
         thread:   {thread}\n\
         location: {location}\n\
         message:  {message}\n\
         backtrace:\n{backtrace}\n\n",
        version = env!("CARGO_PKG_VERSION"),
    )
}

fn append_report(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    rotate_if_oversized(path)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(text.as_bytes())?;
    file.flush()?;
    // `process::exit` follows immediately, so the page cache is not good enough.
    file.sync_data()
}

/// One rename, replacing the previous archive, and only past the cap.
fn rotate_if_oversized(path: &Path) -> std::io::Result<()> {
    let oversized = match fs::metadata(path) {
        Ok(metadata) => metadata.len() > MAX_REPORT_BYTES,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if oversized {
        let mut archive = path.as_os_str().to_owned();
        archive.push(".1");
        fs::rename(path, PathBuf::from(archive))?;
    }
    Ok(())
}

/// Show the crash dialog when one can still appear. A panic on the main thread cannot have one:
/// the thread that would pump GTK is the one unwinding, and that is a documented limitation.
fn show_dialog(path: &Path, report_written: bool) {
    let Some(app) = APP.get() else {
        return;
    };
    // `dialog()` resolves plugin state by unwrapping, so check it here rather than panic in a hook.
    if app.try_state::<Dialog<Wry>>().is_none() {
        return;
    }
    if !main_loop_responds(app) {
        return;
    }

    let body = if report_written {
        strings::crash_body(&path.display().to_string())
    } else {
        strings::CRASH_BODY_NO_REPORT.to_owned()
    };

    let app = app.clone();
    let (done, finished) = channel();
    let dialog = std::thread::Builder::new()
        .name("sublore-crash-dialog".to_owned())
        .spawn(move || {
            // blocking_show marshals onto the main thread and must not run on it.
            app.dialog()
                .message(body)
                .title(strings::CRASH_TITLE)
                .kind(MessageDialogKind::Error)
                .blocking_show();
            let _ = done.send(());
        });

    if dialog.is_ok() {
        // A disconnect means the dialog thread is gone; either way the wait is bounded.
        let _ = finished.recv_timeout(DIALOG_TIMEOUT);
    }
}

/// Ask the main loop to run a closure and wait a moment for the answer. This is the deterministic
/// way to know whether a native dialog can appear at all right now.
fn main_loop_responds(app: &AppHandle) -> bool {
    let (sender, receiver) = channel();
    if app
        .run_on_main_thread(move || {
            let _ = sender.send(());
        })
        .is_err()
    {
        return false;
    }
    receiver.recv_timeout(PROBE_TIMEOUT).is_ok()
}
