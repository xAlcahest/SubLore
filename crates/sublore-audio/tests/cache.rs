//! The peaks cache on a real disk: what keys an entry, what is refused, what eviction drops, and
//! what a second open costs. See BACKLOG.md M2.4 and decision 20.
//!
//! Nothing here needs ffmpeg or a fixture except the two tests that prove the cache stops a child
//! from being spawned at all. Those drive a stand-in program the way `child.rs` does, and are
//! Linux only for the same reason it is: Linux is where this project's behaviour is proved
//! (CONTRIBUTING.md §5.5).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use sublore_audio::{
    peaks_cached, AudioErrorKind, Bucket, CacheErrorKind, CacheKey, Cancel, PeakRequest,
    PeaksCache, CHUNK_BUCKETS, PEAK_FORMAT_VERSION,
};

/// A directory of this test file's own, cleaned between runs.
fn workspace(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("sublore-audio-cache-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("the test directory should be creatable");
    root
}

/// A file whose bytes are known and whose length is a parameter. Deterministic, so a key computed
/// over it twice is the same key.
fn source(path: &Path, length: usize, seed: u8) -> PathBuf {
    let bytes: Vec<u8> = (0..length)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(seed))
        .collect();
    fs::write(path, &bytes).expect("the source should be writable");
    path.to_path_buf()
}

fn buckets(count: usize) -> Vec<Bucket> {
    (0..count)
        .map(|index| {
            let step = (index % 20_000) as i16;
            Bucket {
                min: step - 20_000,
                max: step,
            }
        })
        .collect()
}

/// A moment `hours` before the test started. Fixed offsets from one instant, so the ordering the
/// eviction sees is decided here and not by how long the test took.
fn hours_ago(hours: u64) -> SystemTime {
    SystemTime::now() - Duration::from_secs(hours * 3_600)
}

/// Every file under `root`, relative and sorted, with its length.
fn listing(root: &Path) -> Vec<(String, u64)> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let length = entry.metadata().map(|data| data.len()).unwrap_or_default();
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            found.push((name, length));
        }
    }
    found.sort();
    found
}

fn total_bytes(dir: &Path) -> u64 {
    listing(dir).iter().map(|(_, length)| length).sum()
}

/// What one entry of `buckets` takes on disk, measured in a cache of its own rather than worked
/// out from a header length this file would then have to be told about.
fn entry_size(root: &Path, key: &CacheKey, buckets: &[Bucket]) -> u64 {
    let scratch = PeaksCache::new(root.join("measure"));
    scratch
        .store(key, buckets, SystemTime::now())
        .expect("a cache at the default cap should take an entry");
    let length = fs::metadata(scratch.path_of(key))
        .expect("the entry should be where the cache put it")
        .len();
    fs::remove_dir_all(&scratch.dir).expect("the scratch cache should be removable");
    length
}

#[test]
fn the_key_follows_the_bytes_and_not_the_name() {
    let root = workspace("key-moves");
    let here = source(&root.join("episode-01.mkv"), 4096, 7);
    let key = CacheKey::of(&here, 1).expect("a readable file has a key");

    // A rename and a move are the cases that actually happen, and they keep the entry.
    let moved = root.join("season 1/ep01 [1080p].mkv");
    fs::create_dir_all(root.join("season 1")).expect("the subdirectory should be creatable");
    fs::rename(&here, &moved).expect("the source should be movable");
    let moved_key = CacheKey::of(&moved, 1).expect("the moved file has a key");
    assert_eq!(
        moved_key, key,
        "the same bytes under another name key the same entry"
    );
    assert_eq!(
        moved_key.file_name(),
        key.file_name(),
        "and land on the same file"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn the_key_changes_with_the_track_the_length_and_either_end_of_the_file() {
    let root = workspace("key-changes");
    // Past two megabytes the head and the tail are different spans, which is the case the key was
    // designed for: everything between them is never read.
    let length = 3 * 1024 * 1024;
    let media = source(&root.join("a.mkv"), length, 7);
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");

    // The file name and not the key: it is the name that decides what is read back, and it is cut
    // from the digest alone.
    let name_of = |media: &Path, ff_index: u32| {
        CacheKey::of(media, ff_index)
            .expect("a readable file has a key")
            .file_name()
    };

    assert_ne!(
        name_of(&media, 2),
        key.file_name(),
        "two tracks of one file are two entries"
    );

    let mut bytes = fs::read(&media).expect("the source should be readable");
    bytes[10] ^= 0xff;
    fs::write(&media, &bytes).expect("the source should be writable");
    assert_ne!(
        name_of(&media, 1),
        key.file_name(),
        "a byte changed in the first megabyte is another file"
    );

    let mut bytes = source_bytes(length, 7);
    bytes[length - 10] ^= 0xff;
    fs::write(&media, &bytes).expect("the source should be writable");
    assert_ne!(
        name_of(&media, 1),
        key.file_name(),
        "a byte changed in the last megabyte is another file"
    );

    let shorter = source(&root.join("b.mkv"), length - 1, 7);
    assert_ne!(
        name_of(&shorter, 1),
        key.file_name(),
        "a different length is another file"
    );

    fs::remove_dir_all(&root).ok();
}

/// The bytes [`source`] writes, without writing them.
fn source_bytes(length: usize, seed: u8) -> Vec<u8> {
    (0..length)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

#[test]
fn a_source_that_cannot_be_read_has_no_key_and_no_panic() {
    let root = workspace("no-source");
    let error = CacheKey::of(&root.join("there-is-no-file-here.mkv"), 1)
        .expect_err("a file that is not there cannot be hashed");
    assert_eq!(error.kind, CacheErrorKind::SourceUnreadable);
    assert!(
        error.path.ends_with("there-is-no-file-here.mkv"),
        "the failure names the file: {}",
        error.path.display()
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn what_was_stored_is_what_comes_back() {
    let root = workspace("round-trip");
    let media = source(&root.join("a.mkv"), 4096, 7);
    let cache = PeaksCache::new(root.join("cache"));
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");

    assert!(
        cache.load(&key, SystemTime::now()).peaks.is_none(),
        "an empty cache holds nothing"
    );
    let written = buckets(2_500);
    cache
        .store(&key, &written, SystemTime::now())
        .expect("an empty directory should take an entry");

    let found = cache.load(&key, SystemTime::now());
    assert!(
        found.warning.is_none(),
        "a whole entry is no warning: {:?}",
        found.warning
    );
    assert_eq!(
        found.peaks.expect("the entry was just written"),
        written,
        "every bucket comes back as it went in"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_entry_from_another_format_version_is_a_miss_and_a_warning() {
    let root = workspace("old-format");
    let media = source(&root.join("a.mkv"), 4096, 7);
    let cache = PeaksCache::new(root.join("cache"));
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");
    cache
        .store(&key, &buckets(50), SystemTime::now())
        .expect("an empty directory should take an entry");

    // The version sits at byte 8, right after the magic.
    let path = cache.path_of(&key);
    let mut bytes = fs::read(&path).expect("the entry should be readable");
    bytes[8..12].copy_from_slice(&(PEAK_FORMAT_VERSION + 1).to_le_bytes());
    fs::write(&path, &bytes).expect("the entry should be writable");

    let found = cache.load(&key, SystemTime::now());
    assert!(
        found.peaks.is_none(),
        "an entry from another format is never handed back"
    );
    let warning = found.warning.expect("and it is a sentence for the log");
    assert_eq!(warning.kind, CacheErrorKind::EntryUnusable);
    assert!(
        warning
            .detail
            .contains(&format!("format {}", PEAK_FORMAT_VERSION + 1)),
        "the sentence names the format that was found: {}",
        warning.detail
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_broken_entry_is_a_recompute_and_a_warning_rather_than_a_panic() {
    let root = workspace("broken");
    let media = source(&root.join("a.mkv"), 4096, 7);
    let cache = PeaksCache::new(root.join("cache"));
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");
    cache
        .store(&key, &buckets(300), SystemTime::now())
        .expect("an empty directory should take an entry");
    let path = cache.path_of(&key);
    let whole = fs::read(&path).expect("the entry should be readable");

    // Whatever the entry holds that is not a bucket is the header, so nothing here has to be told
    // how long that is.
    let header = whole.len() - 300 * 4;
    let mut damaged: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("half a header", whole[..header / 2].to_vec()),
        ("a header and no buckets", whole[..header].to_vec()),
        ("cut mid-bucket", whole[..whole.len() - 3].to_vec()),
        (
            "not a peaks file at all",
            b"<html>not yours</html>".to_vec(),
        ),
    ];
    // One bit turned in the bucket count, which is the header's last field: the length no longer
    // backs it.
    let mut miscounted = whole.clone();
    miscounted[header - 4] ^= 0x40;
    damaged.push(("a count nothing backs", miscounted));

    for (what, bytes) in damaged {
        fs::write(&path, &bytes).expect("the entry should be writable");
        let found = cache.load(&key, SystemTime::now());
        assert!(
            found.peaks.is_none(),
            "{what} was read as peaks instead of refused"
        );
        let warning = found
            .warning
            .unwrap_or_else(|| panic!("{what} should leave a sentence in the log"));
        assert_eq!(warning.kind, CacheErrorKind::EntryUnusable, "{what}");
        assert!(!warning.detail.is_empty(), "{what} says nothing");
    }
    fs::remove_dir_all(&root).ok();
}

#[test]
fn writing_past_the_cap_drops_the_least_recently_read_and_keeps_the_rest() {
    let root = workspace("evict");
    let dir = root.join("cache");
    let each = buckets(1_000);
    let keys: Vec<CacheKey> = (0..4)
        .map(|index| {
            let media = source(&root.join(format!("{index}.mkv")), 4096, index as u8);
            CacheKey::of(&media, 1).expect("a readable file has a key")
        })
        .collect();
    // Three entries fit and a fourth does not.
    let cache = PeaksCache {
        dir: dir.clone(),
        cap: entry_size(&root, &keys[0], &each) * 3 + 1,
    };

    for key in &keys[..3] {
        cache
            .store(key, &each, hours_ago(9))
            .expect("three entries fit under the cap");
    }
    assert_eq!(listing(&dir).len(), 3, "three entries are under the cap");

    // Read them in an order that decides which is the oldest: 2, then 0, then 1.
    cache.load(&keys[2], hours_ago(8));
    cache.load(&keys[0], hours_ago(7));
    cache.load(&keys[1], hours_ago(6));

    cache
        .store(&keys[3], &each, hours_ago(5))
        .expect("a fourth entry is written and the oldest makes room for it");

    let left: Vec<String> = listing(&dir).into_iter().map(|(name, _)| name).collect();
    assert!(
        !left.contains(&keys[2].file_name()),
        "the least recently read entry is the one that went: {left:?}"
    );
    for kept in [&keys[0], &keys[1], &keys[3]] {
        assert!(
            left.contains(&kept.file_name()),
            "{} should still be there: {left:?}",
            kept.file_name()
        );
    }
    assert!(
        total_bytes(&dir) <= cache.cap,
        "{} bytes are over the {} byte cap after a write returned",
        total_bytes(&dir),
        cache.cap
    );

    // And what is left is still readable: eviction deletes files, it does not damage them.
    assert_eq!(
        cache
            .load(&keys[3], SystemTime::now())
            .peaks
            .expect("the newest entry survived"),
        each
    );
    fs::remove_dir_all(&root).ok();
}

/// The atomic write writes *through* a symlink, which is right for a user's subtitle and wrong
/// for a cache entry: it would put derived data somewhere the user never chose.
#[cfg(unix)]
#[test]
fn a_symlink_at_an_entrys_name_is_never_written_through_or_read_through() {
    let root = workspace("symlink");
    let media = source(&root.join("a.mkv"), 4096, 7);
    let dir = root.join("cache");
    fs::create_dir_all(&dir).expect("the cache directory should be creatable");
    let outside = root.join("outside.txt");
    fs::write(&outside, b"not the cache's to touch").expect("the target should be writable");

    let cache = PeaksCache::new(dir);
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");
    std::os::unix::fs::symlink(&outside, cache.path_of(&key)).expect("the link should be makeable");

    let error = cache
        .store(&key, &buckets(100), SystemTime::now())
        .expect_err("an entry is a regular file or it is nothing");
    assert_eq!(error.kind, CacheErrorKind::CacheUnwritable);
    assert_eq!(
        fs::read(&outside).expect("the target should still be readable"),
        b"not the cache's to touch",
        "the write went through the link and landed outside the cache"
    );

    let found = cache.load(&key, SystemTime::now());
    assert!(found.peaks.is_none(), "a link is not an entry");
    assert!(
        found.warning.is_some(),
        "and it is a sentence for the log rather than a silent miss"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_entry_larger_than_the_cap_is_refused_rather_than_emptying_the_directory() {
    let root = workspace("too-big");
    let dir = root.join("cache");
    let small = buckets(100);
    let kept_media = source(&root.join("kept.mkv"), 4096, 1);
    let kept = CacheKey::of(&kept_media, 1).expect("a readable file has a key");
    let cache = PeaksCache {
        dir: dir.clone(),
        cap: entry_size(&root, &kept, &small),
    };
    cache
        .store(&kept, &small, hours_ago(2))
        .expect("one entry is exactly the cap");

    let huge_media = source(&root.join("huge.mkv"), 4096, 2);
    let huge = CacheKey::of(&huge_media, 1).expect("a readable file has a key");
    let error = cache
        .store(&huge, &buckets(10_000), SystemTime::now())
        .expect_err("an entry past the cap can never be trimmed back to it");
    assert_eq!(error.kind, CacheErrorKind::CacheUnwritable);
    assert!(
        error.detail.contains("limit"),
        "the sentence says what it is past: {}",
        error.detail
    );

    assert!(!cache.path_of(&huge).exists(), "the refusal wrote nothing");
    assert_eq!(
        cache
            .load(&kept, SystemTime::now())
            .peaks
            .expect("the entry that fits is untouched"),
        small,
        "a refused write must not cost the entries that were already there"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn nothing_the_cache_did_not_write_is_ever_counted_or_deleted() {
    let root = workspace("neighbours");
    let dir = root.join("cache");
    fs::create_dir_all(&dir).expect("the cache directory should be creatable");
    // A neighbour of every shape the directory could hold, each far larger than the cap.
    let neighbours = [
        "chooser-folders.json",
        ".sublore-tmp-1234-0",
        "0123456789abcdef0123456789abcdef.peaks.bak",
        "not-hex-at-all.peaks",
    ];
    for name in neighbours {
        fs::write(dir.join(name), vec![0u8; 4096]).expect("the neighbour should be writable");
    }

    let each = buckets(100);
    let sizer = source(&root.join("sizer.mkv"), 4096, 9);
    let cache = PeaksCache {
        dir: dir.clone(),
        cap: entry_size(
            &root,
            &CacheKey::of(&sizer, 1).expect("a readable file has a key"),
            &each,
        ),
    };
    for index in 0..3 {
        let media = source(&root.join(format!("{index}.mkv")), 4096, index as u8);
        let key = CacheKey::of(&media, 1).expect("a readable file has a key");
        cache
            .store(&key, &each, hours_ago(9 - index as u64))
            .expect("every write stays under the cap by dropping the entry before it");
    }

    let left: Vec<String> = listing(&dir).into_iter().map(|(name, _)| name).collect();
    for name in neighbours {
        assert!(
            left.contains(&name.to_owned()),
            "{name} was deleted by an eviction that had no business with it: {left:?}"
        );
    }
    assert_eq!(
        left.len(),
        neighbours.len() + 1,
        "one entry of ours is left, and every neighbour: {left:?}"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn losing_the_whole_directory_costs_the_next_open_nothing_but_the_work() {
    let root = workspace("deleted-dir");
    let media = source(&root.join("a.mkv"), 4096, 7);
    let dir = root.join("cache");
    let cache = PeaksCache::new(dir.clone());
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");
    let written = buckets(1_500);
    cache
        .store(&key, &written, SystemTime::now())
        .expect("an empty directory should take an entry");

    // The app is closed, and the user empties the cache directory.
    fs::remove_dir_all(&dir).expect("the cache directory should be removable");

    let found = cache.load(&key, SystemTime::now());
    assert!(found.peaks.is_none(), "there is nothing left to find");
    assert!(
        found.warning.is_none(),
        "an empty cache is a miss and not a complaint: {:?}",
        found.warning
    );
    cache
        .store(&key, &written, SystemTime::now())
        .expect("the directory is made again by the next write");
    assert_eq!(
        cache
            .load(&key, SystemTime::now())
            .peaks
            .expect("the entry is back"),
        written,
        "the peaks after the loss are the peaks before it"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn nothing_is_ever_written_beside_the_media_or_in_the_project_folder() {
    let root = workspace("read-only-media");
    let media_dir = root.join("media");
    let project = root.join("project");
    fs::create_dir_all(&media_dir).expect("the media directory should be creatable");
    fs::create_dir_all(&project).expect("the project directory should be creatable");
    fs::write(project.join("episode-01.sublore"), b"a project file")
        .expect("the project file should be writable");
    let media = source(&media_dir.join("episode-01.mkv"), 3 * 1024 * 1024, 7);

    let before_media = listing(&media_dir);
    let before_project = listing(&project);
    let modified = fs::metadata(&media)
        .expect("the media should be readable")
        .modified()
        .ok();

    let cache = PeaksCache::new(root.join("cache"));
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");
    cache
        .store(&key, &buckets(4_000), SystemTime::now())
        .expect("an empty directory should take an entry");
    cache.load(&key, SystemTime::now());

    assert_eq!(
        listing(&media_dir),
        before_media,
        "something was written beside the user's media"
    );
    assert_eq!(
        listing(&project),
        before_project,
        "something was written into the project folder"
    );
    assert_eq!(
        fs::metadata(&media)
            .expect("the media should still be readable")
            .modified()
            .ok(),
        modified,
        "the media was opened for more than reading"
    );
    assert!(
        !listing(&root.join("cache")).is_empty(),
        "and the entry did land in the cache directory"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_cached_read_of_a_twenty_four_minute_episode_is_under_a_tenth_of_a_second() {
    let root = workspace("budget");
    let media = source(&root.join("a.mkv"), 4096, 7);
    let cache = PeaksCache::new(root.join("cache"));
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");
    // 24 minutes at one bucket a millisecond. The fixture that holds this much audio is generated
    // behind a flag and is never in CI, so the entry is synthesised at the size instead.
    let written = buckets(24 * 60 * 1000);
    cache
        .store(&key, &written, SystemTime::now())
        .expect("an empty directory should take an entry");

    let started = Instant::now();
    let found = cache.load(&key, SystemTime::now());
    let elapsed = started.elapsed();
    let read = found.peaks.expect("the entry was just written");
    println!(
        "a 24-minute entry ({} buckets) read in {elapsed:?}",
        read.len()
    );

    assert_eq!(read.len(), written.len());
    assert_eq!(read[read.len() - 1], written[written.len() - 1]);
    assert!(
        elapsed < Duration::from_millis(100),
        "a cached read took {elapsed:?}, and it is on the path the first paint's two seconds pay for"
    );
    fs::remove_dir_all(&root).ok();
}

// The two below are the ones that need a child process, so they are Linux only, for the reason
// `child.rs` gives: this is where the project's behaviour is proved.
#[cfg(target_os = "linux")]
mod through_a_child {
    use super::{listing, source, workspace, Path, PathBuf};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    use sublore_audio::{
        peaks_cached, AudioErrorKind, Bucket, Cancel, PeakRequest, PeaksCache, CHUNK_BUCKETS,
    };

    /// A stand-in for ffmpeg that records every time it is run and then writes a fixed two seconds
    /// of samples. Deterministic, so two runs produce the same buckets and a difference means the
    /// second one was not a run at all.
    fn stand_in(root: &Path, log: &Path, samples: &Path) -> PathBuf {
        let path = root.join("ffmpeg-stand-in");
        fs::write(
            &path,
            format!(
                "#!/bin/sh\necho ran >> {}\nexec cat {}\n",
                log.display(),
                samples.display()
            ),
        )
        .expect("the stand-in should be writable");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("the stand-in should be executable");
        path
    }

    /// Two seconds of `s16le` at 48 kHz, varying so the buckets are not all one value.
    fn samples(path: &Path) -> PathBuf {
        let bytes: Vec<u8> = (0..48_000u32 * 2)
            .flat_map(|index| (((index * 37) % 20_001) as i16 - 10_000).to_le_bytes())
            .collect();
        fs::write(path, &bytes).expect("the samples should be writable");
        path.to_path_buf()
    }

    fn runs(log: &Path) -> usize {
        fs::read_to_string(log)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    /// Peak `media`, collecting every chunk and checking the stream for gaps as it arrives.
    fn peak(cache: &PeaksCache, ffmpeg: &Path, media: &Path) -> (Vec<Bucket>, bool) {
        let collected: Mutex<Vec<Bucket>> = Mutex::new(Vec::new());
        let run = peaks_cached(
            cache,
            ffmpeg,
            &PeakRequest::new(media.to_path_buf(), 1),
            &Cancel::new(),
            &|first, chunk| {
                let mut collected = collected.lock().expect("the test sink is never poisoned");
                assert_eq!(
                    first as usize,
                    collected.len(),
                    "a chunk started at the wrong millisecond: gap or overlap"
                );
                assert!(chunk.len() <= CHUNK_BUCKETS, "a chunk is at most a second");
                collected.extend_from_slice(chunk);
            },
        )
        .expect("the stand-in writes a whole number of milliseconds");
        let collected = collected
            .into_inner()
            .expect("the test sink is never poisoned");
        assert_eq!(run.buckets as usize, collected.len());
        assert!(
            run.warnings.is_empty(),
            "a run over a writable cache has nothing to complain about: {:?}",
            run.warnings
        );
        (collected, run.from_cache)
    }

    #[test]
    fn peaking_the_same_file_twice_runs_ffmpeg_once() {
        let root = workspace("twice");
        let log = root.join("runs");
        let ffmpeg = stand_in(&root, &log, &samples(&root.join("samples.raw")));
        let media = source(&root.join("episode-01.mkv"), 4096, 7);
        let cache = PeaksCache::new(root.join("cache"));

        let (first, from_cache) = peak(&cache, &ffmpeg, &media);
        assert_eq!(first.len(), 2_000, "two seconds of samples is 2000 buckets");
        assert!(!from_cache, "the first run has nothing to read");
        assert_eq!(runs(&log), 1, "the first run spawns ffmpeg once");

        let (second, from_cache) = peak(&cache, &ffmpeg, &media);
        assert!(from_cache, "the second run says where its peaks came from");
        assert_eq!(
            runs(&log),
            1,
            "the second run produced a child process, so nothing was cached"
        );
        assert_eq!(second, first, "and it produced the same waveform");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn deleting_the_cache_directory_costs_the_run_again_and_nothing_else() {
        let root = workspace("wiped");
        let log = root.join("runs");
        let ffmpeg = stand_in(&root, &log, &samples(&root.join("samples.raw")));
        let media = source(&root.join("episode-01.mkv"), 4096, 7);
        let dir = root.join("cache");
        let cache = PeaksCache::new(dir.clone());

        let (first, _) = peak(&cache, &ffmpeg, &media);
        assert!(!listing(&dir).is_empty(), "the first run left an entry");

        // The app is closed, and the user deletes the cache directory.
        fs::remove_dir_all(&dir).expect("the cache directory should be removable");

        let (again, from_cache) = peak(&cache, &ffmpeg, &media);
        assert!(!from_cache, "there was nothing left to read");
        assert_eq!(runs(&log), 2, "so it was computed again");
        assert_eq!(again, first, "and the peaks are the same peaks");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_run_that_can_spawn_nothing_still_answers_from_the_cache() {
        let root = workspace("no-ffmpeg-needed");
        let log = root.join("runs");
        let ffmpeg = stand_in(&root, &log, &samples(&root.join("samples.raw")));
        let media = source(&root.join("episode-01.mkv"), 4096, 7);
        let cache = PeaksCache::new(root.join("cache"));
        let (first, _) = peak(&cache, &ffmpeg, &media);

        // Nothing can be spawned from this path, so an answer can only have come from the entry.
        let (second, from_cache) = peak(&cache, &root.join("there-is-no-ffmpeg-here"), &media);
        assert!(from_cache);
        assert_eq!(second, first);

        // And with the entry gone it is the failure it should be, so the check above is not
        // passing for want of anything to spawn.
        fs::remove_dir_all(root.join("cache")).expect("the cache directory should be removable");
        let error = peaks_cached(
            &cache,
            &root.join("there-is-no-ffmpeg-here"),
            &PeakRequest::new(media.clone(), 1),
            &Cancel::new(),
            &|_, _| {},
        )
        .expect_err("nothing to run and nothing to read");
        assert_eq!(error.kind, AudioErrorKind::FfmpegMissing);
        fs::remove_dir_all(&root).ok();
    }
}

#[test]
fn a_cancel_stops_a_cached_run_partway_the_way_it_stops_a_decode() {
    let root = workspace("cancel-replay");
    let media = source(&root.join("a.mkv"), 4096, 7);
    let cache = PeaksCache::new(root.join("cache"));
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");
    cache
        .store(&key, &buckets(CHUNK_BUCKETS * 5), SystemTime::now())
        .expect("an empty directory should take an entry");

    let cancel = Cancel::new();
    let seen = AtomicUsize::new(0);
    let error = peaks_cached(
        &cache,
        Path::new("/there-is-no-ffmpeg-here"),
        &PeakRequest::new(media, 1),
        &cancel,
        &|_, _| {
            seen.fetch_add(1, Ordering::Relaxed);
            cancel.cancel();
        },
    )
    .expect_err("the caller stopped listening after the first chunk");

    assert_eq!(error.kind, AudioErrorKind::Cancelled);
    assert_eq!(
        seen.load(Ordering::Relaxed),
        1,
        "the rest of the entry was handed over after the cancel"
    );
    fs::remove_dir_all(&root).ok();
}

/// Chunks come back from the cache in the shape a live run hands them over.
#[test]
fn a_cached_run_replays_the_same_chunking_a_live_one_produces() {
    let root = workspace("chunking");
    let media = source(&root.join("a.mkv"), 4096, 7);
    let cache = PeaksCache::new(root.join("cache"));
    let key = CacheKey::of(&media, 1).expect("a readable file has a key");
    // Two whole chunks and a short one.
    let written = buckets(CHUNK_BUCKETS * 2 + 17);
    cache
        .store(&key, &written, SystemTime::now())
        .expect("an empty directory should take an entry");

    let seen: Mutex<Vec<(u32, usize)>> = Mutex::new(Vec::new());
    let run = peaks_cached(
        &cache,
        Path::new("/there-is-no-ffmpeg-here"),
        &PeakRequest::new(media, 1),
        &Cancel::new(),
        &|first, chunk| {
            seen.lock()
                .expect("the test sink is never poisoned")
                .push((first, chunk.len()));
        },
    )
    .expect("the entry answers without anything being spawned");

    assert!(run.from_cache);
    assert_eq!(run.buckets as usize, written.len());
    assert_eq!(
        seen.into_inner().expect("the test sink is never poisoned"),
        vec![
            (0, CHUNK_BUCKETS),
            (CHUNK_BUCKETS as u32, CHUNK_BUCKETS),
            ((CHUNK_BUCKETS * 2) as u32, 17)
        ],
        "chunks arrive a second at a time, in order, without a gap"
    );
    fs::remove_dir_all(&root).ok();
}
