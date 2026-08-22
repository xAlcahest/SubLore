//! Behavioural tests for M0.2, written from the acceptance criteria in BACKLOG.md:
//! open fixtures/video/sample.mkv, play, pause, seek to 0:30, shut down with nothing left running.
//! These drive a real libmpv core headless (vo=null, ao=null), so they need no display.

use std::ffi::CStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use sublore_lib::video::error::VideoErrorCode;
use sublore_lib::video::player::{Player, PlayerConfig};

const FIXTURE_DURATION: f64 = 60.0;

/// The locale test moves a process-global setting, so no two mpv cores are built at the same time.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/video/sample.mkv");
    assert!(
        p.is_file(),
        "missing fixture {}: run fixtures/video/make-sample.sh",
        p.display()
    );
    p
}

fn fixture_path() -> String {
    fixture().to_string_lossy().into_owned()
}

fn player() -> Player {
    Player::new(PlayerConfig::headless(), None)
        .expect("headless player should start; is libmpv installed?")
}

/// Poll `check` until it is true or `timeout` elapses. Never a bare sleep-then-read.
fn wait_until(timeout: Duration, mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if check() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(25));
    }
}

fn current_numeric_locale() -> String {
    // SAFETY: a null locale argument only queries the current setting.
    let name = unsafe { libc::setlocale(libc::LC_NUMERIC, std::ptr::null()) };
    assert!(!name.is_null(), "setlocale query returned null");
    unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned()
}

#[test]
fn opens_fixture_and_reports_duration() {
    let _serial = serial();
    let player = player();
    let opened = player.open(&fixture_path()).expect("fixture should open");

    assert!(
        (opened.duration - FIXTURE_DURATION).abs() < 0.5,
        "duration was {}, expected about {FIXTURE_DURATION}",
        opened.duration
    );
    // Compare canonical forms: on Windows the path handed to mpv has its `\\?\` prefix stripped.
    let loaded = Path::new(&opened.path)
        .canonicalize()
        .expect("the reported path exists");
    let expected = fixture()
        .canonicalize()
        .expect("fixture path canonicalises");
    assert_eq!(loaded, expected);
}

#[test]
fn opens_paused_at_start() {
    let _serial = serial();
    let player = player();
    player.open(&fixture_path()).expect("fixture should open");

    assert!(
        player.paused().expect("pause is readable"),
        "open must leave the file paused"
    );
    assert!(
        wait_until(
            Duration::from_secs(5),
            || matches!(player.position(), Ok(t) if t < 0.5)
        ),
        "position should settle at the start of the file, last read was {:?}",
        player.position()
    );
}

#[test]
fn play_advances_position() {
    let _serial = serial();
    let player = player();
    player.open(&fixture_path()).expect("fixture should open");
    player.play().expect("play should be accepted");

    sleep(Duration::from_millis(1500));
    let t = player
        .position()
        .expect("position is readable while playing");
    assert!(
        (1.0..3.0).contains(&t),
        "position after 1.5 s of playback was {t}, expected between 1.0 and 3.0"
    );
}

#[test]
fn pause_freezes_position() {
    let _serial = serial();
    let player = player();
    player.open(&fixture_path()).expect("fixture should open");
    player.play().expect("play should be accepted");
    sleep(Duration::from_millis(500));
    player.pause().expect("pause should be accepted");
    sleep(Duration::from_millis(200));

    let first = player
        .position()
        .expect("position is readable while paused");
    sleep(Duration::from_millis(800));
    let second = player
        .position()
        .expect("position is readable while paused");

    assert_eq!(first, second, "a paused player must not advance");
}

#[test]
fn seek_to_thirty_moves_position() {
    let _serial = serial();
    let player = player();
    player.open(&fixture_path()).expect("fixture should open");

    player
        .seek(30.0)
        .expect("seek should be accepted while paused");
    assert!(
        wait_until(
            Duration::from_secs(5),
            || matches!(player.position(), Ok(t) if (t - 30.0).abs() < 0.5)
        ),
        "paused seek to 30 s left position at {:?}",
        player.position()
    );

    player.seek(0.0).expect("seek back to the start");
    player.play().expect("play should be accepted");
    assert!(
        wait_until(
            Duration::from_secs(5),
            || matches!(player.position(), Ok(t) if t > 0.2)
        ),
        "playback should advance before the second seek"
    );

    player
        .seek(30.0)
        .expect("seek should be accepted while playing");
    assert!(
        wait_until(
            Duration::from_secs(5),
            || matches!(player.position(), Ok(t) if (29.5..33.0).contains(&t))
        ),
        "playing seek to 30 s left position at {:?}",
        player.position()
    );
}

#[test]
fn seek_clamps_out_of_range() {
    let _serial = serial();
    let player = player();
    let opened = player.open(&fixture_path()).expect("fixture should open");

    player
        .seek(-5.0)
        .expect("a negative seek is clamped, not an error");
    assert!(
        wait_until(
            Duration::from_secs(5),
            || matches!(player.position(), Ok(t) if t < 0.5)
        ),
        "seek(-5.0) should land at the start, position was {:?}",
        player.position()
    );

    player
        .seek(9_999.0)
        .expect("a seek past the end is clamped, not an error");
    assert!(
        wait_until(Duration::from_secs(5), || {
            matches!(player.position(), Ok(t) if (t - opened.duration).abs() < 0.5)
        }),
        "seek(9999.0) should land at the end ({}), position was {:?}",
        opened.duration,
        player.position()
    );
}

#[test]
fn open_missing_file_reports_invalid_path() {
    let _serial = serial();
    let player = player();

    let error = player
        .open("/nonexistent/nope.mkv")
        .expect_err("a missing file must not open");
    assert_eq!(error.code, VideoErrorCode::InvalidPath);

    player
        .open(&fixture_path())
        .expect("the player still works after a rejected path");
}

#[test]
fn open_empty_path_reports_invalid_path() {
    let _serial = serial();
    let player = player();

    let error = player.open("").expect_err("an empty path must not open");
    assert_eq!(error.code, VideoErrorCode::InvalidPath);
}

#[test]
fn open_directory_reports_invalid_path() {
    let _serial = serial();
    let player = player();
    let dir = fixture()
        .parent()
        .expect("the fixture has a parent directory")
        .to_string_lossy()
        .into_owned();

    let error = player.open(&dir).expect_err("a directory must not open");
    assert_eq!(error.code, VideoErrorCode::InvalidPath);
}

#[test]
fn open_non_media_file_reports_open_failed() {
    let _serial = serial();
    let path = std::env::temp_dir().join(format!("sublore-not-a-video-{}.txt", std::process::id()));
    fs::write(&path, b"this is not a video file").expect("temp fixture is writable");

    let player = player();
    let error = player
        .open(&path.to_string_lossy())
        .expect_err("mpv must refuse a file it cannot demux");
    assert_eq!(error.code, VideoErrorCode::OpenFailed);

    player
        .open(&fixture_path())
        .expect("the player still works after a refused file");

    let _ = fs::remove_file(&path);
}

#[test]
fn shutdown_is_idempotent_and_releases_the_player() {
    let _serial = serial();
    let player = player();
    player.open(&fixture_path()).expect("fixture should open");

    let started = Instant::now();
    assert!(player.shutdown(), "shutdown must destroy the mpv core");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "shutdown took {:?}",
        started.elapsed()
    );

    assert!(
        player.shutdown(),
        "a second shutdown must report the same verdict"
    );

    let error = player
        .play()
        .expect_err("commands fail once the player is gone");
    assert_eq!(error.code, VideoErrorCode::PlayerUnavailable);
}

#[test]
fn player_drops_without_hanging() {
    let _serial = serial();
    let player = player();
    player.open(&fixture_path()).expect("fixture should open");
    player.play().expect("play should be accepted");
    sleep(Duration::from_millis(300));

    let started = Instant::now();
    {
        let owned = player;
        assert!(owned.shutdown(), "shutdown must destroy the mpv core");
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "shutdown plus drop took {:?}",
        started.elapsed()
    );
}

/// Regression for the M0.2 open/shutdown race: `open` used to clone the mpv handle and arm its
/// response channel under two separate locks, so a shutdown landing between the two left the call
/// waiting on an event thread that was already stopped. Both calls must stay bounded.
#[test]
fn shutdown_during_open_never_strands_either_call() {
    let _serial = serial();
    let path = fixture_path();

    for step in 0..12 {
        let player = Arc::new(player());
        let opener = {
            let player = Arc::clone(&player);
            let path = path.clone();
            thread::spawn(move || {
                let started = Instant::now();
                let outcome = player.open(&path);
                (outcome, started.elapsed())
            })
        };

        // Sweep the window between the two locks `open` used to take separately.
        sleep(Duration::from_micros(step * 150));
        let closed = Instant::now();
        let destroyed = player.shutdown();
        let close_took = closed.elapsed();

        let (outcome, open_took) = opener.join().expect("the opening thread must not panic");
        // Below the 5 s drain and the 10 s open timeout, so either regression is a failure here.
        assert!(
            close_took < Duration::from_secs(3),
            "step {step}: shutdown blocked for {close_took:?}"
        );
        assert!(
            open_took < Duration::from_secs(3),
            "step {step}: open blocked for {open_took:?}"
        );
        assert!(
            destroyed,
            "step {step}: shutdown must destroy the core, not give up on the drain"
        );
        match outcome {
            Ok(_) => {}
            Err(error) => assert_ne!(
                error.code,
                VideoErrorCode::OpenTimeout,
                "step {step}: open waited out the full timeout on a stopped event thread"
            ),
        }
    }
}

/// Regression for the M0.2 launch failure: GTK calls setlocale(LC_ALL, "") during init, after which
/// mpv_create returns null because LC_NUMERIC is no longer "C".
#[test]
fn player_forces_the_c_numeric_locale() {
    let _serial = serial();
    // SAFETY: adopt the environment's numeric locale, exactly as GTK's init does.
    unsafe { libc::setlocale(libc::LC_NUMERIC, c"".as_ptr()) };

    let player = Player::new(PlayerConfig::headless(), None)
        .expect("the player must start whatever the process locale is");
    assert_eq!(current_numeric_locale(), "C");

    player.open(&fixture_path()).expect("fixture should open");
}
