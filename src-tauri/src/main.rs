// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Startup errors propagate out of main so a failed launch is reported, never silently swallowed.
fn main() -> tauri::Result<()> {
    // libmpv's --wid needs an X11 window id and the Wayland VO has no equivalent, so force X11
    // before GTK picks its backend. See BACKLOG.md M0.2.
    #[cfg(target_os = "linux")]
    std::env::set_var("GDK_BACKEND", "x11");

    sublore_lib::run()
}
