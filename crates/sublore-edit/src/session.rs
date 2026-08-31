//! One open subtitle file: the document, its undo stack, and where it came from.
//!
//! This is where the mutation API (M2.1) and the history (M2.2) meet, and it is deliberately the
//! only place they do. It holds no Tauri type and does no I/O, so the whole editing state machine
//! is testable with `cargo test -p sublore-edit`, without a display. See BACKLOG.md M2.3.

use std::path::{Path, PathBuf};
use std::time::Instant;

use sublore_formats::SubtitleDocument;

use crate::diff::{self, CuePatch, CueView};
use crate::error::EditError;
use crate::history::{History, Run};
use crate::plan::{self, Edit};

pub struct EditSession {
    path: PathBuf,
    document: SubtitleDocument,
    history: History,
    /// The list as the UI last saw it, so a patch is a diff of two lists rather than a re-walk.
    views: Vec<CueView>,
    revision: u64,
}

impl EditSession {
    /// A file as it was opened: nothing to undo, nothing unsaved.
    pub fn open(path: PathBuf, document: SubtitleDocument) -> Self {
        let views = diff::views(&document);
        Self {
            path,
            document,
            history: History::new(),
            views,
            revision: 0,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn document(&self) -> &SubtitleDocument {
        &self.document
    }

    pub fn views(&self) -> &[CueView] {
        &self.views
    }

    /// Bumped on every accepted mutation, undo and redo. The UI echoes it back, so a click made
    /// against a list that has since moved is refused rather than applied to the wrong cue.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn dirty(&self) -> bool {
        self.history.dirty()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// True once the undo bound dropped an entry: the file as opened is no longer reachable.
    pub fn truncated(&self) -> bool {
        self.history.truncated()
    }

    /// Plan, apply, verify, record. On any failure the session is exactly as it was: the document,
    /// the list, the history and the revision all still describe the same bytes (CONTRIBUTING.md §3).
    ///
    /// `run` says whether this edit continues the one before it; see [`Run`].
    pub fn apply(&mut self, edit: &Edit, run: Run, now: Instant) -> Result<CuePatch, EditError> {
        let edited = plan::edit(&self.document, edit)?;
        // Committing a field the user did not change must not dirty the file or grow the stack.
        if edited.splice.is_noop() {
            return Ok(CuePatch {
                from: 0,
                removed: 0,
                cues: Vec::new(),
            });
        }

        self.history
            .record(edited.splice, edited.label, edited.cue_delta, run, now);
        Ok(self.commit(edited.document))
    }

    /// The step below the cursor, replayed backwards. `Ok(None)` at the bottom of the stack: there
    /// being nothing to undo is an answer, not a failure.
    pub fn undo(&mut self) -> Result<Option<CuePatch>, EditError> {
        let Some(step) = self.history.undo() else {
            return Ok(None);
        };
        match plan::replay(&self.document, &step.splice, step.cue_delta) {
            Ok(document) => Ok(Some(self.commit(document))),
            Err(error) => {
                // A replay that did not land leaves the entry where it was, so the cursor goes back.
                self.history.redo();
                Err(error)
            }
        }
    }

    /// The step at the cursor, replayed forwards. Same rule as [`Self::undo`], mirrored.
    pub fn redo(&mut self) -> Result<Option<CuePatch>, EditError> {
        let Some(step) = self.history.redo() else {
            return Ok(None);
        };
        match plan::replay(&self.document, &step.splice, step.cue_delta) {
            Ok(document) => Ok(Some(self.commit(document))),
            Err(error) => {
                self.history.undo();
                Err(error)
            }
        }
    }

    /// The file this session would write. Byte for byte what a save puts on disk.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.document.to_bytes()
    }

    /// The bytes in hand are now the bytes on disk.
    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }

    /// Take a verified document, and report the run of rows that changed.
    fn commit(&mut self, document: SubtitleDocument) -> CuePatch {
        self.document = document;
        let after = diff::views(&self.document);
        let patch = diff::patch(&self.views, &after);
        self.views = after;
        self.revision = self.revision.saturating_add(1);
        patch
    }
}
