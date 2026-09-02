//! What was open when Sublore last ran, and the projects it has opened before. Decision 24, D5.
//!
//! Derived convenience rather than the user's own work — reopening a project regenerates all of it —
//! so it lives in the app's own store beside the models and the remembered chooser folders
//! (decision 20), never in a project folder. Losing the file costs one Open.
//!
//! Nothing here is worth failing a command over: an unreadable session is no session, said out loud
//! in the log and then ignored, exactly as `chooser.rs` treats its remembered folders.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::log;

const SESSION_FILE: &str = "projects.json";

/// How many projects File > Recent projects offers (decision 24, D5).
pub const RECENT_CAP: usize = 10;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Session {
    /// The project that was open, or none if the last thing the user did was close one.
    pub folder: Option<String>,
    /// The episode selected in that project. Meaningless without `folder`.
    pub episode_id: Option<i64>,
    /// Most recently opened first, at most [`RECENT_CAP`] of them.
    pub recent: Vec<String>,
}

pub fn read(app: &AppHandle) -> Session {
    let Some(path) = session_path(app) else {
        return Session::default();
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            // A first launch has no file, which is not something to say anything about.
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("project session: the file could not be read: {error}");
            }
            return Session::default();
        }
    };
    match serde_json::from_str::<Session>(&text) {
        Ok(session) => trim(session),
        Err(error) => {
            log::warn!("project session: the file is not readable JSON: {error}");
            Session::default()
        }
    }
}

/// Record a project as open, and put it at the head of the recent list.
pub fn opened(app: &AppHandle, folder: &Path) {
    let Some(folder) = folder.to_str() else {
        log::warn!("project session: a folder that is not UTF-8 is not remembered");
        return;
    };
    write(app, opened_in(read(app), folder));
}

fn opened_in(mut session: Session, folder: &str) -> Session {
    // The launch reopens the project that was open, so that open must keep the episode it was on;
    // a different project has no episode of its own here.
    if session.folder.as_deref() != Some(folder) {
        session.episode_id = None;
    }
    session.recent.retain(|entry| entry != folder);
    session.recent.insert(0, folder.to_owned());
    session.folder = Some(folder.to_owned());
    trim(session)
}

/// Record which episode is selected, so the next launch comes back to it.
pub fn selected(app: &AppHandle, episode_id: Option<i64>) {
    let mut session = read(app);
    if session.folder.is_none() || session.episode_id == episode_id {
        return;
    }
    session.episode_id = episode_id;
    write(app, session);
}

/// Nothing is open any more, so the next launch opens nothing. The recent list stands.
pub fn closed(app: &AppHandle) {
    let mut session = read(app);
    if session.folder.is_none() && session.episode_id.is_none() {
        return;
    }
    session.folder = None;
    session.episode_id = None;
    write(app, session);
}

/// The project at `folder` is gone: drop it from the recent list as well as from what is open.
/// This runs only where the user deleted it themselves, never because a folder looked absent.
pub fn forgotten(app: &AppHandle, folder: &Path) {
    let mut session = read(app);
    let gone = folder.to_str();
    session.recent.retain(|entry| Some(entry.as_str()) != gone);
    session.folder = None;
    session.episode_id = None;
    write(app, session);
}

/// The recent list holds ten, and holds no duplicates. Applied on the way in as well as on the way
/// out, because the file on disk is editable by hand.
fn trim(mut session: Session) -> Session {
    let mut seen = Vec::with_capacity(session.recent.len());
    session.recent.retain(|entry| {
        if entry.is_empty() || seen.iter().any(|kept| kept == entry) {
            return false;
        }
        seen.push(entry.clone());
        true
    });
    session.recent.truncate(RECENT_CAP);
    session
}

/// Written whole and renamed over the old one: a half-written file would be read back as no session
/// at all, and the next launch would open nothing with nothing said.
fn write(app: &AppHandle, session: Session) {
    let Some(path) = session_path(app) else {
        return;
    };
    if let Err(error) = write_to(&path, &session) {
        log::warn!("project session: it could not be stored: {error}");
    }
}

fn write_to(path: &Path, session: &Session) -> std::io::Result<()> {
    let text = serde_json::to_string(session).map_err(std::io::Error::other)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)
}

fn session_path(app: &AppHandle) -> Option<PathBuf> {
    use tauri::Manager;

    match app.path().app_data_dir() {
        Ok(dir) => Some(dir.join(SESSION_FILE)),
        Err(error) => {
            log::warn!("project session: no app data directory, so nothing is remembered: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{opened_in, trim, write_to, Session, RECENT_CAP};

    fn session(recent: &[&str]) -> Session {
        Session {
            folder: Some("/series/one".to_owned()),
            episode_id: Some(3),
            recent: recent.iter().map(|entry| (*entry).to_owned()).collect(),
        }
    }

    #[test]
    fn the_recent_list_keeps_ten_without_repeating_one() {
        let many: Vec<String> = (0..25).map(|n| format!("/series/{n}")).collect();
        let trimmed = trim(session(
            &many.iter().map(String::as_str).collect::<Vec<_>>(),
        ));
        assert_eq!(trimmed.recent.len(), RECENT_CAP);
        assert_eq!(trimmed.recent[0], "/series/0", "the head is the newest");

        let deduped = trim(session(&["/a", "/b", "/a", "", "/b"]));
        assert_eq!(deduped.recent, vec!["/a".to_owned(), "/b".to_owned()]);
    }

    #[test]
    fn reopening_the_project_that_was_open_keeps_the_episode_it_was_on() {
        // What the launch does: the frontend reads the session and then opens the project in it.
        // An open that cleared the episode here would lose the selection before it was restored.
        let same = opened_in(session(&["/series/one", "/series/two"]), "/series/one");
        assert_eq!(same.folder.as_deref(), Some("/series/one"));
        assert_eq!(same.episode_id, Some(3));
        assert_eq!(
            same.recent,
            vec!["/series/one".to_owned(), "/series/two".to_owned()],
            "already the newest, and not listed twice for it"
        );

        let other = opened_in(session(&["/series/one"]), "/series/two");
        assert_eq!(other.folder.as_deref(), Some("/series/two"));
        assert_eq!(
            other.episode_id, None,
            "another project's episode is not ours"
        );
        assert_eq!(
            other.recent,
            vec!["/series/two".to_owned(), "/series/one".to_owned()],
            "the newest goes to the head"
        );
    }

    /// A hand-edited file, a truncated one and a file from a future Sublore all have to read back
    /// as something usable, because the alternative is a launch that fails on a convenience.
    #[test]
    fn a_file_that_is_not_ours_reads_back_as_no_session() {
        for text in ["", "null", "{\"folder\": 7}", "{\"recent\": \"one\"}"] {
            assert!(
                serde_json::from_str::<Session>(text).is_err(),
                "{text:?} is not a session"
            );
        }
        // Unknown fields and missing ones are both fine: this file outlives the build that wrote it.
        let partial: Session =
            serde_json::from_str("{\"folder\": \"/series/one\", \"unknown\": 1}").expect("reads");
        assert_eq!(partial.folder.as_deref(), Some("/series/one"));
        assert_eq!(partial.episode_id, None);
        assert!(partial.recent.is_empty());
        // Serde reads a struct out of a sequence too, so `[]` is an empty session rather than a
        // failure. Recorded because it looks like a hole and is not one: it opens nothing.
        assert_eq!(
            serde_json::from_str::<Session>("[]").expect("reads"),
            Session::default()
        );
    }

    #[test]
    fn the_file_is_written_whole_and_leaves_no_temp_behind() {
        let dir = std::env::temp_dir().join(format!("sublore-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
        let path = dir.join("projects.json");
        let stored = session(&["/a"]);

        write_to(&path, &stored).expect("the session should be written");

        let read_back: Session =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("readable"))
                .expect("the written file is one we can read");
        assert_eq!(read_back, stored);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "the temp file is renamed over the session, never left beside it"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
