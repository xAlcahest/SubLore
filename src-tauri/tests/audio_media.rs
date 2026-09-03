//! The half of W4 that needs real media: what mpv says the audio tracks are, and what a job
//! produces from a file whose audio is known. See BACKLOG.md M2.4, W4.
//!
//! Behind the `real-media` feature, mirroring `crates/sublore-audio/tests/fixtures.rs`, because it
//! needs a real ffmpeg and generated fixtures. Off by default, so on a machine without them there
//! is no test to skip: there is no test at all.
//!
//! ```text
//! sh fixtures/video/make-waveform-fixtures.sh
//! cargo test -p sublore --features real-media -- --nocapture
//! ```
#![cfg(feature = "real-media")]

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use sublore_audio::{Cancel, PeakRequest};
use sublore_lib::audio::{run_job, AudioEvent};
use sublore_lib::video::error::VideoErrorCode;
use sublore_lib::video::player::{AudioTrack, Player, PlayerConfig};

/// The smallest peak a full-scale 440 Hz tone leaves in a millisecond bucket, measured over
/// `waveform-60s.mkv` and stated in the script that writes it: a bucket is shorter than one cycle
/// of the tone, so 98.2% of full scale is as loud as the quietest tone bucket gets.
const TONE_FLOOR: u32 = 32_188;
/// The fixture's blocks: ten seconds of tone, ten of silence, tone first, six of them.
const BLOCK_MS: u32 = 10_000;
const BLOCKS: u32 = 6;

/// One mpv core at a time in this binary, the reason `video_playback.rs` serialises its own:
/// building a core moves a process-global locale setting.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/video")
        .join(name);
    assert!(
        path.is_file(),
        "{} is missing: run sh fixtures/video/make-waveform-fixtures.sh",
        path.display()
    );
    path
}

fn ffmpeg() -> PathBuf {
    // The same override the app reads, so one setting points both at the same binary.
    match std::env::var_os("SUBLORE_FFMPEG_BIN") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from("ffmpeg"),
    }
}

fn player() -> Player {
    Player::new(PlayerConfig::headless(), None)
        .expect("headless player should start; is libmpv installed?")
}

/// The tracks of one fixture, after opening it.
fn tracks_of(name: &str) -> Vec<AudioTrack> {
    let player = player();
    player
        .open(&fixture(name).to_string_lossy())
        .unwrap_or_else(|error| panic!("{name} should open: {error}"));
    player
        .audio_tracks()
        .unwrap_or_else(|error| panic!("{name} should list its tracks: {error}"))
}

#[test]
fn a_file_with_two_audio_tracks_lists_both_with_their_languages_and_marks_the_one_playing() {
    let _serial = serial();
    let tracks = tracks_of("waveform-tracks.mkv");

    assert_eq!(
        tracks.len(),
        2,
        "the fixture carries two audio streams: {tracks:?}"
    );
    assert_eq!(
        tracks
            .iter()
            .map(|track| track.ff_index)
            .collect::<Vec<_>>(),
        vec![1, 2],
        "the ff-index is ffmpeg's own stream index, which is what an extraction maps"
    );
    assert_eq!(
        tracks
            .iter()
            .map(|track| track.lang.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("jpn"), Some("eng")],
        "the language tags the fixture was written with"
    );
    assert_eq!(
        tracks
            .iter()
            .map(|track| track.title.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Japanese original"), Some("English dub")]
    );
    assert_eq!(
        tracks.iter().filter(|track| track.playing).count(),
        1,
        "exactly one track is playing, and mpv is what says which: {tracks:?}"
    );
    assert!(
        tracks[0].playing,
        "mpv plays the first audio track of this file, so that is the one the waveform draws"
    );
    // The two ids are what a switch sets, so they have to be distinct and they are not the index.
    assert_ne!(tracks[0].id, tracks[1].id);
}

#[test]
fn a_file_with_no_audio_stream_lists_no_tracks_rather_than_failing() {
    let _serial = serial();
    let tracks = tracks_of("waveform-silent.mkv");
    assert!(
        tracks.is_empty(),
        "the fixture has no audio stream at all, so there is nothing to list: {tracks:?}"
    );
}

#[test]
fn a_file_with_one_audio_track_lists_that_one_and_plays_it() {
    let _serial = serial();
    let tracks = tracks_of("waveform-60s.mkv");
    assert_eq!(tracks.len(), 1, "{tracks:?}");
    assert_eq!(tracks[0].ff_index, 1);
    assert!(tracks[0].playing);
}

/// An empty track list and no media are two different facts, and a waveform panel that cannot tell
/// them apart draws E3's sentence over a file that is simply not open yet.
#[test]
fn no_media_open_is_not_an_empty_track_list() {
    let _serial = serial();
    let player = player();

    assert_eq!(player.loaded_path(), None, "nothing is open yet");
    let error = player
        .audio_tracks()
        .expect_err("there is no media to list the tracks of");
    assert_eq!(error.code, VideoErrorCode::NotLoaded);

    let path = fixture("waveform-60s.mkv").to_string_lossy().into_owned();
    player.open(&path).expect("the fixture should open");
    assert!(
        player
            .loaded_path()
            .is_some_and(|open| open.ends_with("waveform-60s.mkv")),
        "the open file is what a job peaks: {:?}",
        player.loaded_path()
    );
}

#[test]
fn peaking_the_sixty_second_fixture_covers_it_end_to_end_and_ends_in_one_done() {
    let media = fixture("waveform-60s.mkv");
    let recorder = Mutex::new(Vec::new());
    run_job(
        &ffmpeg(),
        5,
        &PeakRequest::new(media, 1),
        &Cancel::new(),
        &|event| {
            recorder
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(event);
        },
        None,
    );
    let events = recorder
        .into_inner()
        .unwrap_or_else(|error| error.into_inner());

    // Every chunk starts where the last one ended, so the run tiles the timeline: no gap, no
    // overlap, and the total below is therefore the media's own length.
    let mut buckets: Vec<(i16, i16)> = Vec::new();
    let mut terminal = None;
    for event in events {
        match event {
            AudioEvent::Peaks(peaks) => {
                assert_eq!(peaks.job_id, 5);
                assert_eq!(
                    peaks.first_ms as usize,
                    buckets.len(),
                    "a chunk started at the wrong millisecond: gap or overlap"
                );
                assert_eq!(peaks.min.len(), peaks.max.len());
                buckets.extend(peaks.min.into_iter().zip(peaks.max));
            }
            other => {
                assert!(terminal.is_none(), "a job ends once, not twice");
                terminal = Some(other);
            }
        }
    }

    let done = match terminal.expect("a job always ends") {
        AudioEvent::Done(done) => done,
        other => panic!("the fixture should peak, got {other:?}"),
    };
    assert_eq!(done.job_id, 5);
    assert_eq!(done.buckets as usize, buckets.len());
    // 60 s of media, one bucket per millisecond, and the last one may be short.
    assert!(
        (60_000..=60_001).contains(&done.buckets),
        "the whole duration should be covered, got {} buckets",
        done.buckets
    );

    // The two lanes are the smallest and the largest sample of the millisecond, in that order. A
    // tone bucket's are far apart, so a payload that swapped them reads min above max here.
    if let Some((at, (min, max))) = buckets
        .iter()
        .copied()
        .enumerate()
        .find(|(_, (min, max))| min > max)
    {
        panic!("the bucket at {at} ms has min {min} above max {max}");
    }

    // The milestone's own sentence, read out of the payload the UI receives rather than out of the
    // crate: silence reads flat, the 440 Hz tone reads full.
    for block in 0..BLOCKS {
        let centre = (block * BLOCK_MS + BLOCK_MS / 2) as usize;
        let (min, max) = buckets[centre];
        let magnitude = i32::from(min)
            .unsigned_abs()
            .max(i32::from(max).unsigned_abs());
        if block % 2 == 0 {
            assert!(
                magnitude >= TONE_FLOOR,
                "block {block} is a tone; the bucket at {centre} ms read {magnitude}"
            );
        } else {
            assert_eq!(
                (min, max),
                (0, 0),
                "block {block} is digital silence; the bucket at {centre} ms was not flat"
            );
        }
    }
}
