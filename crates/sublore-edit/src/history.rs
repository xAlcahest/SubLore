//! The undo stack: a list of byte splices with a cursor over it.
//!
//! Splices rather than snapshots, because a splice can prove it still applies: `splice::apply`
//! refuses unless the bytes it recorded are the bytes that are there, so a stale entry is an error
//! message and never a wrong write (CONTRIBUTING.md §3). Snapshots cannot fail, which means they can
//! silently restore the wrong thing.
//!
//! The stack knows nothing about subtitles: it holds byte replacements and the label they were
//! made under, and that is the whole reason it is testable on its own. See BACKLOG.md M2.2.
//!
//! Which edits belong to one run is the caller's word ([`Run`]), never read out of the bytes: a
//! keystroke and a second finished edit of the same field are the same replacement from in here.

use std::time::{Duration, Instant};

use crate::splice::{EditLabel, Splice};

/// Entries kept. Past this the oldest is dropped, and edits below it can no longer be undone.
pub const MAX_ENTRIES: usize = 200;
/// Total splice bytes kept. Whichever bound is hit first drops from the bottom.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
/// Consecutive edits with the same label merge when they land within this window of each other.
pub const COALESCE_WINDOW: Duration = Duration::from_millis(1_000);

/// Whether an edit continues the interaction that produced the previous one. Only the caller knows:
/// one more keystroke and a second finished edit of the same field are the same bytes here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    /// A finished edit. Always its own undo step, whatever it looks like.
    New,
    /// One more step of the run above it, so the two may merge.
    Continues,
}

/// One recorded edit. `at` is passed in, never read from the clock in here, so tests are exact.
#[derive(Clone, Debug)]
pub struct Entry {
    pub splice: Splice,
    pub label: EditLabel,
    pub cue_delta: isize,
    pub at: Instant,
}

/// One step to replay, in the direction asked for. An undo step carries the inverted splice and
/// the negated delta, so the caller replays it without knowing which direction it came from.
#[derive(Clone, Debug)]
pub struct Step {
    pub splice: Splice,
    pub cue_delta: isize,
}

#[derive(Debug)]
pub struct History {
    entries: Vec<Entry>,
    /// How many entries are applied to the document: `undo` takes the one below the cursor,
    /// `redo` the one at it.
    cursor: usize,
    /// Cursor value at the last save. `None` once that position stopped being reachable, which
    /// means the document can no longer be proven saved.
    saved: Option<usize>,
    /// Sum of the entries' splice weights, for [`MAX_BYTES`].
    bytes: usize,
    truncated: bool,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    /// A history for a file as it was opened: nothing to undo, and not dirty.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            saved: Some(0),
            bytes: 0,
            truncated: false,
        }
    }

    /// Record an edit that has already been applied to the document: drop what was undone, merge
    /// into the entry above when `run` and [`Self::merge_target`] both allow, then hold both bounds.
    pub fn record(
        &mut self,
        splice: Splice,
        label: EditLabel,
        cue_delta: isize,
        run: Run,
        now: Instant,
    ) {
        self.drop_redo_tail();
        let target = match run {
            Run::New => None,
            Run::Continues => self.merge_target(&splice, label, now),
        };
        match target.and_then(|index| self.entries.get_mut(index)) {
            Some(previous) => {
                let before = previous.splice.weight();
                // The exact composition of the two: the second replaces precisely what the first
                // wrote, in place, so the pair is one replacement.
                previous.splice.inserted = splice.inserted;
                previous.cue_delta = previous.cue_delta.saturating_add(cue_delta);
                previous.at = now;
                let after = previous.splice.weight();
                self.bytes = self.bytes.saturating_sub(before).saturating_add(after);
            }
            None => {
                self.bytes = self.bytes.saturating_add(splice.weight());
                self.entries.push(Entry {
                    splice,
                    label,
                    cue_delta,
                    at: now,
                });
                self.cursor = self.entries.len();
            }
        }
        self.enforce_bounds();
    }

    /// The step that undoes the last edit, or `None` at the bottom of the stack. The cursor moves
    /// here, before the caller has replayed it: a caller whose replay fails puts it back with
    /// [`Self::redo`], which hands out the same entry again.
    pub fn undo(&mut self) -> Option<Step> {
        let index = self.cursor.checked_sub(1)?;
        let entry = self.entries.get(index)?;
        let step = Step {
            splice: entry.splice.inverse(),
            cue_delta: entry.cue_delta.saturating_neg(),
        };
        self.cursor = index;
        Some(step)
    }

    /// The step that redoes the next edit, or `None` at the top of the stack. Same cursor rule as
    /// [`Self::undo`], mirrored.
    pub fn redo(&mut self) -> Option<Step> {
        let entry = self.entries.get(self.cursor)?;
        let step = Step {
            splice: entry.splice.clone(),
            cue_delta: entry.cue_delta,
        };
        self.cursor = self.cursor.saturating_add(1);
        Some(step)
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    /// The current position is now on disk.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.cursor);
    }

    /// True unless the cursor sits exactly where it did at the last save.
    pub fn dirty(&self) -> bool {
        self.saved != Some(self.cursor)
    }

    /// Entries the stack holds, redo tail included: what [`MAX_ENTRIES`] bounds.
    pub fn depth(&self) -> usize {
        self.entries.len()
    }

    /// True once a bound dropped an entry: the file as opened is no longer reachable by undo.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// The entry `splice` continues rather than starting a new one: same label, inside the window,
    /// replacing exactly the bytes that entry wrote. Composition only, never a heuristic, and only
    /// ever asked for an edit the caller called [`Run::Continues`].
    /// Call after [`Self::drop_redo_tail`], so the entry below the cursor is the last one.
    fn merge_target(&self, splice: &Splice, label: EditLabel, now: Instant) -> Option<usize> {
        // Never merge into the entry a save landed on: the merged step would undo past the save
        // point, and dirty() would then call unsaved work clean. See BACKLOG.md M2.2.
        if self.saved == Some(self.cursor) {
            return None;
        }
        let index = self.cursor.checked_sub(1)?;
        let previous = self.entries.get(index)?;
        // An `at` older than the entry's own is a caller bug, not a fast keystroke: never merge it.
        let gap = now.checked_duration_since(previous.at)?;
        let continues = previous.label == label
            && gap <= COALESCE_WINDOW
            && previous.splice.at == splice.at
            && previous.splice.inserted == splice.removed;
        continues.then_some(index)
    }

    /// Entries above the cursor were undone and are now unreachable, so a new edit reclaims them.
    fn drop_redo_tail(&mut self) {
        for entry in self.entries.iter().skip(self.cursor) {
            self.bytes = self.bytes.saturating_sub(entry.splice.weight());
        }
        self.entries.truncate(self.cursor);
        // A save point in the tail went with it: those bytes can no longer be reached.
        if self.saved.is_some_and(|saved| saved > self.cursor) {
            self.saved = None;
        }
    }

    /// Drop from the bottom until both bounds hold. The newest entry always survives: an edit that
    /// cannot be undone at all is worse than a stack one entry over its bound.
    fn enforce_bounds(&mut self) {
        while self.entries.len() > 1 && (self.entries.len() > MAX_ENTRIES || self.bytes > MAX_BYTES)
        {
            let Some(dropped) = self.entries.first() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(dropped.splice.weight());
            self.entries.remove(0);
            self.cursor = self.cursor.saturating_sub(1);
            // The save point was the state before the dropped entry: below zero it is unprovable.
            self.saved = match self.saved {
                Some(0) | None => None,
                Some(saved) => Some(saved.saturating_sub(1)),
            };
            self.truncated = true;
        }
    }
}
