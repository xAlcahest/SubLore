fn main() {
    // Windows CI builds libmpv's import library into a temp dir; Linux finds libmpv on the
    // default search path. See BACKLOG.md M0.2.
    if let Ok(dir) = std::env::var("LIBMPV_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
    }
    println!("cargo:rerun-if-env-changed=LIBMPV_LIB_DIR");

    // Integration tests need the manifest tauri_build gives the app: without the Common-Controls 6
    // dependency, comctl32 resolves to the System32 5.82 that has no TaskDialogIndirect and the
    // binary dies at load with STATUS_ENTRYPOINT_NOT_FOUND. See BACKLOG.md M0.2.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-tests.manifest");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }

    tauri_build::build()
}
