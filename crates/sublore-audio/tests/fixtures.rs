//! Peaks read out of media whose audio is known, against the numbers
//! `fixtures/video/make-waveform-fixtures.sh` states. See BACKLOG.md M2.4.
//!
//! Behind the `real-media` feature because it needs a real ffmpeg and generated fixtures:
//!
//! ```text
//! sh fixtures/video/make-waveform-fixtures.sh
//! cargo test -p sublore-audio --features real-media -- --nocapture
//! ```
#![cfg(feature = "real-media")]

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sublore_audio::{extract_peaks, AudioErrorKind, Bucket, Cancel, PeakRequest};

/// The smallest peak a full-scale 440 Hz tone leaves in a millisecond bucket, measured over
/// `waveform-60s.mkv` and stated in the script that writes it: a bucket is shorter than one cycle
/// of the tone, so 98.2% of full scale is as loud as the quietest tone bucket gets.
const TONE_FLOOR: u32 = 32_188;
/// The largest peak a 16-bit sample can leave: -32768 read as a magnitude.
const FULL_SCALE: u32 = 32_768;
/// How far from a block boundary an assertion stands off. The fixture's blocks start on exact
/// sample boundaries; this is the milestone's own tolerance for where a transition may land.
const GUARD_MS: usize = 50;

fn fixture(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/video")
        .join(name);
    assert!(
        path.is_file(),
        "{} is missing: run sh fixtures/video/make-waveform-fixtures.sh",
        path.display()
    );
    path
}

fn ffmpeg() -> PathBuf {
    // The same override the ASR path reads, so one setting points both at the same binary.
    match std::env::var_os("SUBLORE_FFMPEG_BIN") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from("ffmpeg"),
    }
}

/// The loudest sample in the bucket, either way from zero.
fn magnitude(bucket: Bucket) -> u32 {
    i32::from(bucket.min)
        .unsigned_abs()
        .max(i32::from(bucket.max).unsigned_abs())
}

/// Every bucket of one track, with the chunks checked for gaps and overlaps as they arrive.
fn peaks_of(name: &str, ff_index: u32) -> (Vec<Bucket>, Duration) {
    let media = fixture(name);
    let collected: Mutex<Vec<Bucket>> = Mutex::new(Vec::new());
    let started = Instant::now();
    let total = extract_peaks(
        &ffmpeg(),
        &PeakRequest::new(media, ff_index),
        &Cancel::new(),
        &|first, buckets| {
            let mut collected = collected.lock().expect("the test sink is never poisoned");
            assert_eq!(
                first as usize,
                collected.len(),
                "a chunk started at the wrong millisecond: gap or overlap"
            );
            collected.extend_from_slice(buckets);
        },
    )
    .unwrap_or_else(|error| panic!("{name} track {ff_index} should peak: {error}"));
    let elapsed = started.elapsed();
    let collected = collected
        .into_inner()
        .expect("the test sink is never poisoned");
    assert_eq!(total as usize, collected.len());
    (collected, elapsed)
}

#[test]
fn the_alternating_fixture_reads_full_where_it_is_a_tone_and_flat_where_it_is_silent() {
    let media = fixture("waveform-60s.mkv");
    let before = std::fs::metadata(&media).expect("the fixture should be readable");
    let (buckets, elapsed) = peaks_of("waveform-60s.mkv", 1);
    println!("waveform-60s.mkv: {} buckets in {elapsed:?}", buckets.len());

    // 60 seconds of media, one bucket per millisecond, and the last one may be short.
    assert!(
        (59_999..=60_001).contains(&buckets.len()),
        "{} buckets for a 60 s file",
        buckets.len()
    );

    // Tone [0,10) [20,30) [40,50), silence [10,20) [30,40) [50,60), with the transitions given
    // the milestone's 50 ms of room either side of each boundary.
    for block in 0..6 {
        let start = block * 10_000 + GUARD_MS;
        let end = (block + 1) * 10_000 - GUARD_MS;
        let tone = block % 2 == 0;
        for (offset, bucket) in buckets[start..end].iter().enumerate() {
            let at = start + offset;
            if tone {
                let peak = magnitude(*bucket);
                assert!(
                    (TONE_FLOOR..=FULL_SCALE).contains(&peak),
                    "millisecond {at} of a tone block peaked at {peak}"
                );
            } else {
                assert_eq!(
                    *bucket,
                    Bucket { min: 0, max: 0 },
                    "millisecond {at} of a silence block is not silent"
                );
            }
        }
    }

    // The budget the crate is measured against here is loose on purpose: a CI runner is not a
    // laptop, and the number that counts is the one W10 measures on the owner's machine.
    assert!(
        elapsed < Duration::from_secs(5),
        "peaking 60 s of media took {elapsed:?}"
    );

    // Source media is read-only (CONTRIBUTING.md §3.1).
    let after = std::fs::metadata(&media).expect("the fixture should still be readable");
    assert_eq!(before.len(), after.len(), "the fixture's size changed");
    assert_eq!(
        before.modified().ok(),
        after.modified().ok(),
        "the fixture was written to"
    );
}

#[test]
fn the_track_that_is_named_is_the_track_that_is_read() {
    let (first, _) = peaks_of("waveform-tracks.mkv", 1);
    let (second, _) = peaks_of("waveform-tracks.mkv", 2);
    assert!(
        (29_999..=30_001).contains(&first.len()),
        "{} buckets for a 30 s file",
        first.len()
    );
    assert_eq!(first.len(), second.len());

    // Track 1 is the full-scale 440 Hz tone; track 2 is 880 Hz at a quarter of it, which fits
    // inside a bucket and so peaks between 8181 and 8192. The guard keeps the first and last
    // milliseconds of the file out of it.
    for at in GUARD_MS..first.len() - GUARD_MS {
        let loud = magnitude(first[at]);
        let quiet = magnitude(second[at]);
        assert!(
            (TONE_FLOOR..=FULL_SCALE).contains(&loud),
            "millisecond {at} of track 1 peaked at {loud}"
        );
        assert!(
            (8_181..=8_192).contains(&quiet),
            "millisecond {at} of track 2 peaked at {quiet}"
        );
    }
}

#[test]
fn a_stream_that_holds_no_audio_is_a_sentence_rather_than_an_empty_waveform() {
    // waveform-silent.mkv has one stream and it is video: index 0 cannot be decoded as audio and
    // index 1 does not exist. Both are the same answer for the user (decision 24 E3).
    for ff_index in [0, 1] {
        let outcome = extract_peaks(
            &ffmpeg(),
            &PeakRequest::new(fixture("waveform-silent.mkv"), ff_index),
            &Cancel::new(),
            &|_, _| {},
        );
        let error = match outcome {
            Ok(buckets) => panic!("stream {ff_index} produced {buckets} buckets of nothing"),
            Err(error) => error,
        };
        assert_eq!(error.kind, AudioErrorKind::MediaUnreadable, "{error}");
    }
}

#[test]
fn a_file_that_is_not_media_is_refused_with_ffmpegs_own_words() {
    let not_media = std::env::temp_dir().join(format!(
        "sublore-audio-not-media-{}.mkv",
        std::process::id()
    ));
    std::fs::write(
        &not_media,
        b"Sublore keeps your terminology, not your video.",
    )
    .expect("the temp file should be writable");
    let error = extract_peaks(
        &ffmpeg(),
        &PeakRequest::new(not_media.clone(), 1),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("a text file is not media");
    assert_eq!(error.kind, AudioErrorKind::MediaUnreadable);
    assert!(
        !error.detail.is_empty(),
        "the detail should carry what ffmpeg said"
    );
    std::fs::remove_file(not_media).ok();
}

#[test]
fn a_cancel_from_the_first_chunk_ends_a_real_run() {
    let cancel = Cancel::new();
    let started = Instant::now();
    let error = extract_peaks(
        &ffmpeg(),
        &PeakRequest::new(fixture("waveform-60s.mkv"), 1),
        &cancel,
        &|_, _| cancel.cancel(),
    )
    .expect_err("cancelled on its first chunk");
    assert_eq!(error.kind, AudioErrorKind::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a cancelled run took {:?}",
        started.elapsed()
    );
}
