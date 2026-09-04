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
use std::sync::Mutex;
use std::thread::ThreadId;

use sublore_edit::history::Run;
use sublore_edit::plan::Edit;
use sublore_edit::session::EditSession;
use sublore_formats::SubtitleFormat;
use sublore_module_api::{
    SubloreCue, SubloreDocument, SubloreHitFn, SubloreHost, SubloreProposal, SubloreStr,
    SUBLORE_ABI_MINOR, SUBLORE_ERR_BAD_STRING, SUBLORE_ERR_DENIED, SUBLORE_ERR_NOTHING_OPEN,
    SUBLORE_ERR_NO_SUCH_CUE, SUBLORE_ERR_PANIC, SUBLORE_ERR_STALE_REVISION,
    SUBLORE_ERR_UNSUPPORTED, SUBLORE_ERR_UNWRITABLE_TEXT, SUBLORE_ERR_WRONG_THREAD,
    SUBLORE_FIND_OPTIONS, SUBLORE_FORMAT_ASS, SUBLORE_FORMAT_SRT, SUBLORE_FORMAT_VTT,
    SUBLORE_HOST_SIZE, SUBLORE_LOG_DEBUG, SUBLORE_LOG_ERROR, SUBLORE_LOG_INFO, SUBLORE_LOG_WARN,
    SUBLORE_OK, SUBLORE_PROPOSAL_SET_CUE_TEXT,
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
    /// What the module changed during this call, in the order it changed it.
    ///
    /// A module proposes one cue at a time and may propose more than once, and each one is an edit
    /// the window has to be told about. They are collected here rather than returned, because
    /// `propose` answers a code and the interface has no way back for a value (§2.2).
    proposed: Vec<CuePatchDto>,
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
    /// `session` is the host's own locked session, lent for the call. The returned guard borrows it
    /// for its own lifetime, so the lock cannot be released while the record still names it, and
    /// the guard disarms on drop, so a body that returns early or panics leaves the context closed
    /// behind it.
    pub fn enter<'a>(
        &'a self,
        name: &str,
        session: Option<&'a mut Option<EditSession>>,
    ) -> Entered<'a> {
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
            proposed: Vec::new(),
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

/// Safety: the record holds a raw pointer, which is what costs `HostCtx` its automatic marker
/// traits, and sharing the context across threads is exactly what it is for: a module that calls
/// from a thread of its own has to reach a refusal rather than a missing symbol. The pointer is
/// dereferenced only after the thread comparison in [`HostCtx::session`] has passed, and that
/// comparison fails on every thread but the one the record was armed from, so no second thread can
/// ever reach the session through it.
unsafe impl Send for HostCtx {}
unsafe impl Sync for HostCtx {}

/// One armed call. Disarms the context when it goes.
///
/// Held rather than discarded, always: dropping it on the spot arms the gate and closes it again
/// before the call it was armed for is made, which would refuse every callback that call attempts.
#[must_use = "the gate is armed only while this guard is alive"]
pub struct Entered<'a> {
    ctx: &'a HostCtx,
    /// The session borrow the record holds a pointer to. Nothing reads this field: it is what stops
    /// the caller from releasing the lock while the record still names it.
    borrowed: PhantomData<&'a mut Option<EditSession>>,
}

impl Entered<'_> {
    /// What the module changed during this call. Taken before the guard goes, because the record it
    /// reads is what the guard clears.
    pub fn proposed(&self) -> Vec<CuePatchDto> {
        self.ctx
            .record(|call| std::mem::take(&mut call.proposed))
            .unwrap_or_default()
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
        should_cancel: None,
        progress: None,
        document: Some(host_document),
        cue_at: Some(host_cue_at),
        for_each_line: None,
        propose: Some(host_propose),
        find: Some(host_find),
        db_run: None,
        db_transaction: None,
        panel_begin: None,
        panel_row: None,
        panel_end: None,
        status: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use sublore_module_api::{
        SUBLORE_ERR_CANCELLED, SUBLORE_FIND_MATCH_CASE, SUBLORE_FIND_SKIP_TAGS,
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
        let _entered = ctx.enter(NAME, None);
        let (code, collected) = find_in(&ctx, "By then the fog had eaten the boats.", "fog", 0);
        assert_eq!(code, SUBLORE_OK);
        assert_eq!(collected.hits, vec![(12, 3)]);
    }

    #[test]
    fn a_call_from_another_thread_is_refused_and_does_not_block() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, None);
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
        drop(ctx.enter(NAME, None));
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
        let _entered = ctx.enter(NAME, None);
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
        let _entered = ctx.enter(NAME, None);
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
        let _entered = ctx.enter(NAME, None);
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
        let _entered = ctx.enter(NAME, None);
        // The next bit up. A mask would answer this as an ordinary folded search, and the module
        // would believe it had asked for something it did not get.
        let (code, collected) = find_in(&ctx, "the fog", "fog", 4);
        assert_eq!(code, SUBLORE_ERR_UNSUPPORTED);
        assert!(collected.hits.is_empty());
    }

    #[test]
    fn a_find_with_no_sink_function_is_refused_rather_than_jumped_through() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, None);
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
        let _entered = ctx.enter(NAME, None);
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
        let _entered = ctx.enter(NAME, Some(&mut session));

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
        let _entered = ctx.enter(NAME, Some(&mut none));
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
        let _entered = ctx.enter(NAME, None);
        assert_eq!(document_of(&ctx).0, SUBLORE_ERR_NOTHING_OPEN);
        assert_eq!(cue_of(&ctx, 0).0, SUBLORE_ERR_NOTHING_OPEN);
    }

    #[test]
    fn a_crlf_file_hands_over_the_form_an_edit_has_to_be_proposed_in() {
        let document =
            sublore_formats::parse(SubtitleFormat::Srt, SRT.as_bytes()).expect("srt should parse");
        let mut session = Some(EditSession::untitled(document));
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME, Some(&mut session));

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
        let _entered = ctx.enter(NAME, Some(&mut session));

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
        let _entered = ctx.enter(NAME, Some(&mut session));

        assert_eq!(cue_of(&ctx, 2).0, SUBLORE_OK);
        assert_eq!(cue_of(&ctx, 3).0, SUBLORE_ERR_NO_SUCH_CUE);
        // The value a 32-bit build would wrap into a valid index.
        assert_eq!(cue_of(&ctx, u64::MAX).0, SUBLORE_ERR_NO_SUCH_CUE);
    }

    #[test]
    fn a_read_with_nowhere_to_write_is_refused_rather_than_written_through_null() {
        let ctx = HostCtx::new();
        let mut session = opened();
        let _entered = ctx.enter(NAME, Some(&mut session));
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
        let _entered = ctx.enter(NAME, Some(&mut session));
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
        let entered = ctx.enter(NAME, Some(&mut session));

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
            let entered = ctx.enter(NAME, Some(&mut session));
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
        let _entered = ctx.enter(NAME, Some(&mut session));
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
        let _entered = ctx.enter(NAME, Some(&mut session));
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
        let _entered = ctx.enter(NAME, Some(&mut session));
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
        let _entered = ctx.enter(NAME, Some(&mut session));
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
        let _entered = ctx.enter(NAME, Some(&mut none));
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
        let _entered = ctx.enter(NAME, Some(&mut session));
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
}
