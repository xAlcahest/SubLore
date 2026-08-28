//! The fixture harness every format suite shares. Frozen once M1.1 lands: M1.2 and M1.3 consume it
//! as given and keep any extra helpers in their own test files.
//!
//! Byte-identity alone would also pass for a parser that returned the input untouched, so
//! [`assert_round_trip`] proves the model too: the segments tile the body, and every span a cue
//! points at lies inside its own segment. See BACKLOG.md M1.1.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use sublore_formats::{SegmentKind, SubtitleDocument, SubtitleFormat};

pub struct FixtureDirs {
    pub clean: PathBuf,
    pub malformed: PathBuf,
}

/// `fixtures/subtitles/{format_dir}/{clean,malformed}`, resolved from the crate manifest so the
/// tests do not care which directory cargo was run from.
pub fn dirs(format_dir: &str) -> FixtureDirs {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("subtitles")
        .join(format_dir);
    FixtureDirs {
        clean: root.join("clean"),
        malformed: root.join("malformed"),
    }
}

/// Every fixture in `dir` with one of `extensions`, sorted by name, read as raw bytes. A missing
/// directory or a shrinking tree fails loudly: deleting fixtures must turn the suite red.
pub fn fixtures(dir: &Path, extensions: &[&str], minimum: usize) -> Vec<(PathBuf, Vec<u8>)> {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|error| {
        panic!("fixture directory {} is unreadable: {error}", dir.display())
    });
    let mut found = Vec::new();
    for entry in entries {
        let path = entry
            .unwrap_or_else(|error| panic!("{} holds an unreadable entry: {error}", dir.display()))
            .path();
        let wanted = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension));
        if !wanted {
            continue;
        }
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
        found.push((path, bytes));
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    assert!(
        found.len() >= minimum,
        "{} holds {} {extensions:?} fixtures, the suite guards at least {minimum}",
        dir.display(),
        found.len()
    );
    found
}

/// Parse, then hold the parser to the whole contract: the bytes come back exactly, the segments
/// tile the body, and no cue span escapes the segment that owns it.
pub fn assert_round_trip(format: SubtitleFormat, path: &Path, bytes: &[u8]) -> SubtitleDocument {
    let name = path.display();
    let document = sublore_formats::parse(format, bytes).unwrap_or_else(|error| {
        panic!("{name} must parse: {error} at {:?}", error.snippet);
    });

    assert_eq!(
        document.check_coverage(),
        Ok(()),
        "{name}: the segments must tile the body exactly"
    );

    let rebuilt = document.to_bytes();
    if rebuilt != bytes {
        let at = rebuilt
            .iter()
            .zip(bytes)
            .position(|(left, right)| left != right);
        panic!(
            "{name}: serializing changed the file (rebuilt {} bytes, original {} bytes, first difference at {at:?})",
            rebuilt.len(),
            bytes.len()
        );
    }

    for segment in document.segments() {
        let SegmentKind::Cue(cue) = &segment.kind else {
            continue;
        };
        for (label, span) in [
            ("start", cue.start.raw()),
            ("end", cue.end.raw()),
            ("text", cue.text),
        ] {
            assert!(
                span.start >= segment.span.start && span.end <= segment.span.end,
                "{name}: the cue {label} span {span:?} escapes its segment {:?}",
                segment.span
            );
        }
    }

    document
}

/// Read `<fixture>.expected` and hold the parse to it: the same line, the same kind, every time.
pub fn assert_expected_error(format: SubtitleFormat, path: &Path, bytes: &[u8]) {
    let name = path.display();
    let mut sidecar_name = OsString::from(path.as_os_str());
    sidecar_name.push(".expected");
    let sidecar = PathBuf::from(sidecar_name);

    let contents = std::fs::read_to_string(&sidecar)
        .unwrap_or_else(|error| panic!("{name} needs a .expected sidecar: {error}"));
    let first = contents.lines().next().unwrap_or("").trim();
    let (line, rest) = first.split_once(':').unwrap_or_else(|| {
        panic!(
            "{}: a sidecar reads <line>:<Kind>[:note], not {first:?}",
            sidecar.display()
        )
    });
    let kind = rest.split_once(':').map_or(rest, |(kind, _note)| kind);
    let expected_line: u32 = line.trim().parse().unwrap_or_else(|error| {
        panic!(
            "{}: {line:?} is not a line number: {error}",
            sidecar.display()
        )
    });

    match sublore_formats::parse(format, bytes) {
        Ok(document) => panic!(
            "{name} must fail to parse, and it returned {} cues instead",
            document.cues().count()
        ),
        Err(error) => {
            assert_eq!(
                format!("{:?}", error.kind),
                kind.trim(),
                "{name}: the sidecar names a different failure"
            );
            assert_eq!(
                error.line, expected_line,
                "{name}: reported at the wrong line"
            );
            assert!(error.column >= 1, "{name}: columns are 1-based");
        }
    }
}
