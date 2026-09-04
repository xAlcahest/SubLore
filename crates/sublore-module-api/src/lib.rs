//! Licensed under the GNU GPL v3 or later, with the section 7 additional permission for modules
//! loaded through `sublore-module-api`. See LICENSE at the root of the repository.

//! The interface a module is loaded through: two exported symbols, two `#[repr(C)]` tables and the
//! constants both sides read. Types only — no loader, no host, no module. See BACKLOG.md N8a.
//!
//! Two rules hold everywhere below, and every signature here depends on them.
//!
//! Nothing allocated on either side outlives the call it appears in. There is no free function in
//! either table and nothing for one to free: a value that would have to be returned and released is
//! pushed through a callback instead, while the caller still holds it.
//!
//! A module calls the host only from the thread of the host call it is inside, and only before that
//! call returns. Calls never overlap and never nest.

use core::ffi::c_void;

// The pinned table sizes below are a 64-bit layout, which is every target Sublore ships
// (decision 24 G2).
const _: () = assert!(usize::BITS == 64, "the module interface is 64-bit only");

// ---------------------------------------------------------------------------------------------
// The handshake

/// The layout of every struct and the meaning of every slot. A difference is fatal in both
/// directions, with no compatibility shim.
pub const SUBLORE_ABI_MAJOR: u32 = 1;

/// Appended slots and appended trailing fields, nothing else. A host loads a module whose minor is
/// at or below its own and refuses one above it, because the module would be reading slots that
/// are not there.
pub const SUBLORE_ABI_MINOR: u32 = 0;

/// What `sublore_module_abi` returns: major in the high 32 bits, minor in the low 32.
pub const SUBLORE_ABI_VERSION: u64 = ((SUBLORE_ABI_MAJOR as u64) << 32) | SUBLORE_ABI_MINOR as u64;

/// Split what `sublore_module_abi` returned back into major and minor.
pub const fn abi_parts(version: u64) -> (u32, u32) {
    ((version >> 32) as u32, version as u32)
}

/// The handshake symbol. Resolved first and on its own: a version check that passes a struct has
/// to agree on that struct's layout in order to find out whether the layouts agree.
pub const SUBLORE_ABI_SYMBOL: &[u8] = b"sublore_module_abi\0";

/// The entry point, resolved only once the handshake agrees. NUL-terminated, like the symbol
/// above, because `dlsym` takes a C string.
pub const SUBLORE_LOAD_SYMBOL: &[u8] = b"sublore_module_load\0";

/// What `SUBLORE_ABI_SYMBOL` resolves to. No arguments and a scalar return, so a version check
/// cannot depend on the layouts it exists to check.
pub type SubloreAbiFn = unsafe extern "C" fn() -> u64;

/// What `SUBLORE_LOAD_SYMBOL` resolves to. The host allocates both tables.
pub type SubloreLoadFn =
    unsafe extern "C" fn(host: *const SubloreHost, out: *mut SubloreModule) -> i32;

/// The value a writer stamps into `SubloreHost::size`: its own `sizeof`, checked by the reader.
pub const SUBLORE_HOST_SIZE: u32 = 128;

/// The value a writer stamps into `SubloreModule::size`.
pub const SUBLORE_MODULE_SIZE: u32 = 72;

// ---------------------------------------------------------------------------------------------
// Refusals

/// Success. Every other value is a refusal, and the list below is exhaustive and frozen for the
/// whole of major version 1: a new value must break a mapping, never slip past a wildcard.
pub const SUBLORE_OK: i32 = 0;

/// The two sides disagree: the handshake, a table size, or stored data a module is too old for.
pub const SUBLORE_ERR_VERSION: i32 = 1;

/// A string that is not UTF-8, or a null pointer with a non-zero length. The receiver validates;
/// neither side trusts the other's encoding.
pub const SUBLORE_ERR_BAD_STRING: i32 = 2;

/// A tagged value the receiver has no meaning for: a proposal kind, an item kind, a cell kind.
pub const SUBLORE_ERR_UNSUPPORTED: i32 = 3;

/// Asked for a document or a project that is not open.
pub const SUBLORE_ERR_NOTHING_OPEN: i32 = 4;

/// No cue with that index in this document.
pub const SUBLORE_ERR_NO_SUCH_CUE: i32 = 5;

/// The proposal names a revision the session has already moved past.
pub const SUBLORE_ERR_STALE_REVISION: i32 = 6;

/// The replacement cannot be written in this format without changing the file's structure.
pub const SUBLORE_ERR_UNWRITABLE_TEXT: i32 = 7;

/// The call or the statement is refused by the host's guard: a table that is not the module's, a
/// pragma, an attach.
pub const SUBLORE_ERR_DENIED: i32 = 8;

/// The statement failed on its own terms.
pub const SUBLORE_ERR_STORAGE: i32 = 9;

/// The user cancelled, or the host is shutting the work down.
pub const SUBLORE_ERR_CANCELLED: i32 = 10;

/// A host call from another thread, or after the call it belonged to had returned.
pub const SUBLORE_ERR_WRONG_THREAD: i32 = 11;

/// A panic caught at the boundary. Panics never cross.
pub const SUBLORE_ERR_PANIC: i32 = 12;

// ---------------------------------------------------------------------------------------------
// Strings

/// A borrowed UTF-8 string, valid only for the duration of the call it appears in. `ptr` is null
/// only when `len` is 0, and the receiver validates the bytes.
///
/// Not NUL-terminated, and it cannot be: `SourceText::from_bytes` scans only the first 1024 bytes
/// of a file for a NUL (`crates/sublore-formats/src/text.rs`), so a NUL further in is accepted and
/// reaches a cue's text. A C string would truncate that cue silently at the NUL and every
/// comparison after it would run against half a subtitle. A pointer and a length cannot.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SubloreStr {
    pub ptr: *const u8,
    pub len: usize,
}

impl SubloreStr {
    /// Borrow a Rust string across the boundary. The result must not outlive `text`.
    pub fn borrowed(text: &str) -> Self {
        Self {
            ptr: text.as_ptr(),
            len: text.len(),
        }
    }

    /// Read a string the other side handed over.
    ///
    /// # Safety
    /// `ptr` must point at `len` readable bytes that stay valid for the whole of the call this
    /// value appears in, and at nothing after it.
    pub unsafe fn as_str<'a>(&self) -> Result<&'a str, i32> {
        // A zero length says nothing about ptr, and from_raw_parts refuses null even for none.
        if self.len == 0 {
            return Ok("");
        }
        if self.ptr.is_null() {
            return Err(SUBLORE_ERR_BAD_STRING);
        }
        let bytes = unsafe { core::slice::from_raw_parts(self.ptr, self.len) };
        core::str::from_utf8(bytes).map_err(|_| SUBLORE_ERR_BAD_STRING)
    }
}

// ---------------------------------------------------------------------------------------------
// Reading the open document

/// The format a document was parsed from, in `SubloreDocument::format`.
pub const SUBLORE_FORMAT_SRT: u32 = 1;
pub const SUBLORE_FORMAT_VTT: u32 = 2;
pub const SUBLORE_FORMAT_ASS: u32 = 3;

/// The open document. `cue_count` counts every cue, ASS `Comment:` events included, so the index a
/// module holds is the index an edit takes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SubloreDocument {
    pub format: u32,
    pub cue_count: u64,
    pub revision: u64,
    pub dirty: u8,
    /// Empty for a document that has never had a file.
    pub path: SubloreStr,
}

/// One cue. The text is in normalized form, every line break `\n`, which is the form an edit must
/// be proposed in: one rule rather than two.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SubloreCue {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: SubloreStr,
    /// An ASS `Comment:` event.
    pub is_comment: u8,
    pub has_number: u8,
    pub number: u32,
}

// ---------------------------------------------------------------------------------------------
// Reading every line of every episode

/// The roles a file can hold. Also the bits of `for_each_line`'s `roles` mask.
pub const SUBLORE_ROLE_SOURCE: u32 = 1;
pub const SUBLORE_ROLE_TARGET: u32 = 2;

/// Set in `SubloreLine::flags` for the one push that stands in for a file that is gone.
pub const SUBLORE_LINE_FLAG_FILE_MISSING: u32 = 1;

/// One line of one file of one episode, pushed by `for_each_line`.
///
/// A file that is gone arrives as exactly one push in the place its lines would have occupied,
/// with `SUBLORE_LINE_FLAG_FILE_MISSING` set, `index` at `SUBLORE_NO_CUE` and a zeroed cue. A walk
/// that aborted because episode fourteen was moved is a walk that fails for the whole series.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SubloreLine {
    pub episode_id: i64,
    /// The episode's position in the series, 1-based.
    pub ordinal: u32,
    pub role: u32,
    /// Bit 0: this file is gone and the cue below is empty.
    pub flags: u32,
    /// The cue's index within its own file, `SUBLORE_NO_CUE` when the file is gone.
    pub index: u64,
    pub cue: SubloreCue,
}

// ---------------------------------------------------------------------------------------------
// Proposing an edit

/// Replace the text of one cue. The only proposal there is: every other value is refused, and a
/// second one is a minor bump when the core grows something to address.
pub const SUBLORE_PROPOSAL_SET_CUE_TEXT: u32 = 1;

/// What a module asks the host to do to the open document. The host performs the edit; a module
/// never writes the document and never writes the file.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SubloreProposal {
    pub kind: u32,
    /// The revision the module read. A stale one is refused.
    pub revision: u64,
    /// Index into the document's cues.
    pub cue: u64,
    /// Normalized form, as `SubloreCue::text` was handed over.
    pub text: SubloreStr,
}

// ---------------------------------------------------------------------------------------------
// Storage

/// The kind of a bound parameter or a returned cell, in `SubloreValue::kind`.
pub const SUBLORE_VALUE_NULL: u32 = 0;
pub const SUBLORE_VALUE_INT: u32 = 1;
pub const SUBLORE_VALUE_REAL: u32 = 2;
pub const SUBLORE_VALUE_TEXT: u32 = 3;
pub const SUBLORE_VALUE_BLOB: u32 = 4;

/// One value in or out of a statement. Only the field `kind` names is read.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SubloreValue {
    pub kind: u32,
    pub i: i64,
    pub f: f64,
    /// Text, or the bytes of a blob.
    pub s: SubloreStr,
}

// ---------------------------------------------------------------------------------------------
// Contributions

/// What a described item is, in `SubloreItem::kind`.
pub const SUBLORE_ITEM_MENU_TITLE: u32 = 1;
pub const SUBLORE_ITEM_MENU_ITEM: u32 = 2;
pub const SUBLORE_ITEM_SEPARATOR: u32 = 3;
pub const SUBLORE_ITEM_TOOLBAR_BUTTON: u32 = 4;
pub const SUBLORE_ITEM_PANEL: u32 = 5;

/// The state an item is enabled on, in `SubloreItem::enable_when`. The core answers these against
/// state it already has, so it never learns what an item does.
pub const SUBLORE_ENABLE_ALWAYS: u32 = 1;
pub const SUBLORE_ENABLE_DOCUMENT_OPEN: u32 = 2;
pub const SUBLORE_ENABLE_PROJECT_OPEN: u32 = 3;
pub const SUBLORE_ENABLE_SELECTION_NON_EMPTY: u32 = 4;

/// `SubloreItem::flags` bit 0: this panel covers the video, so it registers as a layer and the
/// native surface hides while it is up.
pub const SUBLORE_ITEM_FLAG_LAYER: u32 = 1;

/// One thing a module puts in the menu bar, on the toolbar or on screen. Labels arrive rendered in
/// the locale `create` was given; the core never learns a module's vocabulary.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SubloreItem {
    /// The module's own, echoed back to `invoke`.
    pub id: u32,
    pub kind: u32,
    /// The id of the title or panel this belongs to, 0 for top level.
    pub parent: u32,
    pub enable_when: u32,
    pub flags: u32,
    pub label: SubloreStr,
    /// A name from a fixed core set, empty for none.
    pub icon: SubloreStr,
}

/// What a panel cell holds, in `SubloreCell::kind`.
pub const SUBLORE_CELL_TEXT: u32 = 1;
pub const SUBLORE_CELL_NUMBER: u32 = 2;
pub const SUBLORE_CELL_PERCENT: u32 = 3;
pub const SUBLORE_CELL_BADGE: u32 = 4;

/// One cell of one panel row. The core knows how to draw a table; it does not know what a row
/// means and it never asks.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SubloreCell {
    pub kind: u32,
    pub text: SubloreStr,
    pub number: i64,
    /// The module's own handle for this row, echoed back to `invoke`.
    pub r#ref: u64,
}

/// `SubloreInvocation::cue` when nothing is selected.
pub const SUBLORE_NO_CUE: u64 = u64::MAX;

/// The state a user gesture arrives with. Appended to at a minor bump, never reordered.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SubloreInvocation {
    pub revision: u64,
    /// The selected cue, or `SUBLORE_NO_CUE`.
    pub cue: u64,
    /// The `SubloreCell::ref` of the activated panel row, and zero when `panel_id` is zero. A
    /// module that reads it without looking at `panel_id` is reading a handle it was not given.
    pub row: u64,
    pub panel_id: u32,
    pub project_key: i64,
}

// ---------------------------------------------------------------------------------------------
// Callbacks
//
// Every slot is nullable: the table was filled by a different compilation and a slot it left empty
// must be a refusal, not a jump.

/// How loud a logged line is, in `log`'s `level`.
pub const SUBLORE_LOG_ERROR: u32 = 1;
pub const SUBLORE_LOG_WARN: u32 = 2;
pub const SUBLORE_LOG_INFO: u32 = 3;
pub const SUBLORE_LOG_DEBUG: u32 = 4;

/// One described item, valid for this call only.
pub type SubloreItemFn =
    Option<unsafe extern "C" fn(sink: *mut c_void, item: *const SubloreItem) -> i32>;

/// One line of one episode, valid for this call only.
pub type SubloreLineFn =
    Option<unsafe extern "C" fn(sink: *mut c_void, line: *const SubloreLine) -> i32>;

/// One match, as byte offsets into the haystack that was passed in.
pub type SubloreHitFn =
    Option<unsafe extern "C" fn(sink: *mut c_void, start: usize, len: usize) -> i32>;

/// One row of a statement's result, valid for this call only.
pub type SubloreRowFn = Option<
    unsafe extern "C" fn(sink: *mut c_void, cells: *const SubloreValue, cell_count: usize) -> i32,
>;

/// The body of a transaction. The host commits on `SUBLORE_OK` and rolls back on anything else.
pub type SubloreWorkFn = Option<unsafe extern "C" fn(work_ctx: *mut c_void) -> i32>;

// ---------------------------------------------------------------------------------------------
// The two tables

/// What the host may ask a module to do. The module fills it; the host allocates it, so a module
/// never allocates a table the host would have to free.
#[repr(C)]
pub struct SubloreModule {
    /// `SUBLORE_MODULE_SIZE` as the writer compiled it.
    pub size: u32,
    /// The minor version this table is filled to.
    pub minor: u32,

    /// `ctx` is the module's own and opaque to the host. `config_dir` is the module's own
    /// directory; `locale` is a BCP-47 tag.
    pub create: Option<
        unsafe extern "C" fn(
            ctx_out: *mut *mut c_void,
            config_dir: SubloreStr,
            locale: SubloreStr,
        ) -> i32,
    >,
    pub destroy: Option<unsafe extern "C" fn(ctx: *mut c_void)>,

    /// What the module contributes, pushed already rendered in the locale it was created with.
    pub describe: Option<
        unsafe extern "C" fn(ctx: *mut c_void, sink: *mut c_void, push_item: SubloreItemFn) -> i32,
    >,

    pub project_opened: Option<unsafe extern "C" fn(ctx: *mut c_void, project_key: i64) -> i32>,
    pub project_closing: Option<unsafe extern "C" fn(ctx: *mut c_void) -> i32>,

    /// The version the module's own tables want to be at.
    pub schema_version: Option<unsafe extern "C" fn(ctx: *mut c_void, out: *mut u32) -> i32>,
    /// Run inside a transaction the host opens; the host writes the new version itself, in it.
    pub schema_upgrade: Option<unsafe extern "C" fn(ctx: *mut c_void, from: u32, to: u32) -> i32>,

    /// A contributed item was activated.
    pub invoke: Option<
        unsafe extern "C" fn(
            ctx: *mut c_void,
            item_id: u32,
            where_: *const SubloreInvocation,
        ) -> i32,
    >,
}

/// What a module may ask the host for. The host fills it and lends it for the life of the module.
#[repr(C)]
pub struct SubloreHost {
    /// `SUBLORE_HOST_SIZE` as the writer compiled it.
    pub size: u32,
    /// The minor version this table is filled to.
    pub minor: u32,
    /// The host's own and opaque. This is the `host` argument every call below takes.
    pub ctx: *mut c_void,

    pub log: Option<unsafe extern "C" fn(host: *mut c_void, level: u32, message: SubloreStr)>,
    /// Non-zero once the work should stop. A module that never asks cannot be interrupted.
    pub should_cancel: Option<unsafe extern "C" fn(host: *mut c_void) -> i32>,
    pub progress: Option<unsafe extern "C" fn(host: *mut c_void, done: u64, total: u64)>,

    pub document: Option<unsafe extern "C" fn(host: *mut c_void, out: *mut SubloreDocument) -> i32>,
    pub cue_at:
        Option<unsafe extern "C" fn(host: *mut c_void, index: u64, out: *mut SubloreCue) -> i32>,

    /// Every cue of every attached file whose role is in `roles`, in episode order.
    pub for_each_line: Option<
        unsafe extern "C" fn(
            host: *mut c_void,
            roles: u32,
            sink: *mut c_void,
            on_line: SubloreLineFn,
        ) -> i32,
    >,

    pub propose:
        Option<unsafe extern "C" fn(host: *mut c_void, proposal: *const SubloreProposal) -> i32>,

    /// The core's comparison, so the free product's find and a module's cannot drift apart.
    /// `options` is not fixed yet: its first caller is the core's own find, which defines it.
    pub find: Option<
        unsafe extern "C" fn(
            host: *mut c_void,
            haystack: SubloreStr,
            needle: SubloreStr,
            options: u32,
            sink: *mut c_void,
            on_hit: SubloreHitFn,
        ) -> i32,
    >,

    /// One statement, bound and run on the host's connection. Trailing text after it is a refusal.
    pub db_run: Option<
        unsafe extern "C" fn(
            host: *mut c_void,
            sql: SubloreStr,
            params: *const SubloreValue,
            param_count: usize,
            sink: *mut c_void,
            on_row: SubloreRowFn,
        ) -> i32,
    >,
    pub db_transaction: Option<
        unsafe extern "C" fn(host: *mut c_void, work_ctx: *mut c_void, work: SubloreWorkFn) -> i32,
    >,

    pub panel_begin: Option<unsafe extern "C" fn(host: *mut c_void, panel_id: u32) -> i32>,
    pub panel_row: Option<
        unsafe extern "C" fn(
            host: *mut c_void,
            cells: *const SubloreCell,
            cell_count: usize,
        ) -> i32,
    >,
    pub panel_end: Option<unsafe extern "C" fn(host: *mut c_void) -> i32>,
    pub status: Option<unsafe extern "C" fn(host: *mut c_void, message: SubloreStr)>,
}

/// Every field of every table, named with the type it must keep.
///
/// Never called: the body is the pin. A field added, removed or renamed stops the destructuring
/// compiling, and one whose type changes stops the binding under it compiling.
///
/// This exists because neither of the two pins in the tests can see a field widened into its own
/// padding. Taking `SubloreLine::flags` from `u32` to `u64` leaves the size at sixty-four, the
/// alignment at eight, and all six offsets exactly where they were, while a module compiled
/// against the old header now writes four bytes where the host reads eight. Found by mutation on
/// 2026-09-04, after the offset pin had been added and still let it through.
#[allow(dead_code, clippy::too_many_arguments)]
fn every_field_holds_its_type(
    sublorestr: SubloreStr,
    subloredocument: SubloreDocument,
    sublorecue: SubloreCue,
    subloreline: SubloreLine,
    subloreproposal: SubloreProposal,
    sublorevalue: SubloreValue,
    subloreitem: SubloreItem,
    sublorecell: SubloreCell,
    subloreinvocation: SubloreInvocation,
    subloremodule: SubloreModule,
    sublorehost: SubloreHost,
) {
    let SubloreStr { ptr, len } = sublorestr;
    let _: *const u8 = ptr;
    let _: usize = len;
    let SubloreDocument {
        format,
        cue_count,
        revision,
        dirty,
        path,
    } = subloredocument;
    let _: u32 = format;
    let _: u64 = cue_count;
    let _: u64 = revision;
    let _: u8 = dirty;
    let _: SubloreStr = path;
    let SubloreCue {
        start_ms,
        end_ms,
        text,
        is_comment,
        has_number,
        number,
    } = sublorecue;
    let _: u32 = start_ms;
    let _: u32 = end_ms;
    let _: SubloreStr = text;
    let _: u8 = is_comment;
    let _: u8 = has_number;
    let _: u32 = number;
    let SubloreLine {
        episode_id,
        ordinal,
        role,
        flags,
        index,
        cue,
    } = subloreline;
    let _: i64 = episode_id;
    let _: u32 = ordinal;
    let _: u32 = role;
    let _: u32 = flags;
    let _: u64 = index;
    let _: SubloreCue = cue;
    let SubloreProposal {
        kind,
        revision,
        cue,
        text,
    } = subloreproposal;
    let _: u32 = kind;
    let _: u64 = revision;
    let _: u64 = cue;
    let _: SubloreStr = text;
    let SubloreValue { kind, i, f, s } = sublorevalue;
    let _: u32 = kind;
    let _: i64 = i;
    let _: f64 = f;
    let _: SubloreStr = s;
    let SubloreItem {
        id,
        kind,
        parent,
        enable_when,
        flags,
        label,
        icon,
    } = subloreitem;
    let _: u32 = id;
    let _: u32 = kind;
    let _: u32 = parent;
    let _: u32 = enable_when;
    let _: u32 = flags;
    let _: SubloreStr = label;
    let _: SubloreStr = icon;
    let SubloreCell {
        kind,
        text,
        number,
        r#ref,
    } = sublorecell;
    let _: u32 = kind;
    let _: SubloreStr = text;
    let _: i64 = number;
    let _: u64 = r#ref;
    let SubloreInvocation {
        revision,
        cue,
        row,
        panel_id,
        project_key,
    } = subloreinvocation;
    let _: u64 = revision;
    let _: u64 = cue;
    let _: u64 = row;
    let _: u32 = panel_id;
    let _: i64 = project_key;
    let SubloreModule {
        size,
        minor,
        create,
        destroy,
        describe,
        project_opened,
        project_closing,
        schema_version,
        schema_upgrade,
        invoke,
    } = subloremodule;
    let _ = size;
    let _ = minor;
    let _ = create;
    let _ = destroy;
    let _ = describe;
    let _ = project_opened;
    let _ = project_closing;
    let _ = schema_version;
    let _ = schema_upgrade;
    let _ = invoke;
    let SubloreHost {
        size,
        minor,
        ctx,
        log,
        should_cancel,
        progress,
        document,
        cue_at,
        for_each_line,
        propose,
        find,
        db_run,
        db_transaction,
        panel_begin,
        panel_row,
        panel_end,
        status,
    } = sublorehost;
    let _ = size;
    let _ = minor;
    let _ = ctx;
    let _ = log;
    let _ = should_cancel;
    let _ = progress;
    let _ = document;
    let _ = cue_at;
    let _ = for_each_line;
    let _ = propose;
    let _ = find;
    let _ = db_run;
    let _ = db_transaction;
    let _ = panel_begin;
    let _ = panel_row;
    let _ = panel_end;
    let _ = status;
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    /// A C ABI drifts in silence: a field added, reordered or widened compiles on both sides and is
    /// misread at run time. These numbers are the layout, and moving one is a major bump.
    #[test]
    fn every_type_holds_its_layout() {
        fn layout<T>() -> (usize, usize) {
            (size_of::<T>(), align_of::<T>())
        }

        assert_eq!(layout::<SubloreStr>(), (16, 8), "SubloreStr");
        assert_eq!(layout::<SubloreDocument>(), (48, 8), "SubloreDocument");
        assert_eq!(layout::<SubloreCue>(), (32, 8), "SubloreCue");
        assert_eq!(layout::<SubloreLine>(), (64, 8), "SubloreLine");
        assert_eq!(layout::<SubloreProposal>(), (40, 8), "SubloreProposal");
        assert_eq!(layout::<SubloreValue>(), (40, 8), "SubloreValue");
        assert_eq!(layout::<SubloreItem>(), (56, 8), "SubloreItem");
        assert_eq!(layout::<SubloreCell>(), (40, 8), "SubloreCell");
        assert_eq!(layout::<SubloreInvocation>(), (40, 8), "SubloreInvocation");
        assert_eq!(layout::<SubloreModule>(), (72, 8), "SubloreModule");
        assert_eq!(layout::<SubloreHost>(), (128, 8), "SubloreHost");
    }

    /// Where every field of every table actually sits.
    ///
    /// The size and alignment pin above is not enough on its own, and a mutation showed it:
    /// widening `SubloreLine::flags` from `u32` to `u64` puts the extra four bytes into padding
    /// that was already there, so the struct stays sixty-four bytes and that pin stays green while
    /// the two sides have started disagreeing about where `index` begins. Swapping two fields of
    /// the same width is invisible to it for the same reason. An offset is what a C layout is, so
    /// this is what holds it.
    ///
    /// Every number here was read out of `rustc` through `offset_of!`, never counted by hand.
    #[test]
    fn every_field_holds_its_offset() {
        assert_eq!(
            [offset_of!(SubloreStr, ptr), offset_of!(SubloreStr, len),],
            [0, 8],
            "SubloreStr"
        );
        assert_eq!(
            [
                offset_of!(SubloreDocument, format),
                offset_of!(SubloreDocument, cue_count),
                offset_of!(SubloreDocument, revision),
                offset_of!(SubloreDocument, dirty),
                offset_of!(SubloreDocument, path),
            ],
            [0, 8, 16, 24, 32],
            "SubloreDocument"
        );
        assert_eq!(
            [
                offset_of!(SubloreCue, start_ms),
                offset_of!(SubloreCue, end_ms),
                offset_of!(SubloreCue, text),
                offset_of!(SubloreCue, is_comment),
                offset_of!(SubloreCue, has_number),
                offset_of!(SubloreCue, number),
            ],
            [0, 4, 8, 24, 25, 28],
            "SubloreCue"
        );
        assert_eq!(
            [
                offset_of!(SubloreLine, episode_id),
                offset_of!(SubloreLine, ordinal),
                offset_of!(SubloreLine, role),
                offset_of!(SubloreLine, flags),
                offset_of!(SubloreLine, index),
                offset_of!(SubloreLine, cue),
            ],
            [0, 8, 12, 16, 24, 32],
            "SubloreLine"
        );
        assert_eq!(
            [
                offset_of!(SubloreProposal, kind),
                offset_of!(SubloreProposal, revision),
                offset_of!(SubloreProposal, cue),
                offset_of!(SubloreProposal, text),
            ],
            [0, 8, 16, 24],
            "SubloreProposal"
        );
        assert_eq!(
            [
                offset_of!(SubloreValue, kind),
                offset_of!(SubloreValue, i),
                offset_of!(SubloreValue, f),
                offset_of!(SubloreValue, s),
            ],
            [0, 8, 16, 24],
            "SubloreValue"
        );
        assert_eq!(
            [
                offset_of!(SubloreItem, id),
                offset_of!(SubloreItem, kind),
                offset_of!(SubloreItem, parent),
                offset_of!(SubloreItem, enable_when),
                offset_of!(SubloreItem, flags),
                offset_of!(SubloreItem, label),
                offset_of!(SubloreItem, icon),
            ],
            [0, 4, 8, 12, 16, 24, 40],
            "SubloreItem"
        );
        assert_eq!(
            [
                offset_of!(SubloreCell, kind),
                offset_of!(SubloreCell, text),
                offset_of!(SubloreCell, number),
                offset_of!(SubloreCell, r#ref),
            ],
            [0, 8, 24, 32],
            "SubloreCell"
        );
        assert_eq!(
            [
                offset_of!(SubloreInvocation, revision),
                offset_of!(SubloreInvocation, cue),
                offset_of!(SubloreInvocation, row),
                offset_of!(SubloreInvocation, panel_id),
                offset_of!(SubloreInvocation, project_key),
            ],
            [0, 8, 16, 24, 32],
            "SubloreInvocation"
        );
        assert_eq!(
            [
                offset_of!(SubloreModule, size),
                offset_of!(SubloreModule, minor),
                offset_of!(SubloreModule, create),
                offset_of!(SubloreModule, destroy),
                offset_of!(SubloreModule, describe),
                offset_of!(SubloreModule, project_opened),
                offset_of!(SubloreModule, project_closing),
                offset_of!(SubloreModule, schema_version),
                offset_of!(SubloreModule, schema_upgrade),
                offset_of!(SubloreModule, invoke),
            ],
            [0, 4, 8, 16, 24, 32, 40, 48, 56, 64],
            "SubloreModule"
        );
        assert_eq!(
            [
                offset_of!(SubloreHost, size),
                offset_of!(SubloreHost, minor),
                offset_of!(SubloreHost, ctx),
                offset_of!(SubloreHost, log),
                offset_of!(SubloreHost, should_cancel),
                offset_of!(SubloreHost, progress),
                offset_of!(SubloreHost, document),
                offset_of!(SubloreHost, cue_at),
                offset_of!(SubloreHost, for_each_line),
                offset_of!(SubloreHost, propose),
                offset_of!(SubloreHost, find),
                offset_of!(SubloreHost, db_run),
                offset_of!(SubloreHost, db_transaction),
                offset_of!(SubloreHost, panel_begin),
                offset_of!(SubloreHost, panel_row),
                offset_of!(SubloreHost, panel_end),
                offset_of!(SubloreHost, status),
            ],
            [0, 4, 8, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120],
            "SubloreHost"
        );
    }

    /// A nullable slot must not cost a word: `Option` around a function pointer is the null the
    /// other side may leave, not a tag beside it.
    #[test]
    fn a_nullable_slot_is_one_pointer_wide() {
        assert_eq!(size_of::<SubloreLineFn>(), size_of::<*const c_void>());
        assert!(SubloreLineFn::None.is_none());
    }

    #[test]
    fn each_table_declares_the_size_it_was_compiled_at() {
        assert_eq!(SUBLORE_HOST_SIZE as usize, size_of::<SubloreHost>());
        assert_eq!(SUBLORE_MODULE_SIZE as usize, size_of::<SubloreModule>());
    }

    #[test]
    fn the_version_splits_into_the_numbers_it_was_built_from() {
        assert_eq!(
            abi_parts(SUBLORE_ABI_VERSION),
            (SUBLORE_ABI_MAJOR, SUBLORE_ABI_MINOR)
        );
    }

    #[test]
    fn a_symbol_name_ends_where_dlsym_expects() {
        assert_eq!(SUBLORE_ABI_SYMBOL.last(), Some(&0));
        assert_eq!(SUBLORE_LOAD_SYMBOL.last(), Some(&0));
        assert_eq!(SUBLORE_ABI_SYMBOL.iter().filter(|b| **b == 0).count(), 1);
        assert_eq!(SUBLORE_LOAD_SYMBOL.iter().filter(|b| **b == 0).count(), 1);
    }

    #[test]
    fn a_borrowed_string_reads_back_byte_for_byte() {
        let text = "Ceci n'est pas une pipe — 日本語";
        let borrowed = SubloreStr::borrowed(text);
        assert_eq!(borrowed.len, text.len());
        assert_eq!(unsafe { borrowed.as_str() }, Ok(text));
    }

    #[test]
    fn an_empty_string_survives_a_null_pointer() {
        let empty = SubloreStr {
            ptr: core::ptr::null(),
            len: 0,
        };
        assert_eq!(unsafe { empty.as_str() }, Ok(""));
        assert_eq!(unsafe { SubloreStr::borrowed("").as_str() }, Ok(""));
    }

    #[test]
    fn a_null_pointer_with_a_length_is_refused() {
        let malformed = SubloreStr {
            ptr: core::ptr::null(),
            len: 7,
        };
        assert_eq!(unsafe { malformed.as_str() }, Err(SUBLORE_ERR_BAD_STRING));
    }

    #[test]
    fn bytes_that_are_not_utf8_are_refused() {
        let bytes: [u8; 4] = [b'a', 0xC3, 0x28, b'b'];
        let borrowed = SubloreStr {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        };
        assert_eq!(unsafe { borrowed.as_str() }, Err(SUBLORE_ERR_BAD_STRING));
    }

    /// The reason the type is a pointer and a length: a NUL past the 1024 bytes `SourceText`
    /// scans reaches a cue's text, and a C string would hand over the half before it.
    #[test]
    fn a_nul_in_the_middle_is_carried_whole() {
        let mut text = "x".repeat(2000);
        text.push('\0');
        text.push_str("the half a C string would drop");

        let borrowed = SubloreStr::borrowed(&text);
        let read = unsafe { borrowed.as_str() }.expect("valid UTF-8, NUL and all");
        assert_eq!(read, text);
        assert_eq!(read.len(), text.len());
        assert!(read.len() > 2000);
    }
}
