//! Running the module scan at startup, and reporting what it found.
//!
//! The scan itself is `sublore-module-host`. This is the part that decides where to look, when to
//! look, and what the window is told, and it is deliberately the only place in the product that
//! knows a module can exist at all.
//!
//! **The core does not know what it may load.** It matches a shape, never a product name, so
//! nothing here and nothing in `src/i18n/en.ts` learns the word for whatever the module turns out
//! to be. The sentence the user reads is assembled in the frontend from a core string and the data
//! below, which is what keeps that true. See `module-abi.md` §3.4 and §3.5.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use sublore_module_api::{SubloreHost, SUBLORE_ABI_MINOR, SUBLORE_HOST_SIZE};
use sublore_module_host::{scan, Loaded, Refusal};

use crate::log;

/// Why a file that looked like a module was not used, as the frontend receives it.
///
/// A code and its numbers rather than a sentence: CONTRIBUTING.md §9 keeps every user-facing string
/// in `src/i18n/en.ts`, and §3.5 wants the file name in the sentence, so the two halves meet there.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RefusalDto {
    /// The library would not open. `detail` is the loader's own, for the log rather than the user.
    Unopenable {
        detail: String,
    },
    NotAModule,
    VersionDiffers {
        ours: u32,
        theirs: u32,
    },
    RevisionTooNew {
        ours: u32,
        theirs: u32,
    },
    TableSize {
        ours: u32,
        theirs: u32,
    },
    Refused {
        code: i32,
    },
}

impl From<&Refusal> for RefusalDto {
    fn from(refusal: &Refusal) -> Self {
        match refusal {
            Refusal::Unopenable(detail) => Self::Unopenable {
                detail: detail.clone(),
            },
            Refusal::NotAModule => Self::NotAModule,
            Refusal::MajorDiffers { ours, theirs } => Self::VersionDiffers {
                ours: *ours,
                theirs: *theirs,
            },
            Refusal::MinorTooNew { ours, theirs } => Self::RevisionTooNew {
                ours: *ours,
                theirs: *theirs,
            },
            Refusal::TableSize { ours, theirs } => Self::TableSize {
                ours: *ours,
                theirs: *theirs,
            },
            Refusal::LoadRefused(code) => Self::Refused { code: *code },
        }
    }
}

/// One module that would not load, named by its file.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefusedDto {
    /// The file's own name, never its path: a path in a message is a path in a screenshot.
    pub file: String,
    #[serde(flatten)]
    pub why: RefusalDto,
}

/// What the scan found, as the window receives it.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleReport {
    /// One entry per module that loaded, by file name.
    pub loaded: Vec<String>,
    pub refused: Vec<RefusedDto>,
    /// The scan was skipped because the launch asked for it.
    pub skipped: bool,
}

/// The modules this process holds, and the table it lent them.
///
/// Field order is the drop order and it matters: every module keeps the host pointer it was given,
/// so the modules go first and the table they were pointing at goes second.
struct Held {
    /// Held so the libraries stay mapped for the life of the process. Nothing reads them yet: the
    /// calls that will are N8e's, and they take this same lock.
    _modules: Vec<Loaded>,
    /// Lent to every module for its whole life. Boxed so it does not move under them.
    _host: Box<SubloreHost>,
}

/// Safety: `Held` is reachable only through the mutex below, and that mutex is what §2.5 of
/// `module-abi.md` requires: the host holds it across every module call, so two module calls never
/// overlap. The raw pointer inside the host table is the host's own context, which this process
/// never dereferences; it exists to be handed back by a module, and the callbacks that will read it
/// check the calling thread first.
unsafe impl Send for Held {}

pub struct ModuleState {
    report: ModuleReport,
    /// The loaded libraries, behind the lock §2.5 requires. Nothing takes it yet, and the field is
    /// here because dropping it would close every module the scan just opened.
    #[allow(dead_code)]
    held: Mutex<Held>,
}

impl ModuleState {
    /// A process that will not look for modules at all.
    pub fn skipped() -> Self {
        Self {
            report: ModuleReport {
                skipped: true,
                ..ModuleReport::default()
            },
            held: Mutex::new(Held {
                _modules: Vec::new(),
                _host: Box::new(empty_host()),
            }),
        }
    }

    pub fn report(&self) -> &ModuleReport {
        &self.report
    }
}

/// The host table as this build fills it.
///
/// Every slot is empty for now: the calls a module may make back into the app are N8e's, and a slot
/// left empty is a refusal the module can see rather than a jump it cannot.
fn empty_host() -> SubloreHost {
    SubloreHost {
        size: SUBLORE_HOST_SIZE,
        minor: SUBLORE_ABI_MINOR,
        ctx: std::ptr::null_mut(),
        log: None,
        should_cancel: None,
        progress: None,
        document: None,
        cue_at: None,
        for_each_line: None,
        propose: None,
        find: None,
        db_run: None,
        db_transaction: None,
        panel_begin: None,
        panel_row: None,
        panel_end: None,
        status: None,
    }
}

/// Where a module ships: beside the running executable, and nowhere else (§3.4).
fn module_directory() -> Option<PathBuf> {
    let mut here = std::env::current_exe().ok()?;
    here.pop();
    Some(here)
}

/// Look for modules and load what agrees with this build.
///
/// Runs once, at startup, on the thread that starts the app. Nothing else in the process loads a
/// library, and no path from a configuration file or from the user ever reaches here.
pub fn load() -> ModuleState {
    let host = Box::new(empty_host());
    let Some(directory) = module_directory() else {
        // The executable cannot say where it is, which is not a fault the user can act on and not
        // a reason to stop: the app runs, without modules.
        log::warn!(
            "modules: the executable's own directory could not be read, so none were sought"
        );
        return ModuleState {
            report: ModuleReport::default(),
            held: Mutex::new(Held {
                _modules: Vec::new(),
                _host: host,
            }),
        };
    };

    // Safety: the directory is the executable's own, which §3.4 makes the only one ever scanned,
    // and `host` is boxed here and dropped after the modules that hold it.
    let found = unsafe { scan(&directory, host.as_ref()) };

    let loaded: Vec<String> = found
        .loaded
        .iter()
        .map(|module| file_name(module.path()))
        .collect();
    let refused: Vec<RefusedDto> = found
        .refused
        .iter()
        .map(|(path, why)| RefusedDto {
            file: file_name(path),
            why: RefusalDto::from(why),
        })
        .collect();

    // An absent module is silence to the user, and one line at debug level here, which is the
    // difference §3.5 exists to draw.
    if loaded.is_empty() && refused.is_empty() {
        log::debug!("modules: none found beside the executable");
    } else {
        log::info!(
            "modules: {} loaded, {} refused",
            loaded.len(),
            refused.len()
        );
        for (path, why) in &found.refused {
            log::warn!("modules: {} was not used because {why}", file_name(path));
        }
    }

    ModuleState {
        report: ModuleReport {
            loaded,
            refused,
            skipped: false,
        },
        held: Mutex::new(Held {
            _modules: found.loaded,
            _host: host,
        }),
    }
}

/// A file's own name, or its whole path when it somehow has none.
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn module_report(state: tauri::State<'_, ModuleState>) -> ModuleReport {
    state.inner().report().clone()
}
