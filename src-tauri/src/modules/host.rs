//! The host's half of the boundary: the context a module is handed, and the callbacks it reaches
//! through it.
//!
//! **The gate is the whole of the safety here.** `module-abi.md` §2.5 rules that a module may call
//! host functions only from the thread of the host call it is inside, and only before that call
//! returns. This file arms a record before every call into a module and disarms it after, and every
//! callback below reads that record before it does anything else. A module that squirrels the host
//! pointer away and calls it from a thread of its own gets `SUBLORE_ERR_WRONG_THREAD`, which is an
//! error code rather than a deadlock or a use-after-free.
//!
//! §2.5 also asks for a generation number beside the thread id, and there is none, because a
//! generation is only a check if the caller presents one to compare against and a module presents
//! nothing. A call arriving after the one it belonged to returned is caught by the record being
//! absent, and a call from elsewhere by the thread id. See docs/module-host-tasks.md H1.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

use sublore_edit::diff::CueView;
use sublore_edit::history::Run;
use sublore_edit::plan::Edit;
use sublore_edit::session::EditSession;
use sublore_formats::SubtitleFormat;
use sublore_module_api::{
    SubloreCell, SubloreCue, SubloreDocument, SubloreHitFn, SubloreHost, SubloreLine,
    SubloreLineFn, SubloreProposal, SubloreRowFn, SubloreStr, SubloreValue, SubloreWorkFn,
    SUBLORE_ABI_MINOR, SUBLORE_ERR_BAD_STRING, SUBLORE_ERR_DENIED, SUBLORE_ERR_NOTHING_OPEN,
    SUBLORE_ERR_NO_SUCH_CUE, SUBLORE_ERR_PANIC, SUBLORE_ERR_STALE_REVISION, SUBLORE_ERR_STORAGE,
    SUBLORE_ERR_UNSUPPORTED, SUBLORE_ERR_UNWRITABLE_TEXT, SUBLORE_ERR_WRONG_THREAD,
    SUBLORE_FIND_OPTIONS, SUBLORE_FORMAT_ASS, SUBLORE_FORMAT_SRT, SUBLORE_FORMAT_VTT,
    SUBLORE_HOST_SIZE, SUBLORE_LINE_FLAG_FILE_MISSING, SUBLORE_LOG_DEBUG, SUBLORE_LOG_ERROR,
    SUBLORE_LOG_INFO, SUBLORE_LOG_WARN, SUBLORE_NO_CUE, SUBLORE_OK, SUBLORE_PROPOSAL_SET_CUE_TEXT,
    SUBLORE_ROLE_SOURCE, SUBLORE_ROLE_TARGET, SUBLORE_VALUE_BLOB, SUBLORE_VALUE_INT,
    SUBLORE_VALUE_NULL, SUBLORE_VALUE_REAL, SUBLORE_VALUE_TEXT,
};
use sublore_project::model::FileRole;
use sublore_project::module_store::{self, Cell, OpenTransaction, StoreRefusal};
use sublore_project::records::{self, Project};
use tauri::{AppHandle, Emitter};

use super::{
    cell_kind, CellDto, ModuleProgress, ModuleStatus, PanelDto, RowDto, EVENT_PROGRESS,
    EVENT_STATUS,
};
use crate::log;
use crate::subtitle::error::{SubtitleError, SubtitleErrorCode};
use crate::subtitle::{check_revision, describe, guard_size, CuePatchDto};

/// The longest module log line this build writes.
///
/// A module can hand over a string of any length, and a megabyte of it in the log file is the kind
/// of annoyance CONTRIBUTING.md §3 puts a budget on. Cut on a character boundary, so what is
/// written is still a string.
const MAX_LOG_BYTES: usize = 4096;

/// The most rows one panel run may push.
///
/// A budget, on the same footing as the cap above and for the same reason: the rows are the host's
/// memory and the window's work, and a module that loops for ever has to cost a refusal rather than
/// the process. Five times CONTRIBUTING.md §7's two-thousand-line document, so a table with one row
/// per cue of an episode is nowhere near it.
const MAX_PANEL_ROWS: usize = 10_000;

/// What `should_cancel` answers. Two meanings only: zero keeps going, anything else stops.
///
/// Every code in the frozen list is positive (`sublore-module-api`), so a refusal returned here
/// would read as "stop" whatever it said. It therefore says stop on purpose, and the one place that
/// keeps going is the one with no user behind it: `create`, `describe` and `destroy` have no
/// activation, so nobody has asked for anything.
const CANCEL_KEEP_GOING: i32 = 0;
const CANCEL_STOP: i32 = 1;

/// What one activation lends beyond the two locks: the window to speak to while the call runs,
/// the call's own id, the flag a Stop sets, and the panels this module may fill.
///
/// One field on the record rather than four, so "am I inside an activation" is one question, and
/// its absence is the single refusal that covers `status`, `progress` and the three panel calls
/// during `create`, `describe` and `destroy`.
pub struct Activation {
    window: Window,
    call: u64,
    /// Which module this is, by the index the window drew it from. Written onto every panel it
    /// publishes, because the record is what a panel callback can reach and the module is not.
    module: usize,
    cancel: Arc<AtomicBool>,
    /// The panel ids this module contributed. An id outside it is refused, so a module cannot fill
    /// a panel it never described, nor one another module did.
    panels: Vec<u32>,
}

impl Activation {
    pub fn new(
        app: AppHandle,
        call: u64,
        module: usize,
        cancel: Arc<AtomicBool>,
        panels: Vec<u32>,
    ) -> Self {
        Self {
            window: Window::Live(app),
            call,
            module,
            cancel,
            panels,
        }
    }
}

/// Where a `status` and a `progress` go.
///
/// A named type over the window rather than the handle itself, and that is not indirection for its
/// own sake: `AppHandle` is `AppHandle<Wry>`, nothing in a unit check can build one, and an
/// activation that needed one would leave the two panel bodies and `should_cancel` with no check at
/// all. The second variant exists only in a test build and keeps what was sent.
enum Window {
    Live(AppHandle),
    #[cfg(test)]
    Collected(Arc<Mutex<Vec<Said>>>),
}

/// One thing a module said while its work ran, as a check reads it back.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Said {
    Status(u64, String),
    Progress(u64, u64, u64),
}

impl Window {
    /// Emitting is best effort on purpose. A window that has gone is not a reason to refuse a
    /// module's work, and the same `let _ =` is what `asr` does with its own four events.
    fn status(&self, call: u64, message: String) {
        match self {
            Self::Live(app) => {
                let _ = app.emit(
                    EVENT_STATUS,
                    ModuleStatus {
                        call_id: call,
                        message,
                    },
                );
            }
            #[cfg(test)]
            Self::Collected(said) => {
                if let Ok(mut held) = said.lock() {
                    held.push(Said::Status(call, message));
                }
            }
        }
    }

    fn progress(&self, call: u64, done: u64, total: u64) {
        match self {
            Self::Live(app) => {
                let _ = app.emit(
                    EVENT_PROGRESS,
                    ModuleProgress {
                        call_id: call,
                        done,
                        total,
                    },
                );
            }
            #[cfg(test)]
            Self::Collected(said) => {
                if let Ok(mut held) = said.lock() {
                    held.push(Said::Progress(call, done, total));
                }
            }
        }
    }
}

/// The panel run a module has open, between its `panel_begin` and its `panel_end`.
struct OpenPanel {
    id: u32,
    /// Fixed by the first row and held to by every row after it, so a ragged table cannot exist and
    /// no number had to be picked for how wide one is.
    width: Option<usize>,
    rows: Vec<RowDto>,
}

/// What is true for the duration of one call into a module.
struct InFlight {
    /// The thread the host made the call on.
    thread: ThreadId,
    /// The module's file name, so a line it logs can be told from a core one.
    name: String,
    /// The session the host locked before making this call, or none when it made the call without
    /// one.
    ///
    /// **The lock is held across the whole module call and that is a memory-safety requirement, not
    /// a performance note.** `cue_at` hands over text borrowed out of the session, and the thing
    /// that can drop that text is not the module: it is the user, committing an edit from the
    /// window's own thread. A lock released between the borrow and the module's read of it leaves
    /// the module holding a freed string. See `module-abi.md` §2.5 and docs/module-host-tasks.md H4.
    ///
    /// The pointer is valid for exactly as long as this record is armed, which [`Entered`] ties to
    /// the borrow it was made from.
    session: Option<*mut Option<EditSession>>,
    /// The project the host locked before making this call, or none when none is open.
    ///
    /// Lent on the same terms as the session and for a different reason: the connection under it is
    /// `Send` and not `Sync`, so the lock is what keeps two threads off one connection, and holding
    /// it across the call is what lets a module's own transaction stay open while its body runs.
    ///
    /// **Nothing in this process takes this lock and the session's in the other order**, which is
    /// what stops the pair deadlocking. `crate::project` never reaches the session and
    /// `crate::subtitle` never reaches the project; the two meet here and nowhere else.
    project: Option<*mut Option<Project>>,
    /// The name this module's own tables are prefixed with, or none when its file name yields no id
    /// the storage will accept. None costs it its storage and never gives it another module's.
    storage: Option<String>,
    /// The transaction this module has open, while it has one.
    ///
    /// A statement made from inside a transaction runs through this and not through the project
    /// above, for two reasons that both have to hold: the transaction already installed the guard
    /// for the whole of its body, and it already holds the only mutable borrow of the project.
    open: Option<OpenTransaction>,
    /// What the module changed during this call, in the order it changed it.
    ///
    /// A module proposes one cue at a time and may propose more than once, and each one is an edit
    /// the window has to be told about. They are collected here rather than returned, because
    /// `propose` answers a code and the interface has no way back for a value (§2.2).
    proposed: Vec<CuePatchDto>,
    /// The activation this call is, or none when the host called for its own reasons.
    activation: Option<Activation>,
    /// The panel run open right now. Nothing in it is published until `panel_end` closes it, which
    /// is what makes a half filled table impossible rather than merely unlikely.
    panel_open: Option<OpenPanel>,
    /// The runs this call closed, in the order it closed them.
    filled: Vec<PanelDto>,
    /// The last pair `progress` emitted, so an exact repeat carries nothing and is dropped. A
    /// repeat dropped and not a throttle: there is no clock here.
    last_progress: Option<(u64, u64)>,
}

/// The host's own context: the value behind `SubloreHost::ctx`, handed to every module once and
/// given back by it unchanged on every call.
///
/// Boxed by its owner and never moved, because the modules hold the pointer for their whole life.
pub struct HostCtx {
    /// The call in flight, or none. Locked only for the moment it takes to read or write, never
    /// across a call into a module: a module callback takes this same lock.
    call: Mutex<Option<InFlight>>,
}

impl HostCtx {
    pub fn new() -> Self {
        Self {
            call: Mutex::new(None),
        }
    }

    /// Arm the context for one call into the module named `name`, on this thread.
    ///
    /// `lent` is what the host locked before making the call. The returned guard borrows all of it
    /// for its own lifetime, so no lock can be released while the record still names what it
    /// guards, and the guard disarms on drop, so a body that returns early or panics leaves the
    /// context closed behind it.
    pub fn enter<'a>(&'a self, name: &str, lent: Lent<'a>) -> Entered<'a> {
        let Lent {
            session,
            project,
            storage,
            activation,
        } = lent;
        // Recovered rather than refused: nothing but this assignment runs under this lock, so a
        // poisoning could only come from a panic elsewhere, and refusing for ever afterwards would
        // cost every later module call for a fault that is not in them.
        let mut call = self.call.lock().unwrap_or_else(|held| {
            self.call.clear_poison();
            held.into_inner()
        });
        *call = Some(InFlight {
            thread: std::thread::current().id(),
            name: name.to_owned(),
            session: session.map(|held| held as *mut Option<EditSession>),
            project: project.map(|held| held as *mut Option<Project>),
            storage,
            open: None,
            proposed: Vec::new(),
            activation,
            panel_open: None,
            filled: Vec::new(),
            last_progress: None,
        });
        Entered {
            ctx: self,
            borrowed: PhantomData,
        }
    }

    /// Whether the caller is inside a call this context armed, on the thread it was armed from.
    fn mine(&self) -> bool {
        self.with(|call| call.thread == std::thread::current().id())
            .unwrap_or(false)
    }

    /// The name of the module whose call the caller is inside, or none when it is not inside one.
    fn name(&self) -> Option<String> {
        self.with(|call| (call.thread == std::thread::current().id()).then(|| call.name.clone()))
            .flatten()
    }

    /// The session lent to the call the caller is inside, as a pointer, or none.
    ///
    /// A pointer and not a reference on purpose. Handing out a `&mut` from a `&self` would let two
    /// callbacks hold two mutable references to one session, and no comment can stop that: the
    /// borrow has to be made at the point of use, in a scope narrow enough to see. A reader takes a
    /// shared reference and never a mutable one.
    ///
    /// The pointer is valid for as long as the record is armed, which [`Entered`] ties to the
    /// borrow the host lent.
    fn session(&self) -> Option<*mut Option<EditSession>> {
        self.with(|call| {
            (call.thread == std::thread::current().id())
                .then_some(call.session)
                .flatten()
        })
        .flatten()
    }

    /// The project lent to the call the caller is inside, as a pointer, on the same terms and for
    /// the same reason as [`HostCtx::session`].
    fn project(&self) -> Option<*mut Option<Project>> {
        self.with(|call| {
            (call.thread == std::thread::current().id())
                .then_some(call.project)
                .flatten()
        })
        .flatten()
    }

    /// The prefix this module's own tables live under, or none when it has no usable id.
    fn storage(&self) -> Option<String> {
        self.with(|call| {
            (call.thread == std::thread::current().id())
                .then(|| call.storage.clone())
                .flatten()
        })
        .flatten()
    }

    /// The transaction this module has open, or none.
    fn open(&self) -> Option<OpenTransaction> {
        self.with(|call| {
            (call.thread == std::thread::current().id())
                .then_some(call.open)
                .flatten()
        })
        .flatten()
    }

    /// Read the activation this call is, on the thread it was armed from, or none.
    ///
    /// None means two different things to two callers and both are right: `should_cancel` reads it
    /// as nobody having asked for anything, and the panel calls read it as a refusal, because a
    /// panel filled outside an activation has no window waiting for it.
    fn with_activation<R>(&self, read: impl FnOnce(&Activation) -> R) -> Option<R> {
        self.with(|call| {
            (call.thread == std::thread::current().id())
                .then(|| call.activation.as_ref().map(read))
                .flatten()
        })
        .flatten()
    }

    /// Note the transaction now open, or that the one that was is finished.
    fn set_open(&self, open: Option<OpenTransaction>) {
        self.record(|call| call.open = open);
    }

    /// Change the record, holding the lock for the change and nothing else.
    fn record<R>(&self, write: impl FnOnce(&mut InFlight) -> R) -> Option<R> {
        self.call.lock().ok()?.as_mut().map(write)
    }

    /// Read the record, holding the lock for the read and nothing else.
    ///
    /// A poisoned lock answers none, which every caller turns into a refusal: this one fails
    /// closed, because the alternative is serving a module out of a record nobody can vouch for.
    fn with<R>(&self, read: impl FnOnce(&InFlight) -> R) -> Option<R> {
        self.call.lock().ok()?.as_ref().map(read)
    }
}

impl Default for HostCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// Safety: the record holds raw pointers, which is what costs `HostCtx` its automatic marker
/// traits, and sharing the context across threads is exactly what it is for: a module that calls
/// from a thread of its own has to reach a refusal rather than a missing symbol. Every one of them
/// is dereferenced only after the thread comparison in the reader that hands it out has passed, and
/// that comparison fails on every thread but the one the record was armed from, so no second thread
/// can ever reach the session, the project or an open transaction through them.
unsafe impl Send for HostCtx {}
unsafe impl Sync for HostCtx {}

/// What the host locks before a call into a module and lends it for the length of that call.
///
/// A value rather than a list of arguments, so a caller adding one more thing to lend cannot leave
/// an old call site quietly lending less than it holds.
#[derive(Default)]
pub struct Lent<'a> {
    session: Option<&'a mut Option<EditSession>>,
    project: Option<&'a mut Option<Project>>,
    storage: Option<String>,
    activation: Option<Activation>,
}

impl<'a> Lent<'a> {
    /// The open document, locked by the caller for the whole of the call.
    ///
    /// Takes an option, because a caller whose own lock was poisoned has a session it holds and
    /// cannot lend, and that is not the same thing as a caller with nothing to lend.
    pub fn with_session(mut self, session: Option<&'a mut Option<EditSession>>) -> Self {
        self.session = session;
        self
    }

    /// The open project and the prefix this module's own tables live under.
    ///
    /// The two together, because a project with no id to reach it by is a project no module may
    /// touch: `storage` is what every statement is held inside, so lending one without the other
    /// would be lending storage nobody can name.
    pub fn with_project(
        mut self,
        project: &'a mut Option<Project>,
        storage: Option<String>,
    ) -> Self {
        self.project = Some(project);
        self.storage = storage;
        self
    }

    /// The window, the call id, the cancel flag and the panels this module may fill.
    ///
    /// Lent only by `ModuleState::invoke`, because it is the only call a user made: `create`,
    /// `describe` and `destroy` lend none, and every callback that needs one refuses there.
    pub fn with_activation(mut self, activation: Activation) -> Self {
        self.activation = Some(activation);
        self
    }
}

/// One armed call. Disarms the context when it goes.
///
/// Held rather than discarded, always: dropping it on the spot arms the gate and closes it again
/// before the call it was armed for is made, which would refuse every callback that call attempts.
#[must_use = "the gate is armed only while this guard is alive"]
pub struct Entered<'a> {
    ctx: &'a HostCtx,
    /// The borrows the record holds pointers to. Nothing reads this field: it is what stops the
    /// caller from releasing a lock while the record still names what it guards.
    borrowed: PhantomData<(&'a mut Option<EditSession>, &'a mut Option<Project>)>,
}

impl Entered<'_> {
    /// What the module changed during this call. Taken before the guard goes, because the record it
    /// reads is what the guard clears.
    pub fn proposed(&self) -> Vec<CuePatchDto> {
        self.ctx
            .record(|call| std::mem::take(&mut call.proposed))
            .unwrap_or_default()
    }

    /// The panel runs this call closed, on the same terms and taken at the same moment.
    ///
    /// A run closed by `panel_end` publishes whatever `invoke` answers afterwards, which is the
    /// rule `modules/mod.rs` already applies to the cues a module changed before it refused.
    pub fn panels(&self) -> Vec<PanelDto> {
        self.ctx
            .record(|call| std::mem::take(&mut call.filled))
            .unwrap_or_default()
    }

    /// The run the module left open, if it left one, as its id and the rows it had pushed.
    ///
    /// Taken rather than read, so the caller has to decide what to say about it and the record
    /// cannot carry it into anything else.
    pub fn abandoned_panel(&self) -> Option<(u32, usize)> {
        self.ctx
            .record(|call| {
                call.panel_open
                    .take()
                    .map(|open| (open.id, open.rows.len()))
            })
            .flatten()
    }
}

impl Drop for Entered<'_> {
    fn drop(&mut self) {
        // Recovered on the same terms as `enter`, so a poisoned lock cannot leave a record behind
        // naming a thread that has already left.
        let mut call = self.ctx.call.lock().unwrap_or_else(|held| {
            self.ctx.call.clear_poison();
            held.into_inner()
        });
        *call = None;
    }
}

/// The host table as this build fills it.
///
/// A slot left empty is a refusal the module can see rather than a jump it cannot, so the ones
/// whose bodies do not exist yet stay `None` on purpose. See docs/module-host-tasks.md.
pub fn table(ctx: &HostCtx) -> SubloreHost {
    SubloreHost {
        size: SUBLORE_HOST_SIZE,
        minor: SUBLORE_ABI_MINOR,
        // The one pointer a module carries and never dereferences. `HostCtx` is the only type it is
        // ever cast to or from, and both casts are in this file.
        ctx: (ctx as *const HostCtx).cast_mut().cast::<c_void>(),
        log: Some(host_log),
        should_cancel: Some(host_should_cancel),
        progress: Some(host_progress),
        document: Some(host_document),
        cue_at: Some(host_cue_at),
        for_each_line: Some(host_for_each_line),
        propose: Some(host_propose),
        find: Some(host_find),
        db_run: Some(host_db_run),
        db_transaction: Some(host_db_transaction),
        panel_begin: Some(host_panel_begin),
        panel_row: Some(host_panel_row),
        panel_end: Some(host_panel_end),
        status: Some(host_status),
    }
}

/// The context behind the pointer a module handed back, or none when it handed back nothing.
///
/// # Safety
/// `host` is the pointer this process wrote into `SubloreHost::ctx` and a module gave back
/// unchanged. The context outlives every module, because `Held` declares it after them.
unsafe fn ctx_of<'a>(host: *mut c_void) -> Option<&'a HostCtx> {
    if host.is_null() {
        return None;
    }
    Some(unsafe { &*host.cast::<HostCtx>() })
}

/// Run a callback body, turning a panic into a code rather than an unwind into foreign frames
/// (§2.4).
///
/// `AssertUnwindSafe` because the captures are raw pointers the module owns and a shared reference
/// to a context whose only state is a small record behind a mutex: a panic here cannot leave this
/// process holding half of anything.
fn guarded(work: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(work)).unwrap_or(SUBLORE_ERR_PANIC)
}

/// As much of `text` as the log takes, cut on a character boundary.
fn capped(text: &str) -> &str {
    if text.len() <= MAX_LOG_BYTES {
        return text;
    }
    let mut end = MAX_LOG_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// A module's own diagnostics, at the four levels §4.2 fixes.
///
/// # Safety
/// `host` is the context pointer, and `message` points at bytes valid for this call.
unsafe extern "C" fn host_log(host: *mut c_void, level: u32, message: SubloreStr) {
    // No return value, so a refusal is silent to the module and said on this side instead.
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            log::warn!("modules: a log line arrived with no host context and was dropped");
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(name) = ctx.name() else {
            log::warn!(
                "modules: a log line arrived outside the call it belonged to and was dropped"
            );
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Ok(text) = (unsafe { message.as_str() }) else {
            log::warn!("modules: {name} logged a line that is not valid text");
            return SUBLORE_ERR_BAD_STRING;
        };
        let text = capped(text);
        // A level outside the four is not a fifth level, it is a module defect, and the line is
        // dropped rather than guessed at.
        match level {
            SUBLORE_LOG_ERROR => log::error!("module {name}: {text}"),
            SUBLORE_LOG_WARN => log::warn!("module {name}: {text}"),
            SUBLORE_LOG_INFO => log::info!("module {name}: {text}"),
            SUBLORE_LOG_DEBUG => log::debug!("module {name}: {text}"),
            _ => {
                log::warn!("modules: {name} logged at level {level}, which is not one of the four");
                return SUBLORE_ERR_UNSUPPORTED;
            }
        }
        SUBLORE_OK
    });
}

/// A line a module puts on screen while its own work runs (§4.2).
///
/// An event and not a value carried back, for the reason `module_invoke` writes out: the call is
/// synchronous on a blocking thread with the window waiting for it, so anything returned arrives
/// after the work it described has finished.
///
/// No return value, so like `host_log` a refusal is silent to the module and said on this side.
///
/// # Safety
/// `host` is the context pointer, and `message` points at bytes valid for this call.
unsafe extern "C" fn host_status(host: *mut c_void, message: SubloreStr) {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            log::warn!("modules: a status arrived with no host context and was dropped");
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(name) = ctx.name() else {
            log::warn!("modules: a status arrived outside the call it belonged to");
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Ok(text) = (unsafe { message.as_str() }) else {
            log::warn!("modules: {name} set a status that is not valid text");
            return SUBLORE_ERR_BAD_STRING;
        };
        // The same cap a log line takes, on a character boundary: this is one line on screen and a
        // megabyte of it is the same annoyance in a different place.
        let text = capped(text).to_owned();
        let said = ctx.with_activation(|active| active.window.status(active.call, text));
        if said.is_none() {
            log::warn!("modules: {name} set a status outside an activation, so nothing shows it");
            return SUBLORE_ERR_DENIED;
        }
        SUBLORE_OK
    });
}

/// How far a module says its own work has got (§4.2), on the same terms as `status`.
///
/// The two numbers are the module's own and are not compared with each other: the core draws what
/// it was given and knows nothing about the work behind it.
///
/// # Safety
/// `host` is the context pointer.
unsafe extern "C" fn host_progress(host: *mut c_void, done: u64, total: u64) {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            log::warn!("modules: a progress arrived with no host context and was dropped");
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(name) = ctx.name() else {
            log::warn!("modules: a progress arrived outside the call it belonged to");
            return SUBLORE_ERR_WRONG_THREAD;
        };
        // A pair the last call already sent says nothing new, so nothing is sent. A repeat dropped
        // rather than a rate limited, which is why this holds no clock.
        let fresh = ctx
            .record(|held| {
                let repeat = held.last_progress == Some((done, total));
                held.last_progress = Some((done, total));
                !repeat
            })
            .unwrap_or(false);
        let inside = ctx.with_activation(|active| {
            if fresh {
                active.window.progress(active.call, done, total);
            }
        });
        if inside.is_none() {
            log::warn!("modules: {name} reported progress outside an activation");
            return SUBLORE_ERR_DENIED;
        }
        SUBLORE_OK
    });
}

/// Whether the user has asked this activation to stop (§4.2).
///
/// **A request and nothing more.** The call is foreign code on a blocking thread: killing that
/// thread would leave the session lock, the project lock and the armed record behind, and unwinding
/// out of `extern "C"` aborts the process, which is the ceiling §2.4 already records. So a module
/// that never asks, or asks and ignores the answer, is not stopped by anything, and that is its own
/// defect. `ModuleState::invoke` writes one warn line naming it. See docs/module-host-tasks.md H8.
///
/// # Safety
/// `host` is the context pointer.
unsafe extern "C" fn host_should_cancel(host: *mut c_void) -> i32 {
    guarded(|| {
        // Every refusal below answers stop, and so does the `SUBLORE_ERR_PANIC` `guarded` returns
        // for a panic in this body: a host that cannot answer must not be read as "carry on".
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return CANCEL_STOP;
        };
        if !ctx.mine() {
            return CANCEL_STOP;
        }
        // No activation is `create`, `describe` or `destroy`, where no user has asked for anything.
        ctx.with_activation(|active| i32::from(active.cancel.load(Ordering::SeqCst)))
            .unwrap_or(CANCEL_KEEP_GOING)
    })
}

/// Open a panel run for `panel_id` (§5.3).
///
/// Nothing is published until `panel_end` closes it, so a panel between the two shows what it
/// showed before, and a module that stops halfway leaves no half table behind.
///
/// # Safety
/// `host` is the context pointer.
unsafe extern "C" fn host_panel_begin(host: *mut c_void, panel_id: u32) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(name) = ctx.name() else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        // A panel filled outside an activation has no window waiting for it, and no gesture behind
        // it: `describe` pushes items, it does not fill them.
        let Some(allowed) = ctx.with_activation(|active| active.panels.contains(&panel_id)) else {
            log::warn!("modules: {name} began a panel outside an activation");
            return SUBLORE_ERR_DENIED;
        };
        // The allowlist is the host's, from what this module described, so a module cannot fill a
        // panel it never contributed nor one that belongs to another module (H6's id rule).
        if !allowed {
            log::warn!("modules: {name} began panel {panel_id}, which it never contributed");
            return SUBLORE_ERR_DENIED;
        }
        ctx.record(|held| {
            // Refused rather than nested, which is the shape `db_transaction` already refuses in
            // and for the same reason: a second run is one the host cannot account for.
            if held.panel_open.is_some() {
                return SUBLORE_ERR_DENIED;
            }
            held.panel_open = Some(OpenPanel {
                id: panel_id,
                width: None,
                rows: Vec::new(),
            });
            SUBLORE_OK
        })
        .unwrap_or(SUBLORE_ERR_WRONG_THREAD)
    })
}

/// One row of the open panel run (§5.3).
///
/// Every cell is validated before any of it is kept, so a refused row leaves nothing behind.
///
/// # Safety
/// `host` is the context pointer; `cells` points at `cell_count` readable cells for this call, or
/// is null when the count is zero, and every string among them points at bytes valid for this call.
unsafe extern "C" fn host_panel_row(
    host: *mut c_void,
    cells: *const SubloreCell,
    cell_count: usize,
) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(name) = ctx.name() else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        if cells.is_null() {
            return SUBLORE_ERR_BAD_STRING;
        }
        // A row with no cells is a row the table has nothing to draw, and the first row is what
        // fixes the width, so zero is not a width this can take.
        if cell_count == 0 {
            log::warn!("modules: {name} pushed a row with no cells");
            return SUBLORE_ERR_DENIED;
        }
        let given = unsafe { std::slice::from_raw_parts(cells, cell_count) };

        let mut row = Vec::with_capacity(cell_count);
        for cell in given {
            let Some(kind) = cell_kind(cell.kind) else {
                log::warn!(
                    "modules: {name} pushed a cell of kind {}, which this build has no \
                            meaning for",
                    cell.kind
                );
                return SUBLORE_ERR_UNSUPPORTED;
            };
            let Ok(text) = (unsafe { cell.text.as_str() }) else {
                log::warn!("modules: {name} pushed a cell whose text is not valid text");
                return SUBLORE_ERR_BAD_STRING;
            };
            row.push(CellDto {
                kind,
                text: text.to_owned(),
                number: cell.number,
            });
        }
        // The handle belongs to the row, so the row's is the first cell's. The core never compares
        // it with the others, because it never reads a handle for anything but handing it back.
        let handle = given[0].r#ref.to_string();

        ctx.record(|held| {
            let Some(open) = held.panel_open.as_mut() else {
                return SUBLORE_ERR_DENIED;
            };
            match open.width {
                None => open.width = Some(cell_count),
                // A ragged table cannot exist, and the bound is the first row rather than a number
                // anybody had to pick.
                Some(width) if width != cell_count => return SUBLORE_ERR_DENIED,
                Some(_) => {}
            }
            if open.rows.len() >= MAX_PANEL_ROWS {
                return SUBLORE_ERR_DENIED;
            }
            open.rows.push(RowDto { handle, cells: row });
            SUBLORE_OK
        })
        .unwrap_or(SUBLORE_ERR_WRONG_THREAD)
    })
}

/// Close the open panel run and publish it (§5.3).
///
/// An empty run clears the panel: a module saying it has no rows is a module saying there is
/// nothing to show, and that is a publish rather than an absence.
///
/// # Safety
/// `host` is the context pointer.
unsafe extern "C" fn host_panel_end(host: *mut c_void) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(name) = ctx.name() else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        ctx.record(|held| {
            let Some(module) = held.activation.as_ref().map(|active| active.module) else {
                return SUBLORE_ERR_DENIED;
            };
            let Some(open) = held.panel_open.take() else {
                log::warn!("modules: {name} ended a panel run it had not begun");
                return SUBLORE_ERR_DENIED;
            };
            held.filled.push(PanelDto {
                module,
                panel_id: open.id,
                rows: open.rows,
            });
            SUBLORE_OK
        })
        .unwrap_or(SUBLORE_ERR_WRONG_THREAD)
    })
}

/// The format word for the document a module is reading.
fn format_of(format: SubtitleFormat) -> u32 {
    match format {
        SubtitleFormat::Srt => SUBLORE_FORMAT_SRT,
        SubtitleFormat::Vtt => SUBLORE_FORMAT_VTT,
        SubtitleFormat::Ass => SUBLORE_FORMAT_ASS,
    }
}

/// The path as a module receives it, borrowed out of the session.
///
/// Borrowed and never converted, because the result is written into an out parameter the module
/// reads after the call returns: a lossy conversion would allocate here and be dropped before the
/// module ever looked at it. A path that is not UTF-8 is therefore handed over as empty, which
/// cannot happen today because a document is opened by a path that arrived as a string.
fn path_of(session: &EditSession, name: &str) -> SubloreStr {
    let Some(path) = session.path() else {
        return SubloreStr::borrowed("");
    };
    match path.to_str() {
        Some(text) => SubloreStr::borrowed(text),
        None => {
            log::warn!("modules: {name} was given no path, because this one is not valid text");
            SubloreStr::borrowed("")
        }
    }
}

/// The open document, as §4.3 defines it.
///
/// # Safety
/// `host` is the context pointer and `out` is one writable `SubloreDocument`.
unsafe extern "C" fn host_document(host: *mut c_void, out: *mut SubloreDocument) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        if out.is_null() {
            return SUBLORE_ERR_BAD_STRING;
        }
        let Some(name) = ctx.name() else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(held) = ctx.session() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };
        // Safety: armed for this call on this thread, so the host still holds the lock, and the
        // reference is shared, is used here and is not kept.
        let Some(session) = (unsafe { &*held }).as_ref() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };

        let answer = SubloreDocument {
            format: format_of(session.document().format()),
            // `views()` and not `displayed_cue_count()`: the count a module holds has to be the
            // index an edit takes, and that index space includes ASS `Comment:` events (§4.3).
            cue_count: session.views().len() as u64,
            revision: session.revision(),
            dirty: u8::from(session.dirty()),
            path: path_of(session, &name),
        };
        unsafe { out.write(answer) };
        SUBLORE_OK
    })
}

/// One cue of the open document, by the index `document`'s count is over.
///
/// # Safety
/// `host` is the context pointer and `out` is one writable `SubloreCue`.
unsafe extern "C" fn host_cue_at(host: *mut c_void, index: u64, out: *mut SubloreCue) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        if out.is_null() {
            return SUBLORE_ERR_BAD_STRING;
        }
        if !ctx.mine() {
            return SUBLORE_ERR_WRONG_THREAD;
        }
        let Some(held) = ctx.session() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };
        // Safety: as `host_document`.
        let Some(session) = (unsafe { &*held }).as_ref() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };
        // The interface pins 64-bit, so this cannot fail today. It is `try_from` rather than an
        // `as` cast so that the day the pin moves, the wrap is a refusal and not a valid index.
        let Ok(index) = usize::try_from(index) else {
            return SUBLORE_ERR_NO_SUCH_CUE;
        };
        let Some(cue) = session.views().get(index) else {
            return SUBLORE_ERR_NO_SUCH_CUE;
        };

        let answer = SubloreCue {
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            // Borrowed out of the session, which the host has locked for the whole of this module
            // call. That lock is what keeps this pointer alive until the module reads it.
            text: SubloreStr::borrowed(&cue.text),
            is_comment: u8::from(cue.comment),
            has_number: u8::from(cue.number.is_some()),
            number: cue.number.unwrap_or(0),
        };
        unsafe { out.write(answer) };
        SUBLORE_OK
    })
}

/// What a refused edit is, on the interface's own closed list.
///
/// Written as a total match so a new `SubtitleErrorCode` is a compile error here rather than a
/// silent `DENIED`. The five that map to something specific are the five a module can act on; every
/// other refusal is the host's guard saying no, which is what `DENIED` means, and the detail is in
/// the log rather than in the code.
fn refusal_of(error: &SubtitleError) -> i32 {
    match error.code {
        SubtitleErrorCode::InvalidCue => SUBLORE_ERR_NO_SUCH_CUE,
        SubtitleErrorCode::UnwritableText => SUBLORE_ERR_UNWRITABLE_TEXT,
        SubtitleErrorCode::StaleRevision => SUBLORE_ERR_STALE_REVISION,
        SubtitleErrorCode::NoDocument => SUBLORE_ERR_NOTHING_OPEN,
        SubtitleErrorCode::UnsupportedEncoding => SUBLORE_ERR_BAD_STRING,
        SubtitleErrorCode::InvalidPath
        | SubtitleErrorCode::NotAFile
        | SubtitleErrorCode::TooLarge
        | SubtitleErrorCode::ReadFailed
        | SubtitleErrorCode::UnknownFormat
        | SubtitleErrorCode::ParseFailed
        | SubtitleErrorCode::WriteFailed
        | SubtitleErrorCode::BackupFailed
        | SubtitleErrorCode::PermissionDenied
        | SubtitleErrorCode::EditRefused
        | SubtitleErrorCode::UnsavedChanges
        | SubtitleErrorCode::NoPath
        | SubtitleErrorCode::TranscriptionGone
        | SubtitleErrorCode::CommandFailed => SUBLORE_ERR_DENIED,
    }
}

/// The module proposes and the host edits (§4.5).
///
/// Nothing here is a new mutation path. The revision check, the size guard and `EditSession::apply`
/// are the three the interactive commands run, in that order, so everything under them holds
/// without a line written for it: the plan, the splice refusing unless the bytes it recorded are
/// the bytes that are there, the coverage guard, the verify, and the undo entry.
///
/// **There is no save on this boundary and there will not be one.** The document goes dirty and the
/// user saves.
///
/// # Safety
/// `host` is the context pointer and `proposal` points at one `SubloreProposal` valid for this call.
unsafe extern "C" fn host_propose(host: *mut c_void, proposal: *const SubloreProposal) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        if proposal.is_null() {
            return SUBLORE_ERR_BAD_STRING;
        }
        let Some(name) = ctx.name() else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let proposal = unsafe { &*proposal };
        // One defined kind, and every other value refused. That is what keeps the field from
        // freezing into a wall when the core grows a second thing to address (§4.5).
        if proposal.kind != SUBLORE_PROPOSAL_SET_CUE_TEXT {
            return SUBLORE_ERR_UNSUPPORTED;
        }
        let Ok(text) = (unsafe { proposal.text.as_str() }) else {
            return SUBLORE_ERR_BAD_STRING;
        };
        let Ok(cue) = usize::try_from(proposal.cue) else {
            return SUBLORE_ERR_NO_SUCH_CUE;
        };

        let Some(held) = ctx.session() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };
        // Safety: armed for this call on this thread, so the host holds the lock for the whole of
        // it. The mutable reference is made here, used here, and not kept.
        let Some(session) = (unsafe { &mut *held }).as_mut() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };

        let edit = Edit::SetText {
            cue,
            text: text.to_owned(),
        };
        if let Err(error) =
            check_revision(session, proposal.revision).and_then(|()| guard_size(session, &edit))
        {
            log::warn!(
                "modules: {name} proposed an edit that was refused: {}",
                error.detail
            );
            return refusal_of(&error);
        }
        let patch = match session.apply(&edit, Run::New, std::time::Instant::now()) {
            Ok(patch) => patch,
            Err(error) => {
                let refused = SubtitleError::from_edit(error);
                log::warn!(
                    "modules: {name} proposed an edit that would not apply: {}",
                    refused.detail
                );
                return refusal_of(&refused);
            }
        };
        log::info!(
            "modules: {name} changed cue {cue}, revision {} now",
            session.revision()
        );
        let told = describe(session, patch);
        // Collected rather than answered: `propose` returns a code, and the window is told by the
        // call that carried the module into this one.
        ctx.record(|call| call.proposed.push(told));
        SUBLORE_OK
    })
}

/// The core's comparison, so the free product's find and a module's cannot drift apart (§4.6).
///
/// # Safety
/// `host` is the context pointer; `haystack` and `needle` point at bytes valid for this call; `sink`
/// and `on_hit` are the module's own.
unsafe extern "C" fn host_find(
    host: *mut c_void,
    haystack: SubloreStr,
    needle: SubloreStr,
    options: u32,
    sink: *mut c_void,
    on_hit: SubloreHitFn,
) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        if !ctx.mine() {
            return SUBLORE_ERR_WRONG_THREAD;
        }
        let Some(on_hit) = on_hit else {
            return SUBLORE_ERR_UNSUPPORTED;
        };
        // An allowlist, and not a mask: a module asking for a comparison it is not getting would
        // produce a wrong result rather than a missing one.
        if options & !SUBLORE_FIND_OPTIONS != 0 {
            return SUBLORE_ERR_UNSUPPORTED;
        }
        let (Ok(haystack), Ok(needle)) = (unsafe { haystack.as_str() }, unsafe { needle.as_str() })
        else {
            return SUBLORE_ERR_BAD_STRING;
        };

        let mut answer = SUBLORE_OK;
        sublore_matcher::find(haystack, needle, options, |hit| {
            // Safety: the module's own function, called with the sink it gave, inside the call it
            // is already in.
            answer = unsafe { on_hit(sink, hit.start, hit.end - hit.start) };
            answer == SUBLORE_OK
        });
        answer
    })
}

/// What a refusal from the store means to a module.
fn storage_code(refusal: &StoreRefusal) -> i32 {
    match refusal {
        StoreRefusal::Denied => SUBLORE_ERR_DENIED,
        // The host refused to run it rather than the statement failing, which is what `DENIED`
        // says: nothing behind the semicolon was prepared, let alone executed (§4.7).
        StoreRefusal::MoreThanOneStatement => SUBLORE_ERR_DENIED,
        StoreRefusal::Failed(_) => SUBLORE_ERR_STORAGE,
    }
}

/// The parameters a module bound, read out of its own array.
///
/// # Safety
/// `params` points at `count` readable values for this call, or is null when `count` is zero. Every
/// string among them points at bytes valid for this call.
unsafe fn bound(params: *const SubloreValue, count: usize) -> Result<Vec<Cell>, i32> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if params.is_null() {
        return Err(SUBLORE_ERR_BAD_STRING);
    }
    let given = unsafe { std::slice::from_raw_parts(params, count) };
    given
        .iter()
        .map(|value| match value.kind {
            SUBLORE_VALUE_NULL => Ok(Cell::Null),
            SUBLORE_VALUE_INT => Ok(Cell::Int(value.i)),
            SUBLORE_VALUE_REAL => Ok(Cell::Real(value.f)),
            SUBLORE_VALUE_TEXT => {
                unsafe { value.s.as_str() }.map(|text| Cell::Text(text.to_owned()))
            }
            SUBLORE_VALUE_BLOB => Ok(Cell::Blob(unsafe { blob(value.s) }.to_vec())),
            // A kind this build has no meaning for is refused rather than guessed at: a parameter
            // read as the wrong type is a statement that runs and answers the wrong thing.
            _ => Err(SUBLORE_ERR_UNSUPPORTED),
        })
        .collect()
}

/// The bytes of a blob, which are not a string and are not validated as one.
///
/// # Safety
/// `bytes.ptr` points at `bytes.len` readable bytes for this call, or is null when the length is
/// zero.
unsafe fn blob<'a>(bytes: SubloreStr) -> &'a [u8] {
    if bytes.len == 0 || bytes.ptr.is_null() {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(bytes.ptr, bytes.len) }
}

/// One cell on its way back to a module. Borrowed out of `cell`, which the caller keeps alive for
/// the length of the push.
fn returned(cell: &Cell) -> SubloreValue {
    let empty = SubloreStr {
        ptr: std::ptr::null(),
        len: 0,
    };
    match cell {
        Cell::Null => SubloreValue {
            kind: SUBLORE_VALUE_NULL,
            i: 0,
            f: 0.0,
            s: empty,
        },
        Cell::Int(number) => SubloreValue {
            kind: SUBLORE_VALUE_INT,
            i: *number,
            f: 0.0,
            s: empty,
        },
        Cell::Real(number) => SubloreValue {
            kind: SUBLORE_VALUE_REAL,
            i: 0,
            f: *number,
            s: empty,
        },
        Cell::Text(text) => SubloreValue {
            kind: SUBLORE_VALUE_TEXT,
            i: 0,
            f: 0.0,
            s: SubloreStr::borrowed(text),
        },
        Cell::Blob(bytes) => SubloreValue {
            kind: SUBLORE_VALUE_BLOB,
            i: 0,
            f: 0.0,
            s: SubloreStr {
                ptr: bytes.as_ptr(),
                len: bytes.len(),
            },
        },
    }
}

/// The bits `for_each_line` has a meaning for.
const SUBLORE_ROLES: u32 = SUBLORE_ROLE_SOURCE | SUBLORE_ROLE_TARGET;

/// The interface's word for a role, or none for a role it has no bit for.
///
/// Media has none, and that is the interface saying what this walk is: lines of text. A media file
/// asked for by a mask that cannot name it is a file this never opens.
fn role_bit(role: FileRole) -> Option<u32> {
    match role {
        FileRole::Source => Some(SUBLORE_ROLE_SOURCE),
        FileRole::Target => Some(SUBLORE_ROLE_TARGET),
        FileRole::Media => None,
    }
}

/// One line on its way to a module. Borrowed out of `cue`, which the caller keeps alive for the
/// length of the push.
fn line_of(
    episode: &sublore_project::model::Episode,
    role: u32,
    index: usize,
    cue: &CueView,
) -> SubloreLine {
    SubloreLine {
        episode_id: episode.id,
        ordinal: episode.ordinal,
        role,
        flags: 0,
        index: index as u64,
        cue: SubloreCue {
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: SubloreStr::borrowed(&cue.text),
            is_comment: u8::from(cue.comment),
            has_number: u8::from(cue.number.is_some()),
            number: cue.number.unwrap_or(0),
        },
    }
}

/// The one push that stands in for a file that gave the walk nothing (§4.4).
fn missing_line(episode: &sublore_project::model::Episode, role: u32) -> SubloreLine {
    SubloreLine {
        episode_id: episode.id,
        ordinal: episode.ordinal,
        role,
        flags: SUBLORE_LINE_FLAG_FILE_MISSING,
        // Two independent tells at the point the module is already reading: the flag and a index
        // that is no index. A module that tests neither sees one empty line, which §4.4 names as
        // the cost of not adding a second sink every module would have to implement.
        index: SUBLORE_NO_CUE,
        cue: SubloreCue {
            start_ms: 0,
            end_ms: 0,
            text: SubloreStr::borrowed(""),
            is_comment: 0,
            has_number: 0,
            number: 0,
        },
    }
}

/// Every cue of every attached file whose role is in `roles`, in episode order (§4.4).
///
/// # Safety
/// `host` is the context pointer; `sink` and `on_line` are the module's own.
unsafe extern "C" fn host_for_each_line(
    host: *mut c_void,
    roles: u32,
    sink: *mut c_void,
    on_line: SubloreLineFn,
) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(name) = ctx.name() else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(on_line) = on_line else {
            return SUBLORE_ERR_UNSUPPORTED;
        };
        // An allowlist and not a mask, as `find`'s options are: a module asking for a role it is
        // not getting would build its memory out of a subset it believes is the whole.
        if roles & !SUBLORE_ROLES != 0 {
            return SUBLORE_ERR_UNSUPPORTED;
        }
        let Some(held) = ctx.project() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };
        // Safety: armed for this call on this thread, so the host holds the project lock for the
        // whole of it. Shared, and nothing here writes.
        let Some(project) = (unsafe { &*held }).as_ref() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };
        // The open document, when there is one. Read on the same terms and for the same reason.
        let open = ctx
            .session()
            .and_then(|held| unsafe { &*held }.as_ref())
            .filter(|session| session.path().is_some());

        let episodes = match records::episodes(project) {
            Ok(episodes) => episodes,
            Err(error) => {
                log::warn!(
                    "modules: {name} asked for every line and the episodes would not read: {error}"
                );
                return SUBLORE_ERR_STORAGE;
            }
        };

        for episode in &episodes {
            let files = match records::files(project, episode.id) {
                Ok(files) => files,
                Err(error) => {
                    log::warn!(
                        "modules: {name} asked for every line and episode {} would not read: {error}",
                        episode.id
                    );
                    return SUBLORE_ERR_STORAGE;
                }
            };
            for file in &files {
                let Some(bit) = role_bit(file.role) else {
                    continue;
                };
                if roles & bit == 0 {
                    continue;
                }

                // The open document wins over its own bytes on disk: a memory built from stale
                // bytes would disagree with what is on screen (§4.4).
                let held = open.filter(|session| session.path() == Some(file.path.as_path()));
                // Read into a binding that ends with this file, so the document is dropped before
                // the next one is opened and peak memory is one file (§4.4).
                let read = match held {
                    Some(_) => None,
                    None => match crate::subtitle::read_document(&file.path) {
                        Ok(document) => Some(sublore_edit::diff::views(&document)),
                        Err(error) => {
                            // Any file that yields no lines arrives as the one missing push, with
                            // the reason in the log: a walk that aborted for one moved file would
                            // fail for the whole series (§4.4).
                            log::info!(
                                "modules: {name} gets one missing line for {}: {}",
                                file.path.display(),
                                error.detail
                            );
                            let line = missing_line(episode, bit);
                            // Safety: the module's own function, given the sink it handed over,
                            // inside the call it is already in.
                            let answer = unsafe { on_line(sink, &line) };
                            if answer != SUBLORE_OK {
                                return answer;
                            }
                            continue;
                        }
                    },
                };
                let cues: &[CueView] = match (&read, held) {
                    (Some(views), _) => views,
                    (None, Some(session)) => session.views(),
                    // Unreachable: `read` is none only where `held` is some.
                    (None, None) => &[],
                };

                for (index, cue) in cues.iter().enumerate() {
                    let line = line_of(episode, bit, index, cue);
                    // Safety: as above. Every string points into `cues`, which outlives this push.
                    let answer = unsafe { on_line(sink, &line) };
                    if answer != SUBLORE_OK {
                        return answer;
                    }
                }
            }
        }
        SUBLORE_OK
    })
}

/// One statement of the module's own, bound and run on the host's connection (§4.7).
///
/// # Safety
/// `host` is the context pointer; `sql` and every string among `params` point at bytes valid for
/// this call; `sink` and `on_row` are the module's own.
unsafe extern "C" fn host_db_run(
    host: *mut c_void,
    sql: SubloreStr,
    params: *const SubloreValue,
    param_count: usize,
    sink: *mut c_void,
    on_row: SubloreRowFn,
) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(name) = ctx.name() else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Ok(sql) = (unsafe { sql.as_str() }) else {
            return SUBLORE_ERR_BAD_STRING;
        };
        let params = match unsafe { bound(params, param_count) } {
            Ok(params) => params,
            Err(code) => return code,
        };
        // What the module's own sink answered, which ends the walk the way `find`'s does.
        let mut answer = SUBLORE_OK;
        let mut push = |cells: &[Cell]| {
            let Some(on_row) = on_row else {
                return true;
            };
            let returned: Vec<SubloreValue> = cells.iter().map(returned).collect();
            // Safety: the module's own function, given the sink it handed over, inside the call it
            // is already in. Every string points into `cells`, which outlives this push.
            answer = unsafe { on_row(sink, returned.as_ptr(), returned.len()) };
            answer == SUBLORE_OK
        };

        let ran = if let Some(open) = ctx.open() {
            // Inside the module's own transaction: that call holds the only mutable borrow of the
            // project and already installed the guard for the whole of its body, so the statement
            // goes through the handle rather than borrowing the project a second time.
            //
            // Safety: the handle is read out of the record, which the transaction clears before it
            // returns, so it is only ever reached while that call is still on this stack.
            unsafe { open.run(sql, &params, &mut push) }
        } else {
            let Some(held) = ctx.project() else {
                return SUBLORE_ERR_NOTHING_OPEN;
            };
            // Safety: armed for this call on this thread, so the host holds the project lock for
            // the whole of it, and no transaction is open, so nothing else borrows it right now.
            let Some(project) = (unsafe { &*held }).as_ref() else {
                return SUBLORE_ERR_NOTHING_OPEN;
            };
            // No id is no storage, never storage under a name nobody checked: the guard holds a
            // module inside `m_<id>_*` and there is no prefix to hold this one inside.
            //
            // **After the project and not before it**, because a module told it was denied when
            // there is simply nothing open would read that as its own id being refused, which is
            // permanent, and stop asking. Nothing open is the condition that goes away.
            let Some(id) = ctx.storage() else {
                log::warn!(
                    "modules: {name} has no usable storage id, so its statement was refused"
                );
                return SUBLORE_ERR_DENIED;
            };
            module_store::run(project, &id, sql, &params, &mut push)
        };

        match ran {
            Ok(_) => answer,
            Err(refusal) => {
                if let StoreRefusal::Failed(detail) = &refusal {
                    log::warn!("modules: {name} ran a statement that failed: {detail}");
                }
                storage_code(&refusal)
            }
        }
    })
}

/// The module's own work, inside one IMMEDIATE transaction (§4.7).
///
/// # Safety
/// `host` is the context pointer, and `work` is the module's own function, called with `work_ctx`
/// unchanged.
unsafe extern "C" fn host_db_transaction(
    host: *mut c_void,
    work_ctx: *mut c_void,
    work: SubloreWorkFn,
) -> i32 {
    guarded(|| {
        let Some(ctx) = (unsafe { ctx_of(host) }) else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(name) = ctx.name() else {
            return SUBLORE_ERR_WRONG_THREAD;
        };
        let Some(work) = work else {
            log::warn!("modules: {name} asked for a transaction with no body");
            return SUBLORE_ERR_UNSUPPORTED;
        };
        // Refused rather than attempted. SQLite has savepoints and this interface does not expose
        // them, so a transaction inside a transaction would be one the host cannot account for.
        if ctx.open().is_some() {
            log::warn!("modules: {name} asked for a transaction inside one it already had open");
            return SUBLORE_ERR_DENIED;
        }
        let Some(held) = ctx.project() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };
        // Safety: armed for this call on this thread, so the host holds the project lock for the
        // whole of it, and no transaction is open, so this is the only borrow.
        let Some(project) = (unsafe { &mut *held }).as_mut() else {
            return SUBLORE_ERR_NOTHING_OPEN;
        };
        // After the project, for the reason `host_db_run` writes out.
        let Some(id) = ctx.storage() else {
            log::warn!("modules: {name} has no usable storage id, so its transaction was refused");
            return SUBLORE_ERR_DENIED;
        };

        // The module's own code, kept out here so a rollback can still answer with it rather than
        // with the host's word for "the body said no".
        let mut answered = SUBLORE_OK;
        let outcome = module_store::transaction(project, &id, |open| {
            ctx.set_open(Some(open));
            // Safety: the module's own function, given the context it handed over, inside the call
            // it is already in.
            answered = unsafe { work(work_ctx) };
            ctx.set_open(None);
            if answered == SUBLORE_OK {
                Ok(())
            } else {
                Err(StoreRefusal::Failed(format!(
                    "the module's own work reported {answered}"
                )))
            }
        });
        match outcome {
            Ok(()) => SUBLORE_OK,
            // Its own refusal, and the rollback already happened: it hears what it said, not what
            // the host called it.
            Err(_) if answered != SUBLORE_OK => answered,
            Err(refusal) => {
                if let StoreRefusal::Failed(detail) = &refusal {
                    log::warn!("modules: {name}'s transaction failed: {detail}");
                }
                storage_code(&refusal)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use sublore_module_api::{
        SUBLORE_CELL_BADGE, SUBLORE_CELL_TEXT, SUBLORE_ERR_CANCELLED, SUBLORE_FIND_MATCH_CASE,
        SUBLORE_FIND_SKIP_TAGS,
    };

    /// The module name every check below arms with.
    const NAME: &str = "fixture.so";

    /// The context as a module receives it. Named on both sides, so the two casts cannot disagree.
    fn pointer(ctx: &HostCtx) -> *mut c_void {
        (ctx as *const HostCtx).cast_mut().cast::<c_void>()
    }

    /// What one `find` call collected. A named type rather than a tuple: a void pointer is only as
    /// safe as the agreement between the two casts.
    #[derive(Default)]
    struct Collected {
        hits: Vec<(usize, usize)>,
        /// What the sink answers, so a check can stop the walk.
        answer: i32,
        /// How many pushes it took before it stopped.
        pushes: usize,
    }

    unsafe extern "C" fn collect(sink: *mut c_void, start: usize, len: usize) -> i32 {
        let collected = unsafe { &mut *sink.cast::<Collected>() };
        collected.hits.push((start, len));
        collected.pushes += 1;
        collected.answer
    }

    fn find_in(ctx: &HostCtx, haystack: &str, needle: &str, options: u32) -> (i32, Collected) {
        let mut collected = Collected::default();
        let code = unsafe {
            host_find(
                pointer(ctx),
                SubloreStr::borrowed(haystack),
                SubloreStr::borrowed(needle),
                options,
                (&mut collected as *mut Collected).cast(),
                Some(collect),
            )
        };
        (code, collected)
    }

    #[test]
    fn a_call_inside_the_one_the_host_made_reaches_its_body() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        let (code, collected) = find_in(&ctx, "By then the fog had eaten the boats.", "fog", 0);
        assert_eq!(code, SUBLORE_OK);
        assert_eq!(collected.hits, vec![(12, 3)]);
    }

    #[test]
    fn a_call_from_another_thread_is_refused_and_does_not_block() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        // The record is armed for this thread, and the call below is made on another one. It must
        // come back, and come back refused: a module that stashed the pointer gets a code.
        let elsewhere = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let (code, collected) = find_in(&ctx, "the fog", "fog", 0);
                    (code, collected.hits.len())
                })
                .join()
                .expect("the other thread must return rather than block")
        });
        assert_eq!(elsewhere, (SUBLORE_ERR_WRONG_THREAD, 0));
    }

    #[test]
    fn a_call_after_the_one_it_belonged_to_returned_is_refused() {
        let ctx = HostCtx::new();
        drop(ctx.enter(NAME, Lent::default()));
        let (code, collected) = find_in(&ctx, "the fog", "fog", 0);
        assert_eq!(code, SUBLORE_ERR_WRONG_THREAD);
        assert!(collected.hits.is_empty());
    }

    #[test]
    fn a_call_with_no_context_at_all_is_refused_rather_than_dereferenced() {
        let mut collected = Collected::default();
        let code = unsafe {
            host_find(
                std::ptr::null_mut(),
                SubloreStr::borrowed("the fog"),
                SubloreStr::borrowed("fog"),
                0,
                (&mut collected as *mut Collected).cast(),
                Some(collect),
            )
        };
        assert_eq!(code, SUBLORE_ERR_WRONG_THREAD);
    }

    #[test]
    fn a_body_that_panics_answers_with_a_code_and_the_process_survives() {
        // `guarded` directly, and not through a sink that panics, because a sink cannot: an
        // `extern "C"` function that unwinds aborts the process before any `catch_unwind` sees it,
        // measured on 2026-09-04. So the only panic this can ever catch is one raised by the host's
        // own body, which is what is raised here. Section 2.4's ceiling is the other half.
        static REACHED: AtomicUsize = AtomicUsize::new(0);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let code = guarded(|| {
            REACHED.fetch_add(1, Ordering::Relaxed);
            panic!("a host callback body that panics");
        });
        std::panic::set_hook(previous);
        assert_eq!(code, SUBLORE_ERR_PANIC);
        assert_eq!(REACHED.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn the_fold_is_the_matcher_s_own_and_the_case_option_turns_it_off() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        let (code, folded) = find_in(&ctx, "By then the FOG had eaten", "fog", 0);
        assert_eq!(code, SUBLORE_OK);
        assert_eq!(folded.hits, vec![(12, 3)]);

        let (code, exact) = find_in(
            &ctx,
            "By then the FOG had eaten",
            "fog",
            SUBLORE_FIND_MATCH_CASE,
        );
        assert_eq!(code, SUBLORE_OK);
        assert!(exact.hits.is_empty());
    }

    #[test]
    fn tags_are_skipped_only_when_asked_for_and_the_offsets_stay_the_raw_line_s() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        let line = "the {\\i1}fog";
        let (_, plain) = find_in(&ctx, line, "the fog", 0);
        assert!(
            plain.hits.is_empty(),
            "the block is text without the option"
        );

        let (code, skipped) = find_in(&ctx, line, "the fog", SUBLORE_FIND_SKIP_TAGS);
        assert_eq!(code, SUBLORE_OK);
        // Offsets into the raw line, block included, which is what a caller highlights or replaces.
        assert_eq!(skipped.hits, vec![(0, line.len())]);
    }

    #[test]
    fn a_sink_that_stops_the_walk_is_pushed_once_and_its_answer_comes_back() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        let mut collected = Collected {
            answer: SUBLORE_ERR_CANCELLED,
            ..Collected::default()
        };
        let code = unsafe {
            host_find(
                pointer(&ctx),
                SubloreStr::borrowed("fog and fog and fog"),
                SubloreStr::borrowed("fog"),
                0,
                (&mut collected as *mut Collected).cast(),
                Some(collect),
            )
        };
        assert_eq!(code, SUBLORE_ERR_CANCELLED);
        assert_eq!(collected.pushes, 1, "the walk stopped at the first hit");
    }

    #[test]
    fn an_option_bit_this_build_has_no_meaning_for_is_refused_rather_than_masked_off() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        // The next bit up. A mask would answer this as an ordinary folded search, and the module
        // would believe it had asked for something it did not get.
        let (code, collected) = find_in(&ctx, "the fog", "fog", 4);
        assert_eq!(code, SUBLORE_ERR_UNSUPPORTED);
        assert!(collected.hits.is_empty());
    }

    #[test]
    fn a_find_with_no_sink_function_is_refused_rather_than_jumped_through() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        let code = unsafe {
            host_find(
                pointer(&ctx),
                SubloreStr::borrowed("the fog"),
                SubloreStr::borrowed("fog"),
                0,
                std::ptr::null_mut(),
                None,
            )
        };
        assert_eq!(code, SUBLORE_ERR_UNSUPPORTED);
    }

    #[test]
    fn a_haystack_that_is_not_text_is_refused() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        let invalid = [0xffu8, 0xfe];
        let mut collected = Collected::default();
        let code = unsafe {
            host_find(
                pointer(&ctx),
                SubloreStr {
                    ptr: invalid.as_ptr(),
                    len: invalid.len(),
                },
                SubloreStr::borrowed("fog"),
                0,
                (&mut collected as *mut Collected).cast(),
                Some(collect),
            )
        };
        assert_eq!(code, SUBLORE_ERR_BAD_STRING);
        assert!(collected.hits.is_empty());
    }

    /// A CRLF document with an ASS `Comment:` event in it, which is the pair of things the two
    /// reads have to get right: the count includes the comment, and the text arrives with `\n`.
    const ASS: &str = "[Script Info]\r\nScriptType: v4.00+\r\n\r\n[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,By then the fog\\Nhad eaten the boats.\r\nComment: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,a note to the translator\r\nDialogue: 0,0:00:06.00,0:00:08.00,Default,,0,0,0,,And the boats were gone.\r\n";

    /// A CRLF SRT with a two-line cue and its own numbers, which is where the newline rule is
    /// visible: an ASS event is one line in the file, so no CRLF ever lands inside its text.
    const SRT: &str =
        "7\r\n00:00:01,000 --> 00:00:03,000\r\nBy then the fog\r\nhad eaten the boats.\r\n\r\n";

    fn opened() -> Option<EditSession> {
        let document = sublore_formats::parse(SubtitleFormat::Ass, ASS.as_bytes())
            .expect("the fixture should parse");
        Some(EditSession::open(
            std::path::PathBuf::from("/tmp/sublore-host-check/episode.ass"),
            document,
        ))
    }

    fn document_of(ctx: &HostCtx) -> (i32, SubloreDocument) {
        let mut out = SubloreDocument {
            format: 0,
            cue_count: 0,
            revision: 0,
            dirty: 0,
            path: SubloreStr::borrowed(""),
        };
        let code = unsafe { host_document(pointer(ctx), &mut out) };
        (code, out)
    }

    fn cue_of(ctx: &HostCtx, index: u64) -> (i32, SubloreCue) {
        let mut out = SubloreCue {
            start_ms: 0,
            end_ms: 0,
            text: SubloreStr::borrowed(""),
            is_comment: 0,
            has_number: 0,
            number: 0,
        };
        let code = unsafe { host_cue_at(pointer(ctx), index, &mut out) };
        (code, out)
    }

    #[test]
    fn the_document_answers_with_the_count_that_includes_an_ass_comment_event() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));

        let (code, answer) = document_of(&ctx);
        assert_eq!(code, SUBLORE_OK);
        assert_eq!(answer.format, SUBLORE_FORMAT_ASS);
        // Three, not two. `displayed_cue_count` would say two, and an index space that skipped the
        // comment would not be the index space an edit takes.
        assert_eq!(answer.cue_count, 3);
        assert_eq!(answer.dirty, 0);
        assert_eq!(
            unsafe { answer.path.as_str() },
            Ok("/tmp/sublore-host-check/episode.ass")
        );
    }

    #[test]
    fn a_document_that_is_not_open_is_said_rather_than_guessed_at() {
        let ctx = HostCtx::new();
        let mut none: Option<EditSession> = None;
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut none)));
        let (code, answer) = document_of(&ctx);
        assert_eq!(code, SUBLORE_ERR_NOTHING_OPEN);
        // The out parameter is untouched, so a module that ignores the code reads its own zeroes
        // rather than a document that is not there.
        assert_eq!(answer.cue_count, 0);
        assert_eq!(answer.format, 0);
    }

    #[test]
    fn a_call_the_host_lent_no_session_reads_nothing() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        assert_eq!(document_of(&ctx).0, SUBLORE_ERR_NOTHING_OPEN);
        assert_eq!(cue_of(&ctx, 0).0, SUBLORE_ERR_NOTHING_OPEN);
    }

    #[test]
    fn a_crlf_file_hands_over_the_form_an_edit_has_to_be_proposed_in() {
        let document =
            sublore_formats::parse(SubtitleFormat::Srt, SRT.as_bytes()).expect("srt should parse");
        let mut session = Some(EditSession::untitled(document));
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));

        let (code, first) = cue_of(&ctx, 0);
        assert_eq!(code, SUBLORE_OK);
        assert_eq!(first.start_ms, 1000);
        assert_eq!(first.end_ms, 3000);
        let text = unsafe { first.text.as_str() }.expect("the text should be valid");
        assert_eq!(text, "By then the fog\nhad eaten the boats.");
        assert!(
            !text.contains('\r'),
            "a CRLF file handed a module the file's own form, and an edit proposed back in it \
             would carry carriage returns into `plan_set_text`"
        );
        // The file wrote a number and it is not the index: an SRT index line is never renumbered.
        assert_eq!(first.has_number, 1);
        assert_eq!(first.number, 7);

        // An untitled document has never had a file, and §4.3 says that is an empty path rather
        // than an absent one.
        let (code, answer) = document_of(&ctx);
        assert_eq!(code, SUBLORE_OK);
        assert_eq!(answer.format, SUBLORE_FORMAT_SRT);
        assert_eq!(unsafe { answer.path.as_str() }, Ok(""));
    }

    #[test]
    fn an_ass_line_break_stays_the_two_characters_the_file_wrote() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));

        let (code, first) = cue_of(&ctx, 0);
        assert_eq!(code, SUBLORE_OK);
        assert_eq!(first.is_comment, 0);
        // Predicted `\n` and measured `\N`, on 2026-09-04. Normalization collapses `\r\n` and
        // nothing else, so an ASS line break crosses raw like every other override, and a module
        // matching text has to expect it. `has_number` is zero because ASS writes none.
        let text = unsafe { first.text.as_str() }.expect("the text should be valid");
        assert_eq!(text, "By then the fog\\Nhad eaten the boats.");
        assert_eq!(first.has_number, 0);

        // The comment event is reachable at the index the count promised.
        let (code, second) = cue_of(&ctx, 1);
        assert_eq!(code, SUBLORE_OK);
        assert_eq!(second.is_comment, 1);
    }

    #[test]
    fn one_index_past_the_last_cue_is_refused_rather_than_wrapped() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));

        assert_eq!(cue_of(&ctx, 2).0, SUBLORE_OK);
        assert_eq!(cue_of(&ctx, 3).0, SUBLORE_ERR_NO_SUCH_CUE);
        // The value a 32-bit build would wrap into a valid index.
        assert_eq!(cue_of(&ctx, u64::MAX).0, SUBLORE_ERR_NO_SUCH_CUE);
    }

    #[test]
    fn a_read_with_nowhere_to_write_is_refused_rather_than_written_through_null() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));
        assert_eq!(
            unsafe { host_document(pointer(&ctx), std::ptr::null_mut()) },
            SUBLORE_ERR_BAD_STRING
        );
        assert_eq!(
            unsafe { host_cue_at(pointer(&ctx), 0, std::ptr::null_mut()) },
            SUBLORE_ERR_BAD_STRING
        );
    }

    #[test]
    fn a_read_from_another_thread_is_refused_even_with_a_session_lent() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));
        let elsewhere = std::thread::scope(|scope| {
            scope
                .spawn(|| (document_of(&ctx).0, cue_of(&ctx, 0).0))
                .join()
                .expect("the other thread must return rather than block")
        });
        assert_eq!(
            elsewhere,
            (SUBLORE_ERR_WRONG_THREAD, SUBLORE_ERR_WRONG_THREAD)
        );
    }

    fn propose(ctx: &HostCtx, revision: u64, cue: u64, text: &str) -> i32 {
        let asked = SubloreProposal {
            kind: SUBLORE_PROPOSAL_SET_CUE_TEXT,
            revision,
            cue,
            text: SubloreStr::borrowed(text),
        };
        unsafe { host_propose(pointer(ctx), &asked) }
    }

    /// The text of a cue, read the way a module reads it.
    ///
    /// Through the host and not out of the session, because `Entered` holds the session's mutable
    /// borrow for its own lifetime: nothing can look behind the gate while a call is armed, a check
    /// included. That is the invariant working rather than an inconvenience, and reading through
    /// `cue_at` is what a module would see anyway.
    fn text_of(ctx: &HostCtx, index: u64) -> String {
        let (code, cue) = cue_of(ctx, index);
        assert_eq!(code, SUBLORE_OK);
        unsafe { cue.text.as_str() }
            .expect("the text should be valid")
            .to_owned()
    }

    fn revision_of(ctx: &HostCtx) -> u64 {
        let (code, document) = document_of(ctx);
        assert_eq!(code, SUBLORE_OK);
        document.revision
    }

    fn dirty(ctx: &HostCtx) -> bool {
        let (code, document) = document_of(ctx);
        assert_eq!(code, SUBLORE_OK);
        document.dirty == 1
    }

    #[test]
    fn a_proposal_changes_the_cue_and_the_window_is_handed_the_patch() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));

        let before = revision_of(&ctx);
        assert!(!dirty(&ctx));
        assert_eq!(
            propose(&ctx, before, 0, "The fog had eaten nothing."),
            SUBLORE_OK
        );

        assert_eq!(text_of(&ctx, 0), "The fog had eaten nothing.");
        assert!(dirty(&ctx));
        assert_ne!(
            revision_of(&ctx),
            before,
            "the revision moved with the edit"
        );

        // One patch, naming the row that changed, so the grid splices exactly that row.
        let patches = entered.proposed();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].from, 0);
        assert_eq!(patches[0].revision, revision_of(&ctx));
        assert!(
            patches[0].can_undo,
            "it went through the history like any other edit"
        );
        // And taken means taken: a second call collects only what it changed itself.
        assert!(entered.proposed().is_empty());
    }

    #[test]
    fn one_undo_puts_a_proposed_edit_back() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let was = {
            let entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));
            let was = text_of(&ctx, 0);
            let revision = revision_of(&ctx);
            assert_eq!(
                propose(&ctx, revision, 0, "Something else entirely."),
                SUBLORE_OK
            );
            assert_eq!(entered.proposed().len(), 1);
            was
        };
        // The guard is gone, so the session can be reached directly again, which is what the app
        // does between module calls.
        let session = session.as_mut().expect("the document is open");
        session
            .undo()
            .expect("one undo")
            .expect("something to undo");
        assert_eq!(session.views()[0].text, was);
        assert!(!session.dirty(), "and the document is clean again");
    }

    #[test]
    fn a_stale_revision_changes_nothing_and_is_said_rather_than_applied() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));
        let was = text_of(&ctx, 0);

        let behind = revision_of(&ctx).wrapping_sub(1);
        assert_eq!(
            propose(&ctx, behind, 0, "read from a list that has moved"),
            SUBLORE_ERR_STALE_REVISION
        );
        assert_eq!(text_of(&ctx, 0), was);
        assert!(!dirty(&ctx));
    }

    #[test]
    fn a_kind_this_build_has_no_meaning_for_is_refused_and_changes_nothing() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));
        let was = text_of(&ctx, 0);

        // The next value up. Section 4.5 keeps this field open for a segment the core cannot
        // address yet, and every value but the one is refused until it can.
        let asked = SubloreProposal {
            kind: SUBLORE_PROPOSAL_SET_CUE_TEXT + 1,
            revision: revision_of(&ctx),
            cue: 0,
            text: SubloreStr::borrowed("never applied"),
        };
        assert_eq!(
            unsafe { host_propose(pointer(&ctx), &asked) },
            SUBLORE_ERR_UNSUPPORTED
        );
        assert_eq!(text_of(&ctx, 0), was);
        assert!(!dirty(&ctx));
    }

    #[test]
    fn a_cue_past_the_end_is_refused_rather_than_appended() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));
        let revision = revision_of(&ctx);
        assert_eq!(
            propose(&ctx, revision, 3, "nowhere"),
            SUBLORE_ERR_NO_SUCH_CUE
        );
        assert_eq!(
            propose(&ctx, revision, u64::MAX, "nowhere"),
            SUBLORE_ERR_NO_SUCH_CUE
        );
        assert!(!dirty(&ctx));
    }

    #[test]
    fn text_this_format_cannot_write_is_refused_by_the_guard_the_commands_run() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));
        // An ASS event is one line in the file, so a line break inside its text has nowhere to go.
        // The refusal is the planner's own, reached through the same guard `apply_edit` runs.
        let revision = revision_of(&ctx);
        assert_eq!(
            propose(&ctx, revision, 0, "one line\nand a second"),
            SUBLORE_ERR_UNWRITABLE_TEXT
        );
        assert!(!dirty(&ctx));
    }

    #[test]
    fn a_proposal_with_no_document_and_one_with_no_proposal_are_both_refused() {
        let ctx = HostCtx::new();
        let mut none: Option<EditSession> = None;
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut none)));
        assert_eq!(
            propose(&ctx, 0, 0, "nothing to change"),
            SUBLORE_ERR_NOTHING_OPEN
        );
        assert_eq!(
            unsafe { host_propose(pointer(&ctx), std::ptr::null()) },
            SUBLORE_ERR_BAD_STRING
        );
    }

    #[test]
    fn a_proposal_from_another_thread_never_reaches_the_document() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Lent::default().with_session(Some(&mut session)));
        let elsewhere = std::thread::scope(|scope| {
            scope
                .spawn(|| propose(&ctx, 0, 0, "from a thread of its own"))
                .join()
                .expect("the other thread must return rather than block")
        });
        assert_eq!(elsewhere, SUBLORE_ERR_WRONG_THREAD);
        assert!(!dirty(&ctx));
    }

    #[test]
    fn a_log_line_is_cut_to_the_cap_on_a_character_boundary() {
        // The character is three bytes, so the cap falls inside one and the cut has to move back.
        let text = "\u{fffd}".repeat(MAX_LOG_BYTES);
        let cut = capped(&text);
        assert!(cut.len() <= MAX_LOG_BYTES);
        assert!(
            MAX_LOG_BYTES - cut.len() < 3,
            "the cut moved back further than one character"
        );
        assert_eq!(capped("short"), "short");
    }

    // -----------------------------------------------------------------------------------------
    // Storage, H6. Every check below runs against a real project, because the tables a module must
    // not reach have to exist for a refusal to mean anything.

    /// The id the checks below arm with. A name the storage accepts, so a refusal is the guard's
    /// and never the id's.
    const STORAGE: &str = "fixture";

    /// A directory of this check's own, and a project inside it.
    fn project(
        tag: &str,
    ) -> (
        std::path::PathBuf,
        Option<sublore_project::records::Project>,
    ) {
        use std::sync::atomic::AtomicU32;
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "sublore-host-store-{tag}-{}-{nanos}-{unique}",
            std::process::id()
        ));
        let folder = dir.join("Series");
        std::fs::create_dir_all(&folder).expect("the project folder should be creatable");
        let project = sublore_project::records::Project::create(
            &folder,
            "A series",
            std::time::SystemTime::now(),
        )
        .expect("a project should be made");
        (dir, Some(project))
    }

    /// What one statement pushed back. A named type rather than a tuple, on the same terms as
    /// `Collected`: a void pointer is only as safe as the agreement between the two casts.
    #[derive(Default)]
    struct Rows {
        rows: Vec<Vec<Cell>>,
        /// What the sink answers, so a check can stop the walk.
        answer: i32,
        pushes: usize,
    }

    unsafe extern "C" fn collect_row(
        sink: *mut c_void,
        cells: *const SubloreValue,
        count: usize,
    ) -> i32 {
        let collected = unsafe { &mut *sink.cast::<Rows>() };
        let read = unsafe { bound(cells, count) }.expect("the host wrote its own cells");
        collected.rows.push(read);
        collected.pushes += 1;
        collected.answer
    }

    fn run_sql(ctx: &HostCtx, sql: &str, params: &[Cell], rows: &mut Rows) -> i32 {
        let given: Vec<SubloreValue> = params.iter().map(returned).collect();
        unsafe {
            host_db_run(
                pointer(ctx),
                SubloreStr::borrowed(sql),
                if given.is_empty() {
                    std::ptr::null()
                } else {
                    given.as_ptr()
                },
                given.len(),
                (rows as *mut Rows).cast(),
                Some(collect_row),
            )
        }
    }

    /// The same, for a statement whose rows nobody wants.
    fn run_quiet(ctx: &HostCtx, sql: &str) -> i32 {
        let mut rows = Rows::default();
        run_sql(ctx, sql, &[], &mut rows)
    }

    /// What a module's transaction body is given. Named on both sides, for the same reason as the
    /// sinks above.
    struct Work {
        host: *mut c_void,
        /// What the body does once, and what it answered.
        what: fn(*mut c_void) -> i32,
        inner: i32,
        /// What the body itself reports, which is what decides commit or rollback.
        answer: i32,
    }

    unsafe extern "C" fn run_work(work_ctx: *mut c_void) -> i32 {
        let work = unsafe { &mut *work_ctx.cast::<Work>() };
        work.inner = (work.what)(work.host);
        work.answer
    }

    unsafe extern "C" fn empty_body(_: *mut c_void) -> i32 {
        SUBLORE_OK
    }

    fn transact(ctx: &HostCtx, answer: i32, what: fn(*mut c_void) -> i32) -> (i32, i32) {
        let mut work = Work {
            host: pointer(ctx),
            what,
            inner: SUBLORE_OK,
            answer,
        };
        let code = unsafe {
            host_db_transaction(
                pointer(ctx),
                (&mut work as *mut Work).cast(),
                Some(run_work),
            )
        };
        (code, work.inner)
    }

    #[test]
    fn a_module_reads_back_through_the_host_what_it_wrote_through_it() {
        let (dir, mut open) = project("roundtrip");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );

        assert_eq!(
            run_quiet(
                &ctx,
                "CREATE TABLE m_fixture_notes (n INTEGER, r REAL, t TEXT, b BLOB, z INTEGER)"
            ),
            SUBLORE_OK
        );
        let written = [
            Cell::Int(7),
            Cell::Real(1.5),
            Cell::Text("Hütte".into()),
            Cell::Blob(vec![0, 255, 10]),
            Cell::Null,
        ];
        let mut rows = Rows::default();
        assert_eq!(
            run_sql(
                &ctx,
                "INSERT INTO m_fixture_notes VALUES (?1, ?2, ?3, ?4, ?5)",
                &written,
                &mut rows
            ),
            SUBLORE_OK
        );

        let mut rows = Rows::default();
        assert_eq!(
            run_sql(
                &ctx,
                "SELECT n, r, t, b, z FROM m_fixture_notes",
                &[],
                &mut rows
            ),
            SUBLORE_OK
        );
        // Every one of the five kinds, out the way it went in: a blob with a NUL in it included,
        // which is what says the boundary carries bytes and not a C string.
        assert_eq!(rows.rows, vec![written.to_vec()]);

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_statement_with_no_project_lent_says_nothing_is_open() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        assert_eq!(run_quiet(&ctx, "SELECT 1"), SUBLORE_ERR_NOTHING_OPEN);
    }

    #[test]
    fn a_module_whose_file_name_yields_no_id_gets_no_storage() {
        let (dir, mut open) = project("noid");
        let ctx = HostCtx::new();
        // The project is lent and the id is not, which is what a file named `sublore_module_Foo`
        // produces. It must cost that module its storage, never give it somebody else's.
        let _entered = ctx.enter(NAME, Lent::default().with_project(&mut open, None));
        assert_eq!(
            run_quiet(&ctx, "CREATE TABLE m_fixture_notes (x INTEGER)"),
            SUBLORE_ERR_DENIED
        );

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_tables_the_core_owns_are_refused_through_the_host_too() {
        let (dir, mut open) = project("guarded");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );
        assert_eq!(
            run_quiet(&ctx, "SELECT id, title FROM episodes"),
            SUBLORE_ERR_DENIED
        );
        assert_eq!(
            run_quiet(&ctx, "PRAGMA user_version = 99"),
            SUBLORE_ERR_DENIED
        );
        // Two statements are one refusal and neither runs, so nothing is smuggled behind the
        // semicolon.
        assert_eq!(
            run_quiet(
                &ctx,
                "CREATE TABLE m_fixture_a (x INTEGER); CREATE TABLE m_fixture_b (x INTEGER)"
            ),
            SUBLORE_ERR_DENIED
        );
        assert_eq!(
            run_quiet(&ctx, "SELECT count(*) FROM m_fixture_a"),
            SUBLORE_ERR_STORAGE,
            "the first of the two ran"
        );

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_sink_that_stops_the_walk_is_answered_with_its_own_code() {
        let (dir, mut open) = project("stop");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );
        run_quiet(&ctx, "CREATE TABLE m_fixture_notes (id INTEGER)");
        for id in 1..=3 {
            run_sql(
                &ctx,
                "INSERT INTO m_fixture_notes (id) VALUES (?1)",
                &[Cell::Int(id)],
                &mut Rows::default(),
            );
        }

        let mut rows = Rows {
            answer: SUBLORE_ERR_CANCELLED,
            ..Rows::default()
        };
        let code = run_sql(
            &ctx,
            "SELECT id FROM m_fixture_notes ORDER BY id",
            &[],
            &mut rows,
        );
        assert_eq!(code, SUBLORE_ERR_CANCELLED);
        assert_eq!(rows.pushes, 1, "a module that wants one row pays for one");

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_transaction_the_module_refuses_rolls_back_and_answers_its_own_code() {
        let (dir, mut open) = project("rollback");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );
        run_quiet(&ctx, "CREATE TABLE m_fixture_notes (id INTEGER)");

        let (code, inner) = transact(&ctx, SUBLORE_ERR_CANCELLED, |host| unsafe {
            host_db_run(
                host,
                SubloreStr::borrowed("INSERT INTO m_fixture_notes (id) VALUES (7)"),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                None,
            )
        });
        assert_eq!(inner, SUBLORE_OK, "the row went in inside the transaction");
        // Its own word for why, not the host's: a module that cancelled hears that it cancelled.
        assert_eq!(code, SUBLORE_ERR_CANCELLED);

        let mut rows = Rows::default();
        assert_eq!(
            run_sql(&ctx, "SELECT id FROM m_fixture_notes", &[], &mut rows),
            SUBLORE_OK
        );
        assert!(rows.rows.is_empty(), "the rolled back row is still there");

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_transaction_that_succeeds_keeps_what_it_wrote_and_leaves_none_open() {
        let (dir, mut open) = project("commit");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );
        run_quiet(&ctx, "CREATE TABLE m_fixture_notes (id INTEGER)");

        let (code, inner) = transact(&ctx, SUBLORE_OK, |host| unsafe {
            host_db_run(
                host,
                SubloreStr::borrowed("INSERT INTO m_fixture_notes (id) VALUES (7)"),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                None,
            )
        });
        assert_eq!((code, inner), (SUBLORE_OK, SUBLORE_OK));

        let mut rows = Rows::default();
        run_sql(&ctx, "SELECT id FROM m_fixture_notes", &[], &mut rows);
        assert_eq!(rows.rows, vec![vec![Cell::Int(7)]]);
        // And the record is clean afterwards, which a handle left behind would not be: the next
        // statement would run on a connection this call has already given back.
        assert!(ctx.open().is_none());

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_transaction_inside_a_transaction_is_refused_and_the_outer_one_still_commits() {
        let (dir, mut open) = project("nested");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );
        run_quiet(&ctx, "CREATE TABLE m_fixture_notes (id INTEGER)");

        let (code, inner) = transact(&ctx, SUBLORE_OK, |host| unsafe {
            let refused = host_db_transaction(host, std::ptr::null_mut(), Some(empty_body));
            host_db_run(
                host,
                SubloreStr::borrowed("INSERT INTO m_fixture_notes (id) VALUES (7)"),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                None,
            );
            refused
        });
        assert_eq!(
            inner, SUBLORE_ERR_DENIED,
            "the inner transaction was opened"
        );
        assert_eq!(code, SUBLORE_OK);

        // The refusal cost the outer transaction nothing: what it wrote after being told no is
        // still there.
        let mut rows = Rows::default();
        run_sql(&ctx, "SELECT id FROM m_fixture_notes", &[], &mut rows);
        assert_eq!(rows.rows, vec![vec![Cell::Int(7)]]);

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn every_statement_of_a_transaction_is_held_inside_the_module_s_own_tables() {
        let (dir, mut open) = project("txnguard");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );

        let (_, inner) = transact(&ctx, SUBLORE_OK, |host| unsafe {
            host_db_run(
                host,
                SubloreStr::borrowed("CREATE TABLE m_fixture_notes (id INTEGER)"),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                None,
            );
            // The second statement is the one that matters: the guard installed for the body has
            // to still be on when it runs.
            host_db_run(
                host,
                SubloreStr::borrowed("SELECT id FROM episodes"),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                None,
            )
        });
        assert_eq!(inner, SUBLORE_ERR_DENIED);

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_statement_from_another_thread_is_refused_and_does_not_block() {
        let (dir, mut open) = project("elsewhere");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );
        let refused = std::thread::scope(|scope| {
            scope
                .spawn(|| run_quiet(&ctx, "SELECT 1"))
                .join()
                .expect("the other thread must return rather than block")
        });
        assert_eq!(refused, SUBLORE_ERR_WRONG_THREAD);

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    // -----------------------------------------------------------------------------------------
    // Every line of every episode, H7.

    /// An SRT with `count` cues, written where the test puts it.
    fn srt(dir: &std::path::Path, name: &str, count: u32) -> std::path::PathBuf {
        let mut text = String::new();
        for n in 1..=count {
            let start = n * 1000;
            text.push_str(&format!(
                "{n}\n00:00:0{}, 000 --> 00:00:0{}, 000\n{name} line {n}\n\n",
                start / 1000,
                (start / 1000) + 1
            ));
        }
        // The times above are written with the space a parser will not take, so they are fixed here
        // rather than fought with in a format string.
        let text = text.replace(", 000", ",000");
        let path = dir.join(name);
        std::fs::write(&path, text).expect("the fixture should be writable");
        path
    }

    /// What the walk pushed. Named on both sides, like every other sink here.
    #[derive(Default)]
    struct Lines {
        seen: Vec<(i64, u32, u32, u32, u64, String)>,
        answer: i32,
        pushes: usize,
    }

    unsafe extern "C" fn collect_line(sink: *mut c_void, line: *const SubloreLine) -> i32 {
        let lines = unsafe { &mut *sink.cast::<Lines>() };
        let line = unsafe { &*line };
        let text = unsafe { line.cue.text.as_str() }
            .unwrap_or("<not text>")
            .to_owned();
        lines.seen.push((
            line.episode_id,
            line.ordinal,
            line.role,
            line.flags,
            line.index,
            text,
        ));
        lines.pushes += 1;
        lines.answer
    }

    fn walk(ctx: &HostCtx, roles: u32, lines: &mut Lines) -> i32 {
        unsafe {
            host_for_each_line(
                pointer(ctx),
                roles,
                (lines as *mut Lines).cast(),
                Some(collect_line),
            )
        }
    }

    /// A project with two episodes, each with a source and a target, in ordinal order.
    fn series(
        tag: &str,
    ) -> (
        std::path::PathBuf,
        Option<sublore_project::records::Project>,
    ) {
        let (dir, mut open) = project(tag);
        let project = open.as_mut().expect("the project was just made");
        for (ordinal, title) in [(1u32, "One"), (2, "Two")] {
            // The ordinal is the database's own, one past the highest, so the two added here come
            // back as 1 and 2 and the walk's order is the series order.
            let episode =
                sublore_project::records::add_episode(project, title, std::time::SystemTime::now())
                    .expect("an episode should be addable");
            assert_eq!(episode.ordinal, ordinal);
            for (role, name) in [
                (FileRole::Source, format!("e{ordinal}-source.srt")),
                (FileRole::Target, format!("e{ordinal}-target.srt")),
            ] {
                let path = srt(&dir, &name, 2);
                sublore_project::records::attach_file(
                    project,
                    episode.id,
                    role,
                    &path,
                    std::time::SystemTime::now(),
                )
                .expect("a file should be attachable");
            }
        }
        (dir, open)
    }

    #[test]
    fn every_cue_of_every_attached_file_arrives_in_episode_order() {
        let (dir, mut open) = series("walk");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );

        let mut lines = Lines::default();
        assert_eq!(walk(&ctx, SUBLORE_ROLES, &mut lines), SUBLORE_OK);
        // Two episodes, two files each, two cues each. Ordinals never go backwards, and inside one
        // episode the source's cues all arrive before the target's.
        assert_eq!(lines.pushes, 8);
        let order: Vec<(u32, u32, u64)> = lines
            .seen
            .iter()
            .map(|(_, ordinal, role, _, index, _)| (*ordinal, *role, *index))
            .collect();
        assert_eq!(
            order,
            vec![
                (1, SUBLORE_ROLE_SOURCE, 0),
                (1, SUBLORE_ROLE_SOURCE, 1),
                (1, SUBLORE_ROLE_TARGET, 0),
                (1, SUBLORE_ROLE_TARGET, 1),
                (2, SUBLORE_ROLE_SOURCE, 0),
                (2, SUBLORE_ROLE_SOURCE, 1),
                (2, SUBLORE_ROLE_TARGET, 0),
                (2, SUBLORE_ROLE_TARGET, 1),
            ]
        );
        // And the text is the file's own, so the walk read the file rather than counting rows.
        assert_eq!(lines.seen[0].5, "e1-source.srt line 1");
        assert!(lines.seen.iter().all(|line| line.3 == 0));

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_mask_takes_only_the_roles_it_names_and_refuses_a_bit_it_does_not() {
        let (dir, mut open) = series("mask");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );

        let mut lines = Lines::default();
        assert_eq!(walk(&ctx, SUBLORE_ROLE_TARGET, &mut lines), SUBLORE_OK);
        assert_eq!(lines.pushes, 4);
        assert!(lines.seen.iter().all(|line| line.2 == SUBLORE_ROLE_TARGET));

        // A bit this build has no meaning for is refused rather than masked off: a module asking
        // for a role it is not getting would take a subset for the whole.
        let mut ignored = Lines::default();
        assert_eq!(
            walk(&ctx, SUBLORE_ROLES | 4, &mut ignored),
            SUBLORE_ERR_UNSUPPORTED
        );
        assert_eq!(ignored.pushes, 0);

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_gone_is_one_push_in_its_own_place_and_the_walk_finishes() {
        let (dir, mut open) = series("gone");
        // Episode one's target, removed between attaching and reading, which is what a user moving
        // a file does.
        std::fs::remove_file(dir.join("e1-target.srt")).expect("the fixture was there");

        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );
        let mut lines = Lines::default();
        assert_eq!(walk(&ctx, SUBLORE_ROLES, &mut lines), SUBLORE_OK);

        // Seven: the two cues that file would have pushed are one push instead, and everything
        // after it still arrives.
        assert_eq!(lines.pushes, 7);
        let missing: Vec<usize> = lines
            .seen
            .iter()
            .enumerate()
            .filter(|(_, line)| line.3 == SUBLORE_LINE_FLAG_FILE_MISSING)
            .map(|(at, _)| at)
            .collect();
        assert_eq!(
            missing,
            vec![2],
            "it did not arrive where its lines would have"
        );
        let (episode, ordinal, role, _, index, text) = &lines.seen[2];
        assert_eq!(
            (*ordinal, *role, *index),
            (1, SUBLORE_ROLE_TARGET, SUBLORE_NO_CUE)
        );
        assert_eq!(text, "", "the cue beside the flag is empty");
        assert!(*episode > 0);

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_open_document_wins_over_its_own_bytes_on_disk() {
        let (dir, mut open) = series("open");
        let path = dir.join("e1-source.srt");
        // The session holds an edit nobody has saved, which is the whole point: a memory built from
        // the file would disagree with what is on screen.
        let document = crate::subtitle::read_document(&path).expect("the fixture parses");
        let mut session = Some(EditSession::open(path.clone(), document));
        session
            .as_mut()
            .expect("just made")
            .apply(
                &Edit::SetText {
                    cue: 0,
                    text: "unsaved".into(),
                },
                Run::New,
                std::time::Instant::now(),
            )
            .expect("the edit applies");

        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default()
                .with_session(Some(&mut session))
                .with_project(&mut open, Some(STORAGE.to_owned())),
        );
        let mut lines = Lines::default();
        assert_eq!(walk(&ctx, SUBLORE_ROLES, &mut lines), SUBLORE_OK);
        assert_eq!(lines.pushes, 8, "the open file was streamed, not skipped");
        assert_eq!(lines.seen[0].5, "unsaved");
        // And only that file: the other three are still read off disk.
        assert_eq!(lines.seen[2].5, "e1-target.srt line 1");

        drop(_entered);
        drop(session);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_sink_that_stops_the_line_walk_is_answered_with_its_own_code() {
        let (dir, mut open) = series("stopwalk");
        let ctx = HostCtx::new();
        let _entered = ctx.enter(
            NAME,
            Lent::default().with_project(&mut open, Some(STORAGE.to_owned())),
        );
        let mut lines = Lines {
            answer: SUBLORE_ERR_CANCELLED,
            ..Lines::default()
        };
        assert_eq!(walk(&ctx, SUBLORE_ROLES, &mut lines), SUBLORE_ERR_CANCELLED);
        assert_eq!(lines.pushes, 1, "a module that wants one line pays for one");

        drop(_entered);
        drop(open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_walk_with_no_project_lent_says_nothing_is_open() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Lent::default());
        let mut lines = Lines::default();
        assert_eq!(
            walk(&ctx, SUBLORE_ROLES, &mut lines),
            SUBLORE_ERR_NOTHING_OPEN
        );
        assert_eq!(lines.pushes, 0);
    }
    // -----------------------------------------------------------------------------------------
    // The panels, the two diagnostics and the cancel flag, H8. None of these needs a window: the
    // activation carries a collector instead of one, for the reason written on `Window`.

    /// The panel the checks below are allowed to fill, and one they are not.
    const PANEL: u32 = 7;
    const NOT_OURS: u32 = 8;

    /// What one activation collected, and the flag a Stop would set.
    struct Watched {
        said: Arc<Mutex<Vec<Said>>>,
        cancel: Arc<AtomicBool>,
    }

    impl Watched {
        fn new() -> Self {
            Self {
                said: Arc::new(Mutex::new(Vec::new())),
                cancel: Arc::new(AtomicBool::new(false)),
            }
        }

        /// One activation of module 3, allowed to fill [`PANEL`] and nothing else.
        fn activation(&self) -> Activation {
            Activation {
                window: Window::Collected(Arc::clone(&self.said)),
                call: 42,
                module: 3,
                cancel: Arc::clone(&self.cancel),
                panels: vec![PANEL],
            }
        }

        fn said(&self) -> Vec<Said> {
            self.said
                .lock()
                .expect("the collector is not poisoned")
                .clone()
        }
    }

    /// One cell, with the row's handle on it.
    fn cell(kind: u32, text: &str, number: i64, handle: u64) -> SubloreCell {
        SubloreCell {
            kind,
            text: SubloreStr::borrowed(text),
            number,
            r#ref: handle,
        }
    }

    /// Push one row of two text cells carrying `handle`.
    fn push_row(ctx: &HostCtx, handle: u64, first: &str, second: &str) -> i32 {
        let cells = [
            cell(SUBLORE_CELL_TEXT, first, 0, handle),
            cell(SUBLORE_CELL_TEXT, second, 0, handle),
        ];
        unsafe { host_panel_row(pointer(ctx), cells.as_ptr(), cells.len()) }
    }

    #[test]
    fn a_run_that_is_begun_filled_and_ended_publishes_the_rows_the_module_pushed() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        assert_eq!(
            unsafe { host_panel_begin(pointer(&ctx), PANEL) },
            SUBLORE_OK
        );
        // Nothing is published while the run is open, so a panel between the two shows what it
        // showed before and a half filled table cannot be drawn.
        assert!(entered.panels().is_empty());
        assert_eq!(push_row(&ctx, 1, "first", "one"), SUBLORE_OK);
        // The handle a `u64` above 2^53 has, which is the value the decimal string exists for.
        assert_eq!(
            push_row(&ctx, 9_007_199_254_740_993, "second", "two"),
            SUBLORE_OK
        );
        assert!(entered.panels().is_empty(), "still nothing before the end");
        assert_eq!(unsafe { host_panel_end(pointer(&ctx)) }, SUBLORE_OK);

        let published = entered.panels();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].panel_id, PANEL);
        assert_eq!(
            published[0].module, 3,
            "the module is the host's, not the module's"
        );
        assert_eq!(published[0].rows.len(), 2);
        assert_eq!(published[0].rows[0].handle, "1");
        // Every digit of it. A number on the wire would come back 9007199254740992.
        assert_eq!(published[0].rows[1].handle, "9007199254740993");
        assert_eq!(published[0].rows[1].cells.len(), 2);
        assert_eq!(published[0].rows[1].cells[0].kind, "text");
        assert_eq!(published[0].rows[1].cells[0].text, "second");
        // And taken means taken, as it does for a patch.
        assert!(entered.panels().is_empty());
        assert!(entered.abandoned_panel().is_none());
    }

    #[test]
    fn a_run_left_open_publishes_nothing_and_is_named_with_the_rows_it_had() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        assert_eq!(
            unsafe { host_panel_begin(pointer(&ctx), PANEL) },
            SUBLORE_OK
        );
        assert_eq!(push_row(&ctx, 1, "half", "a table"), SUBLORE_OK);

        // The module returned without ending it. Nothing crosses, and the caller is handed what it
        // needs to say so rather than the rows.
        assert!(entered.panels().is_empty());
        assert_eq!(entered.abandoned_panel(), Some((PANEL, 1)));
        assert!(entered.abandoned_panel().is_none(), "taken means taken");
    }

    #[test]
    fn an_empty_run_publishes_and_is_what_clears_a_panel() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        assert_eq!(
            unsafe { host_panel_begin(pointer(&ctx), PANEL) },
            SUBLORE_OK
        );
        assert_eq!(unsafe { host_panel_end(pointer(&ctx)) }, SUBLORE_OK);

        // A publish with no rows, and not an absence: a module saying it has nothing to show is
        // saying something, and the window clears that panel on it.
        let published = entered.panels();
        assert_eq!(published.len(), 1);
        assert!(published[0].rows.is_empty());
    }

    #[test]
    fn a_second_run_while_one_is_open_is_refused_and_the_first_one_survives() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        assert_eq!(
            unsafe { host_panel_begin(pointer(&ctx), PANEL) },
            SUBLORE_OK
        );
        assert_eq!(push_row(&ctx, 1, "kept", "row"), SUBLORE_OK);
        // Refused rather than nested, which is `db_transaction`'s answer in the same shape.
        assert_eq!(
            unsafe { host_panel_begin(pointer(&ctx), PANEL) },
            SUBLORE_ERR_DENIED
        );
        assert_eq!(unsafe { host_panel_end(pointer(&ctx)) }, SUBLORE_OK);

        let published = entered.panels();
        assert_eq!(published.len(), 1);
        assert_eq!(
            published[0].rows.len(),
            1,
            "the refusal did not reset the run"
        );
    }

    #[test]
    fn a_panel_the_module_did_not_contribute_is_refused() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        // The allowlist is the host's, from what this module described. Naming another id is a
        // module reaching for a panel it was never given, which is H6's storage rule again.
        assert_eq!(
            unsafe { host_panel_begin(pointer(&ctx), NOT_OURS) },
            SUBLORE_ERR_DENIED
        );
        // And no run was opened by the refusal, so a row now has nothing to go into.
        assert_eq!(push_row(&ctx, 1, "nowhere", "at all"), SUBLORE_ERR_DENIED);
        assert!(entered.panels().is_empty());
        assert!(entered.abandoned_panel().is_none());
    }

    #[test]
    fn a_row_or_an_end_with_no_run_open_is_refused() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        assert_eq!(push_row(&ctx, 1, "before", "the begin"), SUBLORE_ERR_DENIED);
        assert_eq!(unsafe { host_panel_end(pointer(&ctx)) }, SUBLORE_ERR_DENIED);
        assert!(entered.panels().is_empty());
    }

    #[test]
    fn a_row_of_a_different_width_than_the_first_is_refused() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        assert_eq!(
            unsafe { host_panel_begin(pointer(&ctx), PANEL) },
            SUBLORE_OK
        );
        assert_eq!(push_row(&ctx, 1, "two", "cells"), SUBLORE_OK);

        // The first row fixed the width, so a ragged table cannot exist and no number had to be
        // picked for how wide one is.
        let narrow = [cell(SUBLORE_CELL_TEXT, "one cell", 0, 2)];
        assert_eq!(
            unsafe { host_panel_row(pointer(&ctx), narrow.as_ptr(), narrow.len()) },
            SUBLORE_ERR_DENIED
        );
        // And a row with no cells at all, which is not a width either.
        assert_eq!(
            unsafe { host_panel_row(pointer(&ctx), narrow.as_ptr(), 0) },
            SUBLORE_ERR_DENIED
        );
        assert_eq!(unsafe { host_panel_end(pointer(&ctx)) }, SUBLORE_OK);
        assert_eq!(
            entered.panels()[0].rows.len(),
            1,
            "only the row that fitted"
        );
    }

    #[test]
    fn a_cell_this_build_has_no_kind_for_costs_its_row_and_not_half_of_it() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        assert_eq!(
            unsafe { host_panel_begin(pointer(&ctx), PANEL) },
            SUBLORE_OK
        );
        // The next value up from badge. Refused, and the good cell beside it is not kept: a row is
        // validated whole before any of it is.
        let mixed = [
            cell(SUBLORE_CELL_TEXT, "good", 0, 1),
            cell(SUBLORE_CELL_BADGE + 1, "unknown", 0, 1),
        ];
        assert_eq!(
            unsafe { host_panel_row(pointer(&ctx), mixed.as_ptr(), mixed.len()) },
            SUBLORE_ERR_UNSUPPORTED
        );
        // Text that is not text, refused the same way.
        let invalid = [0xffu8, 0xfe];
        let bad = [
            SubloreCell {
                kind: SUBLORE_CELL_TEXT,
                text: SubloreStr {
                    ptr: invalid.as_ptr(),
                    len: invalid.len(),
                },
                number: 0,
                r#ref: 1,
            },
            cell(SUBLORE_CELL_TEXT, "beside it", 0, 1),
        ];
        assert_eq!(
            unsafe { host_panel_row(pointer(&ctx), bad.as_ptr(), bad.len()) },
            SUBLORE_ERR_BAD_STRING
        );
        // A null pointer, which is a jump this must never make.
        assert_eq!(
            unsafe { host_panel_row(pointer(&ctx), std::ptr::null(), 2) },
            SUBLORE_ERR_BAD_STRING
        );
        assert_eq!(unsafe { host_panel_end(pointer(&ctx)) }, SUBLORE_OK);
        assert!(entered.panels()[0].rows.is_empty());
    }

    #[test]
    fn a_panel_call_outside_an_activation_is_refused() {
        let ctx = HostCtx::new();
        // What `create`, `describe` and `destroy` lend: no activation, so no panel and no window.
        let entered = ctx.enter(NAME, Lent::default());

        assert_eq!(
            unsafe { host_panel_begin(pointer(&ctx), PANEL) },
            SUBLORE_ERR_DENIED
        );
        assert_eq!(push_row(&ctx, 1, "no", "activation"), SUBLORE_ERR_DENIED);
        assert_eq!(unsafe { host_panel_end(pointer(&ctx)) }, SUBLORE_ERR_DENIED);
        assert!(entered.panels().is_empty());
    }

    #[test]
    fn a_panel_call_from_another_thread_never_reaches_the_record() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        let elsewhere = std::thread::scope(|scope| {
            scope
                .spawn(|| unsafe { host_panel_begin(pointer(&ctx), PANEL) })
                .join()
                .expect("the other thread must return rather than block")
        });
        assert_eq!(elsewhere, SUBLORE_ERR_WRONG_THREAD);
        assert!(entered.abandoned_panel().is_none(), "nothing was opened");
    }

    #[test]
    fn a_status_reaches_the_window_and_one_outside_an_activation_does_not() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        {
            let _entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));
            unsafe { host_status(pointer(&ctx), SubloreStr::borrowed("reading the file")) };
            // Not text, so nothing goes out and the module is not told: `status` returns nothing.
            let invalid = [0xffu8, 0xfe];
            unsafe {
                host_status(
                    pointer(&ctx),
                    SubloreStr {
                        ptr: invalid.as_ptr(),
                        len: invalid.len(),
                    },
                )
            };
        }
        {
            // No activation: nothing is showing a status, so nothing is sent.
            let _entered = ctx.enter(NAME, Lent::default());
            unsafe { host_status(pointer(&ctx), SubloreStr::borrowed("from describe")) };
        }
        assert_eq!(
            watched.said(),
            vec![Said::Status(42, "reading the file".to_owned())]
        );
    }

    #[test]
    fn a_progress_that_repeats_the_pair_it_just_sent_sends_nothing() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        let _entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));

        unsafe { host_progress(pointer(&ctx), 1, 40) };
        // The same pair again. A repeat carries nothing new, and dropping it holds no clock: the
        // pair after it goes out at once.
        unsafe { host_progress(pointer(&ctx), 1, 40) };
        unsafe { host_progress(pointer(&ctx), 2, 40) };
        unsafe { host_progress(pointer(&ctx), 1, 40) };

        assert_eq!(
            watched.said(),
            vec![
                Said::Progress(42, 1, 40),
                Said::Progress(42, 2, 40),
                Said::Progress(42, 1, 40),
            ]
        );
    }

    #[test]
    fn should_cancel_answers_the_flag_and_every_refusal_answers_stop() {
        let ctx = HostCtx::new();
        let watched = Watched::new();
        {
            let _entered = ctx.enter(NAME, Lent::default().with_activation(watched.activation()));
            assert_eq!(
                unsafe { host_should_cancel(pointer(&ctx)) },
                CANCEL_KEEP_GOING
            );
            watched.cancel.store(true, Ordering::SeqCst);
            assert_eq!(unsafe { host_should_cancel(pointer(&ctx)) }, CANCEL_STOP);

            // From elsewhere, which is a refusal, and a refusal must never read as "carry on".
            let elsewhere = std::thread::scope(|scope| {
                scope
                    .spawn(|| unsafe { host_should_cancel(pointer(&ctx)) })
                    .join()
                    .expect("the other thread must return rather than block")
            });
            assert_ne!(elsewhere, CANCEL_KEEP_GOING);
        }
        // No context at all, and after the call returned: both stop.
        assert_ne!(
            unsafe { host_should_cancel(std::ptr::null_mut()) },
            CANCEL_KEEP_GOING
        );
        assert_ne!(
            unsafe { host_should_cancel(pointer(&ctx)) },
            CANCEL_KEEP_GOING
        );
    }

    #[test]
    fn a_describe_is_never_told_to_stop_because_nobody_asked_it_to() {
        let ctx = HostCtx::new();
        // What `create`, `describe` and `destroy` are entered with. There is no activation, so
        // there is no user and no Stop, and the answer is to carry on rather than to give up.
        let _entered = ctx.enter(NAME, Lent::default());
        assert_eq!(
            unsafe { host_should_cancel(pointer(&ctx)) },
            CANCEL_KEEP_GOING
        );
    }
}
