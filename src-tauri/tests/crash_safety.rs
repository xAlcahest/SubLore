//! Behavioural tests for M0.4, written from the acceptance criteria in BACKLOG.md:
//! a forced error path writes a crash report and the process ends cleanly, so the app can be
//! started again. The panic hook ends the process, so it cannot be observed in-process: every
//! test here re-runs this test binary as a child and asserts on its exit status and its files.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sublore_lib::crash;
use sublore_lib::crash::force::ForcePoint;

/// Names the scenario the child process must run. Absent in a normal test run.
const CHILD_ENV: &str = "SUBLORE_CRASH_CHILD";
/// Where the child writes its crash report. Absent means "use the fallback".
const DIR_ENV: &str = "SUBLORE_CRASH_DIR";
const HEADER: &str = "==== Sublore";
/// The exit code the default Rust panic runtime uses, and the one the hook must reproduce.
const EXIT_PANIC: i32 = 101;
const CHILD_TIMEOUT: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------------------
// The child half. A no-op unless the parent asked for a crash.
// ---------------------------------------------------------------------------

#[test]
fn crash_child() {
    let Ok(spec) = env::var(CHILD_ENV) else {
        return;
    };
    crash::install();
    if let Ok(dir) = env::var(DIR_ENV) {
        crash::set_report_path(PathBuf::from(dir).join("crash.log"));
    }

    match spec.as_str() {
        "unattached" => panic!("test panic with no report path set"),
        "worker" => panic_on_worker("test panic on a worker thread"),
        "worker-again" => panic_on_worker("test panic on a second run"),
        "storm" => {
            let barrier = Arc::new(Barrier::new(4));
            let mut workers = Vec::new();
            for index in 0..4 {
                let barrier = Arc::clone(&barrier);
                workers.push(
                    thread::Builder::new()
                        .name(format!("sublore-test-storm-{index}"))
                        .spawn(move || {
                            barrier.wait();
                            panic!("test panic from storm thread {index}");
                        })
                        .expect("a storm thread should start"),
                );
            }
            for worker in workers {
                let _ = worker.join();
            }
            panic!("the hook returned instead of ending the process");
        }
        other => panic!("unknown child scenario {other}"),
    }
}

/// Panic on a thread of a known name, so the report can be checked for it.
fn panic_on_worker(message: &'static str) -> ! {
    let worker = thread::Builder::new()
        .name("sublore-test-worker".to_owned())
        .spawn(move || panic!("{message}"))
        .expect("the worker thread should start");
    let _ = worker.join();
    panic!("the hook returned instead of ending the process");
}

// ---------------------------------------------------------------------------
// The parent half.
// ---------------------------------------------------------------------------

/// A directory of this test's own, under the OS temp dir. Removed at the end of the test.
fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!(
        "sublore-crash-test-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// Re-run this binary with only `crash_child` selected. `--test-threads=1` stops libtest from
/// wrapping the scenario in a thread of its own, so a panic lands on the thread the scenario chose.
fn spawn_child(spec: &str, report_dir: Option<&Path>, temp_dir: Option<&Path>) -> Child {
    let exe = env::current_exe().expect("the test binary should have a path");
    let mut command = Command::new(exe);
    command
        .args(["--exact", "crash_child", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, spec)
        .env("RUST_BACKTRACE", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match report_dir {
        Some(dir) => command.env(DIR_ENV, dir),
        None => command.env_remove(DIR_ENV),
    };
    if let Some(dir) = temp_dir {
        // Windows reads TMP/TEMP for its temp dir and never TMPDIR, so all three are set. See M0.4.
        command.env("TMPDIR", dir).env("TMP", dir).env("TEMP", dir);
    }
    command.spawn().expect("the child process should start")
}

/// Wait for `child`, killing it if it outlives `timeout`. A hook that hangs is a failure, not a
/// hung test run.
fn wait_bounded(mut child: Child, timeout: Duration) -> ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        match child
            .try_wait()
            .expect("the child status should be readable")
        {
            Some(status) => return status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the child did not exit within {timeout:?}");
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn run_child(spec: &str, report_dir: Option<&Path>, temp_dir: Option<&Path>) -> ExitStatus {
    wait_bounded(spawn_child(spec, report_dir, temp_dir), CHILD_TIMEOUT)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()))
}

fn assert_panicked(status: ExitStatus) {
    assert_eq!(
        status.code(),
        Some(EXIT_PANIC),
        "the crash hook must end the process with {EXIT_PANIC}, got {status:?}"
    );
}

// ---------------------------------------------------------------------------
// Acceptance: the forced error path writes the log and the process ends cleanly.
// ---------------------------------------------------------------------------

#[test]
fn panic_on_worker_thread_writes_report_and_exits_101() {
    let dir = scratch("worker");
    let status = run_child("worker", Some(&dir), None);
    assert_panicked(status);

    let report = read(&dir.join("crash.log"));
    assert!(
        report.contains(HEADER),
        "missing report header in:\n{report}"
    );
    assert!(
        report.contains("test panic on a worker thread"),
        "missing panic message in:\n{report}"
    );
    assert!(
        report.contains("sublore-test-worker"),
        "missing thread name in:\n{report}"
    );
    assert!(
        report.contains("location: "),
        "missing panic location in:\n{report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn crash_report_is_appended_not_truncated() {
    let dir = scratch("append");
    assert_panicked(run_child("worker", Some(&dir), None));
    let first = read(&dir.join("crash.log"));
    assert_panicked(run_child("worker-again", Some(&dir), None));
    let both = read(&dir.join("crash.log"));

    assert!(
        both.starts_with(&first),
        "the second crash must not overwrite the first:\n{both}"
    );
    assert_eq!(
        both.matches(HEADER).count(),
        2,
        "two crashes must leave two report blocks:\n{both}"
    );
    assert!(
        both.contains("test panic on a worker thread")
            && both.contains("test panic on a second run"),
        "both panic messages must survive:\n{both}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn second_panic_does_not_recurse() {
    let dir = scratch("storm");
    let status = run_child("storm", Some(&dir), None);
    assert_panicked(status);

    let report = read(&dir.join("crash.log"));
    assert_eq!(
        report.matches(HEADER).count(),
        1,
        "concurrent panics must produce exactly one report block:\n{report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn report_is_rotated_when_oversized() {
    let dir = scratch("rotate");
    let path = dir.join("crash.log");
    let old = "old crash report\n"
        .repeat((crash::MAX_REPORT_BYTES as usize / "old crash report\n".len()) + 16);
    fs::write(&path, &old).expect("the oversized report should be writable");

    assert_panicked(run_child("worker", Some(&dir), None));

    let rotated = read(&dir.join("crash.log.1"));
    assert!(
        rotated.contains("old crash report"),
        "the oversized report must move to crash.log.1"
    );
    let current = read(&path);
    assert!(
        !current.contains("old crash report"),
        "the fresh report must not keep the rotated content"
    );
    assert_eq!(
        current.matches(HEADER).count(),
        1,
        "the fresh report holds exactly the new crash:\n{current}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn report_path_falls_back_to_temp_dir_when_unattached() {
    assert!(
        crash::report_path().starts_with(env::temp_dir()),
        "with no app attached the report path must sit in the OS temp dir, got {}",
        crash::report_path().display()
    );

    let dir = scratch("fallback");
    assert_panicked(run_child("unattached", None, Some(&dir)));

    let report = read(&dir.join("sublore-crash.log"));
    assert!(
        report.contains("test panic with no report path set"),
        "the fallback report must hold the panic:\n{report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn force_trip_ignores_unknown_values() {
    assert_eq!(ForcePoint::parse("startup"), Some(ForcePoint::Startup));
    assert_eq!(ForcePoint::parse("open"), Some(ForcePoint::Open));
    assert_eq!(
        ForcePoint::parse("main-thread"),
        Some(ForcePoint::MainThread)
    );

    for value in ["", "  ", "nonsense", "Startup", "open ", "main_thread", "1"] {
        assert_eq!(
            ForcePoint::parse(value),
            None,
            "{value:?} must not select a trip point"
        );
    }
}
