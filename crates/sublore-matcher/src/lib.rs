//! Licensed under the GNU GPL v3 or later, with the section 7 additional permission for modules
//! loaded through `sublore-module-api`. See LICENSE at the root of the repository.

//! Finding a term in a line of subtitle text.
//!
//! One comparison, in the open core, because the free product's find and a loaded module's quality
//! pass ask the same question and two answers to it would put a term in a line the user's own search
//! could not find. The module reaches this through the host table's `find` slot rather than through
//! a dependency, which is a licence line rather than a preference: see `module-abi.md` §4.6.
//!
//! This is a literal term. The user's own regular expression is a different operation that runs
//! where it can be killed, and the two share a subject rather than a mechanism.
//!
//! See docs/matcher-tasks.md.

/// Fold case while comparing. Off means an exact comparison.
pub const MATCH_CASE: u32 = 1;
/// Match against the text a reader sees, with override blocks and tags out of the way.
pub const SKIP_TAGS: u32 = 2;

/// Where a term was found, in offsets into the raw line.
///
/// Raw and not visible, because a caller highlights or replaces the line it was given. A match that
/// spans a tag covers the tag: it sits inside the matched text and neither side of a replacement is
/// where it belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub start: usize,
    pub end: usize,
}

/// The text a reader sees, and the way back to the line it came from.
#[derive(Clone, Debug)]
pub struct Visible {
    text: String,
    /// One entry per stretch that survived: where it starts in `text`, and where in the raw line.
    /// Ascending, and never empty for a line with any visible character in it.
    runs: Vec<(usize, usize)>,
    /// The raw line's length, so an offset at the very end has somewhere to map to.
    raw_len: usize,
}

impl Visible {
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The raw offset a visible offset starts at.
    pub fn raw_start(&self, visible: usize) -> usize {
        self.map(visible, false)
    }

    /// The raw offset a visible offset ends at.
    ///
    /// **Not the same function**, and a check caught it. An offset that sits exactly where one run
    /// ends and the next begins is the start of the next when it opens a match and the end of the
    /// previous when it closes one. Mapping both the same way makes a match that ends just before a
    /// tag swallow the tag.
    pub fn raw_end(&self, visible: usize) -> usize {
        self.map(visible, true)
    }

    fn map(&self, visible: usize, closing: bool) -> usize {
        let mut answer = self.raw_len;
        for &(from, raw) in &self.runs {
            let run_len = self.run_len(from);
            let inside = if closing {
                visible > from && visible <= from + run_len
            } else {
                visible >= from && visible < from + run_len
            };
            if inside {
                return raw + (visible - from);
            }
            if !closing && visible < from {
                return raw;
            }
        }
        if let Some(&(from, raw)) = self.runs.last() {
            answer = raw + self.run_len(from);
        }
        answer.min(self.raw_len)
    }

    /// How many bytes of visible text a run holds: up to the next run's start, or to the end.
    fn run_len(&self, from: usize) -> usize {
        self.runs
            .iter()
            .find(|(start, _)| *start > from)
            .map_or(self.text.len(), |(next, _)| *next)
            - from
    }
}

/// Split a line into what a reader sees and where each piece came from.
///
/// Two shapes are taken out: an ASS override block, `{...}`, and an HTML-style tag, `<...>`. An
/// opener with no closer is **not** a tag: a line that ends mid-block is a broken line, and eating
/// the rest of it would hide words the user wrote.
pub fn visible(raw: &str) -> Visible {
    let bytes = raw.as_bytes();
    let mut text = String::with_capacity(raw.len());
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut at = 0usize;
    let mut run_open = false;

    while at < raw.len() {
        let closer = match bytes[at] {
            b'{' => Some(b'}'),
            b'<' => Some(b'>'),
            _ => None,
        };
        if let Some(closer) = closer {
            if let Some(end) = raw[at..].bytes().position(|byte| byte == closer) {
                at += end + 1;
                run_open = false;
                continue;
            }
            // No closer anywhere after it: this is text, and every byte to the end of the line is.
        }
        if !run_open {
            runs.push((text.len(), at));
            run_open = true;
        }
        // One character at a time, so a multi-byte one is never cut and the two offsets stay in
        // step. `raw[at..]` starts on a boundary by construction: the skips above land on one.
        let ch = raw[at..].chars().next().unwrap_or('\u{fffd}');
        text.push(ch);
        at += ch.len_utf8();
    }

    Visible {
        text,
        runs,
        raw_len: raw.len(),
    }
}

/// Every place `needle` occurs in `haystack`, pushed in order until `on_hit` says to stop.
///
/// `on_hit` returns whether to keep looking, so a caller that wants the first hit pays for the
/// first hit. Returns how many were pushed.
pub fn find(
    haystack: &str,
    needle: &str,
    options: u32,
    mut on_hit: impl FnMut(Hit) -> bool,
) -> usize {
    // An empty needle matches nothing rather than everything. Matching at every position is never
    // what a search meant, and it is the same rule the frontend's own find has.
    if needle.is_empty() || haystack.is_empty() {
        return 0;
    }
    let fold = options & MATCH_CASE == 0;

    if options & SKIP_TAGS == 0 {
        return walk(haystack, needle, fold, |start, end| {
            on_hit(Hit { start, end })
        });
    }

    let seen = visible(haystack);
    walk(seen.text(), needle, fold, |start, end| {
        on_hit(Hit {
            start: seen.raw_start(start),
            end: seen.raw_end(end),
        })
    })
}

/// Every occurrence, in offsets into `haystack` itself.
fn walk(
    haystack: &str,
    needle: &str,
    fold: bool,
    mut on_hit: impl FnMut(usize, usize) -> bool,
) -> usize {
    let mut found = 0usize;
    let mut at = 0usize;
    while at < haystack.len() {
        match matches_at(&haystack[at..], needle, fold) {
            Some(length) if length > 0 => {
                found += 1;
                if !on_hit(at, at + length) {
                    return found;
                }
                // Past the match, so an overlapping one is not reported twice.
                at += length;
            }
            // Either no match here, or one of zero length, which only a folding that produced
            // nothing could give and which would never advance.
            _ => {
                let ch = haystack[at..].chars().next().unwrap_or('\u{fffd}');
                at += ch.len_utf8();
            }
        }
    }
    found
}

/// How many bytes of `haystack` `needle` takes from its start, or none.
///
/// **Nothing here builds a folded string, and that is the whole design.** `"İ".to_lowercase()` is
/// two characters, so a folded haystack carries offsets the real one does not and every hit
/// computed in it lands in the wrong place. Folding one character at a time as the walk goes keeps
/// every offset the haystack's own.
fn matches_at(haystack: &str, needle: &str, fold: bool) -> Option<usize> {
    if !fold {
        return haystack.starts_with(needle).then_some(needle.len());
    }

    let mut taken = 0usize;
    let mut wanted = needle.chars().flat_map(char::to_lowercase);
    let mut have = haystack.chars();
    // The folded forms are compared, and the bytes counted are the haystack's unfolded ones. A
    // character whose folding is longer than one character is consumed whole either way.
    let mut spare: Vec<char> = Vec::new();

    for want in wanted.by_ref() {
        if spare.is_empty() {
            let next = have.next()?;
            taken += next.len_utf8();
            spare.extend(next.to_lowercase());
        }
        if spare.remove(0) != want {
            return None;
        }
    }
    // A haystack character whose folding ran past the end of the needle is not a match: half of a
    // character is not a match on it.
    if spare.is_empty() {
        Some(taken)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(haystack: &str, needle: &str, options: u32) -> Vec<Hit> {
        let mut found = Vec::new();
        find(haystack, needle, options, |hit| {
            found.push(hit);
            true
        });
        found
    }

    #[test]
    fn a_line_with_no_tags_is_its_own_visible_text() {
        let line = "By then the fog had eaten the boats.";
        let seen = visible(line);
        assert_eq!(seen.text(), line);
        assert_eq!(seen.raw_start(0), 0);
        assert_eq!(seen.raw_start(12), 12);
        assert_eq!(seen.raw_end(line.len()), line.len());
    }

    #[test]
    fn an_opener_with_no_closer_is_text_and_not_a_tag() {
        // A line that ends mid-block is a broken line, and eating the rest would hide what the user
        // wrote. Both openers, because both are stripped and both can be left hanging.
        assert_eq!(visible("the {fog").text(), "the {fog");
        assert_eq!(visible("the <fog").text(), "the <fog");
        assert_eq!(visible("a { b < c").text(), "a { b < c");
    }

    #[test]
    fn a_term_is_found_at_the_byte_the_raw_line_has_it() {
        let line = "By then the fog had eaten the boats.";
        assert_eq!(hits(line, "fog", 0), vec![Hit { start: 12, end: 15 }]);
        assert_eq!(&line[12..15], "fog");
    }

    #[test]
    fn case_is_folded_until_it_is_asked_not_to_be() {
        let line = "By then the fog had eaten the boats.";
        assert_eq!(hits(line, "FOG", 0).len(), 1);
        assert!(hits(line, "FOG", MATCH_CASE).is_empty());
        assert_eq!(hits(line, "fog", MATCH_CASE).len(), 1);
    }

    #[test]
    fn a_tag_is_stepped_over_only_when_it_is_asked_for() {
        let line = "the {\\i1}fog";
        assert!(
            hits(line, "fog", 0).len() == 1,
            "the term is there either way"
        );
        // The one that matters: the term is not adjacent to `the` without the tag being skipped.
        assert!(hits(line, "the fog", 0).is_empty());
        let skipped = hits(line, "the fog", SKIP_TAGS);
        assert_eq!(skipped.len(), 1);
        let hit = skipped[0];
        assert_eq!(&line[hit.start..hit.end], "the {\\i1}fog");
    }

    #[test]
    fn a_term_split_by_a_tag_is_found_and_its_range_covers_the_tag() {
        let line = "fo{\\i1}g";
        assert!(hits(line, "fog", 0).is_empty());
        let found = hits(line, "fog", SKIP_TAGS);
        assert_eq!(found.len(), 1);
        // The cost, stated rather than discovered: a replacement here eats the tag, because the tag
        // is inside the matched text and neither side of the replacement is where it belongs.
        assert_eq!(&line[found[0].start..found[0].end], "fo{\\i1}g");
    }

    #[test]
    fn a_match_that_ends_before_a_tag_does_not_swallow_it() {
        let line = "fog{\\i1} rolled in";
        let found = hits(line, "fog", SKIP_TAGS);
        assert_eq!(found.len(), 1);
        assert_eq!(
            &line[found[0].start..found[0].end],
            "fog",
            "the range stopped at the end of the term"
        );
    }

    #[test]
    fn folding_never_moves_an_offset_even_where_it_changes_a_length() {
        // The Turkish dotted capital is two bytes and folds to three, so a comparison that folded
        // the whole line first would count from a string one byte longer than this one and report
        // every hit after it one byte late.
        let line = "the İstanbul boats";
        let found = hits(line, "boats", 0);
        assert_eq!(found.len(), 1);
        assert_eq!(
            &line[found[0].start..found[0].end],
            "boats",
            "the range is the raw line's own bytes, not a folded copy's"
        );
    }

    #[test]
    fn a_capital_that_folds_to_two_characters_is_not_the_one_character_it_looks_like() {
        // Measured rather than assumed, and it surprised the check that first asserted the
        // opposite. Full case folding takes `İ` to `i` plus a combining dot, so a needle spelled
        // with a plain `i` is a different string and does not match. The frontend's own find does
        // the same thing for the same reason, so the two agree, which is what decision 6 is for.
        let line = "the İstanbul boats";
        assert!(hits(line, "istanbul", 0).is_empty());
        let found = hits(line, "İstanbul", 0);
        assert_eq!(found.len(), 1);
        // The range in raw bytes, not the folded needle's length. A comparison that answered with
        // what it folded rather than with what it consumed would end this hit one byte late, and
        // the count above would not notice.
        assert_eq!(&line[found[0].start..found[0].end], "İstanbul");
    }

    #[test]
    fn nothing_matches_nothing() {
        assert!(hits("a line", "", 0).is_empty());
        assert!(hits("", "term", 0).is_empty());
        assert!(hits("short", "a much longer needle", 0).is_empty());
    }

    #[test]
    fn every_occurrence_is_pushed_and_the_caller_may_stop() {
        let line = "fog and fog and fog";
        assert_eq!(hits(line, "fog", 0).len(), 3);

        let mut seen = 0;
        let counted = find(line, "fog", 0, |_| {
            seen += 1;
            false
        });
        assert_eq!((seen, counted), (1, 1), "a caller that stops pays for one");
    }

    #[test]
    fn occurrences_do_not_overlap() {
        // `aa` in `aaa` is one match and not two: the walk resumes past what it took.
        assert_eq!(hits("aaa", "aa", 0).len(), 1);
    }
}
