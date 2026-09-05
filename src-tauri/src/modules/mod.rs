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

mod host;

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sublore_module_api::{
    SubloreHost, SubloreInvocation, SubloreItem, SubloreStr, SUBLORE_ENABLE_ALWAYS,
    SUBLORE_ENABLE_DOCUMENT_OPEN, SUBLORE_ENABLE_PROJECT_OPEN, SUBLORE_ENABLE_SELECTION_NON_EMPTY,
    SUBLORE_ERR_UNSUPPORTED, SUBLORE_ITEM_FLAG_LAYER, SUBLORE_ITEM_MENU_ITEM,
    SUBLORE_ITEM_MENU_TITLE, SUBLORE_ITEM_PANEL, SUBLORE_ITEM_SEPARATOR,
    SUBLORE_ITEM_TOOLBAR_BUTTON, SUBLORE_NO_CUE, SUBLORE_OK,
};
use sublore_module_host::{scan, Loaded, Refusal};

use crate::log;
use crate::project::SharedProject;
use crate::subtitle::{CuePatchDto, SessionSlot};
use host::{HostCtx, Lent};

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

/// What one activation did, as the window receives it.
///
/// The two halves are independent on purpose. A module that changed three cues and then answered a
/// refusal changed three cues, and the window has to be told about them or the grid draws a
/// document that is not the one the session holds.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvokeOutcome {
    /// What the module answered. Zero is success and every other value is its own refusal, which
    /// the core logs and does not translate: it does not know what the module was doing.
    pub code: i32,
    /// The edits it made before it answered, in the order it made them.
    pub patches: Vec<CuePatchDto>,
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

/// One loaded module and the instance it created.
struct Running {
    module: Loaded,
    ctx: *mut c_void,
}

/// The modules this process holds, the table it lent them, and the context behind it.
///
/// Field order is the drop order and it matters: every module keeps the host pointer it was given
/// and that pointer names the context, so the modules go first, the table they were pointing at
/// second, and the context it named last.
struct Held {
    /// The libraries, mapped for the life of the process, each with the instance it created.
    modules: Vec<Running>,
    /// Lent to every module for its whole life. Boxed so it does not move under them.
    _host: Box<SubloreHost>,
    /// What every host callback is reached through. Boxed for the same reason.
    ctx: Box<HostCtx>,
}

impl Held {
    /// Nothing held, and the table and context an empty scan still has to hand out.
    fn empty() -> Self {
        let ctx = Box::new(HostCtx::new());
        let host = Box::new(host::table(ctx.as_ref()));
        Self {
            modules: Vec::new(),
            _host: host,
            ctx,
        }
    }
}

impl Drop for Held {
    /// Destroy every instance while its library is still mapped and the context still exists.
    ///
    /// Here rather than on `Running`, because the gate has to be armed for `destroy` as well and
    /// this is the only place that holds both halves. A module that calls back from its own
    /// `destroy` then meets a refusal instead of a dangling pointer. The fields drop after this
    /// body, in the order they are declared above.
    fn drop(&mut self) {
        for running in &mut self.modules {
            let (Some(destroy), false) = (running.module.table().destroy, running.ctx.is_null())
            else {
                continue;
            };
            // No session at teardown: the window is gone and there is nothing to lend.
            let _entered = self
                .ctx
                .enter(&file_name(running.module.path()), Lent::default());
            // Safety: `ctx` is what this module's own `create` wrote, handed back exactly once.
            unsafe { destroy(running.ctx) };
            running.ctx = std::ptr::null_mut();
        }
    }
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
            held: Mutex::new(Held::empty()),
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
    pub fn start(&self, config_dir: &Path, locale: &str, session: &SessionSlot) {
        let Ok(mut held) = self.held.lock() else {
            log::error!("modules: the module lock was poisoned before they could be started");
            return;
        };
        // Locked for the whole of every module call and lent to the gate, which is what keeps a
        // borrowed cue alive until the module reads it (module-abi.md §2.5). A poisoned session is
        // not a reason to refuse to start a module: it is lent none, and its reads answer that
        // nothing is open.
        let mut session = match session.lock() {
            Ok(held) => Some(held),
            Err(_) => {
                log::warn!("modules: the session lock is poisoned, so no module is lent one");
                None
            }
        };
        let mut all = Vec::new();
        let held = &mut *held;
        let ctx: &HostCtx = &held.ctx;
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
            let mut instance: *mut c_void = std::ptr::null_mut();
            // Safety: both strings live for the whole call, which is the only lifetime the
            // interface promises them (§2.1), and `instance` is one writable pointer. The gate is
            // armed for the call and disarmed the moment it returns (§2.5).
            let made = {
                let _entered =
                    ctx.enter(&name, Lent::default().with_session(session.as_deref_mut()));
                unsafe {
                    create(
                        &mut instance,
                        SubloreStr::borrowed(&directory),
                        SubloreStr::borrowed(locale),
                    )
                }
            };
            if made != SUBLORE_OK || instance.is_null() {
                log::warn!("modules: {name} would not start an instance, reporting {made}");
                continue;
            }
            running.ctx = instance;

            let mut sink = Sink {
                module: index,
                items: Vec::new(),
                refused: Vec::new(),
            };
            // Safety: the sink outlives the call, and `push_item` is this process's own.
            let told = {
                let _entered =
                    ctx.enter(&name, Lent::default().with_session(session.as_deref_mut()));
                unsafe { describe(instance, (&mut sink as *mut Sink).cast(), Some(push_item)) }
            };
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

    /// Carry a user's activation of a contributed item into the module that contributed it.
    ///
    /// The session is locked here and lent to the gate for the whole call, which is what makes a
    /// borrowed cue safe to read and what lets `propose` edit (§2.5). Nothing about the item is
    /// interpreted: the core hands back the id the module gave it and the state the gesture
    /// carried, and learns nothing about what happens next.
    pub fn invoke(
        &self,
        module: usize,
        item: u32,
        at: &SubloreInvocation,
        session: &SessionSlot,
        project: &SharedProject,
    ) -> InvokeOutcome {
        let Ok(mut held) = self.held.lock() else {
            log::error!("modules: the module lock is poisoned, so nothing can be activated");
            return InvokeOutcome {
                code: SUBLORE_ERR_UNSUPPORTED,
                patches: Vec::new(),
            };
        };
        let held = &mut *held;
        let Some(running) = held.modules.get_mut(module) else {
            // The window asked for a module that is not there, which is a core defect rather than
            // a module one: the list it drew from is the list this holds.
            log::error!("modules: item {item} named module {module}, which is not loaded");
            return InvokeOutcome {
                code: SUBLORE_ERR_UNSUPPORTED,
                patches: Vec::new(),
            };
        };
        let name = file_name(running.module.path());
        let (Some(invoke), false) = (running.module.table().invoke, running.ctx.is_null()) else {
            log::warn!("modules: {name} has no way to be activated, so item {item} does nothing");
            return InvokeOutcome {
                code: SUBLORE_ERR_UNSUPPORTED,
                patches: Vec::new(),
            };
        };

        // The module's own storage prefix, taken from its file name and never from the module
        // (module-abi.md §4.7 and docs/module-host-tasks.md H6). None costs it its storage.
        let storage = running.module.id().map(str::to_owned);
        let mut session = match session.lock() {
            Ok(held) => Some(held),
            Err(_) => {
                log::warn!("modules: the session lock is poisoned, so {name} is lent none");
                None
            }
        };
        // Locked after the session and released with it. Nothing else in this process takes both,
        // so this order is the only one there is and the pair cannot deadlock.
        let mut project = match project.lock() {
            Ok(held) => Some(held),
            Err(_) => {
                log::warn!("modules: the project lock is poisoned, so {name} is lent none");
                None
            }
        };
        let mut lent = Lent::default().with_session(session.as_deref_mut());
        if let Some(project) = project.as_deref_mut() {
            lent = lent.with_project(project, storage);
        }
        let entered = held.ctx.enter(&name, lent);
        // Safety: the module's own function, given the instance its `create` wrote and a record
        // valid for the call, with the session locked for the whole of it.
        let code = unsafe { invoke(running.ctx, item, at) };
        let patches = entered.proposed();
        drop(entered);

        if code != SUBLORE_OK {
            log::warn!("modules: {name} refused item {item}, reporting {code}");
        }
        InvokeOutcome { code, patches }
    }

    fn contributions(&self) -> Vec<ContributionDto> {
        self.contributions
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default()
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
    let ctx = Box::new(HostCtx::new());
    // The table points at the box above, and both are moved into `Held` below without moving what
    // they hold: that is what the boxes are for.
    let host = Box::new(host::table(ctx.as_ref()));
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
                ctx,
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
            ctx,
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

/// Where a gesture happened, as the window reports it.
///
/// One argument rather than five, which is also the shape it crosses the boundary in: this is
/// `SubloreInvocation` with the parts the window owns, and nothing about which item it is.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvokeAt {
    pub revision: u64,
    /// The cursor's row, or none when there is no selection.
    pub cue: Option<u64>,
    pub row: u64,
    pub panel_id: u32,
    pub project_key: i64,
}

/// A contributed item was activated. Everything about which item is the module's own (§5.3).
///
/// On the blocking pool rather than the async runtime's poll thread, because a module's own work
/// can be long and CONTRIBUTING.md §7 keeps the window responsive while it runs.
#[tauri::command]
pub async fn module_invoke(
    app: tauri::AppHandle,
    module: usize,
    item: u32,
    at: InvokeAt,
) -> InvokeOutcome {
    let at = SubloreInvocation {
        revision: at.revision,
        // A selection the module can act on, or the sentinel that says there is none.
        cue: at.cue.unwrap_or(SUBLORE_NO_CUE),
        // Zero unless a panel row carried it, which is the rule §4.1 gives for reading it at all.
        row: if at.panel_id == 0 { 0 } else { at.row },
        panel_id: at.panel_id,
        project_key: at.project_key,
    };
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let session = app.state::<crate::subtitle::SubtitleState>().slot();
        let project = app.state::<crate::project::ProjectState>().handle();
        app.state::<ModuleState>()
            .invoke(module, item, &at, &session, &project)
    })
    .await
    .unwrap_or_else(|error| {
        log::error!("modules: the activation task failed: {error}");
        InvokeOutcome::default()
    })
}
