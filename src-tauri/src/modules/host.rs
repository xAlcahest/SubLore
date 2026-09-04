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
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;
use std::thread::ThreadId;

use sublore_module_api::{
    SubloreHitFn, SubloreHost, SubloreStr, SUBLORE_ABI_MINOR, SUBLORE_ERR_BAD_STRING,
    SUBLORE_ERR_PANIC, SUBLORE_ERR_UNSUPPORTED, SUBLORE_ERR_WRONG_THREAD, SUBLORE_FIND_OPTIONS,
    SUBLORE_HOST_SIZE, SUBLORE_LOG_DEBUG, SUBLORE_LOG_ERROR, SUBLORE_LOG_INFO, SUBLORE_LOG_WARN,
    SUBLORE_OK,
};

use crate::log;

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
    /// The guard disarms on drop, so a body that returns early or panics still leaves the context
    /// closed behind it.
    pub fn enter(&self, name: &str) -> Entered<'_> {
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
        });
        Entered { ctx: self }
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

/// One armed call. Disarms the context when it goes.
///
/// Held rather than discarded, always: dropping it on the spot arms the gate and closes it again
/// before the call it was armed for is made, which would refuse every callback that call attempts.
#[must_use = "the gate is armed only while this guard is alive"]
pub struct Entered<'a> {
    ctx: &'a HostCtx,
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
        document: None,
        cue_at: None,
        for_each_line: None,
        propose: None,
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
        let _entered = ctx.enter(NAME);
        let (code, collected) = find_in(&ctx, "By then the fog had eaten the boats.", "fog", 0);
        assert_eq!(code, SUBLORE_OK);
        assert_eq!(collected.hits, vec![(12, 3)]);
    }

    #[test]
    fn a_call_from_another_thread_is_refused_and_does_not_block() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME);
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
        drop(ctx.enter(NAME));
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
        let _entered = ctx.enter(NAME);
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
        let _entered = ctx.enter(NAME);
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
        let _entered = ctx.enter(NAME);
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
        let _entered = ctx.enter(NAME);
        // The next bit up. A mask would answer this as an ordinary folded search, and the module
        // would believe it had asked for something it did not get.
        let (code, collected) = find_in(&ctx, "the fog", "fog", 4);
        assert_eq!(code, SUBLORE_ERR_UNSUPPORTED);
        assert!(collected.hits.is_empty());
    }

    #[test]
    fn a_find_with_no_sink_function_is_refused_rather_than_jumped_through() {
        let ctx = HostCtx::new();
        let _entered = ctx.enter(NAME);
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
        let _entered = ctx.enter(NAME);
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
