//! Reading the child's two pipes: bounded line splitting, and the three lines that mean
//! something. See BACKLOG.md M3.1.
//!
//! whisper prints its percentage to stderr and its transcript to stdout, and both pipes have to
//! be drained or the child blocks the moment one of them fills. Everything unrecognised is
//! dropped here; only the caller's tail keeps a copy, and only for the log.

use std::io::{self, BufRead};

/// The literal whisper's progress callback prints. Anything else on stderr is noise.
const PROGRESS_PREFIX: &str = "whisper_print_progress_callback: progress =";
/// What whisper says when it rejects one of our flags. It exits 0 while saying it.
const UNKNOWN_ARGUMENT: &str = "error: unknown argument";
/// Longest line kept. A stream with no newline in it must not be able to allocate without bound,
/// so past this the rest of the line is read and discarded.
pub const MAX_LINE_BYTES: usize = 8 * 1024;

/// Which pipe a line came from. whisper puts progress on one and transcript on the other, and the
/// difference is the only reason both are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// `whisper_print_progress_callback: progress =  35%`, clamped to 0..=100.
    Progress(u8),
    /// `[00:00:00.000 --> 00:00:04.320]   text`, carrying the end of the segment in ms. A finer
    /// progress signal than the percentage, which only moves every five points.
    Segment {
        end_ms: u32,
    },
    /// A flag whisper does not know. Always a Sublore bug, and it does not change the exit code.
    BadArguments,
    Other,
}

pub fn parse_line(line: &str) -> Line {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(rest) = line.strip_prefix(PROGRESS_PREFIX) {
        if let Some(percent) = parse_percent(rest) {
            return Line::Progress(percent);
        }
        return Line::Other;
    }
    if line.starts_with(UNKNOWN_ARGUMENT) {
        return Line::BadArguments;
    }
    if let Some(end_ms) = parse_segment_end(line) {
        return Line::Segment { end_ms };
    }
    Line::Other
}

fn parse_percent(rest: &str) -> Option<u8> {
    let digits = rest.trim().strip_suffix('%')?;
    let value: u32 = digits.trim().parse().ok()?;
    Some(value.min(100) as u8)
}

/// `[00:00:00.000 --> 00:00:04.320]   text` -> 4320.
fn parse_segment_end(line: &str) -> Option<u32> {
    let inside = line.strip_prefix('[')?;
    let close = inside.find(']')?;
    let (times, _) = inside.split_at(close);
    let (_, end) = times.split_once(" --> ")?;
    parse_timestamp(end.trim())
}

/// `HH:MM:SS.mmm` or `HH:MM:SS,mmm` to milliseconds. Saturating: a nonsense hour count is a
/// clamped number, never a panic and never a wrapped one.
fn parse_timestamp(text: &str) -> Option<u32> {
    let (hours, rest) = text.split_once(':')?;
    let (minutes, seconds) = rest.split_once(':')?;
    let (seconds, millis) = seconds
        .split_once('.')
        .or_else(|| seconds.split_once(','))?;
    if millis.len() != 3 {
        return None;
    }
    let hours: u64 = hours.parse().ok()?;
    let minutes: u64 = minutes.parse().ok()?;
    let seconds: u64 = seconds.parse().ok()?;
    let millis: u64 = millis.parse().ok()?;
    if minutes > 59 || seconds > 59 {
        return None;
    }
    let total = hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + millis;
    Some(total.min(u32::MAX as u64) as u32)
}

/// Split `reader` into lines and hand each one to `sink`, never buffering more than
/// [`MAX_LINE_BYTES`] of any single line. A final line without a terminator is still delivered.
///
/// The bound is the point: `BufRead::read_line` on a child that prints a gigabyte without a
/// newline would allocate all of it.
pub fn for_each_line<R: BufRead>(mut reader: R, mut sink: impl FnMut(&str)) -> io::Result<()> {
    let mut line: Vec<u8> = Vec::with_capacity(256);
    let mut truncated = false;
    loop {
        let consumed;
        let complete;
        {
            let available = loop {
                match reader.fill_buf() {
                    Ok(buffer) => break buffer,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            };
            if available.is_empty() {
                if !line.is_empty() {
                    emit(&line, &mut sink);
                }
                return Ok(());
            }
            match available.iter().position(|&byte| byte == b'\n') {
                Some(index) => {
                    append(&mut line, &available[..index], &mut truncated);
                    consumed = index + 1;
                    complete = true;
                }
                None => {
                    append(&mut line, available, &mut truncated);
                    consumed = available.len();
                    complete = false;
                }
            }
        }
        reader.consume(consumed);
        if complete {
            emit(&line, &mut sink);
            line.clear();
            truncated = false;
        }
    }
}

fn append(line: &mut Vec<u8>, bytes: &[u8], truncated: &mut bool) {
    if *truncated {
        return;
    }
    let room = MAX_LINE_BYTES - line.len();
    if bytes.len() >= room {
        line.extend_from_slice(&bytes[..room]);
        *truncated = true;
        return;
    }
    line.extend_from_slice(bytes);
}

fn emit(line: &[u8], sink: &mut impl FnMut(&str)) {
    // Lossy on purpose: a child that prints a broken byte must not silence the pipe.
    let text = String::from_utf8_lossy(line);
    sink(text.trim_end_matches('\r'));
}

/// The last few kilobytes of a pipe, kept as the technical detail of a failure. Whole lines only,
/// so the log never shows half a character.
#[derive(Debug)]
pub struct Tail {
    lines: std::collections::VecDeque<String>,
    bytes: usize,
    limit: usize,
}

impl Tail {
    pub fn new(limit: usize) -> Self {
        Self {
            lines: std::collections::VecDeque::new(),
            bytes: 0,
            limit,
        }
    }

    pub fn push(&mut self, line: &str) {
        if line.is_empty() {
            return;
        }
        self.bytes += line.len() + 1;
        self.lines.push_back(line.to_owned());
        while self.bytes > self.limit && self.lines.len() > 1 {
            if let Some(dropped) = self.lines.pop_front() {
                self.bytes -= dropped.len() + 1;
            }
        }
    }

    pub fn into_string(self) -> String {
        self.lines.into_iter().collect::<Vec<_>>().join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::{for_each_line, parse_line, Line, Tail, MAX_LINE_BYTES};
    use std::io::Cursor;

    #[test]
    fn the_measured_progress_literal_parses() {
        assert_eq!(
            parse_line("whisper_print_progress_callback: progress =  35%"),
            Line::Progress(35)
        );
        assert_eq!(
            parse_line("whisper_print_progress_callback: progress = 100%"),
            Line::Progress(100)
        );
        assert_eq!(
            parse_line("whisper_print_progress_callback: progress =   5%\r"),
            Line::Progress(5),
            "a CRLF pipe must not hide the percentage"
        );
    }

    #[test]
    fn a_percentage_over_one_hundred_is_clamped_not_wrapped() {
        assert_eq!(
            parse_line("whisper_print_progress_callback: progress = 4000%"),
            Line::Progress(100)
        );
    }

    #[test]
    fn junk_after_the_progress_prefix_is_not_progress() {
        assert_eq!(
            parse_line("whisper_print_progress_callback: progress = ??%"),
            Line::Other
        );
    }

    #[test]
    fn the_measured_segment_literal_parses() {
        assert_eq!(
            parse_line("[00:00:00.000 --> 00:00:04.320]   Subloor keep your terminology"),
            Line::Segment { end_ms: 4320 }
        );
        assert_eq!(
            parse_line("[00:01:02.500 --> 01:00:00.001]   later"),
            Line::Segment { end_ms: 3_600_001 }
        );
    }

    #[test]
    fn a_broken_timestamp_is_not_a_segment() {
        for line in [
            "[00:00:00.000 --> junk]   text",
            "[00:00:00.000 00:00:04.320]   text",
            "[00:00:00.000 --> 00:99:04.320]   text",
            "[00:00:00.000 --> 00:00:04.32]   text",
            "00:00:00.000 --> 00:00:04.320",
        ] {
            assert_eq!(parse_line(line), Line::Other, "{line}");
        }
    }

    #[test]
    fn an_unknown_argument_is_recognised_because_the_exit_code_will_not_say_so() {
        assert_eq!(
            parse_line("error: unknown argument: -zz"),
            Line::BadArguments
        );
    }

    #[test]
    fn lines_are_split_on_lf_and_the_last_one_needs_no_terminator() {
        let mut seen = Vec::new();
        for_each_line(Cursor::new(b"one\ntwo\r\nthree".to_vec()), |line| {
            seen.push(line.to_owned())
        })
        .expect("a cursor cannot fail");
        assert_eq!(seen, vec!["one", "two", "three"]);
    }

    #[test]
    fn an_empty_pipe_produces_nothing() {
        let mut seen = 0;
        for_each_line(Cursor::new(Vec::new()), |_| seen += 1).expect("a cursor cannot fail");
        assert_eq!(seen, 0);
    }

    #[test]
    fn a_line_longer_than_the_cap_is_truncated_and_its_remainder_discarded() {
        let mut blob = vec![b'x'; MAX_LINE_BYTES * 4];
        blob.push(b'\n');
        blob.extend_from_slice(b"after\n");
        let mut seen = Vec::new();
        for_each_line(Cursor::new(blob), |line| seen.push(line.to_owned()))
            .expect("a cursor cannot fail");
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].len(), MAX_LINE_BYTES);
        assert_eq!(seen[1], "after", "the next line still arrives whole");
    }

    #[test]
    fn a_tail_keeps_the_end_and_stays_under_its_limit() {
        let mut tail = Tail::new(64);
        for index in 0..100 {
            tail.push(&format!("line {index}"));
        }
        let text = tail.into_string();
        assert!(text.len() <= 64, "kept {} bytes", text.len());
        assert!(text.ends_with("line 99"), "{text}");
        assert!(!text.contains("line 0\n"));
    }

    #[test]
    fn a_tail_of_one_enormous_line_still_holds_that_line() {
        let mut tail = Tail::new(16);
        tail.push("a line far longer than the limit");
        assert_eq!(tail.into_string(), "a line far longer than the limit");
    }
}
