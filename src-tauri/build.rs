fn main() {
    // Windows CI builds libmpv's import library into a temp dir; Linux finds libmpv on the
    // default search path. See BACKLOG.md M0.2.
    if let Ok(dir) = std::env::var("LIBMPV_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
    }
    println!("cargo:rerun-if-env-changed=LIBMPV_LIB_DIR");
    tauri_build::build()
}
