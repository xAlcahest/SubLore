//! Open a subtitle file and save a copy of it. Two commands, no state: nothing is editable yet, so
//! there is nothing to hold between calls. The IPC names and payloads here are a public interface
//! (CLAUDE.md section 6). See BACKLOG.md M1.5.

pub mod error;

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sublore_formats::{parse, Newline, SubtitleDocument, SubtitleFormat};
use sublore_io::atomic::save_with_backup;
use sublore_io::backup::BackupStore;
use tauri::{AppHandle, Manager};

use error::{SubtitleError, SubtitleErrorCode};

/// Bigger than any subtitle file that exists. A user who points at a 4 GB video gets a sentence
/// rather than an out-of-memory kill.
pub const MAX_SUBTITLE_BYTES: u64 = 16 * 1024 * 1024;

/// Backups live under Sublore's own data directory, never beside the user's file (CLAUDE.md §3.5).
const BACKUP_DIR: &str = "backups";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSummary {
    pub path: String,
    /// "srt" | "vtt" | "ass".
    pub format: String,
    /// Cues a player would draw; ASS `Comment:` events are not among them.
    pub cue_count: usize,
    pub has_bom: bool,
    /// "lf" | "crlf" | "mixed" | "none".
    pub newline: String,
    pub byte_length: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSaved {
    pub path: String,
    pub bytes_written: u64,
    /// Absent when the destination did not exist before.
    pub backup_path: Option<String>,
}

#[tauri::command]
pub async fn subtitle_open(path: String) -> Result<SubtitleSummary, SubtitleError> {
    // Reading and parsing block, so they never run on the async runtime's poll thread (CLAUDE §7).
    tauri::async_runtime::spawn_blocking(move || open_summary(&path))
        .await
        .map_err(|error| {
            SubtitleError::new(
                SubtitleErrorCode::CommandFailed,
                format!("open task failed: {error}"),
            )
        })?
}

#[tauri::command]
pub async fn subtitle_save_as(
    app: AppHandle,
    source: String,
    destination: String,
) -> Result<SubtitleSaved, SubtitleError> {
    let backup_root = app
        .path()
        .app_data_dir()
        .map_err(|error| {
            SubtitleError::new(
                SubtitleErrorCode::BackupFailed,
                format!("no app data directory: {error}"),
            )
        })?
        .join(BACKUP_DIR);

    tauri::async_runtime::spawn_blocking(move || save_copy(&source, &destination, backup_root))
        .await
        .map_err(|error| {
            SubtitleError::new(
                SubtitleErrorCode::CommandFailed,
                format!("save task failed: {error}"),
            )
        })?
}

/// Read `path`, parse it, and describe what came back. The whole behavior of `subtitle_open`.
pub fn open_summary(path: &str) -> Result<SubtitleSummary, SubtitleError> {
    let document = read_document(Path::new(path))?;
    let source = document.source();
    Ok(SubtitleSummary {
        path: path.to_owned(),
        format: document.format().as_str().to_owned(),
        cue_count: document.displayed_cue_count(),
        has_bom: source.has_bom(),
        newline: newline_str(source.newline()).to_owned(),
        byte_length: source.byte_len() as u64,
    })
}

/// Re-read `source`, serialize it, and replace `destination` atomically with a backup kept under
/// `backup_root`. The whole behavior of `subtitle_save_as`.
///
/// The bytes come out of the serializer and never from `fs::copy`: copying would make the
/// round-trip guarantee untested at the level the user actually sees. See BACKLOG.md M1.5.
pub fn save_copy(
    source: &str,
    destination: &str,
    backup_root: PathBuf,
) -> Result<SubtitleSaved, SubtitleError> {
    if destination.is_empty() {
        return Err(SubtitleError::new(
            SubtitleErrorCode::InvalidPath,
            "the destination path is empty",
        ));
    }

    let bytes = read_document(Path::new(source))?.to_bytes();
    let outcome = save_with_backup(
        Path::new(destination),
        &bytes,
        &BackupStore::new(backup_root),
    )
    .map_err(SubtitleError::from_io)?;

    Ok(SubtitleSaved {
        path: outcome.destination.to_string_lossy().into_owned(),
        bytes_written: outcome.bytes_written,
        backup_path: outcome
            .backup
            .map(|path| path.to_string_lossy().into_owned()),
    })
}

fn read_document(path: &Path) -> Result<SubtitleDocument, SubtitleError> {
    if path.as_os_str().is_empty() {
        return Err(SubtitleError::new(
            SubtitleErrorCode::InvalidPath,
            "the path is empty",
        ));
    }

    // Metadata before opening: a directory opens fine on Linux, and "that is not a file" is the
    // sentence the user needs on both platforms.
    let metadata =
        std::fs::metadata(path).map_err(|error| SubtitleError::from_read(&error, path))?;
    if !metadata.is_file() {
        return Err(SubtitleError::new(
            SubtitleErrorCode::NotAFile,
            format!("{} is not a regular file", path.display()),
        ));
    }
    if metadata.len() > MAX_SUBTITLE_BYTES {
        return Err(SubtitleError::new(
            SubtitleErrorCode::TooLarge,
            format!("{} bytes, limit {MAX_SUBTITLE_BYTES}", metadata.len()),
        ));
    }

    let file = File::open(path).map_err(|error| SubtitleError::from_read(&error, path))?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    // One byte past the limit, so a file that grew since it was measured is refused, not truncated.
    let read = file
        .take(MAX_SUBTITLE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| SubtitleError::from_read(&error, path))?;
    if read as u64 > MAX_SUBTITLE_BYTES {
        return Err(SubtitleError::new(
            SubtitleErrorCode::TooLarge,
            format!("more than {MAX_SUBTITLE_BYTES} bytes"),
        ));
    }

    let format = detect(path, &bytes).ok_or_else(|| {
        SubtitleError::new(
            SubtitleErrorCode::UnknownFormat,
            format!("{} is not an SRT, VTT or ASS file", path.display()),
        )
    })?;
    parse(format, &bytes).map_err(SubtitleError::from_parse)
}

/// Content decides, extension breaks ties. Undecodable bytes make the content say nothing, and the
/// extension then picks the parser that reports the encoding problem properly.
fn detect(path: &Path, bytes: &[u8]) -> Option<SubtitleFormat> {
    let extension = path.extension().and_then(|value| value.to_str());
    SubtitleFormat::detect(extension, &String::from_utf8_lossy(bytes))
}

/// The wire spelling of a line terminator. Stable: the UI maps it to copy.
fn newline_str(newline: Newline) -> &'static str {
    match newline {
        Newline::Lf => "lf",
        Newline::Crlf => "crlf",
        Newline::Mixed => "mixed",
        Newline::None => "none",
    }
}
