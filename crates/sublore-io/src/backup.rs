//! Timestamped backups with a rolling cap, kept inside Sublore's own directory. BACKLOG.md M1.4.
//!
//! Layout: `{root}/{sanitized name}-{hash of the resolved path}/{name}.{stamp}[-{n}].bak`. The
//! root is Sublore's own directory, never the user's folder: dropping `.bak` files next to the
//! user's media is exactly the unrequested writing CONTRIBUTING.md §3.5 forbids, and it fails on
//! read-only locations. At M4 the root becomes the project folder, which is a call-site change.
//!
//! Pruning is the only automatic deletion in this crate. It runs at the moment a new backup is
//! created, only inside this root, and only on files whose names this crate itself produced.

use std::cmp::Reverse;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::atomic::copy_atomic;
use crate::error::{IoError, IoErrorKind};

/// How many backups are kept per source file.
pub const BACKUP_CAP: usize = 10;

/// Longest readable part of a per-file directory name; the hash carries the real identity.
const KEY_NAME_MAX: usize = 40;
/// Suffixes tried when several backups land in the same second.
const STAMP_COLLISIONS: u32 = 99;
const BACKUP_SUFFIX: &str = ".bak";
/// `YYYYMMDD-HHMMSS`.
const STAMP_LEN: usize = 15;

/// Sort key of a backup: the timestamp as `YYYYMMDDHHMMSS`, then the same-second suffix.
type BackupKey = (u64, u32);

pub struct BackupStore {
    root: PathBuf,
}

impl BackupStore {
    /// `root` must be Sublore's own directory. The store never writes or deletes outside it.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The per-file directory. Public so the UI can tell the user where backups live.
    pub fn dir_for(&self, source: &Path) -> PathBuf {
        self.root.join(key_for(source))
    }

    /// Archive `source`, then prune to the cap. `Ok(None)` when `source` does not exist.
    pub fn archive(&self, source: &Path, now: SystemTime) -> Result<Option<PathBuf>, IoError> {
        match fs::metadata(source) {
            Ok(metadata) if metadata.is_file() => {}
            // Nothing to protect: no file, or not a file this crate can copy.
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(IoError::from_io(&error, source, IoErrorKind::BackupFailed));
            }
        }

        let dir = self.dir_for(source);
        fs::create_dir_all(&dir)
            .map_err(|error| IoError::from_io(&error, &dir, IoErrorKind::BackupFailed))?;
        let name = file_name_of(source);
        let reserved = reserve(&dir, &name, &stamp(now))?;

        if let Err(error) = copy_atomic(source, &reserved) {
            // The name was reserved moments ago by this call; an empty file must not look like a
            // backup. This is the only deletion outside the cap, and it undoes this call's own work.
            let _ = fs::remove_file(&reserved);
            return Err(error.into_backup_failed());
        }
        prune(&dir, &name);
        Ok(Some(reserved))
    }

    /// Newest first. Only files matching the reserved backup pattern are listed.
    pub fn list(&self, source: &Path) -> Result<Vec<PathBuf>, IoError> {
        let mut found = read_backups(&self.dir_for(source), &file_name_of(source))?;
        found.sort_by_key(|(key, _)| Reverse(*key));
        Ok(found.into_iter().map(|(_, path)| path).collect())
    }
}

/// The per-file directory name: readable for a human who opens the folder, unique for the machine.
fn key_for(source: &Path) -> String {
    let hash = fnv1a64(canonical_path(source).as_os_str().as_encoded_bytes());
    format!("{}-{hash:016x}", sanitize(&file_name_of(source)))
}

/// The directory is resolved, not the file: the key must stay the same whether or not the file
/// exists right now, or a deleted file's backups would become unreachable.
fn canonical_path(source: &Path) -> PathBuf {
    let (Some(parent), Some(name)) = (source.parent(), source.file_name()) else {
        return source.to_path_buf();
    };
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    match fs::canonicalize(parent) {
        Ok(resolved) => resolved.join(name),
        Err(_) => source.to_path_buf(),
    }
}

fn file_name_of(source: &Path) -> String {
    source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Keep what is safe in a directory name on every platform, replace the rest.
fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len().min(KEY_NAME_MAX));
    for character in name.chars() {
        if out.len() >= KEY_NAME_MAX {
            break;
        }
        out.push(match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => character,
            _ => '_',
        });
    }
    out
}

/// FNV-1a, hand-rolled and frozen: `DefaultHasher` is not stable across Rust releases, and a key
/// that moved after a toolchain bump would orphan a user's backups. See BACKLOG.md M1.4.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// `YYYYMMDD-HHMMSS` in UTC. No colons: they are illegal in Windows file names.
fn stamp(now: SystemTime) -> String {
    // A clock set before 1970 stamps the epoch rather than failing a save.
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    let rest = seconds % 86_400;
    format!(
        "{:04}{month:02}{day:02}-{:02}{:02}{:02}",
        year.clamp(0, 9999),
        rest / 3_600,
        (rest / 60) % 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to a civil date, after Howard Hinnant's `civil_from_days`. Integer only,
/// so no dependency and no drift.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

/// Claim a name with `create_new`, so two saves in the same second cannot pick the same one.
fn reserve(dir: &Path, name: &str, stamp: &str) -> Result<PathBuf, IoError> {
    for index in 0..=STAMP_COLLISIONS {
        let candidate = match index {
            0 => format!("{name}.{stamp}{BACKUP_SUFFIX}"),
            _ => format!("{name}.{stamp}-{index}{BACKUP_SUFFIX}"),
        };
        let path = dir.join(candidate);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(IoError::from_io(&error, &path, IoErrorKind::BackupFailed)),
        }
    }
    Err(IoError::new(
        IoErrorKind::BackupFailed,
        dir,
        format!("{STAMP_COLLISIONS} backups already exist for this second"),
    ))
}

/// Drop the oldest backups above the cap, after the new one is safely written. Best effort by
/// design: the old content is already protected, and a file that cannot be deleted (another
/// program has it open) must never turn a good save into a failed one. See BACKLOG.md M1.4.
fn prune(dir: &Path, name: &str) {
    let Ok(mut found) = read_backups(dir, name) else {
        return;
    };
    if found.len() <= BACKUP_CAP {
        return;
    }
    found.sort_by_key(|(key, _)| *key);
    for (_, path) in found.iter().take(found.len() - BACKUP_CAP) {
        let _ = fs::remove_file(path);
    }
}

fn read_backups(dir: &Path, name: &str) -> Result<Vec<(BackupKey, PathBuf)>, IoError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(IoError::from_io(&error, dir, IoErrorKind::ReadFailed)),
    };

    let mut found = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| IoError::from_io(&error, dir, IoErrorKind::ReadFailed))?;
        // Regular files only, and the file type is read without following: a symlink is not ours.
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let file_name = entry.file_name();
        let Some(text) = file_name.to_str() else {
            continue;
        };
        let Some(key) = backup_key(text, name) else {
            continue;
        };
        found.push((key, entry.path()));
    }
    Ok(found)
}

/// `{name}.{YYYYMMDD}-{HHMMSS}[-{n}].bak` and nothing else. Anything that does not match, this
/// crate did not write, and it is left alone forever. See BACKLOG.md M1.4.
fn backup_key(file_name: &str, source_name: &str) -> Option<BackupKey> {
    let middle = file_name
        .strip_prefix(source_name)?
        .strip_prefix('.')?
        .strip_suffix(BACKUP_SUFFIX)?;
    let bytes = middle.as_bytes();

    let date = digits(bytes.get(..8)?)?;
    if bytes.get(8)? != &b'-' {
        return None;
    }
    let time = digits(bytes.get(9..STAMP_LEN)?)?;
    let index = match bytes.len() {
        STAMP_LEN => 0,
        17 | 18 => {
            if bytes.get(STAMP_LEN)? != &b'-' {
                return None;
            }
            digits(bytes.get(STAMP_LEN + 1..)?)? as u32
        }
        _ => return None,
    };
    Some((date * 1_000_000 + time, index))
}

/// The slice as a number, only when every byte is an ASCII digit.
fn digits(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{backup_key, civil_from_days, fnv1a64, key_for, sanitize, stamp, BACKUP_CAP};
    use std::path::Path;
    use std::time::{Duration, UNIX_EPOCH};

    fn at(seconds: u64) -> String {
        stamp(UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn known_dates_convert() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_688), (2026, 8, 23));
        // 2100 is not a leap year: the 28th is followed by the 1st.
        assert_eq!(civil_from_days(47_540), (2100, 2, 28));
        assert_eq!(civil_from_days(47_541), (2100, 3, 1));
    }

    #[test]
    fn stamps_are_utc_and_windows_safe() {
        assert_eq!(at(0), "19700101-000000");
        assert_eq!(at(951_782_400 + 3_661), "20000229-010101");
        assert_eq!(at(1_756_000_000), "20250824-014640");
        assert!(!at(1_756_000_000).contains(':'));
    }

    #[test]
    fn a_clock_before_the_epoch_stamps_the_epoch() {
        let before = UNIX_EPOCH - Duration::from_secs(60);
        assert_eq!(stamp(before), "19700101-000000");
    }

    #[test]
    fn only_this_crate_s_own_names_are_backups() {
        assert_eq!(
            backup_key("ep01.srt.20260823-101500.bak", "ep01.srt"),
            Some((20_260_823_101_500, 0))
        );
        assert_eq!(
            backup_key("ep01.srt.20260823-101500-7.bak", "ep01.srt"),
            Some((20_260_823_101_500, 7))
        );
        assert_eq!(
            backup_key("ep01.srt.20260823-101500-99.bak", "ep01.srt"),
            Some((20_260_823_101_500, 99))
        );

        for foreign in [
            "notes.txt",
            "README.bak",
            "ep01.srt.bak",
            "ep01.srt.2026082-101500.bak",
            "ep01.srt.20260823-1015.bak",
            "ep01.srt.20260823-101500.txt",
            "ep01.srt.20260823-101500-100.bak",
            "ep01.srt.20260823_101500.bak",
            "ep01.srt.2026082a-101500.bak",
            "ep01.srt.20260823-101500-.bak",
            "other.srt.20260823-101500.bak",
        ] {
            assert_eq!(
                backup_key(foreign, "ep01.srt"),
                None,
                "{foreign} is not ours"
            );
        }
    }

    #[test]
    fn the_suffix_orders_backups_inside_one_second() {
        let plain = backup_key("ep01.srt.20260823-101500.bak", "ep01.srt");
        let second = backup_key("ep01.srt.20260823-101500-2.bak", "ep01.srt");
        let tenth = backup_key("ep01.srt.20260823-101500-10.bak", "ep01.srt");
        assert!(plain < second && second < tenth, "-10 is newer than -2");
    }

    #[test]
    fn keys_are_readable_and_stable() {
        let key = key_for(Path::new("/media/S01/ep01.srt"));
        assert!(key.starts_with("ep01.srt-"), "{key} keeps a readable name");
        assert_eq!(key.len(), "ep01.srt-".len() + 16);
        assert_eq!(key, key_for(Path::new("/media/S01/ep01.srt")));
        assert_ne!(key, key_for(Path::new("/media/S02/ep01.srt")));
    }

    #[test]
    fn hashes_are_the_published_fnv_1a_values() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn names_are_sanitized_and_bounded() {
        assert_eq!(sanitize("ep01.srt"), "ep01.srt");
        assert_eq!(sanitize("épisode 1/2*.srt"), "_pisode_1_2_.srt");
        assert_eq!(sanitize(&"x".repeat(200)).len(), 40);
        assert!(sanitize("日本語").chars().all(|c| c == '_'));
    }

    #[test]
    fn the_cap_is_small_and_fixed() {
        assert_eq!(BACKUP_CAP, 10);
    }
}
