//! The resumable, checksummed model download. See BACKLOG.md M3.2.
//!
//! Reached only from the app's download command, which only the Download button invokes: this is
//! the one place in the milestone where a socket may open, and it opens because the user asked
//! (CONTRIBUTING.md §1). The transport is behind a trait so the tests can drive the whole thing without
//! one, and so that a fetcher that must never be called can be handed in and fail the test if it is.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::error::{AsrError, AsrErrorKind};
use crate::model::catalog::ModelSpec;
use crate::model::store::{hex, ModelStore};
use crate::sidecar::Cancel;

/// Read size. Big enough that a 3 GB file is not 3 million syscalls, small enough that a cancel
/// is noticed promptly.
const CHUNK_BYTES: usize = 256 * 1024;

/// A body that starts at a known offset. The offset is the point: a server may ignore `Range` and
/// send the whole file, and a caller that assumed otherwise would append the head of the file to
/// the middle of its own.
pub struct Fetched {
    /// The offset the first byte of `body` belongs at.
    pub start: u64,
    /// What the server says the whole file is, when it says.
    pub total: Option<u64>,
    pub body: Box<dyn Read + Send>,
}

pub trait RangeFetcher: Send + Sync {
    /// Ask for `url` from byte `from`. Implementations must report where the body actually starts.
    fn get(&self, url: &str, from: u64) -> Result<Fetched, AsrError>;
}

/// Fetch `spec` into the store, resuming a previous attempt if there is one.
///
/// Returns the final path. The file only ever gets that name after both its length and its sha256
/// match the catalog, so a corrupt download can never be handed to whisper; and a file already in
/// the store that no longer hashes to its catalog row is fetched again and replaced by the rename.
pub fn download(
    store: &ModelStore,
    spec: &ModelSpec,
    base_url: &str,
    fetcher: &dyn RangeFetcher,
    cancel: &Cancel,
    on_progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<PathBuf, AsrError> {
    let final_path = store.path(spec);
    // Already here and whole: nothing to fetch, and no socket is opened to find out. The hash is
    // what decides, so Download on a file that rotted in place fetches it again rather than
    // reporting success. See BACKLOG.md M3.2.
    if fs::metadata(&final_path).ok().map(|meta| meta.len()) == Some(spec.bytes)
        && store.verify(spec).is_ok()
    {
        on_progress(spec.bytes, spec.bytes);
        return Ok(final_path);
    }
    if cancel.is_cancelled() {
        return Err(AsrError::new(
            AsrErrorKind::Cancelled,
            "the user cancelled the download",
        ));
    }
    fs::create_dir_all(store.dir()).map_err(|error| {
        AsrError::new(
            AsrErrorKind::DownloadWriteFailed,
            format!("cannot create {}: {error}", store.dir().display()),
        )
    })?;

    let part = store.part_path(spec);
    let have = fs::metadata(&part)
        .ok()
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
        .filter(|len| *len < spec.bytes)
        .unwrap_or(0);

    let url = format!("{base_url}{}", spec.file);
    let fetched = fetcher.get(&url, have)?;
    if let Some(total) = fetched.total {
        // The catalog is the authority. A server offering a different file is refused before a
        // single byte is written.
        if total != spec.bytes {
            return Err(AsrError::new(
                AsrErrorKind::SizeMismatch,
                format!("{url} is {total} bytes, the catalog says {}", spec.bytes),
            ));
        }
    }

    let resuming = fetched.start == have && have > 0;
    if fetched.start != 0 && !resuming {
        // A range we did not ask for. Nothing sensible can be appended, and guessing would
        // corrupt the file silently.
        return Err(AsrError::new(
            AsrErrorKind::NetworkFailed,
            format!(
                "{url} answered from byte {}, asked from {have}",
                fetched.start
            ),
        ));
    }

    let mut hasher = Sha256::new();
    let mut file = if resuming {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&part)
            .map_err(|error| write_failed(&part, error))?;
        // The prefix has to go through the hasher before the rest does, which is the one read pass
        // a resume costs. A fresh download hashes as it writes and reads nothing twice.
        let hashed = hash_prefix(&mut file, &mut hasher, have)?;
        if hashed != have {
            return Err(AsrError::new(
                AsrErrorKind::NetworkFailed,
                format!("{} shrank while it was being resumed", part.display()),
            ));
        }
        file
    } else {
        // A restart from zero, so whatever was there is not what the server is sending.
        File::create(&part).map_err(|error| write_failed(&part, error))?
    };

    let mut received = if resuming { have } else { 0 };
    on_progress(received, spec.bytes);
    let mut body = fetched.body;
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let mut oversize = false;
    loop {
        if cancel.is_cancelled() {
            // The partial file stays: its length is the whole of the resume state, and there is
            // no sidecar metadata that could get out of step with it.
            let _ = file.flush();
            return Err(AsrError::new(
                AsrErrorKind::Cancelled,
                "the user cancelled the download",
            ));
        }
        let read = match body.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                let _ = file.flush();
                return Err(AsrError::new(
                    AsrErrorKind::NetworkFailed,
                    format!("{url} stopped after {received} bytes: {error}"),
                ));
            }
        };
        if received + read as u64 > spec.bytes {
            // Never let a stream past the catalogued size fill the disk.
            oversize = true;
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| write_failed(&part, error))?;
        hasher.update(&buffer[..read]);
        received += read as u64;
        on_progress(received, spec.bytes);
    }

    if oversize {
        drop(file);
        let _ = fs::remove_file(&part);
        return Err(AsrError::new(
            AsrErrorKind::SizeMismatch,
            format!("{url} sent more than the catalogued {} bytes", spec.bytes),
        ));
    }
    if received != spec.bytes {
        // Short: keep the part file, because that is exactly what makes the next attempt resume.
        let _ = file.flush();
        return Err(AsrError::new(
            AsrErrorKind::NetworkFailed,
            format!("{url} ended at {received} of {} bytes", spec.bytes),
        ));
    }

    let digest = hex(&hasher.finalize());
    if digest != spec.sha256 {
        drop(file);
        let _ = fs::remove_file(&part);
        return Err(AsrError::new(
            AsrErrorKind::ChecksumMismatch,
            format!("{url} hashes to {digest}, the catalog says {}", spec.sha256),
        ));
    }

    file.sync_all()
        .map_err(|error| write_failed(&part, error))?;
    drop(file);
    fs::rename(&part, &final_path).map_err(|error| write_failed(&final_path, error))?;
    // Persist the rename itself, the same way a subtitle save does.
    sublore_io::atomic::sync_dir(store.dir()).map_err(|error| {
        AsrError::new(
            AsrErrorKind::DownloadWriteFailed,
            format!("cannot persist {}: {error}", store.dir().display()),
        )
    })?;
    Ok(final_path)
}

/// Feed the first `len` bytes of `file` through `hasher`, then leave the cursor at the end of them
/// so the transfer appends rather than overwrites.
fn hash_prefix(file: &mut File, hasher: &mut Sha256, len: u64) -> Result<u64, AsrError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| AsrError::new(AsrErrorKind::DownloadWriteFailed, error.to_string()))?;
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let mut hashed = 0u64;
    while hashed < len {
        let want = ((len - hashed) as usize).min(buffer.len());
        let read = match file.read(&mut buffer[..want]) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(AsrError::new(
                    AsrErrorKind::DownloadWriteFailed,
                    format!("cannot re-read the partial download: {error}"),
                ))
            }
        };
        hasher.update(&buffer[..read]);
        hashed += read as u64;
    }
    file.seek(SeekFrom::Start(hashed))
        .map_err(|error| AsrError::new(AsrErrorKind::DownloadWriteFailed, error.to_string()))?;
    Ok(hashed)
}

fn write_failed(path: &std::path::Path, error: std::io::Error) -> AsrError {
    AsrError::new(
        AsrErrorKind::DownloadWriteFailed,
        format!("cannot write {}: {error}", path.display()),
    )
}
