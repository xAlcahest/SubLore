//! Finding, opening and refusing modules.
//!
//! This crate is the calling half of the boundary `sublore-module-api` describes: it scans one
//! directory, performs the handshake of `module-abi.md` §3, and either keeps a module or closes it
//! and records why. It does not implement the host table, which is N8e's and needs the session.
//!
//! **An absent module is silence.** A scan that found nothing and a scan that refused something are
//! different values here, because the free core is the free product and a user who never bought
//! anything must not be told about a component that was never supposed to be there (§3.5).
//!
//! See docs/module-loader-tasks.md L2.

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use libloading::{Library, Symbol};
use sublore_module_api::{
    SubloreAbiFn, SubloreHost, SubloreLoadFn, SubloreModule, SUBLORE_ABI_MAJOR, SUBLORE_ABI_MINOR,
    SUBLORE_ABI_SYMBOL, SUBLORE_LOAD_SYMBOL, SUBLORE_MODULE_SIZE, SUBLORE_OK,
};

/// What a module file is named where it ships: beside the executable, matched by shape rather than
/// by product name, so the open core never learns what the thing it may load is (§3.4).
#[cfg(windows)]
const SUFFIX: &str = ".dll";
#[cfg(not(windows))]
const SUFFIX: &str = ".so";
const PREFIX: &str = "sublore_module_";

/// The storage id a module file's own name yields, or none when it yields nothing usable.
///
/// A free function so it can be checked without a library to load: what it decides is a name, and a
/// name needs no mapped image to be wrong.
fn id_of(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    let id = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    let usable = !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    usable.then_some(id)
}

/// Why a file that looked like a module was not used. The list is closed and it is §3.5's table,
/// so the sentence the user reads is assembled from these rather than from a string invented here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The library would not open at all.
    Unopenable(String),
    /// No `sublore_module_abi`, so it is not a Sublore module.
    NotAModule,
    /// Fatal in both directions, always, with no shim (§3.2).
    MajorDiffers { ours: u32, theirs: u32 },
    /// Built against slots this host does not have. The other direction loads.
    MinorTooNew { ours: u32, theirs: u32 },
    /// Version skew the minor number failed to describe (§3.3).
    TableSize { ours: u32, theirs: u32 },
    /// `sublore_module_load` refused, and this is the module's own code.
    LoadRefused(i32),
}

impl fmt::Display for Refusal {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unopenable(detail) => write!(out, "it could not be opened: {detail}"),
            Self::NotAModule => write!(out, "it is not a Sublore module"),
            Self::MajorDiffers { ours, theirs } => {
                write!(
                    out,
                    "it speaks interface version {theirs} and this build speaks {ours}"
                )
            }
            Self::MinorTooNew { ours, theirs } => {
                write!(
                    out,
                    "it needs interface revision {theirs} and this build offers {ours}"
                )
            }
            Self::TableSize { ours, theirs } => {
                write!(
                    out,
                    "its interface table is {theirs} bytes and this build's is {ours}"
                )
            }
            Self::LoadRefused(code) => write!(out, "it refused to start, with code {code}"),
        }
    }
}

/// A module that passed the handshake, and the library it lives in.
///
/// The two are one value on purpose: every pointer in `table` points into the loaded image, so a
/// handle dropped while the table is still held would leave the table pointing at unmapped memory.
/// Nothing here hands the table out separately from the library that backs it.
pub struct Loaded {
    path: PathBuf,
    table: SubloreModule,
    /// Dropped last, and only when this value is. Never read directly.
    _library: Library,
}

impl Loaded {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The minor revision this module filled its table to.
    pub fn minor(&self) -> u32 {
        self.table.minor
    }

    /// The name this module's own tables are prefixed with, taken from its file name.
    ///
    /// **Not something the module declares, and that is the point.** The guard exists to hold a
    /// module inside `m_<id>_*`, so a module that named its own prefix could name another module's
    /// and the guard would hand it the key it was built to withhold. The file is what the user
    /// installed and what §3.4 already matches, and a module cannot lie about it.
    ///
    /// `None` when what is left after the prefix is not an id the storage will accept, which costs
    /// that module its storage rather than giving it storage under a name nobody checked.
    pub fn id(&self) -> Option<&str> {
        id_of(&self.path)
    }

    /// The module's own table. Valid only while this value is alive.
    pub fn table(&self) -> &SubloreModule {
        &self.table
    }
}

impl fmt::Debug for Loaded {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("Loaded")
            .field("path", &self.path)
            .field("minor", &self.table.minor)
            .finish_non_exhaustive()
    }
}

/// What one scan of one directory found.
#[derive(Debug, Default)]
pub struct Scan {
    pub loaded: Vec<Loaded>,
    pub refused: Vec<(PathBuf, Refusal)>,
}

impl Scan {
    /// Nothing there at all, which is the case that stays silent.
    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty() && self.refused.is_empty()
    }
}

/// Whether a directory entry is named the way a module ships.
fn is_module_file(name: &OsStr) -> bool {
    // Compared as a string rather than as bytes: a name that is not valid Unicode is not one of
    // ours, because ours are all ASCII by construction.
    match name.to_str() {
        Some(name) => name.starts_with(PREFIX) && name.ends_with(SUFFIX),
        None => false,
    }
}

/// Every module file in `directory`, in sorted order.
///
/// A directory that cannot be read is an empty list rather than an error: the executable's own
/// directory always exists, and a scan that cannot see it is the same outcome for the user as a
/// scan that saw nothing.
fn module_files(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| is_module_file(&entry.file_name()))
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

/// Scan one directory and try every module file in it, in sorted order.
///
/// # Safety
/// Loading a library runs its initialisers, and calling through the table it fills runs code this
/// process did not compile. The caller is asserting that `directory` is one it controls: §3.4 makes
/// that the executable's own directory, never a path from a config file and never one from a user.
///
/// `host` must stay alive and unmoved for as long as any returned [`Loaded`] does, because the
/// module keeps the pointer it was handed.
pub unsafe fn scan(directory: &Path, host: &SubloreHost) -> Scan {
    let mut result = Scan::default();
    for path in module_files(directory) {
        match unsafe { try_one(&path, host) } {
            Ok(loaded) => result.loaded.push(loaded),
            Err(refusal) => result.refused.push((path, refusal)),
        }
    }
    result
}

/// # Safety
/// As [`scan`], for one file.
unsafe fn try_one(path: &Path, host: &SubloreHost) -> Result<Loaded, Refusal> {
    let library = match unsafe { Library::new(path) } {
        Ok(library) => library,
        Err(error) => return Err(Refusal::Unopenable(error.to_string())),
    };

    // Symbol one. Resolved by the constant both sides share, never by a name spelled here.
    let abi: Symbol<'_, SubloreAbiFn> = match unsafe { library.get(SUBLORE_ABI_SYMBOL) } {
        Ok(symbol) => symbol,
        Err(_) => return Err(Refusal::NotAModule),
    };
    let version = unsafe { abi() };
    let theirs_major = (version >> 32) as u32;
    let theirs_minor = version as u32;

    if theirs_major != SUBLORE_ABI_MAJOR {
        return Err(Refusal::MajorDiffers {
            ours: SUBLORE_ABI_MAJOR,
            theirs: theirs_major,
        });
    }
    // Asymmetric on purpose (§3.2). A host ahead of a module loads it: the module uses a subset of
    // what is offered and every slot it knows is where it expects. A module ahead of its host is
    // refused: it was built against slots this table does not carry, and calling one is a jump
    // through whatever memory follows.
    if theirs_minor > SUBLORE_ABI_MINOR {
        return Err(Refusal::MinorTooNew {
            ours: SUBLORE_ABI_MINOR,
            theirs: theirs_minor,
        });
    }

    // Symbol two, resolved only now, because §3.1 says it is reached only after the first agrees.
    let load: Symbol<'_, SubloreLoadFn> = match unsafe { library.get(SUBLORE_LOAD_SYMBOL) } {
        Ok(symbol) => symbol,
        Err(_) => return Err(Refusal::NotAModule),
    };

    let mut table = SubloreModule::empty();
    let answer = unsafe { load(host, &mut table) };
    if answer != SUBLORE_OK {
        return Err(Refusal::LoadRefused(answer));
    }
    if table.size != SUBLORE_MODULE_SIZE {
        return Err(Refusal::TableSize {
            ours: SUBLORE_MODULE_SIZE,
            theirs: table.size,
        });
    }
    if table.minor > SUBLORE_ABI_MINOR {
        // The handshake said one thing and the table says another. Refused for the same reason the
        // handshake would have: a slot beyond what this host knows.
        return Err(Refusal::MinorTooNew {
            ours: SUBLORE_ABI_MINOR,
            theirs: table.minor,
        });
    }

    // The two symbols borrow `library`, and the borrows end at their last use above, which is what
    // lets the library move into the value below. An explicit drop would say the same thing and do
    // nothing: `Symbol` carries a lifetime rather than an implementation of `Drop`.
    Ok(Loaded {
        path: path.to_path_buf(),
        table,
        _library: library,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A module file's name as it ships on this platform.
    fn shipped(stem: &str) -> PathBuf {
        PathBuf::from(format!("/opt/sublore/{PREFIX}{stem}{SUFFIX}"))
    }

    #[test]
    fn a_module_s_storage_id_is_what_its_file_is_called() {
        assert_eq!(id_of(&shipped("notes")), Some("notes"));
        assert_eq!(id_of(&shipped("notes_2")), Some("notes_2"));
    }

    #[test]
    fn a_name_the_storage_would_refuse_yields_no_id_at_all() {
        // Upper case, a space and a quote: each of them is a name `is_module_id` refuses, and the
        // module losing its storage is the direction this has to fail in. Storage under a name
        // nobody checked is the one outcome that must not be reachable.
        for stem in ["Notes", "note book", "notes'; DROP TABLE series; --", ""] {
            assert_eq!(
                id_of(&shipped(stem)),
                None,
                "{stem:?} was accepted as an id"
            );
        }
    }

    #[test]
    fn a_file_that_is_not_shaped_like_a_module_yields_nothing() {
        assert_eq!(id_of(Path::new("/opt/sublore/libfoo.so")), None);
        assert_eq!(id_of(Path::new(&format!("/opt/{PREFIX}notes.txt"))), None);
        assert_eq!(id_of(Path::new("/opt/sublore/")), None);
    }
}
