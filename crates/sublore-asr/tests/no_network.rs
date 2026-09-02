//! M3.2's first acceptance criterion, stated as a test: no network request happens unless the
//! user asks for a download (CONTRIBUTING.md §1).
//!
//! Two proofs, and each says what it reaches. A real listener counts connections while the app
//! does everything it does before anyone presses Download, and it is then used once, on purpose,
//! to show it would have caught a stray request. A grep guard goes red on a second address
//! anywhere in the crate's source, which is what a new call to the network looks like when
//! somebody writes one.
//!
//! A third "proof" stood here until 2026-09-02: a fetcher that failed the test if it was ever
//! called. It was never handed to the workflow it claimed to be driven through, and it could not
//! have been. `download` is the only function in the crate that takes a fetcher, and nothing on
//! the pre-download path takes a URL either, so its count could not have read anything but zero.
//! That shape is the guarantee, and the two proofs below are what stand behind it.
//!
//! See BACKLOG.md M3.2.

mod common;

use std::fs;
use std::time::Duration;

use common::{test_model_body, FakeServer, Policy, Sandbox, TEST_MODEL_SHA256};
use sublore_asr::error::AsrErrorKind;
use sublore_asr::model::catalog::{self, ModelSpec};
use sublore_asr::model::download;
use sublore_asr::model::http::HttpFetcher;
use sublore_asr::model::store::ModelStore;
use sublore_asr::sidecar::{transcribe, Cancel, Compute, Language, TranscribeRequest};

const SPEC: ModelSpec = ModelSpec {
    id: "test",
    file: "ggml-test.bin",
    bytes: 300,
    sha256: TEST_MODEL_SHA256,
};

/// Everything the app does before anyone presses Download: list the models, try to use one, run a
/// transcription and cancel it.
fn ordinary_workflow(sandbox: &Sandbox, store: &ModelStore) {
    let statuses = store.statuses();
    assert_eq!(statuses.len(), catalog::CATALOG.len());

    assert_eq!(
        store
            .resolve("tiny.en")
            .expect_err("nothing downloaded yet")
            .kind,
        AsrErrorKind::ModelMissing
    );

    fs::create_dir_all(store.dir()).expect("creatable");
    fs::write(
        store.path(catalog::find("tiny.en").expect("in the catalog")),
        b"half a model",
    )
    .expect("writable");
    assert_eq!(
        store.resolve("tiny.en").expect_err("the wrong length").kind,
        AsrErrorKind::ModelCorrupt
    );

    let media = sandbox.silence("media.wav", 500);
    let script = sandbox.script("script", &["progress 10", "exit 0"]);
    let mut request =
        TranscribeRequest::new(media, script, Language::Code("en".to_owned()), Compute::Cpu);
    request.stall = Duration::from_secs(30);
    let error = transcribe(&sandbox.tools(), &request, &Cancel::new(), &|_, _| {})
        .expect_err("the fake writes no JSON");
    assert_eq!(error.kind, AsrErrorKind::NoOutput);
}

#[test]
fn a_listener_watching_the_app_counts_zero_connections_until_a_download_is_asked_for() {
    let sandbox = Sandbox::new("accept-count");
    let store = ModelStore::new(sandbox.path("models"));
    let server = FakeServer::start(test_model_body(), Policy::default());

    ordinary_workflow(&sandbox, &store);
    assert_eq!(
        server.accepts(),
        0,
        "the app opened a connection nobody asked for"
    );

    // The same listener, one deliberate download: proof that it would have seen a stray request.
    download(
        &store,
        &SPEC,
        &server.base_url(),
        &HttpFetcher::new(),
        &Cancel::new(),
        &|_, _| {},
    )
    .expect("the download the user asked for should work");
    assert_eq!(server.accepts(), 1);
}

#[test]
fn the_catalog_is_the_only_place_in_the_crate_that_holds_a_url() {
    // The grep guard from the design, run as a test so it fails on the developer's machine before
    // it fails in CI. A second hard-coded address anywhere else is what this catches.
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    visit(&source, &mut |path, text| {
        if path.ends_with("catalog.rs") {
            return;
        }
        for (number, line) in text.lines().enumerate() {
            if line.contains("http://") || line.contains("https://") {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    });
    assert!(
        offenders.is_empty(),
        "only model/catalog.rs may name an address:\n{}",
        offenders.join("\n")
    );
}

fn visit(dir: &std::path::Path, sink: &mut impl FnMut(&std::path::Path, &str)) {
    for entry in fs::read_dir(dir).expect("the source directory should be readable") {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            visit(&path, sink);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = fs::read_to_string(&path).expect("source should be readable");
            sink(&path, &text);
        }
    }
}
