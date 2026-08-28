//! The timestamp scanner every format shares, and the value it produces.
//!
//! Accepted shapes are a closed list: `h+:mm:ss<sep>f{1,3}` and `mm:ss<sep>f{1,3}`, where `<sep>` is
//! `,` or `.`. Hours carry 1 to 9 digits; minutes and seconds exactly 2 each, any value 00-99. The
//! fraction is mandatory: 1 digit is tenths, 2 centiseconds, 3 milliseconds, and 4 or more is a
//! failure. Both separators are read by all three formats, and the exact spelling is kept in `raw`
//! so a save writes back what was read. See BACKLOG.md M1.1.

use crate::error::ParseErrorKind;
use crate::span::Span;

/// 999:59:59.999. Anything larger is [`ParseErrorKind::TimecodeOutOfRange`].
pub const MAX_TIMECODE_MS: u32 = 3_599_999_999;

/// Hours wider than this are a typo, not a timestamp.
const MAX_HOUR_DIGITS: usize = 9;

/// Milliseconds since zero, plus the exact spelling it was written with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timecode {
    millis: u32,
    raw: Span,
}

impl Timecode {
    /// Built by the scanner and by the parsers that call it; the pair is never derived twice.
    pub fn new(millis: u32, raw: Span) -> Self {
        Self { millis, raw }
    }

    pub fn millis(self) -> u32 {
        self.millis
    }

    pub fn raw(self) -> Span {
        self.raw
    }
}

/// Read a timestamp starting exactly at `offset`. Returns the timecode and the offset just past it,
/// so a caller can keep scanning the same line without measuring anything itself.
pub fn parse_timecode(body: &str, offset: usize) -> Result<(Timecode, usize), ParseErrorKind> {
    let bytes = body.as_bytes();

    let (first, first_len, after_first) = digits(bytes, offset);
    if first_len == 0 || bytes.get(after_first) != Some(&b':') {
        return Err(ParseErrorKind::BadTimecode);
    }
    let (second, second_len, after_second) = digits(bytes, after_first + 1);
    if second_len != 2 {
        return Err(ParseErrorKind::BadTimecode);
    }

    let (hours, minutes, seconds, after_seconds) = match bytes.get(after_second) {
        Some(b':') => {
            if first_len > MAX_HOUR_DIGITS {
                return Err(ParseErrorKind::BadTimecode);
            }
            let (third, third_len, after_third) = digits(bytes, after_second + 1);
            if third_len != 2 {
                return Err(ParseErrorKind::BadTimecode);
            }
            (first, second, third, after_third)
        }
        // The `mm:ss` short form VTT allows: minutes are two digits there, exactly like seconds.
        Some(b',' | b'.') => {
            if first_len != 2 {
                return Err(ParseErrorKind::BadTimecode);
            }
            (0, first, second, after_second)
        }
        _ => return Err(ParseErrorKind::BadTimecode),
    };

    if !matches!(bytes.get(after_seconds), Some(b',' | b'.')) {
        return Err(ParseErrorKind::BadTimecode);
    }
    let (fraction, fraction_len, end) = digits(bytes, after_seconds + 1);
    if !(1..=3).contains(&fraction_len) {
        return Err(ParseErrorKind::BadTimecode);
    }
    let scale: u64 = match fraction_len {
        1 => 100,
        2 => 10,
        _ => 1,
    };

    let total = hours
        .saturating_mul(3_600_000)
        .saturating_add(minutes.saturating_mul(60_000))
        .saturating_add(seconds.saturating_mul(1_000))
        .saturating_add(fraction.saturating_mul(scale));
    let millis = u32::try_from(total).map_err(|_| ParseErrorKind::TimecodeOutOfRange)?;
    if millis > MAX_TIMECODE_MS {
        return Err(ParseErrorKind::TimecodeOutOfRange);
    }

    Ok((Timecode::new(millis, Span::new(offset, end)), end))
}

/// The ASCII digit run at `from`: its value, how many digits it had, and where it ended.
/// Values saturate, so a hostile digit run cannot overflow before the range check sees it.
fn digits(bytes: &[u8], from: usize) -> (u64, usize, usize) {
    let mut value: u64 = 0;
    let mut index = from;
    while let Some(&byte) = bytes.get(index) {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value
            .saturating_mul(10)
            .saturating_add(u64::from(byte - b'0'));
        index += 1;
    }
    (value, index.saturating_sub(from), index)
}

#[cfg(test)]
mod tests {
    use super::{parse_timecode, MAX_TIMECODE_MS};
    use crate::error::ParseErrorKind;

    fn millis(text: &str) -> u32 {
        let (timecode, end) = parse_timecode(text, 0).expect("valid timestamp");
        assert_eq!(end, text.len(), "the scanner must consume the whole input");
        assert_eq!(timecode.raw().range(), 0..text.len());
        timecode.millis()
    }

    fn kind(text: &str) -> ParseErrorKind {
        parse_timecode(text, 0).expect_err("invalid timestamp")
    }

    #[test]
    fn reads_the_srt_and_vtt_long_form() {
        assert_eq!(millis("00:00:00,000"), 0);
        assert_eq!(millis("01:02:03,004"), 3_723_004);
        assert_eq!(millis("01:02:03.004"), 3_723_004);
    }

    #[test]
    fn reads_short_and_wide_fractions() {
        assert_eq!(millis("00:00:01,5"), 1_500);
        assert_eq!(millis("00:00:01,50"), 1_500);
        assert_eq!(millis("00:00:01,500"), 1_500);
    }

    #[test]
    fn reads_the_vtt_short_form() {
        assert_eq!(millis("00:01.000"), 1_000);
        assert_eq!(millis("12:34.560"), 754_560);
    }

    #[test]
    fn reads_one_digit_hours_and_out_of_clock_fields() {
        assert_eq!(millis("1:00:00,000"), 3_600_000);
        assert_eq!(millis("00:99:99,999"), 6_039_999);
    }

    #[test]
    fn stops_at_the_first_byte_that_cannot_belong() {
        let (timecode, end) = parse_timecode("00:00:01,000 --> 00:00:02,000", 0).expect("valid");
        assert_eq!(timecode.millis(), 1_000);
        assert_eq!(end, 12);

        let (timecode, end) = parse_timecode("00:00:01,000 --> 00:00:02,000", 17).expect("valid");
        assert_eq!(timecode.millis(), 2_000);
        assert_eq!(end, 29);
        assert_eq!(timecode.raw().range(), 17..29);
    }

    #[test]
    fn refuses_a_missing_or_oversized_fraction() {
        assert_eq!(kind("00:00:01"), ParseErrorKind::BadTimecode);
        assert_eq!(kind("00:00:01,"), ParseErrorKind::BadTimecode);
        assert_eq!(kind("00:00:01,0000"), ParseErrorKind::BadTimecode);
    }

    #[test]
    fn refuses_malformed_fields() {
        assert_eq!(kind(""), ParseErrorKind::BadTimecode);
        assert_eq!(kind("--> 00:00:01,000"), ParseErrorKind::BadTimecode);
        assert_eq!(kind("00:0:01,000"), ParseErrorKind::BadTimecode);
        assert_eq!(kind("00:00:1,000"), ParseErrorKind::BadTimecode);
        assert_eq!(kind("0:01.000"), ParseErrorKind::BadTimecode);
        assert_eq!(kind("1234567890:00:00,000"), ParseErrorKind::BadTimecode);
        assert_eq!(kind("00:00:01;000"), ParseErrorKind::BadTimecode);
    }

    #[test]
    fn refuses_values_past_the_ceiling() {
        assert_eq!(millis("999:59:59,999"), MAX_TIMECODE_MS);
        assert_eq!(kind("1000:00:00,000"), ParseErrorKind::TimecodeOutOfRange);
        assert_eq!(
            kind("999999999:00:00,000"),
            ParseErrorKind::TimecodeOutOfRange
        );
    }
}
