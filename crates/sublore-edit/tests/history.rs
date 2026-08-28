//! Behavioural tests for M2.2, written from the acceptance criteria in BACKLOG.md: any sequence of
//! edits undoes back to the exact original bytes and redoes forward to the exact edited bytes, the
//! depth is bounded, and typing a word is one undo step rather than one per character.
//!
//! Every assertion is on bytes, never on the stack's shape alone. The history stores splices, so a
//! suite that only counted entries would pass for a stack that composed them wrongly; these tests
//! replay every step it hands out over the bytes of a real fixture and compare against the file as
//! it was read, byte for byte.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sublore_edit::history::{History, Run, COALESCE_WINDOW, MAX_BYTES, MAX_ENTRIES};
use sublore_edit::splice::{apply, EditKind, EditLabel, Splice};

/// Wider than `COALESCE_WINDOW`: consecutive edits stay separate entries.
const APART: Duration = Duration::from_secs(2);
/// A typing pause, well inside the window.
const KEYSTROKE: Duration = Duration::from_millis(50);
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
/// Inserted by the multi-byte leg of every case: combining marks, RTL, CJK and an astral emoji.
const NON_LATIN: &str = "café ﻿الأحد こんにちは 🎬";
const NON_LATIN_AGAIN: &str = "cafés ﻿الإثنين さようなら 🎬🎬";

// ---------------------------------------------------------------------------
// The fixture under edit, as bytes.
// ---------------------------------------------------------------------------

/// A fixture split the way the edit layer sees it: splice offsets are body offsets, BOM excluded,
/// exactly like `sublore_formats::Span`.
struct Body {
    bom: bool,
    text: String,
}

impl Body {
    fn open(relative: &str) -> Self {
        let path = fixture(relative);
        let raw = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
        let bom = raw.starts_with(BOM);
        let body = if bom { &raw[BOM.len()..] } else { &raw[..] };
        let text = String::from_utf8(body.to_vec())
            .unwrap_or_else(|error| panic!("{} is not UTF-8: {error}", path.display()));
        Self { bom, text }
    }

    /// The file as it would be written right now. Every byte assertion compares this.
    fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(BOM.len() + self.text.len());
        if self.bom {
            out.extend_from_slice(BOM);
        }
        out.extend_from_slice(self.text.as_bytes());
        out
    }

    /// Move the bytes the way a session does: through the seam, refusing anything stale.
    fn apply(&mut self, splice: &Splice) {
        self.text = apply(&self.text, splice)
            .unwrap_or_else(|error| panic!("the step should apply to the body: {error}"));
    }

    fn offset_of(&self, needle: &str) -> usize {
        self.text
            .find(needle)
            .unwrap_or_else(|| panic!("the fixture no longer contains {needle:?}"))
    }
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("subtitles")
        .join(relative)
}

// ---------------------------------------------------------------------------
// Driving the history the way a session does: record and apply together.
// ---------------------------------------------------------------------------

/// Instants handed out on demand. `Instant` arithmetic is exact, so the coalescing tests are too.
struct Clock(Instant);

impl Clock {
    fn new() -> Self {
        Self(Instant::now())
    }

    fn after(&mut self, gap: Duration) -> Instant {
        self.0 = self
            .0
            .checked_add(gap)
            .expect("the test clock should not overflow");
        self.0
    }
}

fn label(kind: EditKind, cue: usize) -> EditLabel {
    EditLabel { kind, cue }
}

/// Replace the first occurrence of `needle`, the shape an edit to that cue produces.
fn replace(body: &Body, needle: &str, replacement: &str) -> Splice {
    Splice::new(
        body.offset_of(needle),
        needle.to_string(),
        replacement.to_string(),
    )
}

/// Replace exactly what `previous` wrote, in place: the shape one more keystroke produces.
fn retype(previous: &Splice, inserted: &str) -> Splice {
    Splice::new(previous.at, previous.inserted.clone(), inserted.to_string())
}

/// One accepted mutation: the bytes move, then the history records what moved them. A finished
/// edit, which is what the IPC layer sends: its own undo step whatever it looks like.
fn edit(
    history: &mut History,
    body: &mut Body,
    splice: Splice,
    label: EditLabel,
    cue_delta: isize,
    at: Instant,
) {
    body.apply(&splice);
    history.record(splice, label, cue_delta, Run::New, at);
}

/// The same, from a caller that says this edit continues the run above it: one more keystroke or
/// one more nudge, the only thing the coalescing window is asked about.
fn keystroke(
    history: &mut History,
    body: &mut Body,
    splice: Splice,
    label: EditLabel,
    cue_delta: isize,
    at: Instant,
) {
    body.apply(&splice);
    history.record(splice, label, cue_delta, Run::Continues, at);
}

/// Undo one step, returning the cue delta it carried. `None` at the bottom.
fn undo(history: &mut History, body: &mut Body) -> Option<isize> {
    let expected = history.can_undo();
    let step = history.undo();
    assert_eq!(
        step.is_some(),
        expected,
        "can_undo() disagreed with what undo() handed out"
    );
    let step = step?;
    body.apply(&step.splice);
    Some(step.cue_delta)
}

fn redo(history: &mut History, body: &mut Body) -> Option<isize> {
    let expected = history.can_redo();
    let step = history.redo();
    assert_eq!(
        step.is_some(),
        expected,
        "can_redo() disagreed with what redo() handed out"
    );
    let step = step?;
    body.apply(&step.splice);
    Some(step.cue_delta)
}

/// Walk to the bottom, returning the steps taken and the deltas they summed to.
fn undo_all(history: &mut History, body: &mut Body) -> (usize, isize) {
    let (mut steps, mut delta) = (0, 0);
    while let Some(step) = undo(history, body) {
        steps += 1;
        delta += step;
        assert!(steps <= MAX_ENTRIES, "undo did not reach the bottom");
    }
    assert!(!history.can_undo(), "the bottom should refuse another undo");
    (steps, delta)
}

fn redo_all(history: &mut History, body: &mut Body) -> (usize, isize) {
    let (mut steps, mut delta) = (0, 0);
    while let Some(step) = redo(history, body) {
        steps += 1;
        delta += step;
        assert!(steps <= MAX_ENTRIES, "redo did not reach the top");
    }
    assert!(!history.can_redo(), "the top should refuse another redo");
    (steps, delta)
}

fn assert_bytes_eq(actual: &[u8], expected: &[u8], context: &str) {
    if actual == expected {
        return;
    }
    let at = actual
        .iter()
        .zip(expected.iter())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| actual.len().min(expected.len()));
    panic!(
        "{context}: bytes differ at offset {at} ({} bytes now, {} expected)\n     now: {}\nexpected: {}",
        actual.len(),
        expected.len(),
        window(actual, at),
        window(expected, at)
    );
}

fn window(bytes: &[u8], at: usize) -> String {
    let from = at.saturating_sub(24);
    let to = at.saturating_add(24).min(bytes.len());
    String::from_utf8_lossy(bytes.get(from..to).unwrap_or_default())
        .escape_debug()
        .to_string()
}

// ---------------------------------------------------------------------------
// AC: any sequence of edits undoes to the exact original bytes and redoes to the exact edited ones.
// ---------------------------------------------------------------------------

struct Case {
    fixture: &'static str,
    /// Applied in order. `(needle, replacement)`, with `""` for a text a user emptied.
    edits: &'static [(&'static str, &'static str)],
    /// Still in the file once those edits are applied: where the non-Latin leg is planted.
    anchor: &'static str,
}

/// One case per shape that has already broken something: CRLF, LF, a BOM, non-Latin bytes around
/// the edit, VTT cue settings and ASS one-line events.
const CASES: &[Case] = &[
    Case {
        fixture: "srt/clean/basic-crlf.srt",
        edits: &[
            (
                "The harbour was empty when we got there.",
                "Il porto era vuoto quando siamo arrivati.",
            ),
            (
                "00:00:05,000 --> 00:00:08,340",
                "00:00:05,200 --> 00:00:08,540",
            ),
            (
                "so we sat on the dock until it got light.",
                "e siamo rimasti sul molo fino alle prime luci.",
            ),
            ("By then the fog had eaten the boats.", ""),
            (
                "Nobody had told the crew we were coming,",
                "Nobody had told the crew we were coming,\r\nnobody at all,",
            ),
        ],
        anchor: "Il porto era vuoto",
    },
    Case {
        fixture: "srt/clean/basic-lf.srt",
        edits: &[
            (
                "The harbour was empty when we got there.",
                "Il porto era vuoto quando siamo arrivati.",
            ),
            (
                "Nobody had told the crew we were coming,",
                "Nobody had told the crew we were coming,\nnobody at all,",
            ),
            ("By then the fog had eaten the boats.", ""),
        ],
        anchor: "Il porto era vuoto",
    },
    Case {
        fixture: "srt/clean/bom-crlf.srt",
        edits: &[
            (
                "You said the paperwork was done.",
                "Hai detto che le carte erano pronte.",
            ),
            (
                "00:01:15,200 --> 00:01:18,900",
                "00:01:15,400 --> 00:01:19,100",
            ),
            ("Nobody ever said it was done.", ""),
        ],
        anchor: "Hai detto che le carte",
    },
    Case {
        fixture: "srt/clean/non-latin.srt",
        edits: &[
            (
                "00:00:03,200 --> 00:00:05,900",
                "00:00:03,400 --> 00:00:06,100",
            ),
            (
                "00:00:08,600 --> 00:00:11,000",
                "00:00:08,900 --> 00:00:11,300",
            ),
        ],
        anchor: "00:00:01,000",
    },
    Case {
        fixture: "vtt/clean/basic.vtt",
        edits: &[
            (
                "Right, so this is the part where I explain",
                "Ecco la parte in cui spiego",
            ),
            (
                "00:00:05.400 --> 00:00:08.960",
                "00:00:05.600 --> 00:00:09.160",
            ),
            ("It was. Five minutes ago.", ""),
        ],
        anchor: "Ecco la parte in cui spiego",
    },
    Case {
        fixture: "ass/clean/basic.ass",
        edits: &[
            (
                "The harbour freezes over by December.",
                "Il porto ghiaccia entro dicembre.",
            ),
            ("0,0:00:04.12,0:00:06.75", "0,0:00:04.50,0:00:07.00"),
            ("Everyone else lost a boat last year.", ""),
        ],
        anchor: "Il porto ghiaccia entro dicembre.",
    },
];

#[test]
fn a_sequence_of_edits_undoes_to_the_original_bytes_and_redoes_to_the_edited_bytes() {
    for case in CASES {
        let mut body = Body::open(case.fixture);
        let original = body.bytes();
        let mut history = History::new();
        let mut clock = Clock::new();

        for (index, (needle, replacement)) in case.edits.iter().enumerate() {
            let splice = replace(&body, needle, replacement);
            let at = clock.after(APART);
            edit(
                &mut history,
                &mut body,
                splice,
                label(EditKind::SetText, index),
                0,
                at,
            );
        }
        // Two more legs so every case ends with non-Latin bytes in the file and an edit whose
        // offsets sit past them. See BACKLOG.md M2.2.
        let planted = replace(&body, case.anchor, &format!("{NON_LATIN}{}", case.anchor));
        let at = clock.after(APART);
        edit(
            &mut history,
            &mut body,
            planted.clone(),
            label(EditKind::SetText, 90),
            0,
            at,
        );
        let at = clock.after(APART);
        edit(
            &mut history,
            &mut body,
            retype(&planted, &format!("{NON_LATIN_AGAIN}{}", case.anchor)),
            label(EditKind::SetText, 91),
            0,
            at,
        );

        let edited = body.bytes();
        assert_ne!(edited, original, "{}: nothing was edited", case.fixture);
        assert_eq!(
            history.depth(),
            case.edits.len() + 2,
            "{}: edits this far apart must not merge",
            case.fixture
        );
        assert!(
            history.dirty(),
            "{}: edits leave the file dirty",
            case.fixture
        );

        let (steps, delta) = undo_all(&mut history, &mut body);
        assert_eq!(steps, case.edits.len() + 2, "{}", case.fixture);
        assert_eq!(delta, 0, "{}: text edits move no cue count", case.fixture);
        assert_bytes_eq(
            &body.bytes(),
            &original,
            &format!("{}: undo did not restore the file", case.fixture),
        );
        assert!(
            !history.dirty(),
            "{}: undoing back to the saved position is clean",
            case.fixture
        );

        let (steps, _) = redo_all(&mut history, &mut body);
        assert_eq!(steps, case.edits.len() + 2, "{}", case.fixture);
        assert_bytes_eq(
            &body.bytes(),
            &edited,
            &format!("{}: redo did not restore the edits", case.fixture),
        );
    }
}

#[test]
fn undo_and_redo_walk_the_same_byte_ladder_in_both_directions() {
    let mut body = Body::open("srt/clean/basic-crlf.srt");
    let mut history = History::new();
    let mut clock = Clock::new();
    let mut ladder = vec![body.bytes()];

    for (index, (needle, replacement)) in CASES[0].edits.iter().enumerate() {
        let splice = replace(&body, needle, replacement);
        let at = clock.after(APART);
        edit(
            &mut history,
            &mut body,
            splice,
            label(EditKind::SetText, index),
            0,
            at,
        );
        ladder.push(body.bytes());
    }

    // Down, up, down again: every position on the way must be the exact bytes recorded there.
    let mut cursor = CASES[0].edits.len();
    for going_back in [true, true, false, true, true, true, false, false, false] {
        let moved = if going_back {
            undo(&mut history, &mut body).is_some()
        } else {
            redo(&mut history, &mut body).is_some()
        };
        assert!(moved, "the walk should stay inside the stack");
        cursor = if going_back { cursor - 1 } else { cursor + 1 };
        assert_bytes_eq(
            &body.bytes(),
            &ladder[cursor],
            &format!("position {cursor} of the ladder"),
        );
    }
}

// ---------------------------------------------------------------------------
// AC: typing a word is one undo step, not one per character.
// ---------------------------------------------------------------------------

#[test]
fn typing_a_word_is_one_undo_step() {
    let mut body = Body::open("srt/clean/basic-crlf.srt");
    let original = body.bytes();
    let mut history = History::new();
    let mut clock = Clock::new();
    let anchor = "The harbour was empty when we got there.";
    let at = body.offset_of(anchor);

    let mut typed = anchor.to_string();
    for character in "morning,".chars() {
        let mut next = typed.clone();
        next.push(character);
        let when = clock.after(KEYSTROKE);
        keystroke(
            &mut history,
            &mut body,
            Splice::new(at, typed.clone(), next.clone()),
            label(EditKind::SetText, 0),
            0,
            when,
        );
        typed = next;
    }
    assert_eq!(
        history.depth(),
        1,
        "eight keystrokes on one cue are one undo step"
    );
    let after_word = body.bytes();

    // The same cue again, after a pause: a second step, so the word is undoable on its own.
    let mut next = typed.clone();
    next.push('!');
    let when = clock.after(APART);
    keystroke(
        &mut history,
        &mut body,
        Splice::new(at, typed, next),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    assert_eq!(history.depth(), 2, "a pause starts a new undo step");
    let edited = body.bytes();

    undo(&mut history, &mut body);
    assert_bytes_eq(&body.bytes(), &after_word, "one undo drops the last pause");
    undo(&mut history, &mut body);
    assert_bytes_eq(
        &body.bytes(),
        &original,
        "one more undo drops the whole word at once",
    );
    redo_all(&mut history, &mut body);
    assert_bytes_eq(&body.bytes(), &edited, "redo replays both steps");
}

/// Regression: the window used to merge anything that composed, so two finished edits of one cue
/// arriving quickly became one step and the state between them was gone. See BACKLOG.md M2.2.
#[test]
fn two_finished_edits_inside_the_window_are_still_two_steps() {
    let mut body = Body::open("srt/clean/basic-lf.srt");
    let original = body.bytes();
    let mut history = History::new();
    let mut clock = Clock::new();
    let anchor = "The harbour was empty when we got there.";

    let first = replace(&body, anchor, "First draft.");
    let when = clock.after(KEYSTROKE);
    edit(
        &mut history,
        &mut body,
        first.clone(),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    let after_first = body.bytes();

    let when = clock.after(KEYSTROKE);
    edit(
        &mut history,
        &mut body,
        retype(&first, "Second, corrected draft."),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    assert_eq!(
        history.depth(),
        2,
        "the second edit composes with the first, but it is not the same run"
    );

    undo(&mut history, &mut body);
    assert_bytes_eq(&body.bytes(), &after_first, "the first draft comes back");
    undo(&mut history, &mut body);
    assert_bytes_eq(&body.bytes(), &original, "and then the file as opened");
}

#[test]
fn a_long_run_of_keystrokes_stays_one_step_while_the_pauses_are_short() {
    // Thirty keystrokes 50 ms apart span 1.5 s, past COALESCE_WINDOW end to end: the window is
    // measured against the previous keystroke, not the first. See BACKLOG.md M2.2.
    let mut body = Body::open("srt/clean/basic-lf.srt");
    let original = body.bytes();
    let mut history = History::new();
    let mut clock = Clock::new();
    let anchor = "By then the fog had eaten the boats.";
    let at = body.offset_of(anchor);

    let mut typed = anchor.to_string();
    for character in "and the crew stopped counting them".chars().take(30) {
        let mut next = typed.clone();
        next.push(character);
        let when = clock.after(KEYSTROKE);
        keystroke(
            &mut history,
            &mut body,
            Splice::new(at, typed.clone(), next.clone()),
            label(EditKind::SetText, 2),
            0,
            when,
        );
        typed = next;
    }

    assert_eq!(history.depth(), 1, "a steady run of typing is one step");
    let (steps, _) = undo_all(&mut history, &mut body);
    assert_eq!(steps, 1);
    assert_bytes_eq(&body.bytes(), &original, "the merged step is exact");
}

#[test]
fn the_coalescing_window_is_inclusive_and_a_longer_pause_splits() {
    let mut body = Body::open("srt/clean/basic-lf.srt");
    let mut history = History::new();
    let mut clock = Clock::new();
    let anchor = "The harbour was empty when we got there.";
    let at = body.offset_of(anchor);

    let first = Splice::new(at, anchor.to_string(), format!("{anchor} A"));
    let when = clock.after(KEYSTROKE);
    keystroke(
        &mut history,
        &mut body,
        first.clone(),
        label(EditKind::SetText, 0),
        0,
        when,
    );

    let second = retype(&first, &format!("{anchor} AB"));
    let when = clock.after(COALESCE_WINDOW);
    keystroke(
        &mut history,
        &mut body,
        second.clone(),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    assert_eq!(history.depth(), 1, "exactly at the window still merges");

    let when = clock.after(COALESCE_WINDOW + Duration::from_millis(1));
    keystroke(
        &mut history,
        &mut body,
        retype(&second, &format!("{anchor} ABC")),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    assert_eq!(history.depth(), 2, "one millisecond past the window splits");
}

#[test]
fn only_an_edit_that_replaces_what_the_previous_one_wrote_merges() {
    let anchor = "The harbour was empty when we got there.";
    let other = "By then the fog had eaten the boats.";

    // A different cue index, everything else equal: two steps.
    let (mut history, mut body, mut clock) = (
        History::new(),
        Body::open("srt/clean/basic-lf.srt"),
        Clock::new(),
    );
    let original = body.bytes();
    let first = replace(&body, anchor, "one");
    let when = clock.after(KEYSTROKE);
    keystroke(
        &mut history,
        &mut body,
        first.clone(),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    let when = clock.after(KEYSTROKE);
    keystroke(
        &mut history,
        &mut body,
        retype(&first, "two"),
        label(EditKind::SetText, 1),
        0,
        when,
    );
    assert_eq!(history.depth(), 2, "another cue is another step");
    undo_all(&mut history, &mut body);
    assert_bytes_eq(&body.bytes(), &original, "separate steps stay exact");

    // A different kind on the same cue: two steps.
    let (mut history, mut body, mut clock) = (
        History::new(),
        Body::open("srt/clean/basic-lf.srt"),
        Clock::new(),
    );
    let first = replace(&body, anchor, "one");
    let when = clock.after(KEYSTROKE);
    keystroke(
        &mut history,
        &mut body,
        first.clone(),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    let when = clock.after(KEYSTROKE);
    keystroke(
        &mut history,
        &mut body,
        retype(&first, "two"),
        label(EditKind::SetTimes, 0),
        0,
        when,
    );
    assert_eq!(history.depth(), 2, "another kind is another step");

    // Same label and window, but a different offset: two steps.
    let (mut history, mut body, mut clock) = (
        History::new(),
        Body::open("srt/clean/basic-lf.srt"),
        Clock::new(),
    );
    let when = clock.after(KEYSTROKE);
    let first = replace(&body, anchor, "one");
    keystroke(
        &mut history,
        &mut body,
        first,
        label(EditKind::SetText, 0),
        0,
        when,
    );
    let when = clock.after(KEYSTROKE);
    let elsewhere = replace(&body, other, "elsewhere");
    keystroke(
        &mut history,
        &mut body,
        elsewhere,
        label(EditKind::SetText, 0),
        0,
        when,
    );
    assert_eq!(history.depth(), 2, "another offset is another step");

    // Same label, window and offset, but it does not replace the whole of what was written.
    let (mut history, mut body, mut clock) = (
        History::new(),
        Body::open("srt/clean/basic-lf.srt"),
        Clock::new(),
    );
    let original = body.bytes();
    let first = replace(&body, anchor, "one two");
    let when = clock.after(KEYSTROKE);
    keystroke(
        &mut history,
        &mut body,
        first.clone(),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    let when = clock.after(KEYSTROKE);
    keystroke(
        &mut history,
        &mut body,
        Splice::new(first.at, "one".to_string(), "ONE".to_string()),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    assert_eq!(
        history.depth(),
        2,
        "a partial rewrite is not exact composition, so it is another step"
    );
    undo_all(&mut history, &mut body);
    assert_bytes_eq(&body.bytes(), &original, "both steps undo exactly");
}

#[test]
fn cue_deltas_negate_on_undo_and_sum_when_steps_merge() {
    let mut body = Body::open("srt/clean/basic-lf.srt");
    let mut history = History::new();
    let mut clock = Clock::new();
    let anchor = "By then the fog had eaten the boats.";

    // An inserted cue, then the same insertion rewritten into two cues within the window.
    let first = replace(
        &body,
        anchor,
        &format!("{anchor}\n\n4\n00:00:12,000 --> 00:00:13,000\nfirst extra"),
    );
    let when = clock.after(KEYSTROKE);
    keystroke(
        &mut history,
        &mut body,
        first.clone(),
        label(EditKind::Insert, 3),
        1,
        when,
    );
    let when = clock.after(KEYSTROKE);
    keystroke(
        &mut history,
        &mut body,
        retype(
            &first,
            &format!("{anchor}\n\n4\n00:00:12,000 --> 00:00:13,000\nfirst extra\n\n5\n00:00:13,200 --> 00:00:14,000\nsecond extra"),
        ),
        label(EditKind::Insert, 3),
        1,
        when,
    );
    assert_eq!(history.depth(), 1, "the two insertions merged");

    let step = undo(&mut history, &mut body).expect("the merged step should undo");
    assert_eq!(step, -2, "the merged step gives back both cues");
    let step = redo(&mut history, &mut body).expect("the merged step should redo");
    assert_eq!(step, 2, "and takes them back on redo");
}

// ---------------------------------------------------------------------------
// AC: undo depth is bounded and documented.
// ---------------------------------------------------------------------------

/// Rewrite one cue `takes` times, each rewrite a finished edit of its own, keeping the bytes after
/// every edit. Returns the ladder, the history and the body.
fn takes(fixture: &str, anchor: &str, takes: usize, size: usize) -> (Vec<Vec<u8>>, History, Body) {
    let mut body = Body::open(fixture);
    let mut history = History::new();
    let mut clock = Clock::new();
    let mut ladder = vec![body.bytes()];
    let mut current = anchor.to_string();

    for take in 0..takes {
        let next = format!("take {take} {}", "-".repeat(size));
        let splice = Splice::new(body.offset_of(&current), current.clone(), next.clone());
        let when = clock.after(APART);
        edit(
            &mut history,
            &mut body,
            splice,
            label(EditKind::SetText, 0),
            0,
            when,
        );
        ladder.push(body.bytes());
        current = next;
    }
    (ladder, history, body)
}

#[test]
fn one_step_below_the_bound_the_whole_stack_undoes_and_redoes_byte_for_byte() {
    let anchor = "The harbour was empty when we got there.";
    let steps_recorded = MAX_ENTRIES - 1;
    let (ladder, mut history, mut body) =
        takes("srt/clean/basic-crlf.srt", anchor, steps_recorded, 0);

    assert_eq!(history.depth(), steps_recorded);
    assert!(!history.truncated(), "199 entries fit under the bound");
    let edited = body.bytes();

    let (steps, _) = undo_all(&mut history, &mut body);
    assert_eq!(steps, steps_recorded);
    assert_bytes_eq(
        &body.bytes(),
        &ladder[0],
        "199 undos land on the file as it was opened",
    );
    let (steps, _) = redo_all(&mut history, &mut body);
    assert_eq!(steps, steps_recorded);
    assert_bytes_eq(
        &body.bytes(),
        &edited,
        "and 199 redos land back on the edits",
    );
}

#[test]
fn past_the_bound_the_oldest_edits_are_dropped_and_the_rest_still_undo_exactly() {
    let anchor = "The harbour was empty when we got there.";
    let steps_recorded = MAX_ENTRIES + 1;
    let (ladder, mut history, mut body) =
        takes("srt/clean/basic-crlf.srt", anchor, steps_recorded, 0);

    assert_eq!(history.depth(), MAX_ENTRIES, "the depth is bounded");
    assert!(
        history.truncated(),
        "the file as opened is no longer reachable, and the session must say so"
    );
    let edited = body.bytes();

    let (steps, _) = undo_all(&mut history, &mut body);
    assert_eq!(steps, MAX_ENTRIES);
    assert_bytes_eq(
        &body.bytes(),
        &ladder[steps_recorded - MAX_ENTRIES],
        "undo bottoms out on the state the oldest kept entry starts from",
    );
    assert_ne!(
        body.bytes(),
        ladder[0],
        "the dropped edit is not undoable, which is what truncated() reports"
    );
    let (steps, _) = redo_all(&mut history, &mut body);
    assert_eq!(steps, MAX_ENTRIES);
    assert_bytes_eq(
        &body.bytes(),
        &edited,
        "redo still lands on the edited bytes",
    );
}

#[test]
fn the_byte_bound_drops_from_the_bottom_before_the_entry_count_does() {
    // Each take rewrites the previous one, so every entry after the first weighs about 1.2 MB.
    let anchor = "The harbour was empty when we got there.";
    let steps_recorded = 10;
    let (ladder, mut history, mut body) = takes(
        "srt/clean/basic-crlf.srt",
        anchor,
        steps_recorded,
        600 * 1024,
    );

    assert!(
        history.truncated(),
        "ten takes of 600 KB are past the {MAX_BYTES}-byte bound"
    );
    assert!(
        history.depth() < steps_recorded,
        "the byte bound drops entries long before the count bound does"
    );
    assert!(history.depth() > 0, "the newest edits are kept");
    let kept = history.depth();
    let edited = body.bytes();

    let (steps, _) = undo_all(&mut history, &mut body);
    assert_eq!(steps, kept);
    assert_bytes_eq(
        &body.bytes(),
        &ladder[steps_recorded - kept],
        "what is still on the stack undoes exactly",
    );
    redo_all(&mut history, &mut body);
    assert_bytes_eq(&body.bytes(), &edited, "and redoes exactly");
}

#[test]
fn a_single_edit_larger_than_the_byte_bound_is_still_undoable() {
    let mut body = Body::open("srt/clean/basic-lf.srt");
    let original = body.bytes();
    let mut history = History::new();
    let mut clock = Clock::new();

    let anchor = "By then the fog had eaten the boats.";
    let huge = "z".repeat(MAX_BYTES + 1);
    let when = clock.after(APART);
    let oversized = replace(&body, anchor, &huge);
    edit(
        &mut history,
        &mut body,
        oversized,
        label(EditKind::SetText, 2),
        0,
        when,
    );

    assert_eq!(history.depth(), 1, "the newest edit is never dropped");
    assert!(history.can_undo());
    undo(&mut history, &mut body);
    assert_bytes_eq(&body.bytes(), &original, "an oversized edit still undoes");
}

// ---------------------------------------------------------------------------
// The stack's own shape: the redo tail, the empty stack, the saved position.
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_history_has_nothing_to_undo_or_redo() {
    let mut history = History::new();
    assert_eq!(history.depth(), 0);
    assert!(!history.can_undo());
    assert!(!history.can_redo());
    assert!(history.undo().is_none());
    assert!(history.redo().is_none());
    assert!(!history.truncated());
    assert!(!history.dirty(), "a file just opened is not dirty");
}

#[test]
fn recording_after_an_undo_drops_the_redo_tail() {
    let mut body = Body::open("srt/clean/basic-lf.srt");
    let original = body.bytes();
    let mut history = History::new();
    let mut clock = Clock::new();

    for (index, (needle, replacement)) in CASES[1].edits.iter().enumerate() {
        let splice = replace(&body, needle, replacement);
        let when = clock.after(APART);
        edit(
            &mut history,
            &mut body,
            splice,
            label(EditKind::SetText, index),
            0,
            when,
        );
    }
    undo(&mut history, &mut body);
    undo(&mut history, &mut body);
    assert!(history.can_redo(), "there is a tail to redo");

    let when = clock.after(APART);
    let branch = replace(
        &body,
        "so we sat on the dock until it got light.",
        "a new branch",
    );
    edit(
        &mut history,
        &mut body,
        branch,
        label(EditKind::SetText, 1),
        0,
        when,
    );
    assert!(
        !history.can_redo(),
        "the tail is gone once a branch is taken"
    );
    assert_eq!(history.depth(), 2);
    let edited = body.bytes();

    let (steps, _) = undo_all(&mut history, &mut body);
    assert_eq!(steps, 2);
    assert_bytes_eq(
        &body.bytes(),
        &original,
        "the new branch undoes to the file as opened",
    );
    redo_all(&mut history, &mut body);
    assert_bytes_eq(&body.bytes(), &edited, "and redoes to the branch");
}

#[test]
fn dirty_follows_the_distance_from_the_saved_position() {
    let mut body = Body::open("srt/clean/basic-lf.srt");
    let mut history = History::new();
    let mut clock = Clock::new();
    let anchor = "The harbour was empty when we got there.";

    assert!(!history.dirty());
    let first = replace(&body, anchor, "one");
    let when = clock.after(APART);
    edit(
        &mut history,
        &mut body,
        first.clone(),
        label(EditKind::SetText, 0),
        0,
        when,
    );
    assert!(history.dirty(), "an edit dirties the file");
    undo(&mut history, &mut body);
    assert!(
        !history.dirty(),
        "undoing back to the saved position is clean"
    );
    redo(&mut history, &mut body);
    assert!(history.dirty());

    history.mark_saved();
    assert!(!history.dirty(), "saving here clears it");
    undo(&mut history, &mut body);
    assert!(
        history.dirty(),
        "stepping away from the saved position dirties it again"
    );
    redo(&mut history, &mut body);
    assert!(!history.dirty(), "and stepping back onto it clears it");
}

#[test]
fn a_saved_position_the_bound_dropped_is_reported_as_dirty() {
    let anchor = "The harbour was empty when we got there.";
    let (_, mut history, mut body) = takes("srt/clean/basic-crlf.srt", anchor, MAX_ENTRIES + 1, 0);

    // The saved position was the file as opened, which the bound has just dropped.
    assert!(history.truncated());
    undo_all(&mut history, &mut body);
    assert!(
        history.dirty(),
        "with the opening state gone, the file can never be proved saved again"
    );
}

#[test]
fn a_saved_position_inside_a_dropped_redo_tail_is_reported_as_dirty() {
    let mut body = Body::open("srt/clean/basic-lf.srt");
    let mut history = History::new();
    let mut clock = Clock::new();

    for (index, (needle, replacement)) in CASES[1].edits.iter().enumerate() {
        let splice = replace(&body, needle, replacement);
        let when = clock.after(APART);
        edit(
            &mut history,
            &mut body,
            splice,
            label(EditKind::SetText, index),
            0,
            when,
        );
    }
    history.mark_saved();
    undo(&mut history, &mut body);
    undo(&mut history, &mut body);
    assert!(history.dirty(), "two undos away from the save");

    let when = clock.after(APART);
    let branch = replace(
        &body,
        "so we sat on the dock until it got light.",
        "a new branch",
    );
    edit(
        &mut history,
        &mut body,
        branch,
        label(EditKind::SetText, 1),
        0,
        when,
    );
    assert!(
        history.dirty(),
        "the saved position was in the tail this branch dropped"
    );
    undo_all(&mut history, &mut body);
    assert!(history.dirty(), "and it stays unreachable");
}

#[test]
fn a_run_of_timing_nudges_on_one_cue_is_one_undo_step() {
    // The same composition rule as typing, on the kind M2.5 will nudge with. See BACKLOG.md M2.2.
    let mut body = Body::open("srt/clean/basic-lf.srt");
    let original = body.bytes();
    let mut history = History::new();
    let mut clock = Clock::new();

    let mut current = String::from("00:00:05,000 --> 00:00:08,340");
    let at = body.offset_of(&current);
    for step in 1..=5 {
        let next = format!("00:00:05,{:03} --> 00:00:08,340", step * 100);
        let when = clock.after(KEYSTROKE);
        keystroke(
            &mut history,
            &mut body,
            Splice::new(at, current.clone(), next.clone()),
            label(EditKind::SetTimes, 1),
            0,
            when,
        );
        current = next;
    }

    assert_eq!(history.depth(), 1, "five nudges on one cue are one step");
    let (steps, _) = undo_all(&mut history, &mut body);
    assert_eq!(steps, 1);
    assert_bytes_eq(&body.bytes(), &original, "the merged nudge is exact");
}
