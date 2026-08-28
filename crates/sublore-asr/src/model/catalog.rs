//! The models Sublore offers, their exact sizes and their sha256. See BACKLOG.md M3.2.
//!
//! The only place in this crate that holds a URL. Every value below was taken from the Hugging
//! Face tree API (the LFS oids are sha256) and cross-checked against the `x-linked-etag` header;
//! `ggml-tiny.en.bin` was additionally downloaded and hashed against its row.

/// Where model files come from. One constant, used by one function.
pub const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelSpec {
    /// What the UI and the IPC layer call it.
    pub id: &'static str,
    /// The file name upstream and on disk. A user can drop one in by hand and it works.
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

/// Smallest first, which is also the order the UI lists them in.
pub const CATALOG: &[ModelSpec] = &[
    ModelSpec {
        id: "tiny",
        file: "ggml-tiny.bin",
        bytes: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
    },
    ModelSpec {
        id: "tiny.en",
        file: "ggml-tiny.en.bin",
        bytes: 77_704_715,
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
    },
    ModelSpec {
        id: "base",
        file: "ggml-base.bin",
        bytes: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    },
    ModelSpec {
        id: "base.en",
        file: "ggml-base.en.bin",
        bytes: 147_964_211,
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
    },
    ModelSpec {
        id: "small",
        file: "ggml-small.bin",
        bytes: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    },
    ModelSpec {
        id: "small.en",
        file: "ggml-small.en.bin",
        bytes: 487_614_201,
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
    },
    ModelSpec {
        id: "medium",
        file: "ggml-medium.bin",
        bytes: 1_533_763_059,
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
    },
    ModelSpec {
        id: "medium.en",
        file: "ggml-medium.en.bin",
        bytes: 1_533_774_781,
        sha256: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
    },
    ModelSpec {
        id: "large-v3",
        file: "ggml-large-v3.bin",
        bytes: 3_095_033_483,
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
    },
    ModelSpec {
        id: "large-v3-turbo",
        file: "ggml-large-v3-turbo.bin",
        bytes: 1_624_555_275,
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
    },
];

pub fn find(id: &str) -> Option<&'static ModelSpec> {
    CATALOG.iter().find(|spec| spec.id == id)
}

#[cfg(test)]
mod tests {
    use super::{find, BASE_URL, CATALOG};

    #[test]
    fn every_row_is_a_usable_download_target() {
        for spec in CATALOG {
            assert!(!spec.id.is_empty(), "an id is what the UI sends");
            assert!(
                spec.file.starts_with("ggml-") && spec.file.ends_with(".bin"),
                "{} is not an upstream file name",
                spec.file
            );
            assert!(spec.bytes > 1_000_000, "{} looks too small", spec.id);
            assert_eq!(spec.sha256.len(), 64, "{} has no sha256", spec.id);
            assert!(
                spec.sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{} is not lowercase hex",
                spec.id
            );
        }
    }

    #[test]
    fn ids_and_file_names_are_unique_so_one_never_overwrites_another() {
        for (index, spec) in CATALOG.iter().enumerate() {
            for other in &CATALOG[index + 1..] {
                assert_ne!(spec.id, other.id);
                assert_ne!(spec.file, other.file);
                assert_ne!(spec.sha256, other.sha256);
            }
        }
    }

    #[test]
    fn an_unknown_id_finds_nothing() {
        assert_eq!(
            find("tiny.en").map(|spec| spec.file),
            Some("ggml-tiny.en.bin")
        );
        assert!(find("gigantic").is_none());
        assert!(find("").is_none());
    }

    #[test]
    fn the_base_url_can_be_joined_with_a_file_name_directly() {
        assert!(
            BASE_URL.ends_with('/'),
            "{BASE_URL} needs its trailing slash"
        );
        assert!(BASE_URL.starts_with("https://"));
    }
}
