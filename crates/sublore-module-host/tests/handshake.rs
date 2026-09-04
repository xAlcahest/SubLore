//! Criteria 1 to 4 of `module-abi.md` §9, driven against the five fixture artifacts.
//!
//! Every check builds the directory it reads: the loader takes a directory, so a check that
//! inherited one would be asserting about whatever the last one left. The fixtures are copied in
//! under the name a module ships as, which is not the name cargo produces: cargo writes
//! `libsublore_module_x.so` and §3.4's pattern is `sublore_module_*.so`, because the pattern is
//! about what we ship beside the executable.
//!
//! The host table handed over is empty. Nothing here calls back into it: the module's own `create`
//! and `describe` are what run, and the calls that would reach the host arrive with N8e.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sublore_module_api::{
    SubloreHost, SubloreItem, SUBLORE_ABI_MINOR, SUBLORE_ENABLE_ALWAYS, SUBLORE_HOST_SIZE,
    SUBLORE_ITEM_MENU_ITEM, SUBLORE_ITEM_MENU_TITLE, SUBLORE_OK,
};
use sublore_module_host::{scan, Refusal};

/// A directory of this test's own, under the OS temp dir. Same shape as `sublore-io`'s tests, and
/// for the same reason: no dev-dependency.
fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!(
        "sublore-module-host-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// Where cargo left this test's own binary, which is where it left the fixtures too.
fn build_dir() -> PathBuf {
    let mut here = env::current_exe().expect("the test binary knows where it is");
    here.pop();
    // `target/debug/deps` for the test itself; the lib is one up and the examples are beside it.
    if here.ends_with("deps") {
        here.pop();
    }
    here
}

/// Copy one built fixture into `directory` under the name a module ships as.
///
/// The artifact is checked against its own sources first. `cargo test -p sublore-module-host`
/// rebuilds neither the fixture package's examples nor a `cdylib` lib, measured on 2026-09-04, so
/// without this every check here would read whatever an earlier build left and would pass on bytes
/// nobody had compiled. It cost one mutation that reported the wrong reason before it was found.
fn install(directory: &Path, artifact: &str, as_name: &str) {
    let source = build_dir()
        .join("examples")
        .join(format!("lib{artifact}.so"));
    assert!(
        source.is_file(),
        "{artifact} was not built. Every fixture is an example target, so `cargo build -p \
         sublore-module-fixture --examples` produces it and `cargo test --workspace` keeps it \
         fresh. Looked for {source:?}"
    );
    assert_fresh(artifact, &source);
    fs::copy(&source, directory.join(format!("{as_name}.so")))
        .expect("the fixture should be copyable into the scratch directory");
}

/// Refuse an artifact older than a source that actually goes into it.
///
/// Its own example file, plus everything under `src` and the manifest, which the example links
/// against. Deliberately not every sibling example: touching one of those would make all five look
/// stale and the message would name a file the artifact has nothing to do with.
fn assert_fresh(artifact: &str, built_file: &Path) {
    let built = fs::metadata(built_file)
        .and_then(|meta| meta.modified())
        .expect("a file that exists has a modification time");
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .join("sublore-module-fixture");

    // Its own example file and the crate it links. Not the manifest: adding a target there does
    // not invalidate the other artifacts, and cargo rightly does not rebuild them, so a guard that
    // demanded it would ask for something that can never happen. A manifest change that does
    // matter, a new dependency, changes the compilation and cargo rebuilds on its own.
    let mut sources = sources_under(&crate_dir.join("src"));
    sources.push(crate_dir.join("examples").join(format!("{artifact}.rs")));

    for source in sources {
        let Ok(touched) = fs::metadata(&source).and_then(|meta| meta.modified()) else {
            continue;
        };
        assert!(
            touched <= built,
            "{built_file:?} is older than {source:?}. The gate is `cargo test --workspace`, which \
             rebuilds example targets; a narrower `-p` run does not, and reading a stale artifact \
             is how this file once passed against code that had been rewritten."
        );
    }
}

/// Every `.rs` and `.toml` under a directory, one level of nesting being all this needs.
fn sources_under(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(sources_under(&path));
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "rs" || extension == "toml")
        {
            found.push(path);
        }
    }
    found
}

/// An empty host table, sized as this build compiled it.
fn host() -> SubloreHost {
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

#[test]
fn a_directory_with_no_module_in_it_is_silence() {
    let dir = scratch("empty");
    // A file that is not one of ours, so the check is that the pattern refuses it rather than that
    // the directory happened to be bare.
    fs::write(dir.join("libmpv.so.2"), b"not a module").expect("writable");
    fs::write(dir.join("sublore_module_notes.txt"), b"not a library").expect("writable");

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert!(found.is_empty(), "nothing found and nothing refused");
    assert!(found.loaded.is_empty());
    assert!(found.refused.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_good_fixture_loads_and_describes_what_it_contributes() {
    let dir = scratch("good");
    install(&dir, "sublore_module_fixture", "sublore_module_fixture");

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert!(found.refused.is_empty(), "refused {:?}", found.refused);
    assert_eq!(found.loaded.len(), 1);
    let module = &found.loaded[0];
    assert_eq!(module.minor(), SUBLORE_ABI_MINOR);

    // Drive the module's own two calls, which is what says the table it filled is callable and not
    // merely present.
    let table = module.table();
    let create = table.create.expect("the fixture fills create");
    let describe = table.describe.expect("the fixture fills describe");
    let destroy = table.destroy.expect("the fixture fills destroy");

    let mut ctx: *mut std::ffi::c_void = std::ptr::null_mut();
    let made = unsafe {
        create(
            &mut ctx,
            sublore_module_api::SubloreStr::borrowed("/nowhere"),
            sublore_module_api::SubloreStr::borrowed("en-GB"),
        )
    };
    assert_eq!(made, SUBLORE_OK);
    assert!(!ctx.is_null());

    let mut items: Vec<Described> = Vec::new();
    let answer = unsafe {
        describe(
            ctx,
            (&mut items as *mut Vec<Described>).cast(),
            Some(collect),
        )
    };
    assert_eq!(answer, SUBLORE_OK);

    // A title, one item under it, and one the host is meant to refuse. This sink is not the host
    // and accepts everything, which is what lets it see the third one at all.
    assert_eq!(items.len(), 3, "a title and two items: {items:?}");
    assert_eq!(items[0].kind, SUBLORE_ITEM_MENU_TITLE);
    assert_eq!(items[0].parent, 0, "the title is top level");
    assert_eq!(items[1].kind, SUBLORE_ITEM_MENU_ITEM);
    assert_eq!(items[1].parent, items[0].id, "the item hangs off the title");
    assert_eq!(items[1].enable_when, SUBLORE_ENABLE_ALWAYS);
    // The third is the fixture's own trap, and the zero is what makes it one: §5.2 has no value
    // for it, so a host that draws that item has stopped checking.
    assert_eq!(
        items[2].enable_when, 0,
        "the refusable item must carry a state with no meaning"
    );
    // The locale went in through `create` and came back out inside a label, which is the only
    // evidence from this side that the string crossed the boundary intact.
    assert!(
        items[0].label.contains("en-GB"),
        "label was {:?}",
        items[0].label
    );

    unsafe { destroy(ctx) };
    fs::remove_dir_all(&dir).ok();
}

/// One item as this side received it.
///
/// A named type rather than a tuple, and that is not taste. The sink and its caller meet through a
/// `*mut c_void`, so nothing checks that the two agree about what is on the other end: they once
/// disagreed by one field here, and the reinterpretation read a different word of the vector and
/// produced a number that changed between runs. A name is what makes the compiler check it.
#[derive(Debug)]
struct Described {
    id: u32,
    kind: u32,
    parent: u32,
    enable_when: u32,
    label: String,
}

/// A sink for `describe`, collecting what a module pushes.
///
/// # Safety
/// Called by the module with the pointer this test handed it and one item per call.
unsafe extern "C" fn collect(sink: *mut std::ffi::c_void, item: *const SubloreItem) -> i32 {
    let items = unsafe { &mut *sink.cast::<Vec<Described>>() };
    let item = unsafe { &*item };
    let label = unsafe { item.label.as_str() }
        .unwrap_or("<not a string>")
        .to_owned();
    items.push(Described {
        id: item.id,
        kind: item.kind,
        parent: item.parent,
        enable_when: item.enable_when,
        label,
    });
    SUBLORE_OK
}

#[test]
fn a_module_built_for_another_major_is_refused_and_both_numbers_are_named() {
    let dir = scratch("major");
    install(
        &dir,
        "sublore_module_wrong_major",
        "sublore_module_wrong_major",
    );

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert!(found.loaded.is_empty());
    assert_eq!(found.refused.len(), 1);
    match &found.refused[0].1 {
        Refusal::MajorDiffers { ours, theirs } => {
            assert_eq!(*ours, sublore_module_api::SUBLORE_ABI_MAJOR);
            assert_eq!(*theirs, sublore_module_api::SUBLORE_ABI_MAJOR + 1);
        }
        other => panic!("expected a major mismatch, got {other:?}"),
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_that_needs_a_newer_minor_is_refused_and_the_other_direction_is_not() {
    let dir = scratch("minor");
    install(
        &dir,
        "sublore_module_wrong_minor",
        "sublore_module_wrong_minor",
    );

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert!(found.loaded.is_empty());
    assert_eq!(found.refused.len(), 1);
    match &found.refused[0].1 {
        Refusal::MinorTooNew { ours, theirs } => {
            assert_eq!(*ours, SUBLORE_ABI_MINOR);
            assert_eq!(*theirs, SUBLORE_ABI_MINOR + 1);
        }
        other => panic!("expected a minor mismatch, got {other:?}"),
    }

    // The other direction is the whole point of the rule being asymmetric: the good fixture reports
    // exactly the host's minor and loads, and a host ahead of a module would load it too.
    let same = scratch("minor-other-way");
    install(&same, "sublore_module_fixture", "sublore_module_fixture");
    let found = unsafe { scan(&same, &host) };
    assert_eq!(found.loaded.len(), 1);
    assert!(found.refused.is_empty());

    fs::remove_dir_all(&dir).ok();
    fs::remove_dir_all(&same).ok();
}

#[test]
fn a_module_whose_two_numbers_disagree_is_refused_by_the_handshake() {
    // The loader checks the minor twice, at the handshake and again on the table it was handed.
    // This module passes the second and must not pass the first, which makes it the only shape
    // that holds the handshake check up on its own.
    let dir = scratch("lying");
    install(
        &dir,
        "sublore_module_lying_minor",
        "sublore_module_lying_minor",
    );

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert!(
        found.loaded.is_empty(),
        "a module that claimed a revision this host does not have was loaded anyway"
    );
    assert_eq!(found.refused.len(), 1);
    match &found.refused[0].1 {
        Refusal::MinorTooNew { ours, theirs } => {
            assert_eq!(*ours, SUBLORE_ABI_MINOR);
            assert_eq!(*theirs, SUBLORE_ABI_MINOR + 1);
        }
        other => panic!("expected the handshake to refuse it, got {other:?}"),
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_that_lies_in_its_table_is_refused_after_the_handshake() {
    // Honest at the handshake and wrong in the table, which is the only shape the second minor
    // check can refuse on its own. Without it that check is defence nothing reaches.
    let dir = scratch("lyingtable");
    install(
        &dir,
        "sublore_module_lying_table_minor",
        "sublore_module_lying_table_minor",
    );

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert!(
        found.loaded.is_empty(),
        "the table's own claim went unchecked"
    );
    assert_eq!(found.refused.len(), 1);
    match &found.refused[0].1 {
        Refusal::MinorTooNew { ours, theirs } => {
            assert_eq!(*ours, SUBLORE_ABI_MINOR);
            assert_eq!(*theirs, SUBLORE_ABI_MINOR + 1);
        }
        other => panic!("expected the table check to refuse it, got {other:?}"),
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_table_that_is_not_the_size_this_build_compiles_to_is_refused() {
    let dir = scratch("lyingsize");
    install(
        &dir,
        "sublore_module_lying_table_size",
        "sublore_module_lying_table_size",
    );

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert!(found.loaded.is_empty());
    assert_eq!(found.refused.len(), 1);
    match &found.refused[0].1 {
        Refusal::TableSize { ours, theirs } => {
            assert_eq!(*ours, sublore_module_api::SUBLORE_MODULE_SIZE);
            assert_eq!(*theirs, sublore_module_api::SUBLORE_MODULE_SIZE + 8);
        }
        other => panic!("expected a table size mismatch, got {other:?}"),
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_library_without_the_handshake_symbol_is_not_a_module() {
    let dir = scratch("nosymbol");
    install(
        &dir,
        "sublore_module_no_abi_symbol",
        "sublore_module_no_abi_symbol",
    );

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert!(found.loaded.is_empty());
    assert_eq!(found.refused.len(), 1);
    assert_eq!(found.refused[0].1, Refusal::NotAModule);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_module_that_refuses_to_start_is_reported_with_its_own_code() {
    let dir = scratch("loadfails");
    install(
        &dir,
        "sublore_module_load_fails",
        "sublore_module_load_fails",
    );

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert!(found.loaded.is_empty());
    assert_eq!(found.refused.len(), 1);
    assert_eq!(
        found.refused[0].1,
        Refusal::LoadRefused(sublore_module_api::SUBLORE_ERR_STORAGE),
        "the module's own code, not one the loader invented"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn five_files_in_one_directory_load_one_and_refuse_four() {
    let dir = scratch("all");
    install(&dir, "sublore_module_fixture", "sublore_module_fixture");
    install(
        &dir,
        "sublore_module_wrong_major",
        "sublore_module_wrong_major",
    );
    install(
        &dir,
        "sublore_module_wrong_minor",
        "sublore_module_wrong_minor",
    );
    install(
        &dir,
        "sublore_module_no_abi_symbol",
        "sublore_module_no_abi_symbol",
    );
    install(
        &dir,
        "sublore_module_load_fails",
        "sublore_module_load_fails",
    );

    let host = host();
    let found = unsafe { scan(&dir, &host) };

    assert_eq!(found.loaded.len(), 1, "only the good one");
    assert!(found.loaded[0]
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "sublore_module_fixture.so"));
    assert_eq!(found.refused.len(), 4);

    // Each one by its reason, not merely by the count: a rule flipped in the loader can leave the
    // count right and every reason wrong, and a mutation did exactly that on 2026-09-04.
    let reasons: Vec<(&str, &Refusal)> = found
        .refused
        .iter()
        .map(|(path, why)| {
            (
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default(),
                why,
            )
        })
        .collect();
    for (name, why) in &reasons {
        let expected_kind = match *name {
            "sublore_module_load_fails.so" => "load",
            "sublore_module_no_abi_symbol.so" => "notmodule",
            "sublore_module_wrong_major.so" => "major",
            "sublore_module_wrong_minor.so" => "minor",
            other => panic!("an unexpected file was refused: {other}"),
        };
        let actual_kind = match why {
            Refusal::LoadRefused(_) => "load",
            Refusal::NotAModule => "notmodule",
            Refusal::MajorDiffers { .. } => "major",
            Refusal::MinorTooNew { .. } => "minor",
            other => panic!("{name} was refused as {other:?}"),
        };
        assert_eq!(actual_kind, expected_kind, "{name} was refused as {why:?}");
    }

    // Sorted order, so a run reports the same thing twice and the four are where they are named.
    let names: Vec<String> = found
        .refused
        .iter()
        .map(|(path, _)| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(
        names, sorted,
        "the scan walks the directory in sorted order"
    );
    fs::remove_dir_all(&dir).ok();
}
