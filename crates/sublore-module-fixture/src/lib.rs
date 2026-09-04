//! The module the loader is proved against, before any real one exists.
//!
//! It is not a pro module and holds nothing pro. It contributes one menu title with one item under
//! it, which is the smallest contribution that exercises the parent link, and it fills only the
//! three slots the handshake needs. Every other slot in the table stays null, which the interface
//! allows on purpose: a slot the other side left empty is a refusal and never a jump.
//!
//! It stays after the real modules arrive, as the regression fixture for the interface itself.
//!
//! **The logic is here and the exported symbols are in `examples/sublore_module_fixture.rs`**, and
//! that split is not taste. `cargo test --workspace` rebuilds a package's examples and does not
//! rebuild a `cdylib` lib, measured on 2026-09-04, so a fixture whose artifact is the lib goes
//! stale under the gate and the checks read bytes from an earlier build. Every artifact here is an
//! example for that reason. See docs/module-loader-tasks.md L1.

use core::ffi::c_void;

use sublore_module_api::{
    SubloreHost, SubloreInvocation, SubloreItem, SubloreItemFn, SubloreModule, SubloreStr,
    SUBLORE_ABI_VERSION, SUBLORE_ENABLE_ALWAYS, SUBLORE_ERR_BAD_STRING, SUBLORE_ERR_UNSUPPORTED,
    SUBLORE_ITEM_MENU_ITEM, SUBLORE_ITEM_MENU_TITLE, SUBLORE_MODULE_SIZE, SUBLORE_OK,
};

/// The title, and the one item under it. Ids are the module's own and mean nothing to the core.
const TITLE_ID: u32 = 1;
const ITEM_ID: u32 = 2;

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
            label: SubloreStr::borrowed("Say something"),
            icon: SubloreStr::borrowed(""),
        },
    ];
    for item in &items {
        // A sink that refuses stops the walk: the host is entitled to stop listening, and pushing
        // on past a refusal is how a module talks over one.
        let answer = unsafe { push(sink, item) };
        if answer != SUBLORE_OK {
            return answer;
        }
    }
    SUBLORE_OK
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
