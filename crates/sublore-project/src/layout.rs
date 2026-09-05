//! The names Sublore owns inside a project folder, and the numbers stamped in the database
//! header. BACKLOG.md M4.1.
//!
//! Everything here is frozen: changing a name or an id changes what is on a user's disk.

use std::path::{Path, PathBuf};

/// The single file Sublore creates in a project folder.
pub const DATABASE_NAME: &str = "project.sublore";

/// "SUBL". What makes a SQLite file recognisably a Sublore project rather than some database.
pub const APPLICATION_ID: i32 = 0x5355_424c;

/// The schema version this build writes and reads.
///
/// 2 adds `module_schema`, the counter a loaded module keeps its own tables on. 3 adds
/// `project_identity`, the number that tells one project from another across a close and a reopen.
/// Both added before the first public build on purpose: until then the only older databases in the
/// world are the owner's own test projects. See docs/module-abi.md 6.2 and BACKLOG.md N33.
pub const CURRENT_VERSION: u32 = 3;

/// Every name Sublore itself creates inside a project folder, including the journals SQLite keeps
/// beside the database. The delete path may name these and nothing else. See BACKLOG.md M4.3.
pub const OWNED_FILES: [&str; 4] = [
    "project.sublore",
    "project.sublore-wal",
    "project.sublore-shm",
    "project.sublore-journal",
];

/// The database inside `folder`. One join of one literal name: never a pattern, never a glob.
pub fn database_path(folder: &Path) -> PathBuf {
    folder.join(DATABASE_NAME)
}

#[cfg(test)]
mod tests {
    use super::{database_path, APPLICATION_ID, DATABASE_NAME, OWNED_FILES};
    use std::path::{Path, MAIN_SEPARATOR};

    #[test]
    fn the_database_sits_directly_in_the_project_folder() {
        let path = database_path(Path::new("/home/user/Series One"));
        assert!(path.ends_with(DATABASE_NAME));
        assert_eq!(path.parent(), Some(Path::new("/home/user/Series One")));
    }

    #[test]
    fn every_owned_name_is_a_plain_file_name() {
        for name in OWNED_FILES {
            assert!(
                !name.contains(MAIN_SEPARATOR) && !name.contains('/') && !name.contains('*'),
                "{name} must be a bare name, so a join cannot escape the project folder"
            );
        }
        assert!(OWNED_FILES.contains(&DATABASE_NAME));
    }

    #[test]
    fn the_application_id_spells_subl() {
        assert_eq!(APPLICATION_ID.to_be_bytes(), *b"SUBL");
    }
}
