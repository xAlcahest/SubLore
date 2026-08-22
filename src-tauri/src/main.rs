// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Startup errors propagate out of main so a failed launch is reported, never silently swallowed.
fn main() -> tauri::Result<()> {
    tauri::Builder::default().run(tauri::generate_context!())
}
