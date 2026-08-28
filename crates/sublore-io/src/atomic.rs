//! Atomic replace: temp file in the destination's own directory, fsync, rename. BACKLOG.md M1.4.
//!
//! There is no cross-filesystem fallback and there must never be one: the temp file lives in the
//! destination's own directory, so a rename across devices cannot happen, and a copy-based
//! fallback would not be atomic (CLAUDE.md §3.2). An unwritable directory is an error the user
//! can act on, never a degraded write path.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use crate::backup::BackupStore;
use crate::error::{IoError, IoErrorKind};
use crate::fault::{self, FaultPoint};

/// Reserved prefix for the temp file. Nothing outside this crate ever creates or deletes one, and
/// an interrupted save can leave one behind: the name is what tells the user it is not their file.
const TEMP_PREFIX: &str = ".sublore-tmp-";
/// Names tried before giving up on a temp file.
const TEMP_ATTEMPTS: u32 = 8;

#[derive(Clone, Debug)]
pub struct SaveOutcome {
    /// Where the bytes actually landed (symlinks resolved).
    pub destination: PathBuf,
    /// The backup that was kept, when the destination already existed.
    pub backup: Option<PathBuf>,
    pub bytes_written: u64,
}

/// Replace `path` with `bytes`, atomically. On any failure the destination is untouched.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), IoError> {
    replace(&Target::resolve(path)?, bytes)
}

/// Same mechanics, streaming, for archiving an existing file into the backup store.
pub fn copy_atomic(source: &Path, destination: &Path) -> Result<u64, IoError> {
    let target = Target::resolve(destination)?;
    let mut reader = File::open(source)
        .map_err(|error| IoError::from_io(&error, source, IoErrorKind::ReadFailed))?;
    let source_mode = reader.metadata().ok();

    let (temp, mut file) = create_temp(&target.dir)?;
    let filled = fill_from(&mut reader, &mut file, &temp, source_mode.as_ref());
    drop(file);
    let copied = match filled {
        Ok(copied) => copied,
        Err(error) => return Err(discard(&temp, error)),
    };
    if let Err(error) = rename(&temp, &target.destination) {
        return Err(discard(&temp, error));
    }
    sync_dir(&target.dir)?;
    Ok(copied)
}

/// Back up first, then write. A failed backup aborts before the destination is touched.
pub fn save_with_backup(
    path: &Path,
    bytes: &[u8],
    store: &BackupStore,
) -> Result<SaveOutcome, IoError> {
    let target = Target::resolve(path)?;
    let backup = match target.existing {
        Some(_) => store.archive(&target.destination, SystemTime::now())?,
        None => None,
    };
    fault::trip(FaultPoint::AfterBackup);
    replace(&target, bytes)?;
    Ok(SaveOutcome {
        destination: target.destination,
        backup,
        bytes_written: bytes.len() as u64,
    })
}

/// Where a save is going: the resolved destination, the directory that holds it, and what is
/// there now.
struct Target {
    destination: PathBuf,
    dir: PathBuf,
    existing: Option<fs::Metadata>,
}

impl Target {
    fn resolve(path: &Path) -> Result<Self, IoError> {
        if path.as_os_str().is_empty() {
            return Err(IoError::new(
                IoErrorKind::InvalidPath,
                path,
                "the path is empty",
            ));
        }
        let destination = resolve_symlink(path)?;
        let dir = match destination.parent() {
            None => {
                return Err(IoError::new(
                    IoErrorKind::InvalidPath,
                    &destination,
                    "the path has no directory to write into",
                ))
            }
            // A bare file name means the current directory, which is where the temp file belongs.
            Some(parent) if parent.as_os_str().is_empty() => PathBuf::from("."),
            Some(parent) => parent.to_path_buf(),
        };
        let existing = match fs::metadata(&destination) {
            Ok(metadata) if metadata.is_file() => Some(metadata),
            Ok(_) => {
                return Err(IoError::new(
                    IoErrorKind::NotAFile,
                    &destination,
                    "the destination is not a regular file",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(IoError::from_io(
                    &error,
                    &destination,
                    IoErrorKind::NotAFile,
                ))
            }
        };
        Ok(Self {
            destination,
            dir,
            existing,
        })
    }
}

/// Write through a symlink rather than over it: replacing the user's link with a regular file is
/// data loss of a kind. A link that cannot be resolved is not a destination to guess at.
fn resolve_symlink(path: &Path) -> Result<PathBuf, IoError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)
            .map_err(|error| IoError::from_io(&error, path, IoErrorKind::NotAFile)),
        Ok(_) => Ok(path.to_path_buf()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(IoError::from_io(&error, path, IoErrorKind::NotAFile)),
    }
}

/// The write sequence, in the one order that keeps the destination whole at every instant.
fn replace(target: &Target, bytes: &[u8]) -> Result<(), IoError> {
    let (temp, mut file) = create_temp(&target.dir)?;
    fault::trip(FaultPoint::AfterTempCreated);

    let filled = fill(&mut file, &temp, target.existing.as_ref(), bytes);
    drop(file);
    if let Err(error) = filled {
        return Err(discard(&temp, error));
    }
    fault::trip(FaultPoint::AfterSync);

    if let Err(error) = rename(&temp, &target.destination) {
        return Err(discard(&temp, error));
    }
    fault::trip(FaultPoint::AfterRename);
    sync_dir(&target.dir)
}

/// Everything that happens inside the temp file: the mode, the bytes, the sync.
fn fill(
    file: &mut File,
    temp: &Path,
    existing: Option<&fs::Metadata>,
    bytes: &[u8],
) -> Result<(), IoError> {
    copy_permissions(file, temp, existing)?;

    // Debug builds only: the half-written temp the M1.4 crash test exercises. See BACKLOG.md M1.4.
    #[cfg(debug_assertions)]
    if fault::armed(FaultPoint::DuringWrite) {
        let (head, _) = bytes.split_at(bytes.len() / 2);
        let _ = file.write_all(head);
        let _ = file.flush();
        fault::trip(FaultPoint::DuringWrite);
    }

    file.write_all(bytes)
        .map_err(|error| IoError::from_io(&error, temp, IoErrorKind::WriteFailed))?;
    fault::trip(FaultPoint::AfterWrite);
    // sync_all, not sync_data: the length has to reach the disk with the bytes.
    file.sync_all()
        .map_err(|error| IoError::from_io(&error, temp, IoErrorKind::SyncFailed))
}

/// The same, streaming from another file. No fault points: only the destination write is the one
/// the crash tests interrupt.
fn fill_from(
    reader: &mut File,
    file: &mut File,
    temp: &Path,
    mode: Option<&fs::Metadata>,
) -> Result<u64, IoError> {
    copy_permissions(file, temp, mode)?;
    let copied = io::copy(reader, file)
        .map_err(|error| IoError::from_io(&error, temp, IoErrorKind::WriteFailed))?;
    file.sync_all()
        .map_err(|error| IoError::from_io(&error, temp, IoErrorKind::SyncFailed))?;
    Ok(copied)
}

/// Keep the mode of the file being replaced: a save must not change who can read the user's file.
#[cfg(unix)]
fn copy_permissions(
    file: &File,
    temp: &Path,
    existing: Option<&fs::Metadata>,
) -> Result<(), IoError> {
    let Some(existing) = existing else {
        return Ok(());
    };
    file.set_permissions(existing.permissions())
        .map_err(|error| IoError::from_io(&error, temp, IoErrorKind::WriteFailed))
}

/// Windows: a read-only destination blocks the rename anyway, and a read-only temp file could not
/// be cleaned up afterwards, so the attributes are left alone. See BACKLOG.md M1.4.
#[cfg(not(unix))]
fn copy_permissions(
    _file: &File,
    _temp: &Path,
    _existing: Option<&fs::Metadata>,
) -> Result<(), IoError> {
    Ok(())
}

fn create_temp(dir: &Path) -> Result<(PathBuf, File), IoError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    for _ in 0..TEMP_ATTEMPTS {
        let path = dir.join(format!(
            "{TEMP_PREFIX}{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        // create_new is the collision guard: an existing name is never opened, never truncated.
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(IoError::from_io(&error, dir, IoErrorKind::TempCreateFailed)),
        }
    }
    Err(IoError::new(
        IoErrorKind::TempCreateFailed,
        dir,
        format!("{TEMP_ATTEMPTS} temporary names were already taken"),
    ))
}

/// Windows fails this when another program holds the destination open; the UI says so.
fn rename(temp: &Path, destination: &Path) -> Result<(), IoError> {
    fs::rename(temp, destination)
        .map_err(|error| IoError::from_io(&error, destination, IoErrorKind::RenameFailed))
}

/// Persist the rename itself, so it survives a power cut. Public because the model download
/// finishes with the same rename and owes the user the same durability. See BACKLOG.md M3.2.
#[cfg(unix)]
pub fn sync_dir(dir: &Path) -> Result<(), IoError> {
    let handle =
        File::open(dir).map_err(|error| IoError::from_io(&error, dir, IoErrorKind::SyncFailed))?;
    handle
        .sync_all()
        .map_err(|error| IoError::from_io(&error, dir, IoErrorKind::SyncFailed))
}

/// Windows cannot open a directory as a file; the rename is durable there by other means.
#[cfg(not(unix))]
pub fn sync_dir(_dir: &Path) -> Result<(), IoError> {
    Ok(())
}

/// Drop a temp file that will never be renamed. The original failure is what the caller sees: a
/// cleanup that fails is never reported instead of it.
fn discard(temp: &Path, error: IoError) -> IoError {
    let _ = fs::remove_file(temp);
    error
}
