//! The module the loader is proved against, before any real one exists.
//!
//! It is not a pro module and holds nothing pro. It contributes one menu title with one item under
//! it, which is the smallest contribution that exercises the parent link, and it calls back into
//! the host from inside `describe` for the two slots the host has filled. Every slot neither side
//! has filled stays null, which the interface allows on purpose: a slot the other side left empty
//! is a refusal and never a jump, and both directions check before they call.
//!
//! It stays after the real modules arrive, as the regression fixture for the interface itself.
//!
//! **The logic is here and the exported symbols are in `examples/sublore_module_fixture.rs`**, and
//! that split is not taste. `cargo test --workspace` rebuilds a package's examples and does not
//! rebuild a `cdylib` lib, measured on 2026-09-04, so a fixture whose artifact is the lib goes
//! stale under the gate and the checks read bytes from an earlier build. Every artifact here is an
//! example for that reason. See docs/module-loader-tasks.md L1.

use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, Ordering};

use sublore_module_api::{
    SubloreHost, SubloreInvocation, SubloreItem, SubloreItemFn, SubloreModule, SubloreStr,
    SUBLORE_ABI_VERSION, SUBLORE_ENABLE_ALWAYS, SUBLORE_ERR_BAD_STRING, SUBLORE_ERR_UNSUPPORTED,
    SUBLORE_FIND_SKIP_TAGS, SUBLORE_ITEM_MENU_ITEM, SUBLORE_ITEM_MENU_TITLE, SUBLORE_LOG_INFO,
    SUBLORE_MODULE_SIZE, SUBLORE_OK,
};

/// The table the host lent, kept from `load` because `create` is not handed one and `describe`
/// needs it. Valid for this module's whole life, per section 1.
static HOST: AtomicPtr<SubloreHost> = AtomicPtr::new(core::ptr::null_mut());

/// The line the fixture asks the host to search, and the term it looks for.
///
/// The override block in the middle is the point. `the fog` is not in these bytes and it is in what
/// a reader sees, so an answer of two says the comparison came from the core and not from here.
const HAYSTACK: &str = "the {\\i1}fog and the fog";
const NEEDLE: &str = "the fog";

/// The title, and the one item under it. Ids are the module's own and mean nothing to the core.
const TITLE_ID: u32 = 1;
const ITEM_ID: u32 = 2;
/// An item the host has to refuse, so that its allowlist is defended by something.
const REFUSED_ID: u32 = 3;

/// What the fixture's own state is. Nothing yet beyond proving `create` and `destroy` pair up.
struct Fixture {
    /// The locale the host handed over, kept so a test can see the string survived the boundary.
    locale: String,
}

/// What the exported handshake answers. The export itself lives in the example beside this crate.
pub fn abi() -> u64 {
    SUBLORE_ABI_VERSION
}

/// # Safety
/// `host` and `out` point at tables the host owns and keeps alive for this call, per section 1.
pub unsafe fn load(host: *const SubloreHost, out: *mut SubloreModule) -> i32 {
    if host.is_null() || out.is_null() {
        return SUBLORE_ERR_BAD_STRING;
    }
    // The host's own size, checked against what this compilation expects. A mismatch here is
    // version skew the minor number failed to describe, which is a human's mistake (section 3.3).
    let table = unsafe { &*host };
    if table.size != sublore_module_api::SUBLORE_HOST_SIZE {
        return sublore_module_api::SUBLORE_ERR_VERSION;
    }
    HOST.store(host.cast_mut(), Ordering::Release);

    let filled = SubloreModule {
        size: SUBLORE_MODULE_SIZE,
        minor: sublore_module_api::SUBLORE_ABI_MINOR,
        create: Some(create),
        destroy: Some(destroy),
        describe: Some(describe),
        project_opened: None,
        project_closing: None,
        schema_version: None,
        schema_upgrade: None,
        invoke: Some(invoke),
    };
    unsafe { out.write(filled) };
    SUBLORE_OK
}

/// # Safety
/// `ctx_out` points at one writable pointer, and the two strings are valid for this call only.
unsafe extern "C" fn create(
    ctx_out: *mut *mut c_void,
    _config_dir: SubloreStr,
    locale: SubloreStr,
) -> i32 {
    if ctx_out.is_null() {
        return SUBLORE_ERR_BAD_STRING;
    }
    let Ok(locale) = (unsafe { locale.as_str() }) else {
        return SUBLORE_ERR_BAD_STRING;
    };
    let fixture = Box::new(Fixture {
        locale: locale.to_owned(),
    });
    unsafe { ctx_out.write(Box::into_raw(fixture).cast()) };
    SUBLORE_OK
}

/// # Safety
/// `ctx` is what `create` wrote, handed back exactly once.
unsafe extern "C" fn destroy(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(ctx.cast::<Fixture>()) });
}

/// # Safety
/// `ctx` is what `create` wrote; `sink` and `push` belong to the host and are valid for this call.
unsafe extern "C" fn describe(ctx: *mut c_void, sink: *mut c_void, push: SubloreItemFn) -> i32 {
    let Some(push) = push else {
        // A host that asked for a description without a way to receive one.
        return SUBLORE_ERR_UNSUPPORTED;
    };
    if ctx.is_null() {
        return SUBLORE_ERR_BAD_STRING;
    }
    let fixture = unsafe { &*ctx.cast::<Fixture>() };
    // The locale is in the label so a check can see that `create`'s string crossed intact and came
    // back out, which is the only evidence from outside that the boundary carried it.
    let title = format!("Fixture ({})", fixture.locale);
    // Inside the host call, which is the only thread and the only moment section 2.5 allows. The
    // count goes in the label for the same reason the locale does: it is the evidence from outside
    // that the call crossed and came back.
    let found = unsafe { ask_the_host() };
    let action = format!("Say something ({found} found)");

    let items = [
        SubloreItem {
            id: TITLE_ID,
            kind: SUBLORE_ITEM_MENU_TITLE,
            parent: 0,
            enable_when: SUBLORE_ENABLE_ALWAYS,
            flags: 0,
            label: SubloreStr::borrowed(&title),
            icon: SubloreStr::borrowed(""),
        },
        SubloreItem {
            id: ITEM_ID,
            kind: SUBLORE_ITEM_MENU_ITEM,
            parent: TITLE_ID,
            enable_when: SUBLORE_ENABLE_ALWAYS,
            flags: 0,
            label: SubloreStr::borrowed(&action),
            icon: SubloreStr::borrowed(""),
        },
        // Last, and deliberately wrong: `enable_when` is left at zero, which is the value section
        // 5.2 says a module that forgot to set it sends. The host must refuse this item rather than
        // draw a control enabled when it should not be, and the check counts what reached the menu.
        SubloreItem {
            id: REFUSED_ID,
            kind: SUBLORE_ITEM_MENU_ITEM,
            parent: TITLE_ID,
            enable_when: 0,
            flags: 0,
            label: SubloreStr::borrowed("Never drawn"),
            icon: SubloreStr::borrowed(""),
        },
    ];
    for item in &items {
        // A sink that refuses stops the walk: the host is entitled to stop listening, and pushing
        // on past a refusal is how a module talks over one.
        let answer = unsafe { push(sink, item) };
        if answer != SUBLORE_OK {
            // The host stopped listening. Everything before this reached it, which is the shape
            // the last item in the list is here to produce.
            return answer;
        }
    }
    SUBLORE_OK
}

/// What one `find` call counted.
struct Hits {
    count: usize,
}

/// # Safety
/// `sink` is the counter handed to `find`, valid for that call only.
unsafe extern "C" fn count_hit(sink: *mut c_void, _start: usize, _len: usize) -> i32 {
    if sink.is_null() {
        return SUBLORE_ERR_BAD_STRING;
    }
    unsafe { &mut *sink.cast::<Hits>() }.count += 1;
    SUBLORE_OK
}

/// Ask the host for the core's own comparison, and tell its log what came back.
///
/// A slot the host left empty is a refusal and never a jump, so both are checked before they are
/// called: that is the rule the interface is built on, in the direction a module reads it.
///
/// # Safety
/// Called only from inside a host call, with the table `load` was given.
unsafe fn ask_the_host() -> usize {
    let table = HOST.load(Ordering::Acquire);
    if table.is_null() {
        return 0;
    }
    let table = unsafe { &*table };

    let mut hits = Hits { count: 0 };
    if let Some(find) = table.find {
        unsafe {
            find(
                table.ctx,
                SubloreStr::borrowed(HAYSTACK),
                SubloreStr::borrowed(NEEDLE),
                SUBLORE_FIND_SKIP_TAGS,
                (&mut hits as *mut Hits).cast(),
                Some(count_hit),
            )
        };
    }
    if let Some(log) = table.log {
        let said = format!("asked for {NEEDLE:?} and was given {} of them", hits.count);
        unsafe { log(table.ctx, SUBLORE_LOG_INFO, SubloreStr::borrowed(&said)) };
    }
    hits.count
}

/// # Safety
/// `ctx` is what `create` wrote; `where_` is valid for this call.
unsafe extern "C" fn invoke(
    ctx: *mut c_void,
    item_id: u32,
    where_: *const SubloreInvocation,
) -> i32 {
    if ctx.is_null() || where_.is_null() {
        return SUBLORE_ERR_BAD_STRING;
    }
    // Nothing to do until the host can answer: the calls that would act on the document arrive
    // with N8e. Refusing an id it never contributed is the part that is meaningful now.
    if item_id != ITEM_ID {
        return SUBLORE_ERR_UNSUPPORTED;
    }
    SUBLORE_OK
}
