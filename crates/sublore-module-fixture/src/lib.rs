//! The module the loader is proved against, before any real one exists.
//!
//! It is not a paid module and holds nothing of one. It contributes one menu title with items
//! under it and one panel, which is what exercises both parent links: an item under a title is a
//! menu item, and an item under a panel is that panel's secondary row action. It calls back into
//! the host from inside `describe` and from every one of its own activations. Every slot neither
//! side has filled stays null, which the interface allows on purpose: a slot the other side left
//! empty is a refusal and never a jump, and both directions check before they call.
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
    SubloreCell, SubloreHost, SubloreInvocation, SubloreItem, SubloreItemFn, SubloreModule,
    SubloreProposal, SubloreStr, SUBLORE_ABI_VERSION, SUBLORE_CELL_NUMBER, SUBLORE_CELL_PERCENT,
    SUBLORE_CELL_TEXT, SUBLORE_ENABLE_ALWAYS, SUBLORE_ERR_BAD_STRING, SUBLORE_ERR_CANCELLED,
    SUBLORE_ERR_UNSUPPORTED, SUBLORE_FIND_SKIP_TAGS, SUBLORE_ITEM_MENU_ITEM,
    SUBLORE_ITEM_MENU_TITLE, SUBLORE_ITEM_PANEL, SUBLORE_LOG_INFO, SUBLORE_MODULE_SIZE, SUBLORE_OK,
    SUBLORE_PROPOSAL_SET_CUE_TEXT,
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

/// The title, and the items under it. Ids are the module's own and mean nothing to the core.
const TITLE_ID: u32 = 1;
const ITEM_ID: u32 = 2;
/// An item the host has to refuse, so that its allowlist is defended by something.
const REFUSED_ID: u32 = 3;
/// Proposes against a revision one behind the session's, which the host must refuse (section 9.6).
const STALE_ID: u32 = 4;
/// Writes a row into this module's own table, which is the only way to see storage from outside.
const STORE_ID: u32 = 5;
/// The panel, which is also the id a row's own activation arrives under (section 4.1).
const PANEL_ID: u32 = 6;
/// Fills the panel above with the two rows below.
const FILL_ID: u32 = 7;
/// A second action on a row. Its parent is the panel, which is what makes it the secondary one,
/// and it arrives under its own id with the same `row`.
const ROW_ACTION_ID: u32 = 8;
/// Long enough work to report a progress, say something, and be stopped part way through.
const LONG_ID: u32 = 9;

/// The two rows the panel is filled with, as a handle and the three cells beside it.
///
/// The second handle is 2^53 plus one. A `u64` that large does not survive JSON's number, so a row
/// that came back with the handle changed would fail here and nowhere else: this is the fixture for
/// the decimal string the handle crosses as.
const ROWS: [(u64, &str, i64, i64); 2] = [
    (1, "The first row", 11, 25),
    (9_007_199_254_740_993, "The second row", 22, 75),
];

/// How many steps the long item takes, and how long it waits between them.
///
/// The wait is here and not in a check: a person has to be able to reach the Stop button, and a
/// spec waits on what it can see rather than on a clock.
const LONG_STEPS: u64 = 40;
const LONG_STEP_MS: u64 = 250;

/// This module's own table, under the prefix the host derives from this file's name (section 4.7).
///
/// Every statement below is one statement with no semicolon in it, because `db_run` prepares one
/// and refuses trailing text.
const MAKE_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS m_fixture_notes (id INTEGER PRIMARY KEY, note TEXT NOT NULL)";
const ADD_NOTE: &str = "INSERT INTO m_fixture_notes (note) VALUES ('kept')";
const COUNT_NOTES: &str = "SELECT count(*) FROM m_fixture_notes";

/// What the fixture writes into the first cue. Its own words, so a check can tell it apart from
/// anything the core or the user could have put there.
const WROTE: &str = "The module wrote this line.";

/// What the fixture's own state is.
struct Fixture {
    /// The locale the host handed over, kept so a test can see the string survived the boundary.
    locale: String,
    /// The key the last `project_opened` carried.
    ///
    /// Kept because `project_closing` is handed none: the interface separates the two slots, so a
    /// module that wants to say which project is going has to remember which one arrived. Zero
    /// until one has, which is the same value the host carries for "no project is open".
    project_key: i64,
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
        project_opened: Some(project_opened),
        project_closing: Some(project_closing),
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
        project_key: 0,
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
    let action = format!("Rewrite the first line ({found} found)");

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
        SubloreItem {
            id: STALE_ID,
            kind: SUBLORE_ITEM_MENU_ITEM,
            parent: TITLE_ID,
            // A document, because a proposal with none is refused for the wrong reason and would
            // prove nothing about the revision check.
            enable_when: sublore_module_api::SUBLORE_ENABLE_DOCUMENT_OPEN,
            flags: 0,
            label: SubloreStr::borrowed("Rewrite from a stale revision"),
            icon: SubloreStr::borrowed(""),
        },
        SubloreItem {
            id: STORE_ID,
            kind: SUBLORE_ITEM_MENU_ITEM,
            parent: TITLE_ID,
            // A project, because the statements below run on the project's own database and there
            // is nothing to run them on without one.
            enable_when: sublore_module_api::SUBLORE_ENABLE_PROJECT_OPEN,
            flags: 0,
            label: SubloreStr::borrowed("Store a note"),
            icon: SubloreStr::borrowed(""),
        },
        SubloreItem {
            id: PANEL_ID,
            kind: SUBLORE_ITEM_PANEL,
            parent: 0,
            enable_when: SUBLORE_ENABLE_ALWAYS,
            // Not a layer. The core draws this panel under the grid, where it never covers the
            // video, so the bit would be claiming something that is not true of it (section 5.4).
            flags: 0,
            label: SubloreStr::borrowed("Fixture rows"),
            icon: SubloreStr::borrowed(""),
        },
        SubloreItem {
            id: FILL_ID,
            kind: SUBLORE_ITEM_MENU_ITEM,
            parent: TITLE_ID,
            enable_when: SUBLORE_ENABLE_ALWAYS,
            flags: 0,
            label: SubloreStr::borrowed("Fill the table"),
            icon: SubloreStr::borrowed(""),
        },
        // The parent is the panel and not the title, which is the whole of what makes this the
        // secondary row action: it is drawn on every row and never on the menu (section 4.1).
        SubloreItem {
            id: ROW_ACTION_ID,
            kind: SUBLORE_ITEM_MENU_ITEM,
            parent: PANEL_ID,
            enable_when: SUBLORE_ENABLE_ALWAYS,
            flags: 0,
            label: SubloreStr::borrowed("Mark"),
            icon: SubloreStr::borrowed(""),
        },
        SubloreItem {
            id: LONG_ID,
            kind: SUBLORE_ITEM_MENU_ITEM,
            parent: TITLE_ID,
            enable_when: SUBLORE_ENABLE_ALWAYS,
            flags: 0,
            label: SubloreStr::borrowed("Take a while"),
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
    let at = unsafe { &*where_ };
    // Every activation, whichever item it is: the key the host filled is what a check reads back,
    // and an item enabled with nothing open has to be able to say it was handed zero.
    unsafe {
        say(&format!(
            "invoked item {item_id} with key {}",
            at.project_key
        ))
    };
    match item_id {
        // The revision the gesture carried, which is the session's own.
        ITEM_ID => unsafe { rewrite_first_cue(at.revision) },
        // One behind it, which is what a module that read the document and then let the user edit
        // would be holding. The host has to refuse it and change nothing.
        STALE_ID => unsafe { rewrite_first_cue(at.revision.wrapping_sub(1)) },
        STORE_ID => unsafe { store_a_note() },
        FILL_ID => unsafe { fill_the_panel() },
        // A row of the panel was activated. The primary action is the panel's own id, so this is
        // both `item_id` and `panel_id`, and the handle is the one this module gave that row.
        PANEL_ID => unsafe { say_row("a row was activated", at.panel_id, at.row) },
        // The secondary one, under its own id and with the same handle.
        ROW_ACTION_ID => unsafe { say_row("a row action ran", ROW_ACTION_ID, at.row) },
        LONG_ID => unsafe { take_a_while() },
        // An id this module never contributed.
        _ => SUBLORE_ERR_UNSUPPORTED,
    }
}

/// The project in the slot has appeared. Say which one, so a check can see the module's side.
///
/// # Safety
/// `ctx` is what `create` wrote, and the call is the host's own.
unsafe extern "C" fn project_opened(ctx: *mut c_void, project_key: i64) -> i32 {
    if ctx.is_null() {
        return SUBLORE_ERR_BAD_STRING;
    }
    let fixture = unsafe { &mut *ctx.cast::<Fixture>() };
    fixture.project_key = project_key;
    unsafe { say(&format!("a project opened, key {project_key}")) }
}

/// The project in the slot is about to go, and it is still there while this runs.
///
/// The key is this module's own memory of the open edge, because the slot is handed none.
///
/// # Safety
/// `ctx` is what `create` wrote, and the call is the host's own.
unsafe extern "C" fn project_closing(ctx: *mut c_void) -> i32 {
    if ctx.is_null() {
        return SUBLORE_ERR_BAD_STRING;
    }
    let fixture = unsafe { &mut *ctx.cast::<Fixture>() };
    let key = fixture.project_key;
    fixture.project_key = 0;
    unsafe { say(&format!("a project is closing, key {key}")) }
}

/// Put one line in the host's log, which is the only thing a check outside this process can read.
///
/// # Safety
/// Called only from inside a host call, with the table `load` was given.
unsafe fn say(line: &str) -> i32 {
    let table = HOST.load(Ordering::Acquire);
    if table.is_null() {
        return SUBLORE_ERR_UNSUPPORTED;
    }
    let table = unsafe { &*table };
    let Some(log) = table.log else {
        return SUBLORE_ERR_UNSUPPORTED;
    };
    unsafe { log(table.ctx, SUBLORE_LOG_INFO, SubloreStr::borrowed(line)) };
    SUBLORE_OK
}

/// Put the two rows above into the panel, in one run.
///
/// # Safety
/// Called only from inside a host call, with the table `load` was given.
unsafe fn fill_the_panel() -> i32 {
    let table = HOST.load(Ordering::Acquire);
    if table.is_null() {
        return SUBLORE_ERR_UNSUPPORTED;
    }
    let table = unsafe { &*table };
    let (Some(begin), Some(push), Some(end)) =
        (table.panel_begin, table.panel_row, table.panel_end)
    else {
        return SUBLORE_ERR_UNSUPPORTED;
    };

    let code = unsafe { begin(table.ctx, PANEL_ID) };
    if code != SUBLORE_OK {
        return code;
    }
    for (handle, text, number, percent) in ROWS {
        let cells = [
            SubloreCell {
                kind: SUBLORE_CELL_TEXT,
                text: SubloreStr::borrowed(text),
                number: 0,
                r#ref: handle,
            },
            SubloreCell {
                kind: SUBLORE_CELL_NUMBER,
                text: SubloreStr::borrowed(""),
                number,
                r#ref: handle,
            },
            SubloreCell {
                kind: SUBLORE_CELL_PERCENT,
                text: SubloreStr::borrowed(""),
                number: percent,
                r#ref: handle,
            },
        ];
        // Safety: the cells and every string in them outlive this call, which is the whole of what
        // section 2.1 asks of them.
        let code = unsafe { push(table.ctx, cells.as_ptr(), cells.len()) };
        if code != SUBLORE_OK {
            return code;
        }
    }
    // Closed, which is this module asserting the table is whole. A run left open publishes nothing.
    unsafe { end(table.ctx) }
}

/// Log what a row activation carried, which is the evidence from outside that it arrived intact.
///
/// # Safety
/// Called only from inside a host call, with the table `load` was given.
unsafe fn say_row(what: &str, item: u32, row: u64) -> i32 {
    unsafe { say(&format!("{what}: item {item} and row {row}")) }
}

/// Work long enough to be watched and stopped: a progress and a line per step, and a question.
///
/// # Safety
/// Called only from inside a host call, with the table `load` was given.
unsafe fn take_a_while() -> i32 {
    let table = HOST.load(Ordering::Acquire);
    if table.is_null() {
        return SUBLORE_ERR_UNSUPPORTED;
    }
    let table = unsafe { &*table };
    let (Some(log), Some(status), Some(progress), Some(should_cancel)) =
        (table.log, table.status, table.progress, table.should_cancel)
    else {
        return SUBLORE_ERR_UNSUPPORTED;
    };

    for step in 1..=LONG_STEPS {
        // Asked before the step is done, so the step the log names is the one it stopped at.
        if unsafe { should_cancel(table.ctx) } != 0 {
            let said = format!("stopped at step {step} of {LONG_STEPS}");
            unsafe { log(table.ctx, SUBLORE_LOG_INFO, SubloreStr::borrowed(&said)) };
            return SUBLORE_ERR_CANCELLED;
        }
        unsafe { progress(table.ctx, step, LONG_STEPS) };
        let line = format!("step {step} of {LONG_STEPS}");
        unsafe { status(table.ctx, SubloreStr::borrowed(&line)) };
        std::thread::sleep(std::time::Duration::from_millis(LONG_STEP_MS));
    }
    let said = format!("finished all {LONG_STEPS} steps");
    unsafe { log(table.ctx, SUBLORE_LOG_INFO, SubloreStr::borrowed(&said)) };
    SUBLORE_OK
}

/// What one row of a counting statement said.
struct Counted {
    value: i64,
    /// Whether a row arrived at all, so a count of zero is told from a walk that pushed nothing.
    pushed: bool,
}

/// # Safety
/// Called by the host with the sink this module handed it and cells valid for the call.
unsafe extern "C" fn take_count(
    sink: *mut c_void,
    cells: *const sublore_module_api::SubloreValue,
    cell_count: usize,
) -> i32 {
    if sink.is_null() || cells.is_null() || cell_count == 0 {
        return SUBLORE_ERR_BAD_STRING;
    }
    let counted = unsafe { &mut *sink.cast::<Counted>() };
    let first = unsafe { &*cells };
    if first.kind != sublore_module_api::SUBLORE_VALUE_INT {
        return SUBLORE_ERR_UNSUPPORTED;
    }
    counted.value = first.i;
    counted.pushed = true;
    SUBLORE_OK
}

/// Make this module's own table if it is not there, add one row, and log how many it now holds.
///
/// All three inside one transaction, so a run that failed halfway leaves the table as it was and
/// the count in the log is a count of committed rows.
///
/// # Safety
/// Called only from inside a host call, with the table `load` was given.
unsafe fn store_a_note() -> i32 {
    let table = HOST.load(Ordering::Acquire);
    if table.is_null() {
        return SUBLORE_ERR_UNSUPPORTED;
    }
    let table = unsafe { &*table };
    // `db_run` is read inside the transaction's body rather than here, because that is where it is
    // called and a slot read early is a slot that may have been read for nothing.
    let (Some(transaction), Some(log)) = (table.db_transaction, table.log) else {
        return SUBLORE_ERR_UNSUPPORTED;
    };

    let mut counted = Counted {
        value: 0,
        pushed: false,
    };
    // Safety: the module's own function, and a context that outlives the call it is handed to.
    let code = unsafe {
        transaction(
            table.ctx,
            (&mut counted as *mut Counted).cast(),
            Some(write_the_note),
        )
    };
    if code != SUBLORE_OK {
        return code;
    }
    if !counted.pushed {
        return SUBLORE_ERR_UNSUPPORTED;
    }
    // The evidence from outside that the row is in the file: a count that grows across a close and
    // a reopen is a row that was written and read back.
    let said = format!("stored a note, and the table now holds {}", counted.value);
    unsafe { log(table.ctx, SUBLORE_LOG_INFO, SubloreStr::borrowed(&said)) };
    SUBLORE_OK
}

/// The body of the transaction above: make, insert, count.
///
/// # Safety
/// Called by the host from inside `db_transaction`, with the context this module gave it.
unsafe extern "C" fn write_the_note(work_ctx: *mut c_void) -> i32 {
    if work_ctx.is_null() {
        return SUBLORE_ERR_BAD_STRING;
    }
    let table = HOST.load(Ordering::Acquire);
    if table.is_null() {
        return SUBLORE_ERR_UNSUPPORTED;
    }
    let table = unsafe { &*table };
    let Some(run) = table.db_run else {
        return SUBLORE_ERR_UNSUPPORTED;
    };
    for sql in [MAKE_TABLE, ADD_NOTE] {
        // Safety: one statement, no parameters, and no sink: neither returns a row.
        let code = unsafe {
            run(
                table.ctx,
                SubloreStr::borrowed(sql),
                core::ptr::null(),
                0,
                core::ptr::null_mut(),
                None,
            )
        };
        if code != SUBLORE_OK {
            return code;
        }
    }
    // Safety: the sink is the `Counted` the caller owns for the length of this call.
    unsafe {
        run(
            table.ctx,
            SubloreStr::borrowed(COUNT_NOTES),
            core::ptr::null(),
            0,
            work_ctx,
            Some(take_count),
        )
    }
}

/// Ask the host to put [`WROTE`] in the first cue, at the revision given.
///
/// # Safety
/// Called only from inside a host call, with the table `load` was given.
unsafe fn rewrite_first_cue(revision: u64) -> i32 {
    let table = HOST.load(Ordering::Acquire);
    if table.is_null() {
        return SUBLORE_ERR_UNSUPPORTED;
    }
    let table = unsafe { &*table };
    let Some(propose) = table.propose else {
        return SUBLORE_ERR_UNSUPPORTED;
    };
    let asked = SubloreProposal {
        kind: SUBLORE_PROPOSAL_SET_CUE_TEXT,
        revision,
        cue: 0,
        text: SubloreStr::borrowed(WROTE),
    };
    unsafe { propose(table.ctx, &asked) }
}
