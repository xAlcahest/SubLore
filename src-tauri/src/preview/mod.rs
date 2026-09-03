//! The open document, drawn on the video frame.
//!
//! mpv reads a shadow copy of the document from the app's own cache directory. The user's subtitle
//! file is never the file mpv reads, is never written to show a preview, and no backup is made to
//! produce one (CONTRIBUTING.md section 3, decision 7). The shadow is replaced the way every other
//! write here is replaced: temp file, fsync, rename.
//!
//! Every route that changes what should be on the frame ends at [`refresh`]: an edit, an open, a
//! close, an adopted transcription, a video that has just loaded, and View's own toggle. Neither
//! order is special, because mpv is asked what it holds rather than told what it should hold.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use sublore_io::atomic::write_atomic;
use tauri::{AppHandle, Emitter, Manager};

use crate::log;
use crate::subtitle::{DocumentBytes, SessionSlot, SubtitleState};
use crate::video::error::{VideoError, VideoErrorCode};
use crate::video::player::Player;

/// Under the app's cache directory, so a user clearing caches loses previews and nothing else.
const CACHE_DIR: &str = "preview";
/// The shadow's stem. Its extension is the document's own format: mpv reads ASS override tags and
/// SRT has no grammar for them, so a preview of an ASS document has to be ASS to look like it.
const SHADOW_STEM: &str = "document";

/// Raised when the document could not be put on the frame, and again only after it came back.
pub const EVENT_FAILED: &str = "preview://failed";
pub const EVENT_DRAWN: &str = "preview://drawn";

/// What stopped the preview.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewError {
    /// Technical, never shown to the user: the UI draws one sentence of its own, as it does for a
    /// peak job that failed.
    pub detail: String,
}

impl PreviewError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

pub struct PreviewState {
    preview: Arc<Preview>,
}

impl PreviewState {
    pub fn new(app: &AppHandle, player: Arc<Player>) -> Self {
        Self {
            preview: Arc::new(Preview::new(app, player)),
        }
    }

    /// A handle the blocking half of a command can own, as `VideoState` hands out its player.
    fn preview(&self) -> Arc<Preview> {
        Arc::clone(&self.preview)
    }
}

/// The shadow copy mpv was last given, and what is in it.
struct Shadow {
    path: PathBuf,
    bytes: Vec<u8>,
}

pub struct Preview {
    app: AppHandle,
    player: Arc<Player>,
    /// Where the shadow goes, or nothing when the platform gave no cache directory.
    dir: Option<PathBuf>,
    /// View's toggle, on from the start: a translator opens a video to see the lines on it.
    shown: AtomicBool,
    /// The last shadow written. Holding the bytes is what makes an unchanged document cost no
    /// write and no re-read, so a View toggle does not churn the file.
    written: Mutex<Option<Shadow>>,
    /// Whether the UI has already been told, so it is told once and not once per edit.
    failing: AtomicBool,
}

impl Preview {
    /// Resolve the cache directory once, when the app starts, as the peaks cache does: the path
    /// does not change while the process runs and an edit is not the place to discover it is gone.
    fn new(app: &AppHandle, player: Arc<Player>) -> Self {
        let dir = match app.path().app_cache_dir() {
            Ok(dir) => {
                let dir = dir.join(CACHE_DIR);
                log::info!("preview: mpv reads the document from {}", dir.display());
                Some(dir)
            }
            Err(error) => {
                log::warn!(
                    "preview: no cache directory, so nothing can be drawn on a video: {error}"
                );
                None
            }
        };
        Self {
            app: app.clone(),
            player,
            dir,
            shown: AtomicBool::new(true),
            written: Mutex::new(None),
            failing: AtomicBool::new(false),
        }
    }

    /// Put the open document on the frame, whatever just changed it.
    ///
    /// The document is read under this call's own lock, so two refreshes cannot read the session in
    /// one order and write the shadow in the other. This lock is taken before the session's and
    /// never the other way round, which is what keeps the pair deadlock-free.
    fn refresh(&self, slot: &SessionSlot) {
        let mut written = self.written.lock().unwrap_or_else(PoisonError::into_inner);
        let document = crate::subtitle::open_document(slot);
        let outcome = self.draw(&mut written, document);
        self.report(outcome);
    }

    fn draw(
        &self,
        written: &mut Option<Shadow>,
        document: Option<DocumentBytes>,
    ) -> Result<(), PreviewError> {
        // Nothing to draw: no document, or one with no cue a player would draw. mpv refuses a
        // subtitle file that holds no events, so an empty document takes the track off instead.
        let Some(document) = document.filter(|open| open.cues > 0) else {
            *written = None;
            if on_the_frame(self.player.drop_subtitles())?.is_some() {
                log::info!("preview: nothing to draw, so mpv holds no document");
            }
            return Ok(());
        };

        let dir = self.dir.as_ref().ok_or_else(|| {
            PreviewError::new("there is no cache directory to keep the shadow copy in")
        })?;
        let path = dir.join(format!("{SHADOW_STEM}.{}", document.format));
        // Rewritten only when the bytes really changed, so a toggle and a command that left the
        // document alone cost mpv nothing.
        let changed = written
            .as_ref()
            .is_none_or(|shadow| shadow.path != path || shadow.bytes != document.bytes);
        if changed {
            fs::create_dir_all(dir).map_err(|error| {
                PreviewError::new(format!("could not make {}: {error}", dir.display()))
            })?;
            write_atomic(&path, &document.bytes)
                .map_err(|error| PreviewError::new(format!("{error}")))?;
        }

        let visible = self.shown.load(Ordering::Relaxed);
        let drawn = on_the_frame(self.player.show_subtitles(&path, changed, visible))?;
        *written = Some(Shadow {
            path,
            bytes: document.bytes,
        });

        // mpv's own answer, and the only thing outside the window that can be observed about what
        // is on the frame. The line at the playhead is counted and not written down: it is the
        // user's own writing, the rule `subtitle::apply_edit` follows for the same reason.
        match drawn {
            Some(drawn) => log::info!(
                "preview: mpv holds the document, external tracks {}, selected {}, visible {}, {} at the playhead",
                drawn.tracks,
                yes_or_no(drawn.selected),
                yes_or_no(drawn.visible),
                match drawn.chars {
                    Some(chars) => format!("{chars} chars"),
                    None => "no line".to_owned(),
                }
            ),
            None => log::info!("preview: the document is shadowed, and no video is open to draw it on"),
        }
        Ok(())
    }

    /// Tell the UI, once per spell of trouble and once per recovery: a message that reappears on
    /// every keystroke is one the translator learns to look past.
    fn report(&self, outcome: Result<(), PreviewError>) {
        match outcome {
            Ok(()) => {
                if self.failing.swap(false, Ordering::Relaxed) {
                    let _ = self.app.emit(EVENT_DRAWN, ());
                }
            }
            Err(error) => {
                log::error!("preview: {}", error.detail);
                if !self.failing.swap(true, Ordering::Relaxed) {
                    let _ = self.app.emit(EVENT_FAILED, error);
                }
            }
        }
    }
}

/// mpv having no media open, or being gone, is not a preview failure: there is nothing to draw on,
/// and nothing the user could do about it. Everything else is theirs to hear about.
fn on_the_frame<T>(outcome: Result<T, VideoError>) -> Result<Option<T>, PreviewError> {
    match outcome {
        Ok(value) => Ok(Some(value)),
        Err(error)
            if matches!(
                error.code,
                VideoErrorCode::NotLoaded | VideoErrorCode::PlayerUnavailable
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(PreviewError::new(error.to_string())),
    }
}

fn yes_or_no(flag: bool) -> &'static str {
    if flag {
        "yes"
    } else {
        "no"
    }
}

/// Put the open document on the frame. Every route that changes what should be drawn ends here.
///
/// Never fails its caller: an edit is not lost because a preview could not be drawn, so what went
/// wrong reaches the UI as its own message instead.
pub async fn refresh(app: &AppHandle) {
    let app = app.clone();
    // The shadow write and the mpv commands both block, so neither runs on the async runtime's
    // poll thread (CONTRIBUTING.md section 7).
    if let Err(error) = tauri::async_runtime::spawn_blocking(move || refresh_now(&app)).await {
        log::error!("preview: the refresh task failed: {error}");
    }
}

fn refresh_now(app: &AppHandle) {
    let (Some(preview), Some(subtitle)) = (
        app.try_state::<PreviewState>(),
        app.try_state::<SubtitleState>(),
    ) else {
        return;
    };
    preview.preview().refresh(&subtitle.slot());
}

/// View's toggle. mpv keeps decoding the document either way, so turning it back on costs no read.
#[tauri::command]
pub async fn preview_set_shown(app: AppHandle, shown: bool) {
    {
        if let Some(state) = app.try_state::<PreviewState>() {
            state.preview.shown.store(shown, Ordering::Relaxed);
        }
    }
    refresh(&app).await;
}
