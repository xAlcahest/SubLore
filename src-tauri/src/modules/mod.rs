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

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use sublore_module_api::{
    SubloreHost, SubloreItem, SubloreStr, SUBLORE_ABI_MINOR, SUBLORE_ENABLE_ALWAYS,
    SUBLORE_ENABLE_DOCUMENT_OPEN, SUBLORE_ENABLE_PROJECT_OPEN, SUBLORE_ENABLE_SELECTION_NON_EMPTY,
    SUBLORE_HOST_SIZE, SUBLORE_ITEM_FLAG_LAYER, SUBLORE_ITEM_MENU_ITEM, SUBLORE_ITEM_MENU_TITLE,
    SUBLORE_ITEM_PANEL, SUBLORE_ITEM_SEPARATOR, SUBLORE_ITEM_TOOLBAR_BUTTON, SUBLORE_OK,
};
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

/// One thing a module put in the menu bar, on the toolbar or on screen.
///
/// A shape and a label, and nothing that says what it does: the core draws it and hands the id back
/// when it is activated. Every field is checked on the way in, because a module is code this
/// process did not compile (§5.1, §5.2).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributionDto {
    /// Which module it came from, by index into the loaded list. Sent back with an activation.
    pub module: usize,
    /// The module's own id, echoed back to it and meaningless here.
    pub id: u32,
    /// "menuTitle" | "menuItem" | "separator" | "toolbarButton" | "panel".
    pub kind: &'static str,
    /// The id of the title or panel this hangs off, or none for top level.
    pub parent: Option<u32>,
    /// "always" | "documentOpen" | "projectOpen" | "selectionNonEmpty".
    pub enable_when: &'static str,
    /// A panel that covers the video and so has to register as a layer (§5.4).
    pub layer: bool,
    /// Already rendered in the locale the module was given.
    pub label: String,
}

/// The kinds a contribution may be, as an allowlist. Anything else costs that item, which is the
/// direction §5.2 requires the mistake to fail in.
fn kind_of(item: &SubloreItem) -> Option<&'static str> {
    match item.kind {
        SUBLORE_ITEM_MENU_TITLE => Some("menuTitle"),
        SUBLORE_ITEM_MENU_ITEM => Some("menuItem"),
        SUBLORE_ITEM_SEPARATOR => Some("separator"),
        SUBLORE_ITEM_TOOLBAR_BUTTON => Some("toolbarButton"),
        SUBLORE_ITEM_PANEL => Some("panel"),
        _ => None,
    }
}

/// The four states the core can answer about. There is no zero: a field a module forgot to set
/// costs it the item, and never gives the user a control enabled when it should not be (§5.2).
fn enable_when_of(item: &SubloreItem) -> Option<&'static str> {
    match item.enable_when {
        SUBLORE_ENABLE_ALWAYS => Some("always"),
        SUBLORE_ENABLE_DOCUMENT_OPEN => Some("documentOpen"),
        SUBLORE_ENABLE_PROJECT_OPEN => Some("projectOpen"),
        SUBLORE_ENABLE_SELECTION_NON_EMPTY => Some("selectionNonEmpty"),
        _ => None,
    }
}

/// What one `describe` call collects. The module pushes into this and nothing else.
struct Sink {
    module: usize,
    items: Vec<ContributionDto>,
    /// Named rather than counted: an item refused is a module defect its author needs to see.
    refused: Vec<String>,
}

/// # Safety
/// Called by a module with the pointer the host handed it and one item per call, valid for the call.
unsafe extern "C" fn push_item(sink: *mut c_void, item: *const SubloreItem) -> i32 {
    if sink.is_null() || item.is_null() {
        return sublore_module_api::SUBLORE_ERR_BAD_STRING;
    }
    let sink = unsafe { &mut *sink.cast::<Sink>() };
    let item = unsafe { &*item };

    let (Some(kind), Some(enable_when)) = (kind_of(item), enable_when_of(item)) else {
        sink.refused.push(format!(
            "item {} has a kind or a state this build does not know",
            item.id
        ));
        return sublore_module_api::SUBLORE_ERR_UNSUPPORTED;
    };
    let Ok(label) = (unsafe { item.label.as_str() }) else {
        sink.refused.push(format!(
            "item {} has a label that is not valid text",
            item.id
        ));
        return sublore_module_api::SUBLORE_ERR_BAD_STRING;
    };

    sink.items.push(ContributionDto {
        module: sink.module,
        id: item.id,
        kind,
        parent: (item.parent != 0).then_some(item.parent),
        enable_when,
        layer: item.flags & SUBLORE_ITEM_FLAG_LAYER != 0,
        label: label.to_owned(),
    });
    SUBLORE_OK
}

/// One loaded module and the instance it created. Dropped together, in that order.
struct Running {
    module: Loaded,
    ctx: *mut c_void,
}

impl Drop for Running {
    fn drop(&mut self) {
        // Before the library goes, while its code is still mapped.
        if let (Some(destroy), false) = (self.module.table().destroy, self.ctx.is_null()) {
            // Safety: `ctx` is what this module's own `create` wrote, handed back exactly once.
            unsafe { destroy(self.ctx) };
        }
    }
}

/// The modules this process holds, and the table it lent them.
///
/// Field order is the drop order and it matters: every module keeps the host pointer it was given,
/// so the modules go first and the table they were pointing at goes second.
struct Held {
    /// The libraries, mapped for the life of the process, each with the instance it created.
    modules: Vec<Running>,
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
    /// What the loaded modules contribute, filled once by `start` and read by the window.
    contributions: Mutex<Vec<ContributionDto>>,
    /// The loaded libraries, behind the lock §2.5 requires: the host holds it across every module
    /// call, so two of them never overlap.
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
            contributions: Mutex::new(Vec::new()),
            held: Mutex::new(Held {
                modules: Vec::new(),
                _host: Box::new(empty_host()),
            }),
        }
    }

    pub fn report(&self) -> &ModuleReport {
        &self.report
    }

    /// Create every loaded module's instance and collect what it contributes.
    ///
    /// Runs once, from the app's setup hook rather than from `load`, because the configuration
    /// directory a module is given comes from the app and the app does not exist when the scan does.
    /// Nothing here fails the launch: a module that will not start is one the user is told about
    /// and the app runs without.
    pub fn start(&self, config_dir: &Path, locale: &str) {
        let Ok(mut held) = self.held.lock() else {
            log::error!("modules: the module lock was poisoned before they could be started");
            return;
        };
        let mut all = Vec::new();
        for (index, running) in held.modules.iter_mut().enumerate() {
            let name = file_name(running.module.path());
            // Copied out rather than held: these are function pointers, and the borrow of the
            // module ends here so the instance can be written back onto the same value below.
            let create = running.module.table().create;
            let describe = running.module.table().describe;

            let (Some(create), Some(describe)) = (create, describe) else {
                log::warn!(
                    "modules: {name} filled neither create nor describe, so it contributes nothing"
                );
                continue;
            };

            let directory = config_dir.to_string_lossy();
            let mut ctx: *mut c_void = std::ptr::null_mut();
            // Safety: both strings live for the whole call, which is the only lifetime the
            // interface promises them (§2.1), and `ctx` is one writable pointer.
            let made = unsafe {
                create(
                    &mut ctx,
                    SubloreStr::borrowed(&directory),
                    SubloreStr::borrowed(locale),
                )
            };
            if made != SUBLORE_OK || ctx.is_null() {
                log::warn!("modules: {name} would not start an instance, reporting {made}");
                continue;
            }
            running.ctx = ctx;

            let mut sink = Sink {
                module: index,
                items: Vec::new(),
                refused: Vec::new(),
            };
            // Safety: the sink outlives the call, and `push_item` is this process's own.
            let told = unsafe { describe(ctx, (&mut sink as *mut Sink).cast(), Some(push_item)) };
            for refusal in &sink.refused {
                log::warn!("modules: {name} contributed nothing for one item, because {refusal}");
            }
            if told != SUBLORE_OK {
                // What it managed to push before it stopped is kept: a module that gave up halfway
                // still described the part the user can see, and dropping it would hide the half
                // that works as well as the half that does not.
                log::warn!("modules: {name} stopped describing itself, reporting {told}");
            }
            log::info!("modules: {name} contributed {} items", sink.items.len());
            all.extend(sink.items);
        }

        match self.contributions.lock() {
            Ok(mut held) => *held = all,
            Err(_) => log::error!("modules: the contributions lock was poisoned"),
        }
    }

    fn contributions(&self) -> Vec<ContributionDto> {
        self.contributions
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default()
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
            contributions: Mutex::new(Vec::new()),
            held: Mutex::new(Held {
                modules: Vec::new(),
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
        contributions: Mutex::new(Vec::new()),
        held: Mutex::new(Held {
            modules: found
                .loaded
                .into_iter()
                .map(|module| Running {
                    module,
                    ctx: std::ptr::null_mut(),
                })
                .collect(),
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

#[tauri::command]
pub fn module_contributions(state: tauri::State<'_, ModuleState>) -> Vec<ContributionDto> {
    state.inner().contributions()
}
