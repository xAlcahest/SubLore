//! M3.2 acceptance for the download itself: an interrupted transfer resumes instead of starting
//! over, and a corrupt or truncated file is caught by its checksum and refused, never handed to
//! whisper.
//!
//! Everything here runs against a local listener on 127.0.0.1, over the same HTTP client the app
//! uses. No internet, so it runs offline and in CI unchanged. See BACKLOG.md M3.2.

mod common;

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use common::{test_model_body, FakeServer, Policy, Sandbox, TEST_MODEL_SHA256};
use sublore_asr::error::AsrErrorKind;
use sublore_asr::model::catalog::ModelSpec;
use sublore_asr::model::http::HttpFetcher;
use sublore_asr::model::store::{ModelState, ModelStore};
use sublore_asr::model::{download, RangeFetcher};
use sublore_asr::sidecar::Cancel;

/// A model the size of a paragraph, so a transfer is a test rather than a download.
const SPEC: ModelSpec = ModelSpec {
    id: "test",
    file: "ggml-test.bin",
    bytes: 300,
    sha256: TEST_MODEL_SHA256,
};

fn store(sandbox: &Sandbox) -> ModelStore {
    ModelStore::new(sandbox.path("models"))
}

fn fetch(
    store: &ModelStore,
    server: &FakeServer,
    cancel: &Cancel,
) -> Result<std::path::PathBuf, sublore_asr::AsrError> {
    download(
        store,
        &SPEC,
        &server.base_url(),
        &HttpFetcher::new(),
        cancel,
        &|_, _| {},
    )
}

#[test]
fn a_complete_download_lands_under_its_final_name_with_the_catalogued_bytes() {
    let sandbox = Sandbox::new("download-ok");
    let store = store(&sandbox);
    let server = FakeServer::start(test_model_body(), Policy::default());
    let progress = AtomicU64::new(0);

    let path = download(
        &store,
        &SPEC,
        &server.base_url(),
        &HttpFetcher::new(),
        &Cancel::new(),
        &|received, total| {
            assert_eq!(total, SPEC.bytes);
            progress.fetch_max(received, Ordering::SeqCst);
        },
    )
    .expect("the download should finish");

    assert_eq!(path, store.path(&SPEC));
    assert_eq!(fs::read(&path).expect("readable"), test_model_body());
    assert_eq!(progress.load(Ordering::SeqCst), SPEC.bytes);
    assert_eq!(store.status(&SPEC).state, ModelState::Ready);
    assert!(!store.part_path(&SPEC).exists(), "the part file is gone");
    assert_eq!(server.accepts(), 1);
}

#[test]
fn a_download_that_is_already_there_opens_no_socket_at_all() {
    let sandbox = Sandbox::new("download-cached");
    let store = store(&sandbox);
    fs::create_dir_all(store.dir()).expect("creatable");
    fs::write(store.path(&SPEC), test_model_body()).expect("writable");
    let server = FakeServer::start(test_model_body(), Policy::default());

    fetch(&store, &server, &Cancel::new()).expect("nothing to do");

    assert_eq!(
        server.accepts(),
        0,
        "a model already on disk is not re-fetched"
    );
}

/// The repair path M3.2's error message promises: a model damaged in place is the same length as
/// the catalogued one, so only the hash can tell, and Download has to act on it rather than call it
/// finished.
#[test]
fn a_model_damaged_in_place_is_fetched_again_instead_of_reported_as_finished() {
    let sandbox = Sandbox::new("download-repair");
    let store = store(&sandbox);
    fs::create_dir_all(store.dir()).expect("creatable");
    let mut rotted = test_model_body();
    rotted[7] ^= 0x01;
    fs::write(store.path(&SPEC), &rotted).expect("writable");
    assert_eq!(
        store.status(&SPEC).state,
        ModelState::Ready,
        "the length is right, which is the whole point of this case"
    );
    let server = FakeServer::start(test_model_body(), Policy::default());

    let path = fetch(&store, &server, &Cancel::new()).expect("the damaged file is replaced");

    assert_eq!(fs::read(&path).expect("readable"), test_model_body());
    assert_eq!(server.accepts(), 1, "the damaged file cannot be reused");
    assert!(!store.part_path(&SPEC).exists());
}

#[test]
fn an_interrupted_download_resumes_from_where_it_stopped() {
    let sandbox = Sandbox::new("resume");
    let store = store(&sandbox);
    let policy = Policy {
        honour_range: true,
        cut_after: Some(120),
        ..Policy::default()
    };
    let cut = FakeServer::start(test_model_body(), policy);

    let error = fetch(&store, &cut, &Cancel::new()).expect_err("the server hung up");
    assert_eq!(error.kind, AsrErrorKind::NetworkFailed);
    let part = store.part_path(&SPEC);
    assert_eq!(
        fs::metadata(&part).expect("the partial file is kept").len(),
        120,
        "the part file is the whole of the resume state"
    );
    assert!(
        !store.path(&SPEC).exists(),
        "nothing is renamed into place yet"
    );
    drop(cut);

    let whole = FakeServer::start(
        test_model_body(),
        Policy {
            honour_range: true,
            ..Policy::default()
        },
    );
    let path = fetch(&store, &whole, &Cancel::new()).expect("the second attempt finishes it");

    assert_eq!(fs::read(&path).expect("readable"), test_model_body());
    assert_eq!(
        whole.accepts(),
        1,
        "resuming asks for the rest once, not for the whole file again"
    );
    assert_eq!(
        whole.last_range(),
        Some(120),
        "the second attempt has to ask for byte 120 onwards, not start over"
    );
}

#[test]
fn a_server_that_ignores_the_range_restarts_the_file_instead_of_corrupting_it() {
    let sandbox = Sandbox::new("no-range");
    let store = store(&sandbox);
    fs::create_dir_all(store.dir()).expect("creatable");
    // Half a file from an earlier attempt, and a server that answers 200 to every request.
    fs::write(store.part_path(&SPEC), &test_model_body()[..150]).expect("writable");
    let server = FakeServer::start(test_model_body(), Policy::default());

    let path = fetch(&store, &server, &Cancel::new()).expect("it starts over and succeeds");

    assert_eq!(
        fs::read(&path).expect("readable"),
        test_model_body(),
        "appending a whole file to half of one would have doubled the head"
    );
}

#[test]
fn a_file_that_hashes_to_the_wrong_thing_is_refused_and_deleted() {
    let sandbox = Sandbox::new("checksum");
    let store = store(&sandbox);
    let mut wrong = test_model_body();
    wrong[42] ^= 0xFF;
    let server = FakeServer::start(wrong, Policy::default());

    let error = fetch(&store, &server, &Cancel::new()).expect_err("the bytes are not the model");

    assert_eq!(error.kind, AsrErrorKind::ChecksumMismatch);
    assert!(
        !store.path(&SPEC).exists(),
        "a file that failed verification is never renamed into place"
    );
    assert!(
        !store.part_path(&SPEC).exists(),
        "and it is not left behind to be resumed into a valid length"
    );
}

#[test]
fn a_stream_longer_than_the_catalogue_is_stopped_rather_than_filling_the_disk() {
    let sandbox = Sandbox::new("overrun");
    let store = store(&sandbox);
    let server = FakeServer::start(
        test_model_body(),
        Policy {
            overrun: Some(4096),
            ..Policy::default()
        },
    );

    let error = fetch(&store, &server, &Cancel::new()).expect_err("the server sent too much");

    assert_eq!(error.kind, AsrErrorKind::SizeMismatch);
    assert!(!store.path(&SPEC).exists());
    assert!(!store.part_path(&SPEC).exists());
}

#[test]
fn a_server_offering_a_different_file_is_refused_before_a_byte_is_written() {
    let sandbox = Sandbox::new("wrong-size");
    let store = store(&sandbox);
    let server = FakeServer::start(
        test_model_body(),
        Policy {
            lie_about_length: Some(999_999),
            ..Policy::default()
        },
    );

    let error = fetch(&store, &server, &Cancel::new()).expect_err("that is not our model");

    assert_eq!(error.kind, AsrErrorKind::SizeMismatch);
    assert!(!store.part_path(&SPEC).exists(), "nothing was written");
}

#[test]
fn cancelling_a_download_leaves_the_resume_state_exactly_as_it_was() {
    let sandbox = Sandbox::new("cancel-download");
    let store = store(&sandbox);
    fs::create_dir_all(store.dir()).expect("creatable");
    fs::write(store.part_path(&SPEC), &test_model_body()[..150]).expect("writable");
    let server = FakeServer::start(test_model_body(), Policy::default());
    let cancel = Cancel::new();
    cancel.cancel();

    let error = fetch(&store, &server, &cancel).expect_err("the user cancelled");

    assert_eq!(error.kind, AsrErrorKind::Cancelled);
    assert!(
        !store.path(&SPEC).exists(),
        "nothing half-finished is promoted"
    );
    assert_eq!(
        fs::metadata(store.part_path(&SPEC))
            .expect("still there")
            .len(),
        150,
        "the part file is what the next attempt resumes from"
    );
    assert_eq!(server.accepts(), 0, "a cancelled download opens no socket");
}

#[test]
fn a_download_never_writes_outside_the_models_directory() {
    let sandbox = Sandbox::new("contained");
    let store = store(&sandbox);
    let server = FakeServer::start(test_model_body(), Policy::default());

    fetch(&store, &server, &Cancel::new()).expect("the download should finish");

    let mut written: Vec<String> = fs::read_dir(store.dir())
        .expect("the models directory exists")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    assert_eq!(written, vec![SPEC.file.to_owned()]);
    let mut beside: Vec<String> = fs::read_dir(sandbox.dir())
        .expect("the sandbox exists")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    beside.sort();
    assert_eq!(beside, vec!["models".to_owned()]);
}

/// A fetcher that reports a body starting somewhere nobody asked about.
struct LyingFetcher;

impl RangeFetcher for LyingFetcher {
    fn get(
        &self,
        _url: &str,
        _from: u64,
    ) -> Result<sublore_asr::model::Fetched, sublore_asr::AsrError> {
        Ok(sublore_asr::model::Fetched {
            start: 77,
            total: Some(SPEC.bytes),
            body: Box::new(std::io::Cursor::new(test_model_body())),
        })
    }
}

#[test]
fn a_body_that_starts_at_an_offset_nobody_asked_for_is_refused() {
    let sandbox = Sandbox::new("lying-range");
    let store = store(&sandbox);

    let error = download(
        &store,
        &SPEC,
        "http://127.0.0.1:1/",
        &LyingFetcher,
        &Cancel::new(),
        &|_, _| {},
    )
    .expect_err("an unrequested offset cannot be appended to anything");

    assert_eq!(error.kind, AsrErrorKind::NetworkFailed);
    assert!(!store.part_path(&SPEC).exists());
}
