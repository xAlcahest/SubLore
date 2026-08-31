// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use std::borrow::Cow;
#[cfg(target_os = "linux")]
use std::ffi::OsStr;

#[cfg(target_os = "linux")]
const WEBKIT_HATCH: &str = "SUBLORE_WEBKIT_WORKAROUNDS";

/// Should the NVIDIA WebKit workarounds be applied? `hatch` is `WEBKIT_HATCH`: `0/false/no/off`
/// disarms them, `1/true/yes/on` forces them on where the probe cannot see the driver. Every other
/// value, the empty one included, counts as unset and leaves `nvidia_module` deciding.
#[cfg(target_os = "linux")]
fn nvidia_workarounds_wanted(hatch: Option<&str>, nvidia_module: bool) -> bool {
    match hatch
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("0" | "false" | "no" | "off") => false,
        Some("1" | "true" | "yes" | "on") => true,
        _ => nvidia_module,
    }
}

/// The same decision from the environment's own bytes. A value Rust cannot decode matches no
/// keyword, so the probe decides — it never reads as one of the words by accident.
#[cfg(target_os = "linux")]
fn hatch_decision(hatch: Option<&OsStr>, nvidia_module: bool) -> bool {
    nvidia_workarounds_wanted(hatch.and_then(OsStr::to_str), nvidia_module)
}

/// How the hatch reads in the startup line. `unset` means absent from the environment: a value that
/// is not UTF-8 is printed lossily, the way `startup_files` names an argument it cannot decode.
#[cfg(target_os = "linux")]
fn hatch_report(hatch: Option<&OsStr>) -> Cow<'_, str> {
    match hatch {
        None => Cow::Borrowed("unset"),
        Some(value) => value.to_string_lossy(),
    }
}

/// WebKitGTK's DMABUF renderer cannot allocate a buffer on the NVIDIA proprietary driver and the
/// window opens blank. Both variables are applied before the webview exists; neither takes effect
/// afterwards.
#[cfg(target_os = "linux")]
fn mitigate_nvidia_webview() {
    // `var_os`, not `var`: a variable holding bytes that are not UTF-8 is set, and the line below
    // has to say so rather than print it as absent.
    let hatch = std::env::var_os(WEBKIT_HATCH);
    // The module being loaded is a proxy for "NVIDIA is drawing", not the same question: on a
    // hybrid machine it is true while another GPU renders. The hatch is what such a user has.
    let nvidia_module = std::path::Path::new("/sys/module/nvidia").exists();
    let apply = hatch_decision(hatch.as_deref(), nvidia_module);
    if apply {
        std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    // The decision is made before the log plugin exists, so stderr is the only record of which
    // rendering path the app chose. e2e/scripts/webview-paint-check.js reads this line.
    eprintln!(
        "sublore: webview workarounds {} ({WEBKIT_HATCH}={}, /sys/module/nvidia {})",
        if apply { "applied" } else { "not applied" },
        hatch_report(hatch.as_deref()),
        if nvidia_module { "present" } else { "absent" },
    );
}

// Startup errors propagate out of main so a failed launch is reported, never silently swallowed.
fn main() -> tauri::Result<()> {
    // libmpv's --wid needs an X11 window id and the Wayland VO has no equivalent, so force X11
    // before GTK picks its backend. See BACKLOG.md M0.2.
    #[cfg(target_os = "linux")]
    {
        // Both writes run on the process's only thread, before anything is spawned and before GTK
        // reads the environment. Nothing that creates a thread may be added above them.
        std::env::set_var("GDK_BACKEND", "x11");
        mitigate_nvidia_webview();
    }

    sublore_lib::run()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{hatch_decision, hatch_report, nvidia_workarounds_wanted};
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn unset_hatch_leaves_the_module_probe_deciding() {
        assert!(nvidia_workarounds_wanted(None, true));
        assert!(!nvidia_workarounds_wanted(None, false));
    }

    #[test]
    fn the_hatch_disarms_them_where_the_driver_is_loaded() {
        for value in ["0", "false", "no", "off", "OFF", "False", " 0 "] {
            assert!(
                !nvidia_workarounds_wanted(Some(value), true),
                "{value} left the workarounds armed"
            );
        }
    }

    #[test]
    fn the_hatch_forces_them_on_where_the_probe_sees_nothing() {
        for value in ["1", "true", "yes", "on", "ON", "True"] {
            assert!(
                nvidia_workarounds_wanted(Some(value), false),
                "{value} did not force the workarounds on"
            );
        }
    }

    #[test]
    fn an_unrecognised_value_leaves_the_probe_in_charge() {
        for value in ["flase", "", "disabled"] {
            assert!(
                nvidia_workarounds_wanted(Some(value), true),
                "{value} disarmed the workarounds on a machine that needs them"
            );
            assert!(
                !nvidia_workarounds_wanted(Some(value), false),
                "{value} armed the workarounds on a machine with no NVIDIA module"
            );
        }
    }

    #[test]
    fn an_empty_value_decides_exactly_what_no_value_decides() {
        for module in [true, false] {
            assert_eq!(
                hatch_decision(Some(OsStr::new("")), module),
                hatch_decision(None, module),
                "an empty hatch and an absent one disagreed with the module {module}"
            );
            assert_eq!(
                hatch_decision(Some(OsStr::new(" ")), module),
                module,
                "a blank hatch took the decision away from the module probe"
            );
        }
    }

    #[test]
    fn a_value_that_is_not_utf8_leaves_the_probe_deciding() {
        let undecodable = OsStr::from_bytes(b"0\xff");
        assert!(hatch_decision(Some(undecodable), true));
        assert!(!hatch_decision(Some(undecodable), false));
    }

    #[test]
    fn the_startup_line_reports_a_set_hatch_as_set() {
        assert_eq!(hatch_report(None), "unset");
        assert_eq!(hatch_report(Some(OsStr::new(""))), "");
        assert_eq!(hatch_report(Some(OsStr::new("0"))), "0");
        let reported = hatch_report(Some(OsStr::from_bytes(b"0\xff")));
        assert_ne!(
            reported, "unset",
            "a variable that is set printed as absent"
        );
        assert!(
            reported.starts_with('0'),
            "the undecodable value printed as {reported:?}, losing what the user did set"
        );
    }
}
