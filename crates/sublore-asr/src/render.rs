//! Generated cues to SRT bytes.
//!
//! This is the only place in the crate that writes subtitle syntax, and it writes one fixed shape:
//! no BOM, LF terminators, 1-based numbering with no gaps, `HH:MM:SS,mmm --> HH:MM:SS,mmm`, the
//! text lines, and one blank line after every cue including the last.
//!
//! Nothing here builds a document. The bytes go to [`sublore_formats::parse`], which runs M1's
//! coverage guard and gives back a document that writes these exact bytes again, so generated
//! subtitles reach disk through the same path a file the user opened does. See BACKLOG.md M3.3.

use crate::cues::GeneratedCue;

/// Render `cues` as an SRT file. An empty cue list renders an empty file, which parses.
pub fn srt(cues: &[GeneratedCue]) -> Vec<u8> {
    let mut text = String::new();

    for (index, cue) in cues.iter().enumerate() {
        text.push_str(&(index + 1).to_string());
        text.push('\n');
        text.push_str(&timecode(cue.start_ms));
        text.push_str(" --> ");
        text.push_str(&timecode(cue.end_ms));
        text.push('\n');
        for line in &cue.lines {
            text.push_str(line);
            text.push('\n');
        }
        text.push('\n');
    }

    text.into_bytes()
}

/// `HH:MM:SS,mmm`. Hours run past two digits rather than wrapping, so a long file stays readable
/// and the parser reports a time it cannot hold instead of being handed a wrong one.
fn timecode(millis: u32) -> String {
    let (hours, rest) = (millis / 3_600_000, millis % 3_600_000);
    let (minutes, rest) = (rest / 60_000, rest % 60_000);
    let (seconds, milliseconds) = (rest / 1_000, rest % 1_000);
    format!("{hours:02}:{minutes:02}:{seconds:02},{milliseconds:03}")
}

#[cfg(test)]
mod tests {
    use super::{srt, timecode};
    use crate::cues::GeneratedCue;

    fn cue(start_ms: u32, end_ms: u32, lines: &[&str]) -> GeneratedCue {
        GeneratedCue {
            start_ms,
            end_ms,
            lines: lines.iter().map(|line| (*line).to_owned()).collect(),
        }
    }

    #[test]
    fn writes_the_one_shape_it_knows() {
        let bytes = srt(&[
            cue(0, 1_000, &["first"]),
            cue(1_040, 2_500, &["second line", "and its wrap"]),
        ]);

        assert_eq!(
            String::from_utf8(bytes).expect("SRT is written as UTF-8"),
            "1\n00:00:00,000 --> 00:00:01,000\nfirst\n\n\
             2\n00:00:01,040 --> 00:00:02,500\nsecond line\nand its wrap\n\n"
        );
    }

    #[test]
    fn no_cues_is_an_empty_file() {
        assert!(srt(&[]).is_empty());
    }

    #[test]
    fn spells_every_field_at_its_full_width() {
        assert_eq!(timecode(0), "00:00:00,000");
        assert_eq!(timecode(1), "00:00:00,001");
        assert_eq!(timecode(3_599_999), "00:59:59,999");
        assert_eq!(timecode(3_600_000), "01:00:00,000");
        assert_eq!(timecode(359_999_999), "99:59:59,999");
        // Past a day the hours simply get wider; SRT has no two-digit rule.
        assert_eq!(timecode(360_000_000), "100:00:00,000");
    }
}
