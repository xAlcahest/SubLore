//! What opening and saving do, driven through the command bodies rather than through IPC: the
//! async wrappers add a `spawn_blocking` and nothing else. The E2E spec covers the app; this covers
//! the outcomes a GUI test cannot see cheaply, including the ones that need a file to be broken.
//! See BACKLOG.md M1.5, and M2.3 for the session the commands now go through.

use std::fs;
use std::path::{Path, PathBuf};

use sublore_lib::subtitle::error::{SubtitleErrorCode, SubtitleReason};
use sublore_lib::subtitle::{
    open_session, save_as, SessionSlot, SubtitleSummary, MAX_SUBTITLE_BYTES,
};

/// Every clean SRT fixture, so a copy is proven byte-identical for the whole tree, not one file.
const CLEAN_SRT: &str = "fixtures/subtitles/srt/clean";

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is src-tauri; the fixtures live one level up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn fixture(relative: &str) -> String {
    let path = repo_root().join(relative);
    assert!(path.is_file(), "missing fixture {}", path.display());
    path.to_string_lossy().into_owned()
}

/// Open a file into a fresh session and report what the status line would say.
fn open_summary(
    path: &str,
) -> Result<SubtitleSummary, sublore_lib::subtitle::error::SubtitleError> {
    open_session(&SessionSlot::default(), path).map(|opened| opened.summary)
}

/// Open `source` and write it out at `destination`, which is what save-as does.
fn save_copy(
    source: &str,
    destination: &str,
    backup_root: PathBuf,
) -> Result<sublore_lib::subtitle::SubtitleSaved, sublore_lib::subtitle::error::SubtitleError> {
    let slot = SessionSlot::default();
    open_session(&slot, source)?;
    save_as(&slot, 0, destination, backup_root)
}

/// A scratch directory that removes itself, so a failed assertion never leaves litter behind.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sublore-m15-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch directory");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn backups(&self) -> PathBuf {
        self.path.join("backups")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn clean_srt_fixtures() -> Vec<PathBuf> {
    let dir = repo_root().join(CLEAN_SRT);
    let mut found: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("reading {}: {error}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|value| value == "srt"))
        .collect();
    found.sort();
    assert!(
        found.len() >= 15,
        "the clean SRT tree shrank to {} files; fixtures are not deleted, they accumulate",
        found.len()
    );
    found
}

#[test]
fn a_clean_fixture_reports_its_format_and_what_is_in_it() {
    let summary = open_summary(&fixture("fixtures/subtitles/srt/clean/basic-lf.srt"))
        .expect("a clean fixture opens");
    assert_eq!(summary.format, "srt");
    assert_eq!(summary.cue_count, 3);
    assert!(!summary.has_bom);
    assert_eq!(summary.newline, "lf");
    assert_eq!(summary.byte_length, 259);
}

#[test]
fn crlf_bom_and_mixed_endings_are_reported_as_they_are() {
    let crlf = open_summary(&fixture("fixtures/subtitles/srt/clean/basic-crlf.srt"))
        .expect("the CRLF fixture opens");
    assert_eq!(summary_shape(&crlf), ("srt", 3, false, "crlf"));

    let bom = open_summary(&fixture("fixtures/subtitles/srt/clean/bom-crlf.srt"))
        .expect("the BOM fixture opens");
    assert!(bom.has_bom, "a BOM the file has is a BOM the UI shows");
    assert_eq!(bom.newline, "crlf");

    let mixed = open_summary(&fixture("fixtures/subtitles/srt/clean/mixed-eol.srt"))
        .expect("the mixed fixture opens");
    assert_eq!(mixed.newline, "mixed");
}

fn summary_shape(summary: &SubtitleSummary) -> (&str, usize, bool, &str) {
    (
        summary.format.as_str(),
        summary.cue_count,
        summary.has_bom,
        summary.newline.as_str(),
    )
}

#[test]
fn the_other_two_formats_open_through_the_same_command() {
    let vtt =
        open_summary(&fixture("fixtures/subtitles/vtt/clean/basic.vtt")).expect("the VTT opens");
    assert_eq!(vtt.format, "vtt");
    assert!(vtt.cue_count > 0);

    let ass =
        open_summary(&fixture("fixtures/subtitles/ass/clean/basic.ass")).expect("the ASS opens");
    assert_eq!(ass.format, "ass");
    assert!(ass.cue_count > 0);
}

#[test]
fn a_malformed_fixture_names_the_line_and_the_reason() {
    let error = open_summary(&fixture(
        "fixtures/subtitles/srt/malformed/missing-arrow.srt",
    ))
    .expect_err("a file with no arrow must not open");
    assert_eq!(error.code, SubtitleErrorCode::ParseFailed);
    // The sidecar next to the fixture says 6:ExpectedTiming.
    assert_eq!(error.line, Some(6));
    assert_eq!(error.reason, Some(SubtitleReason::ExpectedTiming));
}

#[test]
fn a_file_sublore_cannot_decode_is_refused_whole() {
    let error = open_summary(&fixture("fixtures/subtitles/srt/malformed/utf16le-bom.srt"))
        .expect_err("UTF-16 must be refused");
    assert_eq!(error.code, SubtitleErrorCode::UnsupportedEncoding);
    assert_eq!(error.line, None, "an encoding failure is about the file");
    assert_eq!(error.reason, None);

    let invalid = open_summary(&fixture(
        "fixtures/subtitles/srt/malformed/invalid-utf8.srt",
    ))
    .expect_err("latin-1 bytes must be refused");
    assert_eq!(invalid.code, SubtitleErrorCode::UnsupportedEncoding);
}

#[test]
fn paths_that_are_not_subtitle_files_are_refused_with_the_reason_why() {
    let scratch = Scratch::new("bad-paths");

    assert_eq!(
        open_summary("").expect_err("an empty path").code,
        SubtitleErrorCode::InvalidPath
    );
    assert_eq!(
        open_summary(&scratch.join("nothing-here.srt").to_string_lossy())
            .expect_err("a path with no file")
            .code,
        SubtitleErrorCode::NotAFile
    );
    assert_eq!(
        open_summary(&scratch.path.to_string_lossy())
            .expect_err("a directory")
            .code,
        SubtitleErrorCode::NotAFile
    );

    let unknown = scratch.join("notes.txt");
    fs::write(&unknown, b"just some notes\n").expect("scratch write");
    assert_eq!(
        open_summary(&unknown.to_string_lossy())
            .expect_err("neither the content nor the extension names a format")
            .code,
        SubtitleErrorCode::UnknownFormat
    );
}

#[test]
fn a_file_past_the_limit_is_refused_before_it_is_read() {
    let scratch = Scratch::new("too-large");
    let huge = scratch.join("huge.srt");
    // Sparse where the filesystem allows it: the point is the reported length, not the bytes.
    let file = fs::File::create(&huge).expect("scratch file");
    file.set_len(MAX_SUBTITLE_BYTES + 1).expect("set_len");
    drop(file);

    let error = open_summary(&huge.to_string_lossy()).expect_err("16 MB is the limit");
    assert_eq!(error.code, SubtitleErrorCode::TooLarge);
}

#[test]
fn every_clean_srt_fixture_saves_out_byte_for_byte() {
    let scratch = Scratch::new("roundtrip");
    let mut checked = 0;

    for source in clean_srt_fixtures() {
        let name = source.file_name().expect("fixture name");
        let destination = scratch.join(&name.to_string_lossy());
        let saved = save_copy(
            &source.to_string_lossy(),
            &destination.to_string_lossy(),
            scratch.backups(),
        )
        .unwrap_or_else(|error| panic!("saving {}: {error}", source.display()));

        let original = fs::read(&source).expect("fixture read");
        let copy = fs::read(&destination).expect("copy read");
        assert_eq!(
            copy,
            original,
            "{} did not survive a save",
            source.display()
        );
        assert_eq!(saved.bytes_written, original.len() as u64);
        assert_eq!(saved.backup_path, None, "a new file needs no backup");
        checked += 1;
    }

    assert_eq!(checked, clean_srt_fixtures().len());
}

#[test]
fn overwriting_keeps_the_previous_file_and_never_writes_beside_the_source() {
    let scratch = Scratch::new("overwrite");
    let source = fixture("fixtures/subtitles/srt/clean/basic-crlf.srt");
    let destination = scratch.join("episode.srt");
    fs::write(&destination, b"the file that was already there\n").expect("scratch write");

    let saved = save_copy(&source, &destination.to_string_lossy(), scratch.backups())
        .expect("the save succeeds");

    let backup = saved.backup_path.expect("an existing file is backed up");
    assert_eq!(
        fs::read(&backup).expect("backup read"),
        b"the file that was already there\n",
        "the backup holds what the destination held"
    );
    assert!(
        Path::new(&backup).starts_with(scratch.backups()),
        "backups stay in Sublore's own directory, not next to the user's file: {backup}"
    );
    assert_eq!(
        fs::read(&destination).expect("destination read"),
        fs::read(&source).expect("source read")
    );

    // CLAUDE.md §3.1: the file the user opened is read-only, and nothing lands in its folder.
    let source_dir = Path::new(&source).parent().expect("fixture directory");
    let stray: Vec<PathBuf> = fs::read_dir(source_dir)
        .expect("fixture directory listing")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            !name.ends_with(".srt")
        })
        .collect();
    assert!(
        stray.is_empty(),
        "the save left {stray:?} beside the source"
    );
}

#[test]
fn a_save_that_cannot_land_says_so_and_leaves_the_destination_alone() {
    let scratch = Scratch::new("no-destination");
    let destination = scratch.join("missing-folder").join("episode.srt");

    let error = save_copy(
        &fixture("fixtures/subtitles/srt/clean/basic-lf.srt"),
        &destination.to_string_lossy(),
        scratch.backups(),
    )
    .expect_err("Sublore does not create folders the user did not name");
    assert_eq!(error.code, SubtitleErrorCode::WriteFailed);
    assert!(!destination.exists());

    assert_eq!(
        save_copy(
            &fixture("fixtures/subtitles/srt/clean/basic-lf.srt"),
            "",
            scratch.backups()
        )
        .expect_err("an empty destination")
        .code,
        SubtitleErrorCode::InvalidPath
    );

    assert_eq!(
        save_copy(
            &fixture("fixtures/subtitles/srt/clean/basic-lf.srt"),
            &scratch.path.to_string_lossy(),
            scratch.backups()
        )
        .expect_err("a directory is not a destination")
        .code,
        SubtitleErrorCode::NotAFile
    );
}

#[test]
fn a_malformed_source_is_never_written_anywhere() {
    let scratch = Scratch::new("malformed-source");
    let destination = scratch.join("episode.srt");

    let error = save_copy(
        &fixture("fixtures/subtitles/srt/malformed/missing-arrow.srt"),
        &destination.to_string_lossy(),
        scratch.backups(),
    )
    .expect_err("a file Sublore could not read is a file it will not write");
    assert_eq!(error.code, SubtitleErrorCode::ParseFailed);
    assert!(
        !destination.exists(),
        "nothing is created when the source does not parse"
    );

    // The session stayed empty, so there is nothing a later save could reach for either.
    assert_eq!(
        save_as(
            &SessionSlot::default(),
            0,
            &destination.to_string_lossy(),
            scratch.backups()
        )
        .expect_err("no document")
        .code,
        SubtitleErrorCode::NoDocument
    );
    assert!(!destination.exists());
}
