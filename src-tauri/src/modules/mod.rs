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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sublore_module_api::{
    SubloreHost, SubloreInvocation, SubloreItem, SubloreModule, SubloreStr, SUBLORE_ENABLE_ALWAYS,
    SUBLORE_ENABLE_DOCUMENT_OPEN, SUBLORE_ENABLE_PROJECT_OPEN, SUBLORE_ENABLE_SELECTION_NON_EMPTY,
    SUBLORE_ERR_BAD_STRING, SUBLORE_ERR_UNSUPPORTED, SUBLORE_ITEM_FLAG_LAYER,
    SUBLORE_ITEM_MENU_ITEM, SUBLORE_ITEM_MENU_TITLE, SUBLORE_ITEM_PANEL, SUBLORE_ITEM_SEPARATOR,
    SUBLORE_ITEM_TOOLBAR_BUTTON, SUBLORE_NO_CUE, SUBLORE_OK,
};
use sublore_module_host::{scan, Loaded, Refusal};
use sublore_project::records::Project;
use sublore_project::NO_PROJECT_KEY;
use tauri::{AppHandle, Emitter};

use crate::log;
use crate::project::SharedProject;
use crate::subtitle::{CuePatchDto, SessionSlot};
use host::{Activation, HostCtx, Lent};

/// What the window is told while one activation runs, and when it starts and stops.
///
/// Events rather than values carried back, and `progress` could not have been anything else:
/// `module_invoke` runs the module on `spawn_blocking` and its promise does not resolve until the
/// module returns, so a progress on `InvokeOutcome` would arrive after the thing it measured had
/// already stopped. That is a report and not a progress. The app emits from exactly this position
/// already (`asr/mod.rs`), and `src/hooks/` is the window listening. See docs/module-host-tasks.md
/// H8.
const EVENT_BEGAN: &str = "module://began";
pub(crate) const EVENT_STATUS: &str = "module://status";
pub(crate) const EVENT_PROGRESS: &str = "module://progress";
const EVENT_ENDED: &str = "module://ended";

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
    /// The panels it filled and closed, on the same terms: a run closed by `panel_end` publishes
    /// whatever the call answers afterwards, and one still open when the call returned is dropped.
    pub panels: Vec<PanelDto>,
}

/// One cell of one panel row, as the window receives it.
///
/// A kind word from the same allowlist style as [`kind_of`]: a cell kind this build has no meaning
/// for costs its row, and is never drawn as something it is not.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellDto {
    /// "text" | "number" | "percent" | "badge".
    pub kind: &'static str,
    pub text: String,
    pub number: i64,
}

/// One row of a panel: the module's own handle for it, and the cells it pushed.
///
/// **The handle crosses as a decimal string.** `SubloreCell::ref` is a `u64` and a `u64` above 2^53
/// does not survive JSON's number, so a large handle would come back to the module changed. The
/// core reads the digits back into the `u64` the interface takes and reads nothing else into them.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowDto {
    pub handle: String,
    pub cells: Vec<CellDto>,
}

/// One panel a module filled and closed, as the window receives it.
///
/// An empty `rows` is a publish and not an absence: a module saying it has no rows is a module
/// saying there is nothing to show, and the window clears that panel.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelDto {
    pub module: usize,
    pub panel_id: u32,
    pub rows: Vec<RowDto>,
}

/// One activation beginning, so the window can draw a working line before the module answers.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleBegan {
    call_id: u64,
    module: usize,
    item: u32,
}

/// A line a module put on screen while its work runs.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModuleStatus {
    pub call_id: u64,
    pub message: String,
}

/// How far a module says it has got. The two numbers are the module's own and are not checked
/// against each other: the core draws what it was given and knows nothing about the work.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModuleProgress {
    pub call_id: u64,
    pub done: u64,
    pub total: u64,
}

/// One activation ending, whatever it answered. The band goes on this, and again when the
/// activation's own promise settles: two independent reasons, neither waiting on the other.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleEnded {
    call_id: u64,
    code: i32,
}

/// How long a project edge may take a module before the log names it.
///
/// A measurement rather than a bound, and the doc says why: the call is foreign code on a blocking
/// thread, killing that thread would leave the project lock and the armed record behind, and
/// unwinding out of `extern "C"` aborts. What the host can do is say which module made the user
/// wait, on the same footing as `MAX_PANEL_ROWS`. See docs/module-lifecycle-tasks.md §7.
const SLOW_EDGE_MS: u128 = 250;

/// Which edge a project crossed, as one call into one module.
///
/// `project_closing` carries no key: the interface separates the two slots, and a module that wants
/// to know which project is going remembers what it was told when that project arrived.
#[derive(Clone, Copy)]
enum Edge {
    /// A project reached the slot, with the key it was opened under.
    Opened(i64),
    /// The project in the slot is about to leave it. Called before, so a module's last write has
    /// somewhere to land.
    Closing,
}

impl Edge {
    /// The slot's own name, which is what the log says when a module is slow or refuses.
    fn name(self) -> &'static str {
        match self {
            Self::Opened(_) => "project_opened",
            Self::Closing => "project_closing",
        }
    }
}

/// Tell one module about one edge, or nothing at all when its table left that slot empty.
///
/// **The slot is read before it is called.** Both are optional in the interface, a table filled by a
/// different compilation leaves an unfilled slot null, and reading a null and calling it is
/// undefined behaviour rather than a refusal (module-abi.md §2.5).
///
/// A non-zero answer never stops the project: the user asked for it, so a refusal is one warn line
/// and the command carries on.
fn tell_one(
    ctx: &HostCtx,
    name: &str,
    table: &SubloreModule,
    instance: *mut c_void,
    project: Option<&mut Option<Project>>,
    storage: Option<String>,
    edge: Edge,
) {
    // Its `create` failed, so there is no instance to call, which is what `invoke` finds too.
    if instance.is_null() {
        return;
    }
    let mut lent = Lent::default();
    if let Some(project) = project {
        // The project and the module's storage prefix, and nothing else: no session and no
        // activation, which is what makes the session-backed reads and the panel calls refuse.
        lent = lent.with_project(project, storage);
    }

    let started = std::time::Instant::now();
    let code = match edge {
        Edge::Opened(key) => {
            let Some(opened) = table.project_opened else {
                log::debug!("modules: {name} fills no project_opened, so it is not told");
                return;
            };
            let _entered = ctx.enter(name, lent);
            // Safety: the module's own function, given the instance its `create` wrote, with the
            // project locked for the whole of the call.
            unsafe { opened(instance, key) }
        }
        Edge::Closing => {
            let Some(closing) = table.project_closing else {
                log::debug!("modules: {name} fills no project_closing, so it is not told");
                return;
            };
            let _entered = ctx.enter(name, lent);
            // Safety: as above.
            unsafe { closing(instance) }
        }
    };

    let took = started.elapsed().as_millis();
    if took > SLOW_EDGE_MS {
        log::warn!(
            "modules: {name} spent {took} ms in {}, and the user waited for it",
            edge.name()
        );
    }
    if code != SUBLORE_OK {
        log::warn!(
            "modules: {name} answered {code} to {}, which does not stop the project",
            edge.name()
        );
    }
}

/// Which way a project crossed, before the slot has been read.
///
/// [`Edge`] is what one module is told and carries the key; this is what the caller asks for. The
/// two are separate so the key is read off the project `tell` has locked and never off an earlier
/// look at the slot: one read, so there is no window between deciding what to say and saying it.
#[derive(Clone, Copy)]
enum Crossing {
    Opened,
    Closing,
}

/// Tell every loaded module about one edge, in load order, one at a time.
///
/// **A crossing with nothing in the slot is no crossing.** That is what makes both call sites in
/// `across_a_project_edge` unconditional: a close with nothing open and an open that failed both
/// end here and tell nobody.
///
/// The project is locked once for the whole round and lent to each call, which is what lets a module
/// read and write its own storage from inside one. The module lock is the caller's and is held
/// across this, so no activation overlaps it and one module refusing does not stop the next.
fn tell(held: &mut Held, slot: &SharedProject, crossing: Crossing) {
    let mut project = match slot.lock() {
        Ok(open) => open,
        Err(_) => {
            log::warn!("modules: the project lock is poisoned, so no module is told an edge");
            return;
        }
    };
    // The key off the project in hand. No project is nothing to say, on either crossing.
    let edge = match (crossing, project.as_ref()) {
        (_, None) => return,
        (Crossing::Opened, Some(open)) => Edge::Opened(open.key()),
        (Crossing::Closing, Some(_)) => Edge::Closing,
    };
    let mut project = Some(&mut *project);
    let held = &mut *held;
    let ctx: &HostCtx = &held.ctx;
    for running in &held.modules {
        // `as_deref_mut` below reborrows rather than moves, so one lock serves every module.
        let name = file_name(running.module.path());
        // The module's own storage prefix, taken from its file name, exactly as `invoke` takes it.
        let storage = running.module.id().map(str::to_owned);
        tell_one(
            ctx,
            &name,
            running.module.table(),
            running.ctx,
            project.as_deref_mut(),
            storage,
            edge,
        );
    }
}

/// The activation a Stop can still reach, while there is one.
///
/// Its own mutex rather than a field behind the module lock, and that is the whole point: the
/// cancel path must never take `held`, or Stop would queue behind the call it is trying to stop.
struct ActiveCall {
    id: u64,
    flag: Arc<AtomicBool>,
}

/// What `module_invoke` lends one activation: the two locks to take, and the window to speak to.
///
/// One value rather than three more arguments, which is the shape H5 settled on when
/// `clippy::too_many_arguments` was right about the eight `module_invoke` used to pass.
pub struct Lends<'a> {
    pub session: &'a SessionSlot,
    pub project: &'a SharedProject,
    pub app: AppHandle,
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

/// The four cell kinds §5.3 fixes, as an allowlist on the same terms.
///
/// A fifth value is not a fifth kind, it is a module defect, and it costs the row that carried it
/// rather than being drawn as one of the four.
pub(crate) fn cell_kind(kind: u32) -> Option<&'static str> {
    match kind {
        sublore_module_api::SUBLORE_CELL_TEXT => Some("text"),
        sublore_module_api::SUBLORE_CELL_NUMBER => Some("number"),
        sublore_module_api::SUBLORE_CELL_PERCENT => Some("percent"),
        sublore_module_api::SUBLORE_CELL_BADGE => Some("badge"),
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
    /// The activation a Stop can reach, and nothing while none is running.
    ///
    /// Behind its own mutex, never behind `held`: `held` is taken for the whole of a module call,
    /// so a cancel that wanted it would wait for the call it exists to interrupt.
    active: Mutex<Option<ActiveCall>>,
    /// One per activation, which is why it is a number on the wire: a counter incremented once per
    /// user gesture cannot reach 2^53, so nothing here needs the string a row handle needs.
    next_call: AtomicU64,
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
            active: Mutex::new(None),
            next_call: AtomicU64::new(0),
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

    /// Run a project command with every module told the edges around it.
    ///
    /// **The module lock is held for the whole of it.** That is what puts a project edge behind an
    /// activation already running, and what stops a second project command splitting a close call
    /// from the close it is about. The project lock is taken inside it and after it, which is the
    /// order `invoke` fixed and the only one this process takes the pair in.
    ///
    /// **The slot decides, not the caller.** Whatever is in it before `work` is told it is closing,
    /// and whatever is in it afterwards is told it opened. So the swap `project/mod.rs` actually
    /// performs, where `create` and `open` close the last project themselves and never reach the
    /// `project_close` command, looks to a module exactly like a close followed by an open. A
    /// command that fails and leaves the slot empty tells nobody anything opened. See
    /// docs/module-lifecycle-tasks.md §2.
    pub fn across_a_project_edge<T>(&self, slot: &SharedProject, work: impl FnOnce() -> T) -> T {
        let Ok(mut held) = self.held.lock() else {
            // The project change still happens: a poisoned module lock is our bug, and refusing the
            // user's own command over it would be the worse of the two outcomes.
            log::error!(
                "modules: the module lock is poisoned, so none was told the project changed"
            );
            return work();
        };
        tell(&mut held, slot, Crossing::Closing);
        let outcome = work();
        tell(&mut held, slot, Crossing::Opened);
        outcome
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
        call: u64,
        at: &SubloreInvocation,
        lends: Lends<'_>,
    ) -> InvokeOutcome {
        let Ok(mut held) = self.held.lock() else {
            log::error!("modules: the module lock is poisoned, so nothing can be activated");
            return InvokeOutcome {
                code: SUBLORE_ERR_UNSUPPORTED,
                ..InvokeOutcome::default()
            };
        };
        let held = &mut *held;
        let Some(running) = held.modules.get_mut(module) else {
            // The window asked for a module that is not there, which is a core defect rather than
            // a module one: the list it drew from is the list this holds.
            log::error!("modules: item {item} named module {module}, which is not loaded");
            return InvokeOutcome {
                code: SUBLORE_ERR_UNSUPPORTED,
                ..InvokeOutcome::default()
            };
        };
        let name = file_name(running.module.path());
        let (Some(invoke), false) = (running.module.table().invoke, running.ctx.is_null()) else {
            log::warn!("modules: {name} has no way to be activated, so item {item} does nothing");
            return InvokeOutcome {
                code: SUBLORE_ERR_UNSUPPORTED,
                ..InvokeOutcome::default()
            };
        };

        // The module's own storage prefix, taken from its file name and never from the module
        // (module-abi.md §4.7 and docs/module-host-tasks.md H6). None costs it its storage.
        let storage = running.module.id().map(str::to_owned);
        let mut session = match lends.session.lock() {
            Ok(held) => Some(held),
            Err(_) => {
                log::warn!("modules: the session lock is poisoned, so {name} is lent none");
                None
            }
        };
        // Locked after the session and released with it. Nothing else in this process takes both,
        // so this order is the only one there is and the pair cannot deadlock.
        let mut project = match lends.project.lock() {
            Ok(held) => Some(held),
            Err(_) => {
                log::warn!("modules: the project lock is poisoned, so {name} is lent none");
                None
            }
        };
        // Read off the project just locked, never taken from the window: a key the window supplies
        // is a key the window can get wrong, and it has no business knowing one (BACKLOG.md N33).
        let at = SubloreInvocation {
            project_key: project
                .as_deref()
                .and_then(Option::as_ref)
                .map_or(NO_PROJECT_KEY, Project::key),
            ..*at
        };
        // Armed before the call and cleared after it, so a Stop that arrives late finds another id
        // or nothing, which is `asr_transcribe_cancel`'s rule in its own words (H8 decision 2).
        let cancel = Arc::new(AtomicBool::new(false));
        self.arm(call, &cancel);
        let mut lent = Lent::default()
            .with_session(session.as_deref_mut())
            .with_activation(Activation::new(
                lends.app,
                call,
                module,
                Arc::clone(&cancel),
                self.panels_of(module),
            ));
        if let Some(project) = project.as_deref_mut() {
            lent = lent.with_project(project, storage);
        }
        let entered = held.ctx.enter(&name, lent);
        // Safety: the module's own function, given the instance its `create` wrote and a record
        // valid for the call, with the session locked for the whole of it.
        let code = unsafe { invoke(running.ctx, item, &at) };
        let patches = entered.proposed();
        let panels = entered.panels();
        let abandoned = entered.abandoned_panel();
        drop(entered);
        self.disarm(call);

        if let Some((panel, rows)) = abandoned {
            // Dropped rather than published: `panel_end` is the module's own assertion that the
            // table is whole, and a half filled table drawn as a whole one is what to avoid.
            log::warn!(
                "modules: {name} left panel {panel} open with {rows} rows pushed, so none of \
                 them are drawn"
            );
        }
        if cancel.load(Ordering::SeqCst) {
            // Nothing stops a module that ignores the answer: the call is foreign code on a
            // blocking thread and killing it is worse. The code says which it did (§2.4).
            log::warn!("modules: {name} was asked to stop item {item} and returned {code}");
        }
        if code != SUBLORE_OK {
            log::warn!("modules: {name} refused item {item}, reporting {code}");
        }
        InvokeOutcome {
            code,
            patches,
            panels,
        }
    }

    /// Note the activation a Stop may reach.
    fn arm(&self, call: u64, flag: &Arc<AtomicBool>) {
        match self.active.lock() {
            Ok(mut slot) => {
                *slot = Some(ActiveCall {
                    id: call,
                    flag: Arc::clone(flag),
                });
            }
            // The work still runs and still finishes; what is lost is the Stop button's reach, and
            // saying so is better than a button that does nothing and says nothing.
            Err(_) => {
                log::error!(
                    "modules: the cancel slot is poisoned, so call {call} cannot be stopped"
                )
            }
        }
    }

    /// Forget it again, if it is still the one recorded.
    fn disarm(&self, call: u64) {
        if let Ok(mut slot) = self.active.lock() {
            if slot.as_ref().is_some_and(|active| active.id == call) {
                *slot = None;
            }
        }
    }

    /// Ask the activation `call` to stop, if that is still the one running.
    ///
    /// **Never takes `held`.** That lock is held for the whole of a module call, so a cancel that
    /// wanted it would queue behind the call it exists to interrupt. The id check is the other
    /// half: stopping a call that has just finished is not an error, and stopping an older one must
    /// never reach into the current one, which is also what keeps one module's Stop off another's
    /// work.
    fn cancel(&self, call: u64) {
        let Ok(slot) = self.active.lock() else {
            log::error!("modules: the cancel slot is poisoned, so call {call} was not stopped");
            return;
        };
        if let Some(active) = slot.as_ref().filter(|active| active.id == call) {
            active.flag.store(true, Ordering::SeqCst);
        }
    }

    /// The panel ids this module contributed, which is the allowlist `panel_begin` is held to.
    ///
    /// Lent by the host and never named by the module, which is H6's storage id rule applied to the
    /// other id a module could otherwise claim as another module's.
    fn panels_of(&self, module: usize) -> Vec<u32> {
        self.contributions
            .lock()
            .map(|held| {
                held.iter()
                    .filter(|item| item.module == module && item.kind == "panel")
                    .map(|item| item.id)
                    .collect()
            })
            .unwrap_or_default()
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
            active: Mutex::new(None),
            next_call: AtomicU64::new(0),
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
        active: Mutex::new(None),
        next_call: AtomicU64::new(0),
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
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvokeAt {
    pub revision: u64,
    /// The cursor's row, or none when there is no selection.
    pub cue: Option<u64>,
    /// The activated row's handle, in decimal, for the reason [`RowDto`] carries it that way.
    pub row: String,
    pub panel_id: u32,
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
    use tauri::Manager;

    // Zero unless a panel row carried one, which is the rule §4.1 gives for reading it at all. The
    // digits are read back into the `u64` the interface takes; nothing else is read into them.
    let row = if at.panel_id == 0 {
        0
    } else {
        match at.row.parse::<u64>() {
            Ok(row) => row,
            Err(_) => {
                // The window echoes back a handle this process sent it, so a value that is not one
                // is a core defect. Refused rather than sent on as a zero the module would misread.
                log::error!("modules: item {item} arrived with a row handle that is not a number");
                return InvokeOutcome {
                    code: SUBLORE_ERR_BAD_STRING,
                    ..InvokeOutcome::default()
                };
            }
        }
    };
    let at = SubloreInvocation {
        revision: at.revision,
        // A selection the module can act on, or the sentinel that says there is none.
        cue: at.cue.unwrap_or(SUBLORE_NO_CUE),
        row,
        panel_id: at.panel_id,
        // Filled by `ModuleState::invoke` off the project it locks, which is the only place that
        // knows one. Zero here means nothing yet, and zero there means no project is open.
        project_key: NO_PROJECT_KEY,
    };

    let Some(state) = app.try_state::<ModuleState>() else {
        log::error!("modules: item {item} was activated before the module state existed");
        return InvokeOutcome {
            code: SUBLORE_ERR_UNSUPPORTED,
            ..InvokeOutcome::default()
        };
    };
    let call = state.next_call.fetch_add(1, Ordering::SeqCst) + 1;
    // Before the work starts, so the band and its Stop are on screen while the module runs. The
    // pair to it is emitted after the await below rather than inside the closure, so the panic path
    // the task carries still ends the band.
    let _ = app.emit(
        EVENT_BEGAN,
        ModuleBegan {
            call_id: call,
            module,
            item,
        },
    );

    let ran = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let session = ran.state::<crate::subtitle::SubtitleState>().slot();
        let project = ran.state::<crate::project::ProjectState>().handle();
        let lends = Lends {
            session: &session,
            project: &project,
            app: ran.clone(),
        };
        ran.state::<ModuleState>()
            .invoke(module, item, call, &at, lends)
    })
    .await
    .unwrap_or_else(|error| {
        log::error!("modules: the activation task failed: {error}");
        InvokeOutcome::default()
    });

    let _ = app.emit(
        EVENT_ENDED,
        ModuleEnded {
            call_id: call,
            code: outcome.code,
        },
    );
    outcome
}

/// Ask the activation `call_id` to stop.
///
/// Not a registry command. The control is the one in the band that carries the status and the
/// progress, appearing with the work and going with it, which is the shape `TranscribePanel`
/// already draws and which the 2026-09-03 ruling explicitly leaves outside itself: that ruling is
/// about commands, not panels.
#[tauri::command]
pub fn module_cancel(state: tauri::State<'_, ModuleState>, call_id: u64) {
    state.inner().cancel(call_id);
}

#[cfg(test)]
mod tests {
    //! The rules of a project edge that are cheaper and stricter to hold here than through the app.
    //!
    //! The fixture beside the executable has to fill both slots to prove the E2E criteria at all, so
    //! it cannot also be the module that fills neither. These drive fabricated tables instead: a
    //! table is `#[repr(C)]` and a slot is a nullable function pointer, so one built here is exactly
    //! what a module's own compilation hands over. See docs/module-lifecycle-tasks.md §6.

    use super::{tell_one, Edge, HostCtx};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
    use std::sync::Mutex;
    use sublore_module_api::{
        SubloreCell, SubloreCue, SubloreDocument, SubloreHost, SubloreModule, SubloreProposal,
        SubloreStr, SUBLORE_CELL_TEXT, SUBLORE_ERR_DENIED, SUBLORE_ERR_NOTHING_OPEN, SUBLORE_OK,
        SUBLORE_PROPOSAL_SET_CUE_TEXT,
    };
    use sublore_project::records::Project;

    /// The id the checks arm with, and the file name it is derived from.
    const NAME: &str = "sublore_module_fake.so";
    const STORAGE: &str = "fake";

    /// The table a fake module reaches the host through, exactly as a real one keeps it from `load`.
    static HOST: AtomicPtr<SubloreHost> = AtomicPtr::new(std::ptr::null_mut());

    /// What every fake below wrote down, as a check reads it back. One lock rather than one counter
    /// each, so the order the modules were told in is in the record too.
    static SAID: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// The host table's checks are process-wide through `HOST`, so these run one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn note(line: impl Into<String>) {
        if let Ok(mut held) = SAID.lock() {
            held.push(line.into());
        }
    }

    fn taken() -> Vec<String> {
        SAID.lock()
            .map(|mut held| std::mem::take(&mut *held))
            .unwrap_or_default()
    }

    /// Arm `HOST` with a table over `ctx` and answer a guard that disarms it again.
    ///
    /// The box is leaked for the length of the check rather than dropped under the fakes: a table a
    /// module still holds a pointer to is exactly what must not go away, and a check that freed it
    /// would be checking a use-after-free rather than an edge.
    fn arm(ctx: &HostCtx) {
        let table = Box::leak(Box::new(super::host::table(ctx)));
        HOST.store(table as *mut SubloreHost, Ordering::Release);
    }

    fn host<'a>() -> &'a SubloreHost {
        let table = HOST.load(Ordering::Acquire);
        assert!(!table.is_null(), "the check armed the host table first");
        unsafe { &*table }
    }

    /// A directory of this check's own, and a project inside it.
    fn project(tag: &str) -> (std::path::PathBuf, Option<Project>) {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "sublore-module-edge-{tag}-{}-{nanos}-{unique}",
            std::process::id()
        ));
        let folder = dir.join("Series");
        std::fs::create_dir_all(&folder).expect("the project folder should be creatable");
        let project = Project::create(&folder, "A series", std::time::SystemTime::now())
            .expect("a project should be made");
        (dir, Some(project))
    }

    /// One instance pointer that is not null and that no fake below dereferences.
    fn instance() -> *mut c_void {
        std::ptr::NonNull::<u8>::dangling().as_ptr().cast()
    }

    // ------------------------------------------------------------------------------------------
    // The fakes. Each writes down that it was called, and nothing else.

    unsafe extern "C" fn opened_notes(_ctx: *mut c_void, key: i64) -> i32 {
        note(format!("opened {key}"));
        SUBLORE_OK
    }

    unsafe extern "C" fn closing_notes(_ctx: *mut c_void) -> i32 {
        note("closing");
        SUBLORE_OK
    }

    unsafe extern "C" fn opened_refuses(_ctx: *mut c_void, key: i64) -> i32 {
        note(format!("first refused {key}"));
        SUBLORE_ERR_DENIED
    }

    unsafe extern "C" fn opened_after(_ctx: *mut c_void, key: i64) -> i32 {
        note(format!("second told {key}"));
        SUBLORE_OK
    }

    /// Write into this module's own table from inside the edge, and write down the code.
    unsafe extern "C" fn opened_stores(_ctx: *mut c_void, _key: i64) -> i32 {
        note(format!("opened stored {}", unsafe { store() }));
        SUBLORE_OK
    }

    unsafe extern "C" fn closing_stores(_ctx: *mut c_void) -> i32 {
        note(format!("closing stored {}", unsafe { store() }));
        SUBLORE_OK
    }

    /// One statement in the module's own tables, through the host, and the code it answered.
    unsafe fn store() -> i32 {
        let table = host();
        let Some(run) = table.db_run else {
            return SUBLORE_ERR_DENIED;
        };
        unsafe {
            run(
                table.ctx,
                SubloreStr::borrowed("CREATE TABLE IF NOT EXISTS m_fake_notes (id INTEGER)"),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                None,
            )
        }
    }

    /// Ask for everything a lifecycle call is not lent, and write down what each one answered.
    unsafe extern "C" fn opened_asks_for_everything(_ctx: *mut c_void, _key: i64) -> i32 {
        let table = host();
        let mut document = SubloreDocument {
            format: 0,
            cue_count: 0,
            revision: 0,
            dirty: 0,
            path: SubloreStr::borrowed(""),
        };
        let asked = table
            .document
            .map(|call| unsafe { call(table.ctx, &mut document) });
        note(format!("document {asked:?}"));

        let mut cue = SubloreCue {
            start_ms: 0,
            end_ms: 0,
            text: SubloreStr::borrowed(""),
            is_comment: 0,
            has_number: 0,
            number: 0,
        };
        let asked = table
            .cue_at
            .map(|call| unsafe { call(table.ctx, 0, &mut cue) });
        note(format!("cue_at {asked:?}"));

        let proposal = SubloreProposal {
            kind: SUBLORE_PROPOSAL_SET_CUE_TEXT,
            revision: 0,
            cue: 0,
            text: SubloreStr::borrowed("anything"),
        };
        let asked = table
            .propose
            .map(|call| unsafe { call(table.ctx, &proposal) });
        note(format!("propose {asked:?}"));

        let asked = table.panel_begin.map(|call| unsafe { call(table.ctx, 1) });
        note(format!("panel_begin {asked:?}"));

        let cells = [SubloreCell {
            kind: SUBLORE_CELL_TEXT,
            text: SubloreStr::borrowed("a row"),
            number: 0,
            r#ref: 1,
        }];
        let asked = table
            .panel_row
            .map(|call| unsafe { call(table.ctx, cells.as_ptr(), cells.len()) });
        note(format!("panel_row {asked:?}"));

        let asked = table.panel_end.map(|call| unsafe { call(table.ctx) });
        note(format!("panel_end {asked:?}"));

        if let Some(call) = table.status {
            unsafe { call(table.ctx, SubloreStr::borrowed("working")) };
        }
        if let Some(call) = table.progress {
            unsafe { call(table.ctx, 1, 2) };
        }
        let asked = table.should_cancel.map(|call| unsafe { call(table.ctx) });
        note(format!("should_cancel {asked:?}"));
        SUBLORE_OK
    }

    // ------------------------------------------------------------------------------------------

    fn table_with(
        opened: Option<unsafe extern "C" fn(*mut c_void, i64) -> i32>,
        closing: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    ) -> SubloreModule {
        SubloreModule {
            project_opened: opened,
            project_closing: closing,
            ..SubloreModule::empty()
        }
    }

    fn tell(ctx: &HostCtx, table: &SubloreModule, open: &mut Option<Project>, edge: Edge) {
        tell_one(
            ctx,
            NAME,
            table,
            instance(),
            Some(open),
            Some(STORAGE.to_owned()),
            edge,
        );
    }

    #[test]
    fn a_table_with_neither_slot_filled_is_never_called() {
        // The rule the house style names: read the slot before calling it. A null read and called is
        // undefined behaviour, so the mutation that removes the read aborts rather than reddens, and
        // that abort is this check's evidence.
        let _serial = SERIAL.lock().unwrap_or_else(|held| held.into_inner());
        let (dir, mut open) = project("neither");
        let ctx = HostCtx::new();
        arm(&ctx);
        let empty = SubloreModule::empty();

        tell(&ctx, &empty, &mut open, Edge::Opened(77));
        tell(&ctx, &empty, &mut open, Edge::Closing);

        assert!(taken().is_empty(), "an unfilled slot is no call at all");
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_table_with_one_of_the_two_filled_gets_that_one_and_only_that_one() {
        // Wanting one edge and not the other is not a defect, so the other is silence.
        let _serial = SERIAL.lock().unwrap_or_else(|held| held.into_inner());
        let (dir, mut open) = project("one-of-two");
        let ctx = HostCtx::new();
        arm(&ctx);

        let opens_only = table_with(Some(opened_notes), None);
        tell(&ctx, &opens_only, &mut open, Edge::Opened(11));
        tell(&ctx, &opens_only, &mut open, Edge::Closing);
        assert_eq!(taken(), vec!["opened 11"]);

        let closes_only = table_with(None, Some(closing_notes));
        tell(&ctx, &closes_only, &mut open, Edge::Opened(11));
        tell(&ctx, &closes_only, &mut open, Edge::Closing);
        assert_eq!(taken(), vec!["closing"]);

        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_module_refusing_an_edge_does_not_stop_the_one_after_it() {
        // The user asked for the project, so a refusal is a warn line and the round carries on.
        let _serial = SERIAL.lock().unwrap_or_else(|held| held.into_inner());
        let (dir, mut open) = project("refuses");
        let ctx = HostCtx::new();
        arm(&ctx);

        let first = table_with(Some(opened_refuses), None);
        let second = table_with(Some(opened_after), None);
        tell(&ctx, &first, &mut open, Edge::Opened(5));
        tell(&ctx, &second, &mut open, Edge::Opened(5));

        assert_eq!(taken(), vec!["first refused 5", "second told 5"]);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_module_reads_its_own_storage_from_inside_both_edges() {
        // Why `project_opened` runs after the slot is filled and `project_closing` before it is
        // emptied: a module's per-project setup has to have somewhere to read, and its last write
        // somewhere to land. A call made on the other side of the change is lent no project and
        // answers that nothing is open.
        let _serial = SERIAL.lock().unwrap_or_else(|held| held.into_inner());
        let (dir, mut open) = project("storage");
        let ctx = HostCtx::new();
        arm(&ctx);

        let stores = table_with(Some(opened_stores), Some(closing_stores));
        tell(&ctx, &stores, &mut open, Edge::Opened(9));
        tell(&ctx, &stores, &mut open, Edge::Closing);
        assert_eq!(
            taken(),
            vec![
                format!("opened stored {SUBLORE_OK}"),
                format!("closing stored {SUBLORE_OK}")
            ]
        );

        // And the same call with nothing in the slot says so, which is what the mutation produces.
        let mut nothing = None;
        tell(&ctx, &stores, &mut nothing, Edge::Opened(9));
        assert_eq!(
            taken(),
            vec![format!("opened stored {SUBLORE_ERR_NOTHING_OPEN}")]
        );

        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_edge_is_lent_the_project_and_refused_the_session_and_the_panels() {
        // The nothing-open answer comes before any denial, because "there is nothing to read" is a
        // different fact from "you may not read it", and `host.rs` already orders them that way.
        let _serial = SERIAL.lock().unwrap_or_else(|held| held.into_inner());
        let (dir, mut open) = project("lent");
        let ctx = HostCtx::new();
        arm(&ctx);

        let asks = table_with(Some(opened_asks_for_everything), None);
        tell(&ctx, &asks, &mut open, Edge::Opened(3));

        assert_eq!(
            taken(),
            vec![
                format!("document Some({SUBLORE_ERR_NOTHING_OPEN})"),
                format!("cue_at Some({SUBLORE_ERR_NOTHING_OPEN})"),
                format!("propose Some({SUBLORE_ERR_NOTHING_OPEN})"),
                format!("panel_begin Some({SUBLORE_ERR_DENIED})"),
                format!("panel_row Some({SUBLORE_ERR_DENIED})"),
                format!("panel_end Some({SUBLORE_ERR_DENIED})"),
                // Nobody has asked for anything, which is H8's rule for every call with no
                // activation behind it.
                "should_cancel Some(0)".to_owned(),
            ]
        );

        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_module_whose_create_failed_is_skipped_rather_than_called_through_a_null() {
        let _serial = SERIAL.lock().unwrap_or_else(|held| held.into_inner());
        let (dir, mut open) = project("no-instance");
        let ctx = HostCtx::new();
        arm(&ctx);

        let filled = table_with(Some(opened_notes), Some(closing_notes));
        tell_one(
            &ctx,
            NAME,
            &filled,
            std::ptr::null_mut(),
            Some(&mut open),
            Some(STORAGE.to_owned()),
            Edge::Opened(1),
        );
        assert!(taken().is_empty(), "there is no instance to call");

        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }
}
