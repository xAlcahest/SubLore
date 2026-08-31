//! The project a user has open: create it, open it, add episodes, attach files by path, delete it.
//! One project at a time, held behind a mutex because a SQLite connection is `Send` and not `Sync`.
//! The IPC names and payloads here are a public interface (CLAUDE.md §6). See BACKLOG.md M4.4.
//!
//! Nothing in this module writes to a user's media or subtitle file. Attaching records a path.

pub mod error;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use serde::Serialize;
use sublore_project::delete::delete_project;
use sublore_project::model::FileRole;
use sublore_project::records::{self, Project};
use tauri::{AppHandle, State};

use crate::log;
use crate::strings;
use error::{ProjectError, ProjectErrorCode};

/// The title a project gets when its folder has no usable name, which only a filesystem root has.
/// Stored data, like a file name, not UI chrome: the user can see it and Sublore never reads it.
const UNTITLED: &str = "Untitled";

/// The open project, or none. Commands clone the `Arc` into `spawn_blocking`, the way
/// `VideoState::player` does, because a `State` borrow cannot cross an await.
pub type SharedProject = Arc<Mutex<Option<Project>>>;

#[derive(Default)]
pub struct ProjectState {
    open: SharedProject,
}

impl ProjectState {
    fn handle(&self) -> SharedProject {
        Arc::clone(&self.open)
    }

    /// Close the database on the way out, so a normal quit checkpoints the WAL and leaves one file
    /// behind instead of three. Idempotent: every shutdown event may fire, only the first works.
    pub fn shutdown(&self) {
        // A poisoned lock still guards a valid project, and leaving the database open would be the
        // worse outcome of the two at shutdown.
        close_taken(match self.open.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub folder: String,
    pub title: String,
    pub schema_version: u32,
    pub episodes: Vec<EpisodeView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeView {
    pub id: i64,
    pub ordinal: u32,
    pub title: String,
    pub files: Vec<EpisodeFileView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeFileView {
    pub id: i64,
    /// "media" | "source" | "target".
    pub role: String,
    pub path: String,
    /// Absent when the size could not be read when the file was attached.
    pub byte_length: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDeletedView {
    pub folder: String,
    /// Sublore's own files that were removed. Absolute paths.
    pub removed: Vec<String>,
    /// Sublore's own names that were there and were left alone.
    pub kept: Vec<String>,
}

#[tauri::command]
pub async fn project_create(
    state: State<'_, ProjectState>,
    folder: String,
) -> Result<ProjectView, ProjectError> {
    let slot = state.handle();
    blocking("create", move || create(&slot, &folder)).await
}

#[tauri::command]
pub async fn project_open(
    state: State<'_, ProjectState>,
    folder: String,
) -> Result<ProjectView, ProjectError> {
    let slot = state.handle();
    blocking("open", move || open(&slot, &folder)).await
}

#[tauri::command]
pub async fn project_add_episode(
    state: State<'_, ProjectState>,
    title: String,
) -> Result<ProjectView, ProjectError> {
    let slot = state.handle();
    blocking("add episode", move || add_episode(&slot, &title)).await
}

#[tauri::command]
pub async fn project_attach_file(
    state: State<'_, ProjectState>,
    episode_id: i64,
    role: String,
    path: String,
) -> Result<ProjectView, ProjectError> {
    let slot = state.handle();
    blocking("attach file", move || {
        attach_file(&slot, episode_id, &role, &path)
    })
    .await
}

/// Takes no folder: it deletes the project that is open. The frontend therefore cannot name a
/// directory for Sublore to delete. See BACKLOG.md M4.3.
#[tauri::command]
pub async fn project_delete(
    state: State<'_, ProjectState>,
) -> Result<ProjectDeletedView, ProjectError> {
    let slot = state.handle();
    blocking("delete", move || delete(&slot)).await
}

#[tauri::command]
pub async fn project_choose_path(
    app: AppHandle,
    kind: String,
) -> Result<Option<String>, ProjectError> {
    blocking("choose path", move || choose_path(&app, &kind)).await
}

/// Every command's body runs here: SQLite calls and native dialogs both block, so neither ever
/// runs on the async runtime's poll thread (CLAUDE.md §7).
async fn blocking<T, F>(what: &'static str, work: F) -> Result<T, ProjectError>
where
    F: FnOnce() -> Result<T, ProjectError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            ProjectError::new(
                ProjectErrorCode::CommandFailed,
                format!("{what} task failed: {error}"),
            )
        })?
}

/// Create a project in an existing folder. Sublore never makes directories in a user's filesystem,
/// so the folder has to be there already.
pub fn create(slot: &SharedProject, folder: &str) -> Result<ProjectView, ProjectError> {
    let path = folder_path(folder)?;
    let title = default_title(&path);
    close(slot)?;
    let project =
        Project::create(&path, &title, SystemTime::now()).map_err(ProjectError::from_crate)?;
    adopt(slot, project)
}

pub fn open(slot: &SharedProject, folder: &str) -> Result<ProjectView, ProjectError> {
    let path = folder_path(folder)?;
    close(slot)?;
    let project = Project::open(&path).map_err(ProjectError::from_crate)?;
    adopt(slot, project)
}

pub fn add_episode(slot: &SharedProject, title: &str) -> Result<ProjectView, ProjectError> {
    with_open(slot, |project| {
        records::add_episode(project, title, SystemTime::now())
            .map(|_| ())
            .map_err(ProjectError::from_crate)
    })
}

/// Record a file against an episode. The file is read for its size and nothing else: never opened
/// for writing, never copied, never moved. See CLAUDE.md §3.1.
pub fn attach_file(
    slot: &SharedProject,
    episode_id: i64,
    role: &str,
    path: &str,
) -> Result<ProjectView, ProjectError> {
    // The frontend sends one of three literals, so an unknown one is a bug in it, not a user error.
    let role = FileRole::parse(role).ok_or_else(|| {
        ProjectError::new(
            ProjectErrorCode::CommandFailed,
            format!("unknown file role {role:?}"),
        )
    })?;
    with_open(slot, |project| {
        records::attach_file(
            project,
            episode_id,
            role,
            Path::new(path),
            SystemTime::now(),
        )
        .map(|_| ())
        .map_err(ProjectError::from_crate)
    })
}

/// Close the open project and remove Sublore's own files from its folder. Nothing the project
/// pointed at is touched, and the folder itself stays. See BACKLOG.md M4.3.
pub fn delete(slot: &SharedProject) -> Result<ProjectDeletedView, ProjectError> {
    let project = lock(slot)?.take().ok_or_else(no_project_open)?;
    let folder = project.summary().folder.clone();
    // The handle has to go before the file can: on Windows an open database cannot be removed.
    close_taken(Some(project));

    let outcome = delete_project(&folder).map_err(ProjectError::from_crate)?;
    Ok(ProjectDeletedView {
        folder: text(&folder),
        removed: outcome.removed.iter().map(|path| text(path)).collect(),
        kept: outcome
            .left_in_place
            .iter()
            .map(|path| text(path))
            .collect(),
    })
}

/// What the picker was asked for. Parsed once, so no platform branch matches on a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Folder,
    File,
}

impl Choice {
    /// The frontend sends one of two literals, so an unknown one is a bug in it, not a user error.
    fn parse(kind: &str) -> Result<Self, ProjectError> {
        match kind {
            "folder" => Ok(Self::Folder),
            "file" => Ok(Self::File),
            _ => Err(ProjectError::new(
                ProjectErrorCode::CommandFailed,
                format!("unknown dialog kind {kind:?}"),
            )),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Folder => strings::CHOOSE_PROJECT_FOLDER,
            Self::File => strings::CHOOSE_PROJECT_FILE,
        }
    }

    /// The word the log uses, which is also the literal the frontend sent.
    fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::File => "file",
        }
    }
}

/// Ask the user for a folder or a file through the native dialog. `None` means they cancelled.
pub fn choose_path(app: &AppHandle, kind: &str) -> Result<Option<String>, ProjectError> {
    let choice = Choice::parse(kind)?;
    let Some(path) = pick(app, choice)? else {
        // Every outcome is said out loud: nothing else outside the webview can see one, and the
        // check for BACKLOG N1c is built on these two lines.
        log::info!("project: the {} choice was cancelled", choice.as_str());
        return Ok(None);
    };
    // Lossy here would hand back a path that does not open. The user gets a sentence instead.
    let Some(text) = path.to_str() else {
        return Err(ProjectError::new(
            ProjectErrorCode::PathNotUtf8,
            format!("the chosen path is not valid UTF-8: {}", path.display()),
        ));
    };
    log::info!("project: chose a {}: {text}", choice.as_str());
    Ok(Some(text.to_owned()))
}

/// Raise the chooser on the main thread and wait here for what it answers.
///
/// GTK directly rather than through tauri-plugin-dialog, for the reason `dialog.rs` gives: the
/// plugin's rfd backend starts a second thread and iterates GTK on it for the rest of the process's
/// life (BACKLOG N1c). This runs on the worker `blocking` put the command on, never on the main
/// thread, which the wait below would deadlock.
#[cfg(target_os = "linux")]
fn pick(app: &AppHandle, choice: Choice) -> Result<Option<PathBuf>, ProjectError> {
    use gtk::prelude::*;
    use tauri::Manager;

    // On the main thread the post below runs inline and the wait at the end would then stop the
    // loop that has to answer it. A sentence beats a hung app.
    if gtk::is_initialized_main_thread() {
        return Err(ProjectError::new(
            ProjectErrorCode::CommandFailed,
            "the file chooser cannot be opened from the main thread".to_owned(),
        ));
    }

    let (send, receive) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        // Transient and modal, which the plugin's chooser could not be: rfd builds with a null
        // parent, so it could end up behind the window that asked (BACKLOG N1).
        let parent = handle
            .get_webview_window("main")
            .and_then(|window| window.gtk_window().ok());
        if parent.is_none() {
            log::warn!("project: no GTK parent for the chooser, it cannot be transient");
        }
        let action = match choice {
            Choice::Folder => gtk::FileChooserAction::SelectFolder,
            Choice::File => gtk::FileChooserAction::Open,
        };
        let dialog = gtk::FileChooserDialog::new(Some(choice.title()), parent.as_ref(), action);
        dialog.set_modal(true);
        // A window that closes under its chooser takes the chooser with it, and the guard below
        // turns that into a cancellation instead of leaving one on screen with nobody to answer.
        dialog.set_destroy_with_parent(true);
        // Mnemonics for the reason `ask_close` has them: a button reachable only by aiming a
        // pointer at it is one some users cannot press, and one a harness has to locate by
        // arithmetic. Alt+O, because Alt+S is the chooser's own search.
        for (label, response) in [
            (strings::CHOOSE_CANCEL, gtk::ResponseType::Cancel),
            (strings::CHOOSE_ACCEPT, gtk::ResponseType::Accept),
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
                log::error!("project: nobody was left waiting for the chosen path");
            }
        });
        // `show`, not `show_all`: a file chooser keeps internal widgets hidden on purpose.
        dialog.show();
    })
    .map_err(|error| {
        ProjectError::new(
            ProjectErrorCode::CommandFailed,
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
        log::warn!("project: the chooser went away without an answer, taking it as cancelled");
        None
    })
}

/// Every other platform keeps the plugin, exactly as `dialog::ask_close` does. Its `blocking_pick_*`
/// posts to the main loop and waits for it, which is why this may never run on the main thread.
#[cfg(not(target_os = "linux"))]
fn pick(app: &AppHandle, choice: Choice) -> Result<Option<PathBuf>, ProjectError> {
    use tauri_plugin_dialog::DialogExt;

    let dialog = app.dialog().file().set_title(choice.title());
    let picked = match choice {
        Choice::Folder => dialog.blocking_pick_folder(),
        Choice::File => dialog.blocking_pick_file(),
    };
    let Some(file) = picked else {
        return Ok(None);
    };
    // A URL that will not convert is a failure, not a cancellation: reading it as one would drop
    // the choice the user made without a word anywhere.
    file.simplified().into_path().map(Some).map_err(|error| {
        ProjectError::new(
            ProjectErrorCode::CommandFailed,
            format!("the chosen path could not be read: {error}"),
        )
    })
}

/// Everything the frontend draws, read back after every change. A handful of rows, and returning
/// all of them removes a class of stale-state bugs the deltas would introduce.
fn view(project: &Project) -> Result<ProjectView, ProjectError> {
    let episodes = records::episodes(project).map_err(ProjectError::from_crate)?;
    let mut listed = Vec::with_capacity(episodes.len());
    for episode in episodes {
        let files = records::files(project, episode.id).map_err(ProjectError::from_crate)?;
        listed.push(EpisodeView {
            id: episode.id,
            ordinal: episode.ordinal,
            title: episode.title,
            files: files
                .into_iter()
                .map(|file| EpisodeFileView {
                    id: file.id,
                    role: file.role.as_str().to_owned(),
                    path: text(&file.path),
                    byte_length: file.byte_length,
                })
                .collect(),
        });
    }

    let summary = project.summary();
    Ok(ProjectView {
        folder: text(&summary.folder),
        title: summary.title.clone(),
        schema_version: summary.schema_version,
        episodes: listed,
    })
}

/// Run `work` against the open project and report what the project looks like afterwards.
fn with_open<F>(slot: &SharedProject, work: F) -> Result<ProjectView, ProjectError>
where
    F: FnOnce(&mut Project) -> Result<(), ProjectError>,
{
    let mut guard = lock(slot)?;
    let project = guard.as_mut().ok_or_else(no_project_open)?;
    work(project)?;
    view(project)
}

fn adopt(slot: &SharedProject, project: Project) -> Result<ProjectView, ProjectError> {
    let view = view(&project)?;
    *lock(slot)? = Some(project);
    Ok(view)
}

/// Close whatever is open and leave the slot empty. A close that fails has released the connection
/// anyway, and the user asked for the next project, not for a report about the last one.
pub fn close(slot: &SharedProject) -> Result<(), ProjectError> {
    close_taken(lock(slot)?.take());
    Ok(())
}

fn close_taken(project: Option<Project>) {
    if let Some(project) = project {
        if let Err(error) = project.close() {
            log::warn!("closing the project failed: {error}");
        }
    }
}

/// A poisoned lock means a command panicked while holding it. That is our bug, not the user's.
fn lock(slot: &SharedProject) -> Result<MutexGuard<'_, Option<Project>>, ProjectError> {
    slot.lock().map_err(|error| {
        ProjectError::new(
            ProjectErrorCode::CommandFailed,
            format!("the project lock is poisoned: {error}"),
        )
    })
}

fn no_project_open() -> ProjectError {
    ProjectError::new(ProjectErrorCode::NoProjectOpen, "no project is open")
}

fn folder_path(folder: &str) -> Result<PathBuf, ProjectError> {
    if folder.trim().is_empty() {
        return Err(ProjectError::new(
            ProjectErrorCode::InvalidPath,
            "the project folder path is empty",
        ));
    }
    Ok(PathBuf::from(folder))
}

/// The folder's own name, which is what a translator already calls the series. No title field:
/// M4.4 is "no editing beyond that".
fn default_title(folder: &Path) -> String {
    folder
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(UNTITLED)
        .to_owned()
}

/// Paths reach this layer as UTF-8 and the database column is text, so nothing is lost here. Lossy
/// rather than fallible because a view is built after the work already succeeded.
fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::{default_title, folder_path, Choice, UNTITLED};
    use crate::project::error::ProjectErrorCode;
    use crate::strings;
    use std::path::Path;

    #[test]
    fn a_project_is_named_after_its_folder() {
        assert_eq!(
            default_title(Path::new("/home/user/Series One")),
            "Series One"
        );
        assert_eq!(default_title(Path::new("/home/user/Сериал")), "Сериал");
    }

    #[test]
    fn a_folder_with_no_name_of_its_own_still_gets_a_title() {
        assert_eq!(default_title(Path::new("/")), UNTITLED);
        assert_eq!(default_title(Path::new("")), UNTITLED);
    }

    #[test]
    fn an_empty_folder_path_is_refused_before_anything_opens() {
        for folder in ["", "   ", "\t"] {
            let error = folder_path(folder).expect_err("an empty folder path is refused");
            assert_eq!(error.code, ProjectErrorCode::InvalidPath, "{folder:?}");
        }
    }

    #[test]
    fn each_dialog_kind_asks_for_its_own_thing_under_its_own_title() {
        let folder = Choice::parse("folder").expect("the frontend's folder literal");
        let file = Choice::parse("file").expect("the frontend's file literal");
        assert_eq!(folder.title(), strings::CHOOSE_PROJECT_FOLDER);
        assert_eq!(file.title(), strings::CHOOSE_PROJECT_FILE);
        assert_eq!(folder.as_str(), "folder");
        assert_eq!(file.as_str(), "file");
    }

    #[test]
    fn a_dialog_kind_the_frontend_never_sends_is_refused_before_anything_opens() {
        for kind in ["", "Folder", "directory", "file ", "media"] {
            let error = Choice::parse(kind).expect_err("an unknown dialog kind is refused");
            assert_eq!(error.code, ProjectErrorCode::CommandFailed, "{kind:?}");
        }
    }

    /// The guard between "the window closed under the chooser" and a project panel that stays busy
    /// for the rest of the session. `dialog.rs` carries the same pair for the close gate.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_chooser_that_goes_away_without_answering_is_a_cancellation() {
        let (send, receive) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
        // The chooser destroyed with its parent, or its task dropped by a main loop on the way out.
        drop(send);

        assert_eq!(super::answer(&receive), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_path_the_chooser_answered_is_the_one_the_caller_is_handed() {
        let chosen = std::path::PathBuf::from("/tmp/a folder the user picked");
        let (send, receive) = std::sync::mpsc::channel();
        send.send(Some(chosen.clone()))
            .expect("the caller is listening");

        assert_eq!(super::answer(&receive), Some(chosen));
    }

    #[test]
    fn a_folder_path_is_taken_exactly_as_the_user_spelled_it() {
        // Not trimmed, not resolved: a name may legitimately end in a space on Linux.
        assert_eq!(
            folder_path("/home/user/Series One ").expect("a real path is accepted"),
            Path::new("/home/user/Series One ")
        );
    }
}
