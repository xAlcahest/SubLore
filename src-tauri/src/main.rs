// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// WebKitGTK's DMABUF renderer cannot allocate a buffer on the NVIDIA proprietary driver: the
/// webview paints nothing at all and the window opens blank, with "Failed to create GBM buffer" on
/// stderr. Tauri documents the escalation in its Linux graphics issues guide.
///
/// Both steps are applied before the webview exists, because neither takes effect afterwards.
/// Measured on an RTX 5070 Ti with driver 610.57.04, capturing the window and reading its luma
/// range: with nothing set, and with `__NV_DISABLE_EXPLICIT_SYNC` alone, the window is flat at
/// 46..46 and the GBM error is printed both times. Only with the DMABUF renderer off does the
/// interface appear, at 16..235. The first variable is kept anyway: it costs nothing and it is the
/// step upstream expects to be enough on other driver versions.
#[cfg(target_os = "linux")]
fn mitigate_nvidia_webview() {
    // An escape hatch, for two kinds of user. Someone on a driver these workarounds hurt rather
    // than help can turn them off without rebuilding, which is what any driver workaround owes its
    // users; and the E2E harness sets it, because under Xvfb the renderer is llvmpipe and the
    // NVIDIA module being loaded says nothing about what is drawing.
    if std::env::var("SUBLORE_WEBKIT_WORKAROUNDS").as_deref() == Ok("0") {
        return;
    }
    std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
    // Reading the module list rather than probing GL: no subprocess, no GL context, and it answers
    // before anything has been drawn.
    if std::path::Path::new("/sys/module/nvidia").exists() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

// Startup errors propagate out of main so a failed launch is reported, never silently swallowed.
fn main() -> tauri::Result<()> {
    // libmpv's --wid needs an X11 window id and the Wayland VO has no equivalent, so force X11
    // before GTK picks its backend. See BACKLOG.md M0.2.
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("GDK_BACKEND", "x11");
        mitigate_nvidia_webview();
    }

    sublore_lib::run()
}
