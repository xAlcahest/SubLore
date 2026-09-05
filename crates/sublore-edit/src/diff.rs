//! The cue list the UI sees, and the smallest patch between two of them. See BACKLOG.md M2.1.

use sublore_formats::{AssEvent, AssEventKind, AssField, CueDetail, Span, SubtitleDocument};

/// A cue as the UI sees it: no spans, text normalized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueView {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
    /// An ASS `Comment:` event. Editable, listed, but not a line a player draws.
    pub comment: bool,
    /// The cue's own number, when the file wrote one (SRT index line). Never renumbered.
    pub number: Option<u32>,
    /// The ASS style the event names, display-trimmed. Empty for every other format, and for an
    /// events section whose `Format:` line declares no style.
    pub style: String,
    /// The ASS `Name` (or `Actor`) field, under the same rule. Called actor because that is the
    /// word on the column. See grid-columns-tasks.md G2.
    pub actor: String,
}

/// One contiguous run of cues replaced by another. Every mutation, undo and redo produces one,
/// because every one of them touches a contiguous run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CuePatch {
    pub from: usize,
    pub removed: usize,
    pub cues: Vec<CueView>,
}

/// UI and IPC form: `\r\n` collapses to `\n`, whatever the file uses. A lone `\r` is ordinary
/// content to the parsers, so it travels through unchanged. See BACKLOG.md M2.1.
pub fn normalize(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    for (index, character) in text.char_indices() {
        // Drop the `\r` of a `\r\n` pair only; a `\r` anywhere else is content.
        if character == '\r' && bytes.get(index + 1) == Some(&b'\n') {
            continue;
        }
        out.push(character);
    }
    out
}

/// One declared ASS field as a column is given it: the file's own bytes with the surrounding
/// spaces, tabs and carriage return dropped, which is what `timecode_of` does to a timing field.
/// Display only, and the file is never touched.
fn named_field(document: &SubtitleDocument, event: &AssEvent, index: Option<usize>) -> String {
    let field: Option<Span> = index.and_then(|at| event.fields.get(at).copied());
    match field {
        Some(span) => document
            .slice(span)
            .trim_start_matches([' ', '\t'])
            .trim_end_matches([' ', '\t', '\r'])
            .to_owned(),
        None => String::new(),
    }
}

/// The whole list, in `cues()` order: ASS `Comment:` events included.
pub fn views(document: &SubtitleDocument) -> Vec<CueView> {
    document
        .cues()
        .map(|cue| CueView {
            start_ms: cue.start.millis(),
            end_ms: cue.end.millis(),
            text: normalize(document.slice(cue.text)),
            comment: matches!(&cue.detail, CueDetail::Ass(event) if event.kind == AssEventKind::Comment),
            number: match &cue.detail {
                CueDetail::Srt(srt) => srt.number,
                CueDetail::Vtt(_) | CueDetail::Ass(_) => None,
            },
            style: match &cue.detail {
                CueDetail::Ass(event) => named_field(document, event, event.field_index(AssField::Style)),
                CueDetail::Srt(_) | CueDetail::Vtt(_) => String::new(),
            },
            actor: match &cue.detail {
                CueDetail::Ass(event) => named_field(document, event, event.field_index(AssField::Actor)),
                CueDetail::Srt(_) | CueDetail::Vtt(_) => String::new(),
            },
        })
        .collect()
}

/// The smallest run that differs: the common prefix, then the common suffix bounded by what the
/// prefix left. Undo and redo carry no plan, so the patch is measured, never predicted.
pub fn patch(before: &[CueView], after: &[CueView]) -> CuePatch {
    let prefix = before
        .iter()
        .zip(after.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let room = before.len().min(after.len()).saturating_sub(prefix);
    let suffix = before
        .iter()
        .rev()
        .zip(after.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
        .min(room);

    let removed = before.len().saturating_sub(prefix).saturating_sub(suffix);
    let end = after.len().saturating_sub(suffix);
    let cues = after.get(prefix..end).unwrap_or(&[]).to_vec();
    CuePatch {
        from: prefix,
        removed,
        cues,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize, patch, views, CuePatch, CueView};
    use sublore_formats::{SubtitleDocument, SubtitleFormat};

    fn ass(body: &str) -> SubtitleDocument {
        sublore_formats::parse(SubtitleFormat::Ass, body.as_bytes()).expect("the fixture parses")
    }

    /// Every row's style and actor, which is what the two columns are drawn from.
    fn named(document: &SubtitleDocument) -> Vec<(String, String)> {
        views(document)
            .into_iter()
            .map(|view| (view.style, view.actor))
            .collect()
    }

    fn view(text: &str) -> CueView {
        CueView {
            start_ms: 0,
            end_ms: 1,
            text: text.to_owned(),
            comment: false,
            number: None,
            style: String::new(),
            actor: String::new(),
        }
    }

    #[test]
    fn normalizes_crlf_and_keeps_a_lone_carriage_return() {
        assert_eq!(normalize("a\r\nb"), "a\nb");
        assert_eq!(normalize("a\rb"), "a\rb");
        assert_eq!(normalize("plain"), "plain");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn a_replaced_row_patches_only_itself() {
        let before = [view("one"), view("two"), view("three")];
        let after = [view("one"), view("TWO"), view("three")];
        assert_eq!(
            patch(&before, &after),
            CuePatch {
                from: 1,
                removed: 1,
                cues: vec![view("TWO")],
            }
        );
    }

    #[test]
    fn an_insert_removes_nothing() {
        let before = [view("one"), view("two")];
        let after = [view("one"), view("new"), view("two")];
        assert_eq!(
            patch(&before, &after),
            CuePatch {
                from: 1,
                removed: 0,
                cues: vec![view("new")],
            }
        );
    }

    #[test]
    fn a_delete_inserts_nothing() {
        let before = [view("one"), view("two"), view("three")];
        let after = [view("one"), view("three")];
        assert_eq!(
            patch(&before, &after),
            CuePatch {
                from: 1,
                removed: 1,
                cues: Vec::new(),
            }
        );
    }

    #[test]
    fn identical_lists_patch_nothing() {
        let before = [view("one"), view("two")];
        assert_eq!(patch(&before, &before).removed, 0);
        assert!(patch(&before, &before).cues.is_empty());
    }

    #[test]
    fn a_split_replaces_one_row_with_two() {
        let before = [view("one"), view("two three"), view("four")];
        let after = [view("one"), view("two"), view("three"), view("four")];
        assert_eq!(
            patch(&before, &after),
            CuePatch {
                from: 1,
                removed: 1,
                cues: vec![view("two"), view("three")],
            }
        );
    }

    #[test]
    fn repeated_rows_do_not_confuse_the_suffix() {
        // The suffix scan must stop where the prefix already claimed, or the runs would overlap.
        let before = [view("x"), view("x")];
        let after = [view("x"), view("x"), view("x")];
        let patch = patch(&before, &after);
        assert_eq!(patch.removed, 0);
        assert_eq!(patch.cues.len(), 1);
        assert!(patch.from <= before.len());
    }

    #[test]
    fn an_ass_row_carries_the_style_and_the_speaker_its_own_format_line_declares() {
        let document = ass(concat!(
            "[Events]\n",
            "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
            "Dialogue: 0,0:00:01.00,0:00:02.00,Sign,,0,0,0,,A shop front\n",
            "Dialogue: 0,0:00:02.00,0:00:04.00,Default,Ingrid,0,0,0,,A line of dialogue\n",
        ));
        assert_eq!(
            named(&document),
            vec![
                ("Sign".to_owned(), String::new()),
                ("Default".to_owned(), "Ingrid".to_owned()),
            ]
        );
    }

    #[test]
    fn a_hand_spaced_field_reaches_the_column_without_its_spaces() {
        // Display trimming only: the file still holds every byte it did, asserted on the same
        // document. See grid-columns-tasks.md G1.
        let body = concat!(
            "[Events]\n",
            "Format: Layer, Start, End, Style, Name, Text\n",
            "Dialogue: 0, 0:00:01.34, 0:00:03.98, Default, Ingrid, Spaced out\n",
        );
        let document = ass(body);
        assert_eq!(
            named(&document),
            vec![("Default".to_owned(), "Ingrid".to_owned())]
        );
        assert_eq!(document.to_bytes(), body.as_bytes());
    }

    #[test]
    fn a_row_of_a_format_with_no_such_field_carries_two_empty_strings() {
        let document = ass(concat!(
            "[Events]\n",
            "Format: Layer, Start, End, Text\n",
            "Dialogue: 0,0:00:01.00,0:00:02.00,Nothing named here\n",
        ));
        assert_eq!(named(&document), vec![(String::new(), String::new())]);
    }

    #[test]
    fn a_patch_that_changes_only_a_style_is_a_patch() {
        // What makes the two fields part of the comparison rather than decoration on it.
        let mut before = vec![view("one"), view("two")];
        let mut after = before.clone();
        before[1].style = "Default".to_owned();
        after[1].style = "Sign".to_owned();
        assert_eq!(
            patch(&before, &after),
            CuePatch {
                from: 1,
                removed: 1,
                cues: vec![after[1].clone()],
            }
        );
    }
}
