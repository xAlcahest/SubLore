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
//!
//! Each kind of chooser opens where that kind last landed (BACKLOG N7). One folder per kind, not one
//! for all five: a translator who has just attached a subtitle to an episode and then opens a video
//! is not in the same folder, and a single memory would send them back to the wrong one every other
//! gesture.

use std::collections::BTreeMap;
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
enum Choice {
    ProjectFolder,
    ProjectFile,
    Video,
    Subtitle,
    SubtitleSave,
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
        }
    }

    /// Naming a file to write is a different question from picking one that exists, and the chooser
    /// asks it differently: a filename field, and a warning before an overwrite.
    fn is_save(self) -> bool {
        matches!(self, Self::SubtitleSave)
    }

    /// The word the log uses, which is also the literal the frontend sent.
    fn as_str(self) -> &'static str {
        match self {
            Self::ProjectFolder => "project-folder",
            Self::ProjectFile => "project-file",
            Self::Video => "video",
            Self::Subtitle => "subtitle",
            Self::SubtitleSave => "subtitle-save",
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

/// Where each kind of chooser last landed. Derived convenience rather than the user's own data, so
/// it lives in the app's own store beside the models and the scratch (decision 20), never in a
/// project folder. Losing it costs a translator one navigation.
const MEMORY_FILE: &str = "chooser-folders.json";

/// The remembered folders, keyed by [`Choice::as_str`]. Nothing here is worth failing a chooser
/// over: an unreadable memory is no memory, said out loud and then ignored.
fn memory(app: &AppHandle) -> BTreeMap<String, String> {
    let Some(path) = memory_path(app) else {
        return BTreeMap::new();
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            // A first launch has no file, which is not something to say anything about.
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("chooser: the remembered folders could not be read: {error}");
            }
            return BTreeMap::new();
        }
    };
    serde_json::from_str(&text).unwrap_or_else(|error| {
        log::warn!("chooser: the remembered folders are not readable JSON: {error}");
        BTreeMap::new()
    })
}

fn memory_path(app: &AppHandle) -> Option<PathBuf> {
    use tauri::Manager;

    match app.path().app_data_dir() {
        Ok(dir) => Some(dir.join(MEMORY_FILE)),
        Err(error) => {
            log::warn!("chooser: no app data directory, so no folder is remembered: {error}");
            None
        }
    }
}

/// The folder this kind of chooser opens at, or `None` for the platform's own default.
///
/// A remembered folder that has been moved or deleted since is dropped here: handing it to the
/// chooser would open on a folder that is not there (BACKLOG N7).
fn opens_at(folders: &BTreeMap<String, String>, choice: Choice) -> Option<PathBuf> {
    let folder = PathBuf::from(folders.get(choice.as_str())?);
    if !folder.is_dir() {
        log::info!(
            "chooser: the {} chooser's remembered folder is gone, opening at the default: {}",
            choice.as_str(),
            folder.display()
        );
        return None;
    }
    Some(folder)
}

/// The folder to remember for this kind, given what the user chose. A folder chooser answers with
/// the folder itself; every other kind answers with a file inside one.
fn folder_of(choice: Choice, chosen: &Path) -> Option<&Path> {
    if choice == Choice::ProjectFolder {
        Some(chosen)
    } else {
        chosen.parent()
    }
}

/// Store where this kind of chooser landed. Only a chosen path reaches here, so a cancelled chooser
/// leaves the memory exactly as it was (BACKLOG N7).
fn remember(app: &AppHandle, choice: Choice, chosen: &Path) {
    let (Some(path), Some(folder)) = (memory_path(app), folder_of(choice, chosen)) else {
        return;
    };
    let Some(folder) = folder.to_str() else {
        return;
    };
    let mut folders = memory(app);
    folders.insert(choice.as_str().to_owned(), folder.to_owned());
    if let Err(error) = write_memory(&path, &folders) {
        log::warn!(
            "chooser: where the {} chooser landed could not be stored: {error}",
            choice.as_str()
        );
    }
}

/// Written whole and renamed over the old one: a half-written file would be read back as no memory
/// at all, and the next chooser would open on the default with nothing said.
fn write_memory(path: &Path, folders: &BTreeMap<String, String>) -> std::io::Result<()> {
    let text = serde_json::to_string(folders).map_err(std::io::Error::other)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)
}

/// Ask the user for a path. `None` means they cancelled, which is an outcome and not a failure.
///
/// `suggested` is a path whose file name a save chooser opens with; the others ignore it.
pub fn choose(
    app: &AppHandle,
    kind: &str,
    suggested: Option<&str>,
) -> Result<Option<String>, ChooserError> {
    let Some(choice) = Choice::parse(kind) else {
        return Err(ChooserError::new(
            ChooserErrorCode::ChooserFailed,
            format!("unknown chooser kind {kind:?}"),
        ));
    };
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
    remember(app, choice, &path);
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
    // Read here rather than in the closure below: that one runs on the main thread, where a file
    // read is a stall of the whole interface.
    let folder = opens_at(&memory(app), choice);
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
            Choice::SubtitleSave => gtk::FileChooserAction::Save,
            _ => gtk::FileChooserAction::Open,
        };
        let dialog = gtk::FileChooserDialog::new(Some(choice.title()), parent.as_ref(), action);
        dialog.set_modal(true);
        // A window that closes under its chooser takes the chooser with it, and the guard below
        // turns that into a cancellation instead of leaving one on screen with nobody to answer.
        dialog.set_destroy_with_parent(true);
        // Before the name below: GTK builds a save chooser's answer from the current folder and the
        // current name, in that order.
        if let Some(folder) = folder.as_deref() {
            if !dialog.set_current_folder(folder) {
                log::warn!("chooser: GTK refused to open at {}", folder.display());
            }
        }
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
    if let Some(folder) = opens_at(&memory(app), choice) {
        dialog = dialog.set_directory(folder);
    }
    // A suggested name is the save chooser's question; the others are picking something that exists.
    if choice.is_save() {
        if let Some(name) = save_name(suggested) {
            dialog = dialog.set_file_name(name);
        }
    }
    let picked = match choice {
        Choice::ProjectFolder => dialog.blocking_pick_folder(),
        Choice::SubtitleSave => dialog.blocking_save_file(),
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
    blocking(move || choose(&app, &kind, suggested.as_deref())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of this test's own, removed when it returns. No `tempfile` dependency for four
    /// tests that need a path that exists and a path that does not.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("sublore-chooser-{}-{name}", std::process::id()));
            std::fs::remove_dir_all(&path).ok();
            std::fs::create_dir_all(&path).expect("a directory under the temp dir");
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        fn subdir(&self, name: &str) -> PathBuf {
            let path = self.join(name);
            std::fs::create_dir_all(&path).expect("a directory under the temp dir");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn folders(entries: &[(Choice, &Path)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(choice, path)| {
                (
                    choice.as_str().to_owned(),
                    path.to_str().expect("a UTF-8 temp path").to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn every_kind_the_frontend_sends_asks_for_its_own_thing_under_its_own_title() {
        let kinds = [
            "project-folder",
            "project-file",
            "video",
            "subtitle",
            "subtitle-save",
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
    fn only_the_save_kind_names_a_file_to_write() {
        assert!(Choice::parse("subtitle-save").expect("a kind").is_save());
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

    #[test]
    fn a_folder_chooser_remembers_the_folder_and_every_other_kind_the_one_it_chose_in() {
        assert_eq!(
            folder_of(Choice::ProjectFolder, Path::new("/media/series")),
            Some(Path::new("/media/series")),
            "the folder chooser answers with the folder itself"
        );
        for kind in ["project-file", "video", "subtitle", "subtitle-save"] {
            let choice = Choice::parse(kind).expect("a kind the frontend sends");
            assert_eq!(
                folder_of(choice, Path::new("/media/series/ep01.srt")),
                Some(Path::new("/media/series")),
                "{kind} answers with a file, and the folder is where it was"
            );
        }
        // Nothing to remember rather than a panic on a path with no parent.
        assert_eq!(folder_of(Choice::Video, Path::new("/")), None);
    }

    #[test]
    fn each_kind_opens_where_that_kind_landed_and_not_where_another_one_did() {
        let temp = TempDir::new("per-kind");
        let videos = temp.subdir("videos");
        let subtitles = temp.subdir("subtitles");
        let stored = folders(&[(Choice::Video, &videos), (Choice::Subtitle, &subtitles)]);

        assert_eq!(opens_at(&stored, Choice::Video), Some(videos));
        assert_eq!(opens_at(&stored, Choice::Subtitle), Some(subtitles));
        assert_eq!(
            opens_at(&stored, Choice::ProjectFolder),
            None,
            "a kind that has never been used has nowhere of its own to open"
        );
    }

    #[test]
    fn a_remembered_folder_that_is_no_longer_there_opens_the_chooser_at_its_default() {
        let temp = TempDir::new("gone");
        let gone = temp.join("moved-away");
        let file = temp.join("ep01.srt");
        std::fs::write(&file, "1\n").expect("a file under the temp dir");
        let stored = folders(&[(Choice::ProjectFolder, &gone), (Choice::Video, &file)]);

        assert_eq!(opens_at(&stored, Choice::ProjectFolder), None);
        assert_eq!(
            opens_at(&stored, Choice::Video),
            None,
            "a path that is now a file is not a folder to open at"
        );
    }

    #[test]
    fn the_folders_written_are_the_folders_read_back() {
        let temp = TempDir::new("round-trip");
        // A name only JSON survives: newlines and quotes are legal in a Linux path.
        let awkward = temp.subdir("two\nlines \"quoted\"");
        let stored = folders(&[(Choice::Subtitle, &awkward)]);
        let path = temp.join("chooser-folders.json");

        write_memory(&path, &stored).expect("the memory is written");
        let text = std::fs::read_to_string(&path).expect("the memory is on disk");
        let read: BTreeMap<String, String> = serde_json::from_str(&text).expect("readable JSON");

        assert_eq!(read, stored);
        assert_eq!(opens_at(&read, Choice::Subtitle), Some(awkward));
        assert!(
            !path.with_extension("json.tmp").exists(),
            "the temp file is renamed over the memory, never left beside it"
        );
    }
}
