//! Peaks kept where derived data belongs: the app cache directory, one file per key, under a cap.
//! See BACKLOG.md M2.4 and decision 20.
//!
//! The directory is handed in rather than found here. `app_cache_dir()` is Tauri's and this crate
//! has no Tauri in it, the way it has no async in it: everything below is provable with
//! `cargo test -p sublore-audio`.
//!
//! The key is a hash of the source's bytes and not of its path, so a rename or a move keeps the
//! entry. Hashing a four-gigabyte film end to end would cost seconds and the first paint is
//! allowed two, so it is the length plus the first and last megabyte: two reads, whatever the
//! file's size. The middle is never read, so the modification time is hashed in as well: a
//! download filling a preallocated file in leaves the length and both ends alone, and without the
//! clock it would be handed the half-empty waveform it was first peaked at. The format version is
//! hashed in too, so a change to the bytes below orphans every entry rather than reading an old
//! one as a new one.
//!
//! Nothing here writes anywhere but the directory it was given. The source is opened for reading
//! and nothing else (CONTRIBUTING.md §3.1), and the entry is written temp file, fsync, rename
//! through `sublore-io` (CONTRIBUTING.md §3.2), so a crash mid-write leaves no half file to be
//! read as a whole one.
//!
//! A cache is derived data, so nothing in it is ever an error the user sees: an entry that cannot
//! be read is a recompute, and an entry that cannot be written is a recompute next time. Both come
//! back as sentences for the log.

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use sublore_io::atomic::write_atomic;

use crate::error::{AudioError, AudioErrorKind};
use crate::extract::{extract_peaks, Cancel, PeakRequest};
use crate::peaks::{Bucket, CHUNK_BUCKETS, SAMPLE_RATE};

/// Bumped whenever the bytes of an entry change meaning. It is hashed into the key and written
/// into the header, so a bump invalidates every entry twice over.
pub const PEAK_FORMAT_VERSION: u32 = 1;

/// How much of the cache directory the peaks may take. A bucket is four bytes of a millisecond,
/// so this is about thirty-five hours of media: more than a season, and small beside one whisper
/// model.
pub const CACHE_CAP_BYTES: u64 = 512 * 1024 * 1024;

/// The largest entry that is ever written or read. It is also the ceiling on what a run holds in
/// memory in order to cache it, which is why it is well under the cap: about four and a half
/// hours of media.
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// What makes a file that is not one of ours a miss rather than a parse.
const MAGIC: [u8; 8] = *b"SLRPEAKS";
const DIGEST_BYTES: usize = 32;
const AT_VERSION: usize = MAGIC.len();
const AT_DIGEST: usize = AT_VERSION + 4;
const AT_FF_INDEX: usize = AT_DIGEST + DIGEST_BYTES;
const AT_SAMPLE_RATE: usize = AT_FF_INDEX + 4;
const AT_COUNT: usize = AT_SAMPLE_RATE + 4;
/// Magic, format version, source hash, track index, sample rate, bucket count.
const HEADER_BYTES: usize = AT_COUNT + 4;
/// One bucket on disk: `min` then `max`, little-endian.
const BUCKET_BYTES: u64 = 4;

/// How much of the digest names the file. The whole digest is in the header, so a name that
/// collided would be refused there rather than served as somebody else's waveform.
const NAME_BYTES: usize = 16;
const ENTRY_SUFFIX: &str = ".peaks";
/// How much of each end of the source is hashed into the key.
const KEY_SAMPLE_BYTES: u64 = 1024 * 1024;
/// How much of the source is read at a time while hashing.
const HASH_BUFFER_BYTES: usize = 64 * 1024;

/// Exhaustive on purpose, and none of it is an error the user sees: adding a variant must break
/// the log lines in the app, not slip past a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheErrorKind {
    /// The source could not be read to compute its key, so nothing can be cached for it. The
    /// extraction that follows says the same thing louder.
    SourceUnreadable,
    /// Something is at the key's name and cannot be used: another format, another file, cut
    /// short, or unreadable. Always a recompute.
    EntryUnusable,
    /// The entry could not be written, stamped as used, or trimmed back to the cap.
    CacheUnwritable,
}

/// A cache failure: a stable kind, the path, and a sentence naming what happened. Shaped like
/// `sublore_io::IoError` and [`crate::AudioError`] on purpose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheError {
    pub kind: CacheErrorKind,
    pub path: PathBuf,
    /// For the log. Never rendered as UI copy: a cache that missed is not something the user is
    /// told about.
    pub detail: String,
}

impl CacheError {
    pub fn new(kind: CacheErrorKind, path: &Path, detail: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }

    fn from_io(kind: CacheErrorKind, path: &Path, error: &io::Error) -> Self {
        Self::new(kind, path, error.to_string())
    }
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} at {}: {}",
            self.kind,
            self.path.display(),
            self.detail
        )
    }
}

impl std::error::Error for CacheError {}

/// Which peaks an entry holds: one track of one file, at one format version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheKey {
    digest: [u8; DIGEST_BYTES],
    ff_index: u32,
}

impl CacheKey {
    /// Hash the source's identity: the format version, the length, the modification time, the
    /// track, and the first and last megabyte of the file. Two reads, so a film costs what a
    /// trailer costs.
    pub fn of(media: &Path, ff_index: u32) -> Result<Self, CacheError> {
        let mut file = File::open(media).map_err(|error| unreadable_source(media, &error))?;
        let metadata = file
            .metadata()
            .map_err(|error| unreadable_source(media, &error))?;
        let length = metadata.len();
        // The middle of the file is never read, so a rewrite under a constant length would key the
        // same. A filesystem with no modification time keys on the bytes alone, as before.
        let modified = metadata
            .modified()
            .ok()
            .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_nanos());

        let mut hasher = Sha256::new();
        hasher.update(PEAK_FORMAT_VERSION.to_le_bytes());
        hasher.update(length.to_le_bytes());
        hasher.update(modified.to_le_bytes());
        hasher.update(ff_index.to_le_bytes());
        hash_span(&mut file, media, &mut hasher)?;
        // Under two megabytes the two spans would overlap, and the head has already covered the
        // whole file: the length is in the hash, so nothing is lost by stopping here.
        if length > KEY_SAMPLE_BYTES {
            file.seek(SeekFrom::Start(length - KEY_SAMPLE_BYTES))
                .map_err(|error| unreadable_source(media, &error))?;
            hash_span(&mut file, media, &mut hasher)?;
        }
        Ok(Self {
            digest: hasher.finalize().into(),
            ff_index,
        })
    }

    /// The entry's file name. Lower-case hex, so [`is_entry_name`] can tell this module's own
    /// files from everything else in the directory.
    pub fn file_name(&self) -> String {
        format!("{}{ENTRY_SUFFIX}", hex(&self.digest[..NAME_BYTES]))
    }
}

/// What a lookup found. Peaks and a warning are independent: a hit whose entry could not be
/// stamped as used is both.
#[derive(Debug)]
pub struct Lookup {
    /// The buckets, when the entry was there and whole.
    pub peaks: Option<Vec<Bucket>>,
    /// A sentence for the log. `None` with no peaks is the ordinary miss: nothing has been cached
    /// for this key yet.
    pub warning: Option<CacheError>,
}

/// The peaks directory, and how large it is allowed to grow.
#[derive(Clone, Debug)]
pub struct PeaksCache {
    /// Inside the app cache directory, chosen by the caller. This module writes nothing outside
    /// it and deletes nothing outside it.
    pub dir: PathBuf,
    /// Usually [`CACHE_CAP_BYTES`]. A field rather than a constant so the eviction can be proved
    /// without writing half a gigabyte, the way [`PeakRequest::stall`] is a field.
    pub cap: u64,
}

/// One file in the directory, as eviction sees it.
struct Stored {
    path: PathBuf,
    length: u64,
    used: SystemTime,
}

impl PeaksCache {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            cap: CACHE_CAP_BYTES,
        }
    }

    pub fn path_of(&self, key: &CacheKey) -> PathBuf {
        self.dir.join(key.file_name())
    }

    /// The largest entry this cache will write or read. An entry past the cap could never be
    /// trimmed back down to it, so it is refused rather than written and then chased.
    fn entry_limit(&self) -> u64 {
        self.cap.min(MAX_ENTRY_BYTES)
    }

    /// Read the entry for `key`, and stamp it as used at `now`.
    pub fn load(&self, key: &CacheKey, now: SystemTime) -> Lookup {
        let path = self.path_of(key);
        let peaks = match read_entry(&path, key, self.entry_limit()) {
            Ok(Some(peaks)) => peaks,
            Ok(None) => {
                return Lookup {
                    peaks: None,
                    warning: None,
                }
            }
            Err(error) => {
                return Lookup {
                    peaks: None,
                    warning: Some(error),
                }
            }
        };
        // The stamp is what eviction sorts on, so a hit that cannot be stamped is worth a line:
        // the entry will keep looking older than it is.
        Lookup {
            peaks: Some(peaks),
            warning: touch(&path, now).err(),
        }
    }

    /// Write the entry for `key`, stamp it, and trim the directory back to the cap.
    pub fn store(
        &self,
        key: &CacheKey,
        buckets: &[Bucket],
        now: SystemTime,
    ) -> Result<(), CacheError> {
        let path = self.path_of(key);
        if buckets.is_empty() {
            return Err(CacheError::new(
                CacheErrorKind::CacheUnwritable,
                &path,
                "there are no peaks to store",
            ));
        }
        let length = entry_bytes(buckets.len());
        if length > self.entry_limit() {
            return Err(CacheError::new(
                CacheErrorKind::CacheUnwritable,
                &path,
                format!(
                    "{length} bytes of peaks are past the {} byte limit for one entry",
                    self.entry_limit()
                ),
            ));
        }
        fs::create_dir_all(&self.dir).map_err(|error| {
            CacheError::from_io(CacheErrorKind::CacheUnwritable, &self.dir, &error)
        })?;
        // The atomic write follows a symlink rather than replacing it, which is right for the
        // user's own subtitle and wrong here: an entry is a regular file or it is nothing.
        regular_file_at(&path, CacheErrorKind::CacheUnwritable)?;
        write_atomic(&path, &encode(key, buckets)).map_err(|error| {
            CacheError::new(CacheErrorKind::CacheUnwritable, &path, error.to_string())
        })?;
        // Both run whatever the other does: a file that reached the disk is a file the cap counts,
        // and the first complaint is the one the caller logs.
        let stamped = touch(&path, now);
        let trimmed = self.evict(&path);
        stamped.and(trimmed)
    }

    /// Delete least-recently-used entries until the directory is inside the cap.
    ///
    /// It runs after the write and never before it: a scan that decided while the new entry did
    /// not exist yet would leave the directory over the cap the moment the rename landed. `keep`
    /// is that new entry, and it is never a candidate.
    fn evict(&self, keep: &Path) -> Result<(), CacheError> {
        let mut stored = self.stored()?;
        let mut total: u64 = stored.iter().map(|entry| entry.length).sum();
        if total <= self.cap {
            return Ok(());
        }
        // Oldest first, and by path where two entries share a stamp, so the choice is the same
        // twice running.
        stored.sort_by(|left, right| {
            left.used
                .cmp(&right.used)
                .then_with(|| left.path.cmp(&right.path))
        });
        for entry in &stored {
            if total <= self.cap {
                break;
            }
            if entry.path == keep {
                continue;
            }
            // A file that is gone no longer occupies the cap, however it went; one another program
            // holds open (Windows) still does, so it stays counted and the trim moves on.
            match fs::remove_file(&entry.path) {
                Ok(()) => total -= entry.length,
                Err(error) if error.kind() == io::ErrorKind::NotFound => total -= entry.length,
                Err(_) => {}
            }
        }
        if total > self.cap {
            return Err(CacheError::new(
                CacheErrorKind::CacheUnwritable,
                &self.dir,
                format!("{total} bytes are left and the cap is {}", self.cap),
            ));
        }
        Ok(())
    }

    /// Every file in the directory this module could have written, with its size and its stamp.
    fn stored(&self) -> Result<Vec<Stored>, CacheError> {
        let listing = match fs::read_dir(&self.dir) {
            Ok(listing) => listing,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(CacheError::from_io(
                    CacheErrorKind::CacheUnwritable,
                    &self.dir,
                    &error,
                ))
            }
        };

        let mut found = Vec::new();
        for entry in listing {
            let entry = entry.map_err(|error| {
                CacheError::from_io(CacheErrorKind::CacheUnwritable, &self.dir, &error)
            })?;
            // Regular files only, and the type is read without following: a symlink is not ours.
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let name = entry.file_name();
            if !name.to_str().is_some_and(is_entry_name) {
                continue;
            }
            // A file that vanished between the listing and the stat is one less thing to evict.
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            found.push(Stored {
                path: entry.path(),
                length: metadata.len(),
                used: metadata
                    .accessed()
                    .or_else(|_| metadata.modified())
                    .unwrap_or(UNIX_EPOCH),
            });
        }
        Ok(found)
    }
}

/// What one run of [`peaks_cached`] did.
#[derive(Debug)]
pub struct PeakRun {
    /// How many millisecond buckets the media has, whether they were computed or read.
    pub buckets: u32,
    /// True when nothing was decoded: the entry was already there.
    pub from_cache: bool,
    /// Sentences for the log, and never for the user: a cache that missed, could not be read, or
    /// could not be written is a run that took longer, not a run that failed.
    pub warnings: Vec<CacheError>,
}

/// Peaks for one track of one file, from the cache when it is there and from ffmpeg when it is
/// not, and cached afterwards either way.
///
/// `on_chunk` sees the same stream both ways: chunks of at most [`CHUNK_BUCKETS`], in order, each
/// carrying the millisecond its first bucket starts at. It is called on this thread when the
/// entry was there and on the reader thread when it was not, which is why it is `Sync`. Blocking,
/// like [`extract_peaks`], and the caller runs it on a blocking task.
pub fn peaks_cached(
    cache: &PeaksCache,
    ffmpeg: &Path,
    request: &PeakRequest,
    cancel: &Cancel,
    on_chunk: &(dyn Fn(u32, &[Bucket]) + Sync),
) -> Result<PeakRun, AudioError> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }

    let mut warnings = Vec::new();
    // A source that cannot be hashed is a run without a cache, not a run that fails: the
    // extraction below reads the same file and says what is wrong with it.
    let key = match CacheKey::of(&request.media, request.ff_index) {
        Ok(key) => Some(key),
        Err(error) => {
            warnings.push(error);
            None
        }
    };

    if let Some(key) = &key {
        let found = cache.load(key, SystemTime::now());
        warnings.extend(found.warning);
        if let Some(peaks) = found.peaks {
            return Ok(PeakRun {
                buckets: replay(&peaks, cancel, on_chunk)?,
                from_cache: true,
                warnings,
            });
        }
    }

    // Held to be written afterwards, and dropped the moment the media turns out to be longer than
    // an entry may be: the ceiling on this is the ceiling on what a run costs in memory.
    let limit = cache.entry_limit();
    let collected: Mutex<Option<Vec<Bucket>>> = Mutex::new(Some(Vec::new()));
    let total = extract_peaks(ffmpeg, request, cancel, &|first, buckets| {
        on_chunk(first, buckets);
        let mut held = collected.lock().unwrap_or_else(|error| error.into_inner());
        let Some(all) = held.as_mut() else {
            return;
        };
        if entry_bytes(all.len() + buckets.len()) > limit {
            *held = None;
        } else {
            all.extend_from_slice(buckets);
        }
    })?;

    if let Some(key) = &key {
        let collected = collected
            .into_inner()
            .unwrap_or_else(|error| error.into_inner());
        // The length is checked against the run's own count: an entry that disagrees with what
        // the caller was handed is not one to keep.
        if let Some(all) = collected.filter(|all| all.len() as u64 == u64::from(total)) {
            if let Err(error) = cache.store(key, &all, SystemTime::now()) {
                warnings.push(error);
            }
        }
    }
    Ok(PeakRun {
        buckets: total,
        from_cache: false,
        warnings,
    })
}

/// Hand cached buckets over in the shape a live run hands them over, and return how many there
/// were. Cancellable between chunks, because closing the media has to stop this the way it stops
/// a decode.
fn replay(
    peaks: &[Bucket],
    cancel: &Cancel,
    on_chunk: &(dyn Fn(u32, &[Bucket]) + Sync),
) -> Result<u32, AudioError> {
    let mut first: u32 = 0;
    for chunk in peaks.chunks(CHUNK_BUCKETS) {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        on_chunk(first, chunk);
        first = first.saturating_add(chunk.len() as u32);
    }
    Ok(first)
}

fn cancelled() -> AudioError {
    AudioError::new(AudioErrorKind::Cancelled, "the caller cancelled the run")
}

/// Hash up to [`KEY_SAMPLE_BYTES`] from where the file is now, in a fixed buffer.
fn hash_span(file: &mut File, media: &Path, hasher: &mut Sha256) -> Result<(), CacheError> {
    let mut left = KEY_SAMPLE_BYTES;
    let mut buffer = vec![0u8; HASH_BUFFER_BYTES];
    while left > 0 {
        let want = left.min(HASH_BUFFER_BYTES as u64) as usize;
        match file.read(&mut buffer[..want]) {
            Ok(0) => break,
            Ok(read) => {
                hasher.update(&buffer[..read]);
                left -= read as u64;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(unreadable_source(media, &error)),
        }
    }
    Ok(())
}

fn unreadable_source(media: &Path, error: &io::Error) -> CacheError {
    CacheError::from_io(CacheErrorKind::SourceUnreadable, media, error)
}

/// `false` when the name is free. An entry is a regular file or it is nothing: a symlink, a
/// directory or a device at the name is refused rather than read through or written through, the
/// same call eviction makes when it decides what it is allowed to delete.
fn regular_file_at(path: &Path, kind: CacheErrorKind) -> Result<bool, CacheError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(CacheError::new(
            kind,
            path,
            "the name is taken by something that is not a regular file",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(CacheError::from_io(kind, path, &error)),
    }
}

fn entry_bytes(buckets: usize) -> u64 {
    HEADER_BYTES as u64 + buckets as u64 * BUCKET_BYTES
}

/// The header and the buckets. Little-endian throughout, so a cache written on one machine is
/// readable on another of the same version.
fn encode(key: &CacheKey, buckets: &[Bucket]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_BYTES + buckets.len() * BUCKET_BYTES as usize);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&PEAK_FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&key.digest);
    out.extend_from_slice(&key.ff_index.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    // The count fits: the caller has already refused anything past MAX_ENTRY_BYTES, which is
    // sixteen million buckets.
    out.extend_from_slice(&(buckets.len() as u32).to_le_bytes());
    for bucket in buckets {
        out.extend_from_slice(&bucket.min.to_le_bytes());
        out.extend_from_slice(&bucket.max.to_le_bytes());
    }
    out
}

/// `Ok(None)` when there is no entry. `Err` when there is one and it cannot be used.
fn read_entry(path: &Path, key: &CacheKey, limit: u64) -> Result<Option<Vec<Bucket>>, CacheError> {
    if !regular_file_at(path, CacheErrorKind::EntryUnusable)? {
        return Ok(None);
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CacheError::from_io(
                CacheErrorKind::EntryUnusable,
                path,
                &error,
            ))
        }
    };
    let length = file
        .metadata()
        .map_err(|error| CacheError::from_io(CacheErrorKind::EntryUnusable, path, &error))?
        .len();
    if length > limit {
        return Err(CacheError::new(
            CacheErrorKind::EntryUnusable,
            path,
            format!("the entry is {length} bytes, past the {limit} byte limit"),
        ));
    }

    // Bounded twice: by the length just checked, and by `take`, because the file on disk is free
    // to be longer than the stat said.
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|error| CacheError::from_io(CacheErrorKind::EntryUnusable, path, &error))?;
    decode(path, key, &bytes).map(Some)
}

/// Every way an entry can fail to be this key's peaks, each with its own sentence.
fn decode(path: &Path, key: &CacheKey, bytes: &[u8]) -> Result<Vec<Bucket>, CacheError> {
    let refuse = |detail: String| CacheError::new(CacheErrorKind::EntryUnusable, path, detail);

    let Some(header) = bytes.first_chunk::<HEADER_BYTES>() else {
        return Err(refuse(format!(
            "{} bytes is less than a {HEADER_BYTES} byte header",
            bytes.len()
        )));
    };
    if header[..MAGIC.len()] != MAGIC {
        return Err(refuse("this is not a peaks file".to_owned()));
    }
    let version = u32_at(header, AT_VERSION);
    if version != PEAK_FORMAT_VERSION {
        return Err(refuse(format!(
            "the entry is format {version} and this build writes {PEAK_FORMAT_VERSION}"
        )));
    }
    if header[AT_DIGEST..AT_FF_INDEX] != key.digest {
        return Err(refuse(
            "the entry was written for another file: the names collided".to_owned(),
        ));
    }
    let ff_index = u32_at(header, AT_FF_INDEX);
    if ff_index != key.ff_index {
        return Err(refuse(format!(
            "the entry holds track {ff_index} and this is track {}",
            key.ff_index
        )));
    }
    let rate = u32_at(header, AT_SAMPLE_RATE);
    if rate != SAMPLE_RATE {
        return Err(refuse(format!(
            "the entry was folded at {rate} Hz and this build folds at {SAMPLE_RATE} Hz"
        )));
    }
    let count = u32_at(header, AT_COUNT);
    if count == 0 {
        return Err(refuse("the entry holds no buckets".to_owned()));
    }
    let expected = entry_bytes(count as usize);
    if bytes.len() as u64 != expected {
        return Err(refuse(format!(
            "{count} buckets need {expected} bytes and the entry is {}",
            bytes.len()
        )));
    }

    // A whole number of buckets, checked above, so the remainder is empty. Two bytes at a time
    // and a plain loop rather than an iterator chain: this runs a million and a half times on the
    // path the first paint has two seconds for, and an unoptimised build walks every step of it.
    let (pairs, _) = bytes[HEADER_BYTES..].as_chunks::<2>();
    let mut peaks = Vec::with_capacity(count as usize);
    let mut at = 0;
    while at + 1 < pairs.len() {
        peaks.push(Bucket {
            min: i16::from_le_bytes(pairs[at]),
            max: i16::from_le_bytes(pairs[at + 1]),
        });
        at += 2;
    }
    Ok(peaks)
}

/// A little-endian `u32` out of the header. Every call site is a constant offset inside a slice
/// whose length is already known to be a whole header.
fn u32_at(header: &[u8; HEADER_BYTES], at: usize) -> u32 {
    u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
}

/// Record the use. The operating system's own access time is not a clock this can sort on: a
/// `noatime` mount never moves it. So the entry is stamped here, and eviction reads back what
/// this wrote.
fn touch(path: &Path, now: SystemTime) -> Result<(), CacheError> {
    // Windows sets times only through a handle with write access. Nothing is written through it.
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| CacheError::from_io(CacheErrorKind::CacheUnwritable, path, &error))?;
    file.set_times(FileTimes::new().set_accessed(now).set_modified(now))
        .map_err(|error| CacheError::from_io(CacheErrorKind::CacheUnwritable, path, &error))
}

/// `{32 lower-case hex digits}.peaks` and nothing else. Anything that does not match, this module
/// did not write, so it is never counted against the cap and never deleted.
fn is_entry_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(ENTRY_SUFFIX) else {
        return false;
    };
    stem.len() == NAME_BYTES * 2
        && stem
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        decode, encode, entry_bytes, is_entry_name, CacheKey, AT_COUNT, AT_DIGEST, AT_FF_INDEX,
        AT_SAMPLE_RATE, AT_VERSION, HEADER_BYTES, PEAK_FORMAT_VERSION,
    };
    use crate::peaks::{Bucket, SAMPLE_RATE};
    use std::path::Path;

    fn key(seed: u8, ff_index: u32) -> CacheKey {
        CacheKey {
            digest: [seed; 32],
            ff_index,
        }
    }

    fn buckets() -> Vec<Bucket> {
        (0..5i16)
            .map(|index| Bucket {
                min: -1000 * (index + 1),
                max: 700 * (index + 1),
            })
            .collect()
    }

    fn at(path: &str) -> &Path {
        Path::new(path)
    }

    #[test]
    fn an_entry_reads_back_as_the_buckets_it_was_written_from() {
        let written = buckets();
        let bytes = encode(&key(0xab, 2), &written);
        assert_eq!(bytes.len() as u64, entry_bytes(written.len()));
        let read = decode(at("/cache/x.peaks"), &key(0xab, 2), &bytes)
            .expect("the entry this build wrote is the entry this build reads");
        assert_eq!(read, written);
    }

    #[test]
    fn an_entry_from_another_format_version_is_refused_by_its_header() {
        let mut bytes = encode(&key(0xab, 2), &buckets());
        bytes[AT_VERSION..AT_VERSION + 4].copy_from_slice(&(PEAK_FORMAT_VERSION + 1).to_le_bytes());
        let error = decode(at("/cache/x.peaks"), &key(0xab, 2), &bytes)
            .expect_err("another format is not this format");
        assert!(
            error
                .detail
                .contains(&(PEAK_FORMAT_VERSION + 1).to_string()),
            "the sentence names the version that was found: {}",
            error.detail
        );
    }

    #[test]
    fn an_entry_written_for_another_file_is_refused_rather_than_handed_back() {
        let bytes = encode(&key(0xab, 2), &buckets());
        let error = decode(at("/cache/x.peaks"), &key(0xcd, 2), &bytes)
            .expect_err("another file's peaks are not these peaks");
        assert!(
            error.detail.contains("another file"),
            "the sentence says whose entry it is: {}",
            error.detail
        );
        // The digest is what carries that: the header holds all thirty-two bytes of it.
        assert_eq!(&bytes[AT_DIGEST..AT_FF_INDEX], &[0xab; 32]);
    }

    #[test]
    fn an_entry_for_another_track_of_the_same_file_is_refused() {
        let bytes = encode(&key(0xab, 1), &buckets());
        let error = decode(at("/cache/x.peaks"), &key(0xab, 2), &bytes)
            .expect_err("track 1 is not track 2");
        assert!(
            error.detail.contains("track 1"),
            "the sentence names the track that was found: {}",
            error.detail
        );
    }

    #[test]
    fn an_entry_cut_short_is_refused_rather_than_read_as_a_shorter_waveform() {
        let whole = encode(&key(0xab, 2), &buckets());
        // Nothing, one byte, a header one byte short, a header with no buckets under it, and a
        // file missing its last bucket: every place a write could have been interrupted.
        for length in [0, 1, HEADER_BYTES - 1, HEADER_BYTES, whole.len() - 4] {
            let outcome = decode(at("/cache/x.peaks"), &key(0xab, 2), &whole[..length]);
            let error = match outcome {
                Ok(read) => panic!("{length} bytes were read as {} buckets", read.len()),
                Err(error) => error,
            };
            assert!(
                error.detail.contains(&length.to_string()),
                "the sentence names what was actually there: {}",
                error.detail
            );
        }
    }

    #[test]
    fn an_entry_that_claims_more_buckets_than_it_holds_is_refused() {
        let mut bytes = encode(&key(0xab, 2), &buckets());
        bytes[AT_COUNT..AT_COUNT + 4].copy_from_slice(&9_000u32.to_le_bytes());
        let error = decode(at("/cache/x.peaks"), &key(0xab, 2), &bytes)
            .expect_err("a count the bytes do not back is not a waveform");
        assert!(
            error.detail.contains("9000"),
            "the sentence names the count that was claimed: {}",
            error.detail
        );
    }

    #[test]
    fn an_entry_folded_at_another_rate_is_refused() {
        let mut bytes = encode(&key(0xab, 2), &buckets());
        bytes[AT_SAMPLE_RATE..AT_SAMPLE_RATE + 4].copy_from_slice(&44_100u32.to_le_bytes());
        let error = decode(at("/cache/x.peaks"), &key(0xab, 2), &bytes)
            .expect_err("44100 Hz buckets are not 48000 Hz buckets");
        assert!(
            error.detail.contains("44100") && error.detail.contains(&SAMPLE_RATE.to_string()),
            "the sentence names both rates: {}",
            error.detail
        );
    }

    #[test]
    fn something_that_is_not_a_peaks_file_is_refused_by_its_first_eight_bytes() {
        let mut bytes = encode(&key(0xab, 2), &buckets());
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        let error = decode(at("/cache/x.peaks"), &key(0xab, 2), &bytes)
            .expect_err("a PNG is not a waveform");
        assert!(
            error.detail.contains("not a peaks file"),
            "the sentence says what it is not: {}",
            error.detail
        );
    }

    #[test]
    fn only_this_modules_own_names_are_ever_counted_or_deleted() {
        assert!(is_entry_name("0123456789abcdef0123456789abcdef.peaks"));
        for other in [
            "0123456789abcdef0123456789abcdef",
            "0123456789ABCDEF0123456789ABCDEF.peaks",
            "0123456789abcdef0123456789abcde.peaks",
            "0123456789abcdef0123456789abcdeff.peaks",
            "0123456789abcdef0123456789abcdeg.peaks",
            ".sublore-tmp-1234-0",
            "chooser-folders.json",
            ".peaks",
        ] {
            assert!(!is_entry_name(other), "{other} is not one of ours");
        }
    }
}
