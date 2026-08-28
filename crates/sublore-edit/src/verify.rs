//! Holding a re-parsed document to the plan that produced it. See BACKLOG.md M2.1.
//!
//! The coverage guard in `sublore_formats::parse` proves the edited bytes still tile; it cannot
//! prove they mean what the plan intended. Three shapes parse cleanly and are still wrong: a line
//! break inside an ASS field splits the event, text written into an empty text span becomes a
//! timing trailer, and a trailing newline grows a blank segment. Each of those moves a segment or a
//! cue the plan did not predict, so predicting them is how they are caught.

use std::mem::discriminant;

use sublore_formats::SubtitleDocument;

use crate::error::{EditError, EditErrorKind};
use crate::plan::Expectation;

/// Hold the re-parsed document to the plan. Anything unexpected is `Unverified`, and an
/// `Unverified` edit is thrown away with the bytes it would have written. See CLAUDE.md §3.
pub fn verify(
    before: &SubtitleDocument,
    after: &SubtitleDocument,
    expect: &Expectation,
) -> Result<(), EditError> {
    if after.format() != before.format() {
        return Err(unverified(format!(
            "the edited bytes parsed as {}, not {}",
            after.format().as_str(),
            before.format().as_str()
        )));
    }
    if after.source().has_bom() != before.source().has_bom() {
        return Err(unverified("the edit moved the byte-order mark"));
    }

    segments(before, after, expect)?;
    cues(before, after, expect)
}

/// The segment count the plan predicted, and the same kinds outside the window it replaced.
fn segments(
    before: &SubtitleDocument,
    after: &SubtitleDocument,
    expect: &Expectation,
) -> Result<(), EditError> {
    let before_segments = before.segments();
    let after_segments = after.segments();

    let expected = before_segments
        .len()
        .checked_sub(expect.segments_removed)
        .and_then(|kept| kept.checked_add(expect.segments_inserted))
        .ok_or_else(|| unverified("the plan removes more segments than the document holds"))?;
    if after_segments.len() != expected {
        return Err(unverified(format!(
            "the edit left {} segments, the plan predicted {expected}",
            after_segments.len()
        )));
    }

    let head = expect
        .segments_from
        .min(before_segments.len())
        .min(after_segments.len());
    for index in 0..head {
        let (Some(old), Some(new)) = (before_segments.get(index), after_segments.get(index)) else {
            return Err(unverified("a segment before the edit went missing"));
        };
        if discriminant(&old.kind) != discriminant(&new.kind) {
            return Err(unverified(format!(
                "segment {index} changed kind ahead of the edit"
            )));
        }
    }

    let old_tail = before_segments
        .get(expect.segments_from.saturating_add(expect.segments_removed)..)
        .unwrap_or(&[]);
    let new_tail = after_segments
        .get(
            expect
                .segments_from
                .saturating_add(expect.segments_inserted)..,
        )
        .unwrap_or(&[]);
    if old_tail.len() != new_tail.len() {
        return Err(unverified(format!(
            "the edit left {} segments after it, the plan predicted {}",
            new_tail.len(),
            old_tail.len()
        )));
    }
    for (index, (old, new)) in old_tail.iter().zip(new_tail.iter()).enumerate() {
        if discriminant(&old.kind) != discriminant(&new.kind) {
            return Err(unverified(format!(
                "segment {index} changed kind after the edit"
            )));
        }
    }
    Ok(())
}

/// Every cue outside the window reads back identical; every cue inside it reads back as planned.
fn cues(
    before: &SubtitleDocument,
    after: &SubtitleDocument,
    expect: &Expectation,
) -> Result<(), EditError> {
    let old: Vec<_> = before.cues().collect();
    let new: Vec<_> = after.cues().collect();

    let expected = old
        .len()
        .checked_sub(expect.removed)
        .and_then(|kept| kept.checked_add(expect.cues.len()))
        .ok_or_else(|| unverified("the plan removes more cues than the document holds"))?;
    if new.len() != expected {
        return Err(unverified(format!(
            "the edit left {} cues, the plan predicted {expected}",
            new.len()
        )));
    }

    let window_end = expect.from.saturating_add(expect.removed);
    for (index, cue) in old.iter().enumerate() {
        if index >= expect.from && index < window_end {
            continue;
        }
        let shifted = if index < expect.from {
            index
        } else {
            index
                .saturating_sub(expect.removed)
                .saturating_add(expect.cues.len())
        };
        let Some(other) = new.get(shifted) else {
            return Err(unverified(format!("cue {index} went missing")));
        };
        if cue.start.millis() != other.start.millis() || cue.end.millis() != other.end.millis() {
            return Err(unverified(format!(
                "cue {index} was not edited and its times moved"
            )));
        }
        if before.slice(cue.text) != after.slice(other.text) {
            return Err(unverified(format!(
                "cue {index} was not edited and its text changed"
            )));
        }
    }

    for (offset, planned) in expect.cues.iter().enumerate() {
        let at = expect.from.saturating_add(offset);
        let Some(cue) = new.get(at) else {
            return Err(unverified(format!("the edit wrote no cue at {at}")));
        };
        if cue.start.millis() != planned.start_ms || cue.end.millis() != planned.end_ms {
            return Err(unverified(format!(
                "cue {at} reads {}..{} ms, the plan wrote {}..{} ms",
                cue.start.millis(),
                cue.end.millis(),
                planned.start_ms,
                planned.end_ms
            )));
        }
        let read_back = after.slice(cue.text);
        if read_back != planned.text_raw {
            return Err(unverified(format!(
                "cue {at} reads back {read_back:?}, the plan wrote {:?}",
                planned.text_raw
            )));
        }
    }
    Ok(())
}

fn unverified(detail: impl Into<String>) -> EditError {
    EditError::new(EditErrorKind::Unverified, detail)
}
