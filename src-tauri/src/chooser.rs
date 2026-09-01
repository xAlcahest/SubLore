//! The native file chooser, for every command that needs a path from the user.
//!
//! It used to live in `project/mod.rs` because the project panel was the only caller. M2.0 removes
//! every field for typing a path, so opening a video, opening a subtitle and saving a copy all come
//! through here too, and a chooser for video files inside the project module would be a lie about
//! where it belongs.
//!
//! On Linux it builds a `gtk::FileChooserDialog` on the main thread, for the reason `dialog.rs`
//! gives: the dialog plugin's rfd backend starts a second thread and iterates GTK on it for the rest
//! of the process's life, and GTK3 is not built to be driven from two threads. Every other platform
//! keeps the plugin.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::AppHandle;

use crate::log;
use crate::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChooserErrorCode {
    /// The dialog could not be raised at all, so the user was never asked.
    ChooserFailed,
    /// A path the app cannot hand back as text. Refused rather than mangled.
    PathNotUtf8,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChooserError {
    pub code: ChooserErrorCode,
    /// Technical, never shown to the user, may be empty.
    pub detail: String,
}

impl ChooserError {
    fn new(code: ChooserErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// What the chooser was asked for. Parsed once, so no platform branch matches on a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    ProjectFolder,
    ProjectFile,
    Video,
    Subtitle,
    SubtitleSave,
    /// Where a document that has never had a file goes, asked by Save rather than by Save a copy
    /// (decision 24, B2).
    SubtitleFirstSave,
}

impl Choice {
    /// The frontend sends one of these literals. An unknown one is a bug in it, so the chooser is
    /// not raised and the caller is told, rather than a dialog appearing that asks for nothing.
    fn parse(kind: &str) -> Option<Self> {
        match kind {
            "project-folder" => Some(Self::ProjectFolder),
            "project-file" => Some(Self::ProjectFile),
            "video" => Some(Self::Video),
            "subtitle" => Some(Self::Subtitle),
            "subtitle-save" => Some(Self::SubtitleSave),
            "subtitle-first-save" => Some(Self::SubtitleFirstSave),
            _ => None,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::ProjectFolder => strings::CHOOSE_PROJECT_FOLDER,
            Self::ProjectFile => strings::CHOOSE_PROJECT_FILE,
            Self::Video => strings::CHOOSE_VIDEO,
            Self::Subtitle => strings::CHOOSE_SUBTITLE,
            Self::SubtitleSave => strings::CHOOSE_SUBTITLE_SAVE,
            Self::SubtitleFirstSave => strings::CHOOSE_SUBTITLE_FIRST_SAVE,
        }
    }

    /// Naming a file to write is a different question from picking one that exists, and the chooser
    /// asks it differently: a filename field, and a warning before an overwrite.
    fn is_save(self) -> bool {
        matches!(self, Self::SubtitleSave | Self::SubtitleFirstSave)
    }

    /// The word the log uses, which is also the literal the frontend sent.
    fn as_str(self) -> &'static str {
        match self {
            Self::ProjectFolder => "project-folder",
            Self::ProjectFile => "project-file",
            Self::Video => "video",
            Self::Subtitle => "subtitle",
            Self::SubtitleSave => "subtitle-save",
            Self::SubtitleFirstSave => "subtitle-first-save",
        }
    }
}

/// The name a save chooser opens with, taken from the path it was handed. `Path` knows its own
/// platform's separators; splitting the path in the frontend would only know one of them.
fn save_name(suggested: Option<&str>) -> Option<String> {
    suggested
        .map(Path::new)
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
}

/// Ask the user for a path. `None` means they cancelled, which is an outcome and not a failure.
///
/// `suggested` is a path whose file name a save chooser opens with; the others ignore it.
pub fn choose(
    app: &AppHandle,
    choice: Choice,
    suggested: Option<&str>,
) -> Result<Option<String>, ChooserError> {
    let Some(path) = pick(app, choice, suggested)? else {
        // Every outcome is said out loud: nothing else outside the webview can see one, and the
        // check for BACKLOG N1c is built on these two lines.
        log::info!("chooser: the {} choice was cancelled", choice.as_str());
        return Ok(None);
    };
    // Lossy here would hand back a path that does not open. The user gets a sentence instead.
    let Some(text) = path.to_str() else {
        return Err(ChooserError::new(
            ChooserErrorCode::PathNotUtf8,
            format!("the chosen path is not valid UTF-8: {}", path.display()),
        ));
    };
    log::info!("chooser: chose a {}: {text}", choice.as_str());
    Ok(Some(text.to_owned()))
}

/// Raise the chooser on the main thread and wait here for what it answers.
///
/// This runs on the worker `blocking` put the command on, never on the main thread, which the wait
/// below would deadlock.
#[cfg(target_os = "linux")]
fn pick(
    app: &AppHandle,
    choice: Choice,
    suggested: Option<&str>,
) -> Result<Option<PathBuf>, ChooserError> {
    use gtk::prelude::*;
    use tauri::Manager;

    // On the main thread the post below runs inline and the wait at the end would then stop the
    // loop that has to answer it. A sentence beats a hung app.
    if gtk::is_initialized_main_thread() {
        return Err(ChooserError::new(
            ChooserErrorCode::ChooserFailed,
            "the file chooser cannot be opened from the main thread".to_owned(),
        ));
    }

    let (send, receive) = std::sync::mpsc::channel();
    let handle = app.clone();
    let name = save_name(suggested);
    app.run_on_main_thread(move || {
        // Transient and modal, which the plugin's chooser could not be: rfd builds with a null
        // parent, so it could end up behind the window that asked (BACKLOG N1).
        let parent = handle
            .get_webview_window("main")
            .and_then(|window| window.gtk_window().ok());
        if parent.is_none() {
            log::warn!("chooser: no GTK parent, the chooser cannot be transient");
        }
        let action = match choice {
            Choice::ProjectFolder => gtk::FileChooserAction::SelectFolder,
            _ if choice.is_save() => gtk::FileChooserAction::Save,
            _ => gtk::FileChooserAction::Open,
        };
        let dialog = gtk::FileChooserDialog::new(Some(choice.title()), parent.as_ref(), action);
        dialog.set_modal(true);
        // A window that closes under its chooser takes the chooser with it, and the guard below
        // turns that into a cancellation instead of leaving one on screen with nobody to answer.
        dialog.set_destroy_with_parent(true);
        if choice.is_save() {
            // Never silently replace a file the user already has. See CONTRIBUTING.md §3.
            dialog.set_do_overwrite_confirmation(true);
            if let Some(name) = name.as_deref() {
                dialog.set_current_name(name);
            }
        }
        // Mnemonics for the reason `ask_close` has them: a button reachable only by aiming a
        // pointer at it is one some users cannot press, and one a harness has to locate by
        // arithmetic. Alt+O and Alt+V, because Alt+S is the chooser's own search.
        let accept = if choice.is_save() {
            strings::CHOOSE_SAVE
        } else {
            strings::CHOOSE_ACCEPT
        };
        for (label, response) in [
            (strings::CHOOSE_CANCEL, gtk::ResponseType::Cancel),
            (accept, gtk::ResponseType::Accept),
        ] {
            // `add_button` hands back a Widget; the underline is a Button property, and GTK3
            // leaves it off unless it is asked for.
            if let Ok(button) = dialog.add_button(label, response).downcast::<gtk::Button>() {
                button.set_use_underline(true);
            }
        }
        // What activating a row in the list answers, so the chooser can be finished with Return.
        dialog.set_default_response(gtk::ResponseType::Accept);

        // `connect_response` takes an `Fn` and GTK can answer more than once — a button press
        // followed by the window manager closing the dialog — while only the first answer counts.
        let send = std::cell::RefCell::new(Some(send));
        dialog.connect_response(move |dialog, response| {
            let Some(send) = send.borrow_mut().take() else {
                return;
            };
            // Read before destroying: the path lives in the widget being torn down.
            let picked = match response {
                gtk::ResponseType::Accept => dialog.filename(),
                _ => None,
            };
            // Destroyed rather than hidden, so a cancelled chooser leaves nothing on screen.
            unsafe { dialog.destroy() };
            if send.send(picked).is_err() {
                log::error!("chooser: nobody was left waiting for the chosen path");
            }
        });
        // `show`, not `show_all`: a file chooser keeps internal widgets hidden on purpose.
        dialog.show();
    })
    .map_err(|error| {
        ChooserError::new(
            ChooserErrorCode::ChooserFailed,
            format!("the file chooser could not be raised: {error}"),
        )
    })?;

    Ok(answer(&receive))
}

/// What the chooser answered, or a cancellation when it went away without answering.
///
/// A closed channel is a chooser destroyed with its parent, or a task dropped by a main loop on the
/// way out. Cancelled is the answer that strands nobody: the caller returns, the panel stops being
/// busy, and the user can ask again. Its own function so a test can close the channel on it.
#[cfg(target_os = "linux")]
fn answer(receive: &std::sync::mpsc::Receiver<Option<PathBuf>>) -> Option<PathBuf> {
    receive.recv().unwrap_or_else(|_| {
        log::warn!("chooser: it went away without an answer, taking that as cancelled");
        None
    })
}

/// Every other platform keeps the plugin, exactly as `dialog::ask_close` does. Its `blocking_*`
/// posts to the main loop and waits for it, which is why this may never run on the main thread.
#[cfg(not(target_os = "linux"))]
fn pick(
    app: &AppHandle,
    choice: Choice,
    suggested: Option<&str>,
) -> Result<Option<PathBuf>, ChooserError> {
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app.dialog().file().set_title(choice.title());
    // A suggested name is the save chooser's question; the others are picking something that exists.
    if choice.is_save() {
        if let Some(name) = save_name(suggested) {
            dialog = dialog.set_file_name(name);
        }
    }
    let picked = match choice {
        Choice::ProjectFolder => dialog.blocking_pick_folder(),
        _ if choice.is_save() => dialog.blocking_save_file(),
        _ => dialog.blocking_pick_file(),
    };
    let Some(file) = picked else {
        return Ok(None);
    };
    // A URL that will not convert is a failure, not a cancellation: reading it as one would drop
    // the choice the user made without a word anywhere.
    file.simplified().into_path().map(Some).map_err(|error| {
        ChooserError::new(
            ChooserErrorCode::ChooserFailed,
            format!("the chosen path could not be read: {error}"),
        )
    })
}

/// The chooser blocks, so it never runs on the async runtime's poll thread (CONTRIBUTING.md §7).
/// On Linux it also must not run on the main thread, which `pick` refuses outright.
async fn blocking<T, F>(work: F) -> Result<T, ChooserError>
where
    F: FnOnce() -> Result<T, ChooserError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            ChooserError::new(
                ChooserErrorCode::ChooserFailed,
                format!("the chooser task failed: {error}"),
            )
        })?
}

#[tauri::command]
pub async fn choose_path(
    app: AppHandle,
    kind: String,
    suggested: Option<String>,
) -> Result<Option<String>, ChooserError> {
    // An unknown kind is a bug in the frontend, so nothing is raised and the caller is told.
    let Some(choice) = Choice::parse(&kind) else {
        return Err(ChooserError::new(
            ChooserErrorCode::ChooserFailed,
            format!("unknown chooser kind {kind:?}"),
        ));
    };
    blocking(move || choose(&app, choice, suggested.as_deref())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_the_frontend_sends_asks_for_its_own_thing_under_its_own_title() {
        let kinds = [
            "project-folder",
            "project-file",
            "video",
            "subtitle",
            "subtitle-save",
            "subtitle-first-save",
        ];
        let mut titles = Vec::new();
        for kind in kinds {
            let choice = Choice::parse(kind).expect("a kind the frontend sends");
            assert_eq!(choice.as_str(), kind, "the log word is the literal sent");
            titles.push(choice.title());
        }
        titles.sort_unstable();
        let mut unique = titles.clone();
        unique.dedup();
        assert_eq!(
            titles, unique,
            "two kinds cannot share a title: the harness finds the chooser by it"
        );
    }

    #[test]
    fn only_the_save_kinds_name_a_file_to_write() {
        for kind in ["subtitle-save", "subtitle-first-save"] {
            assert!(Choice::parse(kind).expect("a kind").is_save());
        }
        for kind in ["project-folder", "project-file", "video", "subtitle"] {
            assert!(
                !Choice::parse(kind).expect("a kind").is_save(),
                "{kind} picks something that exists"
            );
        }
    }

    #[test]
    fn a_kind_the_frontend_never_sends_is_refused_before_anything_opens() {
        assert!(Choice::parse("folder").is_none(), "the old literal is gone");
        assert!(Choice::parse("").is_none());
        assert!(Choice::parse("../etc/passwd").is_none());
    }

    /// The guard that keeps a caller from waiting forever on a chooser that went away.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_chooser_that_goes_away_without_answering_is_a_cancellation() {
        let (send, receive) = std::sync::mpsc::channel::<Option<PathBuf>>();
        drop(send);
        assert_eq!(answer(&receive), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_path_the_chooser_answered_is_the_one_the_caller_is_handed() {
        let (send, receive) = std::sync::mpsc::channel();
        send.send(Some(PathBuf::from("/tmp/chosen.srt")))
            .expect("the receiver is alive");
        assert_eq!(answer(&receive), Some(PathBuf::from("/tmp/chosen.srt")));
    }

    #[test]
    fn the_save_chooser_opens_on_the_file_name_and_never_on_the_whole_path() {
        assert_eq!(
            save_name(Some("/home/a/ep01.srt")).as_deref(),
            Some("ep01.srt")
        );
        // Windows hands back its own separator and the frontend passes the path through untouched,
        // so only a Windows `Path` splits this one. On Linux a backslash is a legal name character.
        let windows = save_name(Some(r"C:\media\ep01.srt"));
        let expected = if cfg!(windows) {
            "ep01.srt"
        } else {
            r"C:\media\ep01.srt"
        };
        assert_eq!(windows.as_deref(), Some(expected));
        assert_eq!(save_name(Some("ep01.srt")).as_deref(), Some("ep01.srt"));
        assert_eq!(save_name(Some("/home/a/")).as_deref(), Some("a"));
        assert_eq!(save_name(Some("")), None);
        assert_eq!(save_name(Some("/")), None);
        assert_eq!(save_name(None), None);
    }
}
