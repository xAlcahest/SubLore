//! M3.2's first acceptance criterion, stated as a test: no network request happens unless the
//! user asks for a download (CONTRIBUTING.md §1).
//!
//! Two independent proofs. A fetcher that fails the test if it is ever called, driven through the
//! whole model and transcription workflow; and a real listener that counts connections, so the
//! zero is a measured zero rather than the absence of a call this test knew to look for. The
//! listener is then used once, on purpose, to show it would have caught a stray request.
//!
//! See BACKLOG.md M3.2.

mod common;

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{test_model_body, FakeServer, Policy, Sandbox, TEST_MODEL_SHA256};
use sublore_asr::error::AsrErrorKind;
use sublore_asr::model::catalog::{self, ModelSpec};
use sublore_asr::model::http::HttpFetcher;
use sublore_asr::model::store::ModelStore;
use sublore_asr::model::{download, Fetched, RangeFetcher};
use sublore_asr::sidecar::{transcribe, Cancel, Compute, Language, TranscribeRequest};
use sublore_asr::AsrError;

const SPEC: ModelSpec = ModelSpec {
    id: "test",
    file: "ggml-test.bin",
    bytes: 300,
    sha256: TEST_MODEL_SHA256,
};

/// A transport that must never be reached. Counting rather than panicking, because a panic on a
/// worker thread could be swallowed; the count is checked on the test's own thread.
#[derive(Default)]
struct NeverFetcher {
    calls: AtomicUsize,
}

impl RangeFetcher for NeverFetcher {
    fn get(&self, _url: &str, _from: u64) -> Result<Fetched, AsrError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(AsrError::new(
            AsrErrorKind::NetworkFailed,
            "must not be called",
        ))
    }
}

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
fn the_whole_workflow_never_reaches_the_transport() {
    let sandbox = Sandbox::new("never-fetcher");
    let store = ModelStore::new(sandbox.path("models"));
    let fetcher = NeverFetcher::default();

    ordinary_workflow(&sandbox, &store);

    assert_eq!(
        fetcher.calls.load(Ordering::SeqCst),
        0,
        "nothing in the model store or the sidecar may fetch anything"
    );
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
