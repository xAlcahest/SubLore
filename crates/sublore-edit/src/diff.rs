//! The cue list the UI sees, and the smallest patch between two of them. See BACKLOG.md M2.1.

use sublore_formats::{
    ass::trim_field, AssEvent, AssEventKind, AssField, CueDetail, Span, SubtitleDocument,
};

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
    /// The ASS `Effect` field, under the same rule.
    pub effect: String,
    /// The ASS `Layer` field as the file spells it, not as a number: `"0000"` stays `"0000"` and a
    /// value that is no integer at all stays itself, because a reader may not refuse a file it can
    /// display and may not invent a value the file does not hold. Empty means the same as it does
    /// for the style: nothing to show. See styles-and-fields-tasks.md F2.
    pub layer: String,
    /// The ASS `MarginL` field, under the same rule as the layer.
    pub margin_l: String,
    /// The ASS `MarginR` field, under the same rule as the layer.
    pub margin_r: String,
    /// The ASS `MarginV` field, under the same rule as the layer.
    pub margin_v: String,
    /// Which of the seven this row's own section declares before the text, in [`AssField::ALL`]
    /// order. The value of an undeclared field and the value of a declared blank one are both the
    /// empty string, so this is the only thing that tells them apart, and it is what decides
    /// whether a control may be used at all: a write to a field that is not on this list is
    /// refused with `NotApplicable`. Empty for SRT and VTT. See styles-and-fields-tasks.md F2.
    ///
    /// Per row and not per document because a file may hold two `[Events]` sections with different
    /// `Format:` lines, and the write path already answers per event.
    pub declared_fields: Vec<AssField>,
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

/// One declared ASS field as a control is given it: the file's own bytes, padding dropped by the
/// same `trim_field` the write path trims a field's core with, so what is shown is what committing
/// it unchanged would write. Display only, and the file is never touched.
fn named_field(document: &SubtitleDocument, event: &AssEvent, field: AssField) -> String {
    let span: Option<Span> = event
        .field_index(field)
        .and_then(|at| event.fields.get(at).copied());
    match span {
        Some(span) => document
            .slice(trim_field(document.source().body(), span))
            .to_owned(),
        None => String::new(),
    }
}

/// The whole list, in `cues()` order: ASS `Comment:` events included.
pub fn views(document: &SubtitleDocument) -> Vec<CueView> {
    document
        .cues()
        .map(|cue| {
            let event = match &cue.detail {
                CueDetail::Ass(event) => Some(event),
                CueDetail::Srt(_) | CueDetail::Vtt(_) => None,
            };
            // Every declared field through one reader: an SRT and a VTT row carry the empty string
            // in all seven. See styles-and-fields-tasks.md F4.
            let field = |name: AssField| match event {
                Some(event) => named_field(document, event, name),
                None => String::new(),
            };
            CueView {
                start_ms: cue.start.millis(),
                end_ms: cue.end.millis(),
                text: normalize(document.slice(cue.text)),
                comment: event.is_some_and(|event| event.kind == AssEventKind::Comment),
                number: match &cue.detail {
                    CueDetail::Srt(srt) => srt.number,
                    CueDetail::Vtt(_) | CueDetail::Ass(_) => None,
                },
                style: field(AssField::Style),
                actor: field(AssField::Actor),
                effect: field(AssField::Effect),
                layer: field(AssField::Layer),
                margin_l: field(AssField::MarginL),
                margin_r: field(AssField::MarginR),
                margin_v: field(AssField::MarginV),
                declared_fields: match event {
                    Some(event) => AssField::ALL
                        .into_iter()
                        .filter(|name| event.field_index(*name).is_some())
                        .collect(),
                    None => Vec::new(),
                },
            }
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
    use super::{normalize, patch, views, AssField, CuePatch, CueView};
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
            effect: String::new(),
            layer: String::new(),
            margin_l: String::new(),
            margin_r: String::new(),
            margin_v: String::new(),
            declared_fields: Vec::new(),
        }
    }

    /// The patch the grid is sent when `set` changes the second row of a two row list, and the row
    /// it should be sent.
    fn patch_after(set: impl Fn(&mut CueView)) -> (CuePatch, CueView) {
        let before = vec![view("one"), view("two")];
        let mut after = before.clone();
        set(&mut after[1]);
        (patch(&before, &after), after.remove(1))
    }

    #[test]
    fn a_patch_that_changes_only_one_of_the_five_new_fields_is_a_patch() {
        // The complaint itself: before these five were carried, a write to any of them produced
        // `from=1 removed=0 cues=0` and the grid showed nothing.
        for (name, (shown, row)) in [
            ("effect", patch_after(|row| row.effect = "fad".to_owned())),
            ("layer", patch_after(|row| row.layer = "7".to_owned())),
            ("marginL", patch_after(|row| row.margin_l = "7".to_owned())),
            ("marginR", patch_after(|row| row.margin_r = "7".to_owned())),
            ("marginV", patch_after(|row| row.margin_v = "7".to_owned())),
        ] {
            assert_eq!(
                shown,
                CuePatch {
                    from: 1,
                    removed: 1,
                    cues: vec![row],
                },
                "a changed {name} must reach the grid"
            );
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

    /// Every declared field of the first row, in the order a panel would draw them.
    fn seven(document: &SubtitleDocument) -> Vec<String> {
        let view = views(document).remove(0);
        vec![
            view.style,
            view.actor,
            view.effect,
            view.layer,
            view.margin_l,
            view.margin_r,
            view.margin_v,
        ]
    }

    #[test]
    fn an_ass_row_carries_every_field_its_own_format_line_declares() {
        // The five that were writable and unreadable until now.
        assert_eq!(
            seven(&ass(concat!(
                "[Events]\n",
                "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
                "Dialogue: 2,0:00:01.00,0:00:02.00,Sign,Ingrid,10,20,30,fad,A shop front\n",
            ))),
            vec!["Sign", "Ingrid", "fad", "2", "10", "20", "30"]
        );
    }

    #[test]
    fn a_row_carries_the_files_own_spelling_of_a_number_and_of_what_is_not_one() {
        // A reader never refuses a file it can display, and never invents a value: `0000` is not
        // `0` and `left` is not an error. See styles-and-fields-tasks.md F2.
        let body = concat!(
            "[Events]\n",
            "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
            "Dialogue: 0000,0:00:01.00,0:00:02.00,Default,, 4 ,left,,,Hello\n",
        );
        let document = ass(body);
        assert_eq!(
            seven(&document),
            vec!["Default", "", "", "0000", "4", "left", ""]
        );
        assert_eq!(document.to_bytes(), body.as_bytes());
    }

    /// Which fields a row reports as declared, in `AssField::ALL` order whatever order the
    /// `Format:` line put them in.
    fn declared(document: &SubtitleDocument) -> Vec<AssField> {
        views(document).remove(0).declared_fields
    }

    #[test]
    fn a_blank_declared_field_and_one_the_format_line_never_declared_read_the_same_and_are_not() {
        // The four-state case a reviewer refused this on: both carry `""`, and only one of them
        // may be written. Nothing but `declared_fields` separates them.
        // See styles-and-fields-tasks.md F2.
        let blank = ass(concat!(
            "[Events]\n",
            "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
            "Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Hello\n",
        ));
        let absent = ass(concat!(
            "[Events]\n",
            "Format: Layer, Start, End, Text\n",
            "Dialogue: 0,0:00:01.00,0:00:02.00,Hello\n",
        ));
        assert_eq!(views(&blank).remove(0).effect, "");
        assert_eq!(views(&absent).remove(0).effect, "");
        assert_eq!(declared(&blank), AssField::ALL.to_vec());
        assert_eq!(declared(&absent), vec![AssField::Layer]);
    }

    #[test]
    fn two_events_sections_give_their_own_rows_their_own_declared_fields() {
        // Why the list is per row and not one answer for the document: each section carries its own
        // `Format:` line, and the write already answers per event. See styles-and-fields-tasks.md F2.
        let document = ass(concat!(
            "[Events]\n",
            "Format: Layer, Start, End, Text\n",
            "Dialogue: 0,0:00:01.00,0:00:02.00,First\n",
            "[Events]\n",
            "Format: Start, End, Style, Effect, Text\n",
            "Dialogue: 0:00:03.00,0:00:04.00,Sign,fad,Second\n",
        ));
        let rows = views(&document);
        assert_eq!(rows[0].declared_fields, vec![AssField::Layer]);
        assert_eq!(
            rows[1].declared_fields,
            vec![AssField::Style, AssField::Effect]
        );
    }

    #[test]
    fn a_row_reports_its_declared_fields_in_one_order_whatever_order_the_line_declares_them() {
        // The list is a set, not a layout: a shuffled `Format:` line reports the same list, so no
        // reader can take a position in it for a position on the line.
        assert_eq!(
            declared(&ass(concat!(
                "[Events]\n",
                "Format: Name, Effect, Start, End, MarginV, Style, Text\n",
                "Dialogue: Ingrid,fad,0:00:01.00,0:00:02.00,30,Sign,Hello\n",
            ))),
            vec![
                AssField::Style,
                AssField::Actor,
                AssField::Effect,
                AssField::MarginV,
            ]
        );
    }

    #[test]
    fn a_field_the_line_puts_on_the_text_column_is_not_declared_to_a_control() {
        // The last column is the text whatever the line calls it, so a field declared there is
        // absent to the write (`before_text`) and must be absent to the row as well. Style sits
        // before it and is declared; Effect is the last column and is not.
        assert_eq!(
            declared(&ass(concat!(
                "[Events]\n",
                "Format: Layer, Start, End, Text, Style, Effect\n",
                "Dialogue: 0,0:00:01.00,0:00:02.00,Hello,Sign,fad\n",
            ))),
            vec![AssField::Style, AssField::Layer]
        );
    }

    #[test]
    fn a_row_of_a_format_that_declares_none_of_them_carries_seven_empty_strings() {
        assert_eq!(
            seven(&ass(concat!(
                "[Events]\n",
                "Format: Start, End, Text\n",
                "Dialogue: 0:00:01.00,0:00:02.00,Nothing named here\n",
            ))),
            vec![""; 7]
        );
    }

    #[test]
    fn an_srt_row_carries_seven_empty_strings() {
        let document = sublore_formats::parse(
            SubtitleFormat::Srt,
            b"1\n00:00:01,000 --> 00:00:02,000\nHello\n",
        )
        .expect("the fixture parses");
        assert_eq!(seven(&document), vec![""; 7]);
        // And declares none of them, so an SRT row draws no field control at all.
        assert!(views(&document).remove(0).declared_fields.is_empty());
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
