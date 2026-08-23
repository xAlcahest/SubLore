//! The main-thread half of the M0.4 acceptance criteria: a panic on the process main thread still
//! writes the crash report and still ends the process with the panic exit code, even though no
//! native dialog can appear there.
//!
//! This target has no libtest harness on purpose. libtest runs every `#[test]` on a thread it
//! spawns, so a `#[test]` can never panic on the real main thread; here `main` is that thread.
//! Running the binary is the test: exit 0 passes, a panic fails it.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sublore_lib::crash;

/// Set by the parent to turn this binary into the crashing child.
const CHILD_ENV: &str = "SUBLORE_CRASH_MAIN_CHILD";
const HEADER: &str = "==== Sublore";
const PANIC_MESSAGE: &str = "test panic on the main thread";
/// The exit code the default Rust panic runtime uses, and the one the hook must reproduce.
const EXIT_PANIC: i32 = 101;
const CHILD_TIMEOUT: Duration = Duration::from_secs(20);

fn main() {
    match env::var(CHILD_ENV) {
        Ok(dir) => crash_on_main_thread(dir),
        Err(_) => panic_on_main_thread_writes_report_and_exits_101(),
    }
}

fn crash_on_main_thread(dir: String) -> ! {
    crash::install();
    crash::set_report_path(PathBuf::from(dir).join("crash.log"));
    panic!("{PANIC_MESSAGE}");
}

fn panic_on_main_thread_writes_report_and_exits_101() {
    let dir = scratch();
    let exe = env::current_exe().expect("the test binary should have a path");
    let child = Command::new(exe)
        .env(CHILD_ENV, &dir)
        .env("RUST_BACKTRACE", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the child process should start");

    let status = wait_bounded(child);
    assert_eq!(
        status.code(),
        Some(EXIT_PANIC),
        "a main-thread panic must end the process with {EXIT_PANIC}, got {status:?}"
    );

    let path = dir.join("crash.log");
    let report = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
    assert!(
        report.contains(HEADER),
        "missing report header in:\n{report}"
    );
    assert!(
        report.contains(PANIC_MESSAGE),
        "missing panic message in:\n{report}"
    );
    assert!(
        report.contains("thread:   main"),
        "the panic must be recorded as the main thread:\n{report}"
    );
    assert!(
        report.contains("location: "),
        "missing panic location in:\n{report}"
    );

    let _ = fs::remove_dir_all(&dir);
    println!("test panic_on_main_thread_writes_report_and_exits_101 ... ok");
}

/// A directory of this test's own, under the OS temp dir. Removed when the test passes.
fn scratch() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let dir = env::temp_dir().join(format!("sublore-crash-main-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// Wait for `child`, killing it if it outlives the timeout. A hook that hangs is a failure, not a
/// hung test run.
fn wait_bounded(mut child: Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + CHILD_TIMEOUT;
    loop {
        match child
            .try_wait()
            .expect("the child status should be readable")
        {
            Some(status) => return status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the child did not exit within {CHILD_TIMEOUT:?}");
            }
            None => sleep(Duration::from_millis(20)),
        }
    }
}
