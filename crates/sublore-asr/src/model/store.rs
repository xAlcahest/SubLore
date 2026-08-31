//! The models directory: what is in it, and whether a file is safe to hand to whisper.
//! See BACKLOG.md M3.2.
//!
//! The directory listing plus the catalog is the whole state. No manifest, no database: a user can
//! drop a model in by hand and it works, and deleting one by hand is not a corruption.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{AsrError, AsrErrorKind};
use crate::model::catalog::{self, ModelSpec};

/// Suffix of a download in progress. The final name is never created before verification, so a
/// half-downloaded file cannot be picked up as a model.
pub const PART_SUFFIX: &str = ".part";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelState {
    Missing,
    /// A `.part` file is on disk. `bytes` is how far it got.
    Partial,
    /// The final file is there with the catalogued length. Listing stops at the length; the sha256
    /// is what `resolve` checks before a run.
    Ready,
    /// The final file exists but its length disagrees with the catalog.
    Corrupt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelStatus {
    pub spec: &'static ModelSpec,
    pub state: ModelState,
    pub downloaded_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct ModelStore {
    dir: PathBuf,
}

impl ModelStore {
    /// `app_data_dir()/models`. Nothing outside it is ever read or written.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self, spec: &ModelSpec) -> PathBuf {
        self.dir.join(spec.file)
    }

    pub fn part_path(&self, spec: &ModelSpec) -> PathBuf {
        self.dir.join(format!("{}{PART_SUFFIX}", spec.file))
    }

    /// What the UI shows next to each model. Filesystem only: listing models never opens a socket.
    pub fn statuses(&self) -> Vec<ModelStatus> {
        catalog::CATALOG
            .iter()
            .map(|spec| self.status(spec))
            .collect()
    }

    pub fn status(&self, spec: &'static ModelSpec) -> ModelStatus {
        let final_len = file_len(&self.path(spec));
        if let Some(length) = final_len {
            let state = if length == spec.bytes {
                ModelState::Ready
            } else {
                ModelState::Corrupt
            };
            return ModelStatus {
                spec,
                state,
                downloaded_bytes: length,
            };
        }
        match file_len(&self.part_path(spec)) {
            Some(length) if length > 0 => ModelStatus {
                spec,
                state: ModelState::Partial,
                downloaded_bytes: length,
            },
            _ => ModelStatus {
                spec,
                state: ModelState::Missing,
                downloaded_bytes: 0,
            },
        }
    }

    /// The path a run may use, or the reason it may not.
    ///
    /// Both checks the catalog can make, cheapest first: the length is O(1) and catches a truncated
    /// or half-copied file, then the sha256 catches a file of the right length holding the wrong
    /// bytes — bit rot, a bad copy, an overwrite. Measured, whisper loads a bit-flipped model
    /// without complaining and transcribes nonsense from it, so hashing here is what makes M3.2's
    /// promise true: a corrupt model is refused, never handed to whisper.
    ///
    /// The hash costs one sequential read of the file whisper is about to read anyway. Measured on
    /// a release build: 53 ms for ggml-tiny.en.bin, about 2 s for large-v3. It runs on a blocking
    /// task, never on the main thread (CONTRIBUTING.md §7).
    pub fn resolve(&self, id: &str) -> Result<PathBuf, AsrError> {
        let Some(spec) = catalog::find(id) else {
            return Err(AsrError::new(
                AsrErrorKind::ModelMissing,
                format!("{id:?} is not a model Sublore knows"),
            ));
        };
        let status = self.status(spec);
        match status.state {
            ModelState::Ready => {
                self.verify(spec)?;
                Ok(self.path(spec))
            }
            ModelState::Corrupt => Err(AsrError::new(
                AsrErrorKind::ModelCorrupt,
                format!(
                    "{} is {} bytes, the catalog says {}",
                    self.path(spec).display(),
                    status.downloaded_bytes,
                    spec.bytes
                ),
            )),
            ModelState::Missing | ModelState::Partial => Err(AsrError::new(
                AsrErrorKind::ModelMissing,
                format!("{} is not in {}", spec.file, self.dir.display()),
            )),
        }
    }

    /// Read the whole file and compare its hash with the catalog. The gate `resolve` puts in front
    /// of whisper, and what `download` re-checks a file already on disk with.
    pub fn verify(&self, spec: &ModelSpec) -> Result<(), AsrError> {
        let path = self.path(spec);
        let mut file = fs::File::open(&path).map_err(|error| {
            AsrError::new(
                AsrErrorKind::ModelMissing,
                format!("cannot open {}: {error}", path.display()),
            )
        })?;
        // Read in chunks rather than `io::copy`: the hasher's `io::Write` impl is an accident of
        // one sha2 version, and the models are gigabytes, so a buffer belongs here anyway.
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|error| {
                AsrError::new(
                    AsrErrorKind::ModelCorrupt,
                    format!("cannot read {}: {error}", path.display()),
                )
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        let digest = hex(&hasher.finalize());
        if digest != spec.sha256 {
            return Err(AsrError::new(
                AsrErrorKind::ChecksumMismatch,
                format!(
                    "{} hashes to {digest}, expected {}",
                    path.display(),
                    spec.sha256
                ),
            ));
        }
        Ok(())
    }
}

/// The length of a regular file, or nothing. A directory in the way is not a model.
fn file_len(path: &Path) -> Option<u64> {
    let metadata = fs::metadata(path).ok()?;
    metadata.is_file().then_some(metadata.len())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{hex, ModelState, ModelStore};
    use crate::error::AsrErrorKind;
    use crate::model::catalog;
    use std::fs;
    use std::path::PathBuf;

    fn store(name: &str) -> ModelStore {
        let dir = std::env::temp_dir().join(format!("sublore-store-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("the test directory should be creatable");
        ModelStore::new(dir)
    }

    #[test]
    fn an_empty_directory_reports_every_model_missing() {
        let store = store("empty");
        let statuses = store.statuses();
        assert_eq!(statuses.len(), catalog::CATALOG.len());
        assert!(statuses
            .iter()
            .all(|status| status.state == ModelState::Missing));
        assert_eq!(
            store.resolve("tiny.en").expect_err("nothing is there").kind,
            AsrErrorKind::ModelMissing
        );
        fs::remove_dir_all(store.dir()).ok();
    }

    #[test]
    fn a_file_of_the_wrong_length_is_corrupt_and_never_resolves() {
        let store = store("corrupt");
        let spec = catalog::find("tiny.en").expect("in the catalog");
        fs::write(store.path(spec), b"not a model").expect("writable");
        assert_eq!(store.status(spec).state, ModelState::Corrupt);
        let error = store
            .resolve("tiny.en")
            .expect_err("a short file is not a model");
        assert_eq!(error.kind, AsrErrorKind::ModelCorrupt);
        assert!(
            error.detail.contains("11"),
            "the detail names the length: {}",
            error.detail
        );
        fs::remove_dir_all(store.dir()).ok();
    }

    /// M3.2's acceptance criterion for a model that is already in the store: the length alone says
    /// nothing about the bytes, so the run is refused by the checksum and whisper never sees it.
    #[test]
    fn a_file_of_the_catalogued_length_holding_the_wrong_bytes_never_resolves() {
        let store = store("ready");
        let spec = catalog::find("tiny.en").expect("in the catalog");
        // Sparse: the length is what the listing reads, and 74 MB of zeros costs nothing here.
        let file = fs::File::create(store.path(spec)).expect("creatable");
        file.set_len(spec.bytes).expect("sparse resize");
        drop(file);

        // The listing is length-only, so it still offers the model...
        assert_eq!(store.status(spec).state, ModelState::Ready);
        // ... and resolving it for a run hashes it and refuses.
        let error = store
            .resolve("tiny.en")
            .expect_err("zeros of the right length are not the model");
        assert_eq!(error.kind, AsrErrorKind::ChecksumMismatch);
        assert!(
            error.detail.contains(spec.sha256),
            "the detail names the sha256 that was expected: {}",
            error.detail
        );
        assert_eq!(
            store.verify(spec).expect_err("the same refusal").kind,
            AsrErrorKind::ChecksumMismatch
        );
        fs::remove_dir_all(store.dir()).ok();
    }

    /// The same file one byte short of the catalogue is stopped by the cheap check first, so a
    /// truncated model never pays for a hash.
    #[test]
    fn a_truncated_file_is_refused_by_its_length_not_by_its_hash() {
        let store = store("truncated");
        let spec = catalog::find("tiny.en").expect("in the catalog");
        let file = fs::File::create(store.path(spec)).expect("creatable");
        file.set_len(spec.bytes - 1).expect("sparse resize");
        drop(file);
        assert_eq!(store.status(spec).state, ModelState::Corrupt);
        assert_eq!(
            store.resolve("tiny.en").expect_err("one byte short").kind,
            AsrErrorKind::ModelCorrupt
        );
        fs::remove_dir_all(store.dir()).ok();
    }

    #[test]
    fn a_part_file_is_partial_and_still_not_a_model() {
        let store = store("partial");
        let spec = catalog::find("tiny.en").expect("in the catalog");
        fs::write(store.part_path(spec), vec![0u8; 4096]).expect("writable");
        let status = store.status(spec);
        assert_eq!(status.state, ModelState::Partial);
        assert_eq!(status.downloaded_bytes, 4096);
        assert_eq!(
            store.resolve("tiny.en").expect_err("half a file").kind,
            AsrErrorKind::ModelMissing
        );
        fs::remove_dir_all(store.dir()).ok();
    }

    #[test]
    fn an_id_that_is_not_in_the_catalog_is_refused_by_name() {
        let store = store("unknown");
        let error = store
            .resolve("../../etc/passwd")
            .expect_err("not a model id");
        assert_eq!(error.kind, AsrErrorKind::ModelMissing);
        assert!(error.detail.contains("passwd"), "{}", error.detail);
        fs::remove_dir_all(store.dir()).ok();
    }

    #[test]
    fn a_directory_where_a_model_should_be_is_not_mistaken_for_one() {
        let store = store("directory");
        let spec = catalog::find("tiny.en").expect("in the catalog");
        fs::create_dir_all(store.path(spec)).expect("creatable");
        assert_eq!(store.status(spec).state, ModelState::Missing);
        fs::remove_dir_all(store.dir()).ok();
    }

    #[test]
    fn verifying_a_file_that_is_not_there_says_so() {
        let store = store("verify-missing");
        let spec = catalog::find("tiny").expect("in the catalog");
        assert_eq!(
            store.verify(spec).expect_err("nothing to verify").kind,
            AsrErrorKind::ModelMissing
        );
        fs::remove_dir_all(store.dir()).ok();
    }

    #[test]
    fn hex_matches_the_spelling_the_catalog_uses() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff, 0xa5]), "000fffa5");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn a_store_never_looks_outside_its_own_directory() {
        let store = ModelStore::new(PathBuf::from("/models"));
        let spec = catalog::find("tiny").expect("in the catalog");
        assert_eq!(store.path(spec), PathBuf::from("/models/ggml-tiny.bin"));
        assert_eq!(
            store.part_path(spec),
            PathBuf::from("/models/ggml-tiny.bin.part")
        );
    }
}
