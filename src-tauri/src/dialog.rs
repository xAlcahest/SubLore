//! The app's native message dialogs.
//!
//! On Linux these are built on GTK directly, on the main thread, instead of going through
//! tauri-plugin-dialog. The plugin uses rfd, which starts a second thread the first time any dialog
//! is shown and iterates GTK on it for the rest of the process's life, which GTK3 is not built for.
//! Removing that thread is worth doing on its own, and it lets the dialog be modal and transient
//! for the window it asks about, which rfd cannot be because it builds with a null parent.
//!
//! It is **not** the fix for N1b's exit crash. That crash survived this change, and a core from
//! this binary shows the same crashing frame with no rfd thread in the process
//! (`docs/reports/n1b-segfault-uscita.md`). Every other platform keeps the plugin.

use tauri::AppHandle;

/// What the user answered when asked about unsaved work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseAnswer {
    Save,
    Discard,
    Cancel,
}

/// Ask what to do with unsaved edits and deliver the answer exactly once.
///
/// Returns as soon as the dialog is on its way; the answer arrives later, on the main thread. An
/// error means nobody will ever be asked, so the caller has to keep the window open and say so.
#[cfg(target_os = "linux")]
pub fn ask_close<F>(app: &AppHandle, label: &str, answer: F) -> tauri::Result<()>
where
    F: FnOnce(CloseAnswer) + Send + 'static,
{
    use gtk::prelude::*;
    use tauri::Manager;

    let handle = app.clone();
    let label = label.to_owned();
    app.run_on_main_thread(move || {
        // Transient and modal, which is what the rfd dialog could not be: its GTK backend built the
        // dialog with a null parent, so on Linux it could end up behind the window it was asking
        // about (BACKLOG N1).
        let parent = handle
            .get_webview_window(&label)
            .and_then(|window| window.gtk_window().ok());
        let dialog = gtk::MessageDialog::new(
            parent.as_ref(),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            gtk::MessageType::Warning,
            gtk::ButtonsType::None,
            crate::strings::CLOSE_UNSAVED_BODY,
        );
        dialog.set_title(crate::strings::CLOSE_UNSAVED_TITLE);
        // Added left to right, so Cancel sits rightmost, where rfd put it and where
        // e2e/scripts/close-gate-check.js looks for it.
        dialog.add_button(crate::strings::CLOSE_SAVE, gtk::ResponseType::Yes);
        dialog.add_button(crate::strings::CLOSE_DISCARD, gtk::ResponseType::No);
        dialog.add_button(crate::strings::CLOSE_CANCEL, gtk::ResponseType::Cancel);

        // `connect_response` takes an `Fn` and the dialog can answer more than once — a button
        // press followed by the window manager closing it — while the answer may only be acted on
        // once, because acting on it destroys the window.
        let answer = std::cell::RefCell::new(Some(answer));
        dialog.connect_response(move |dialog, response| {
            let Some(deliver) = answer.borrow_mut().take() else {
                return;
            };
            // Destroyed rather than closed: GtkDialog answers `close` with another response and
            // keeps the window, which would leave the gate on screen after it was answered.
            unsafe { dialog.destroy() };
            let answered = match response {
                gtk::ResponseType::Yes => CloseAnswer::Save,
                gtk::ResponseType::No => CloseAnswer::Discard,
                _ => CloseAnswer::Cancel,
            };
            // Off the main thread, because acting on the answer writes a file and the main loop is
            // the one thing that must not block: `close_window` posts back to it and would wait on
            // itself.
            std::thread::spawn(move || deliver(answered));
        });
        dialog.show_all();
    })
}

#[cfg(not(target_os = "linux"))]
pub fn ask_close<F>(app: &AppHandle, label: &str, answer: F) -> tauri::Result<()>
where
    F: FnOnce(CloseAnswer) + Send + 'static,
{
    use tauri::Manager;
    use tauri_plugin_dialog::{
        DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
    };

    let mut dialog = app
        .dialog()
        .message(crate::strings::CLOSE_UNSAVED_BODY)
        .title(crate::strings::CLOSE_UNSAVED_TITLE)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            crate::strings::CLOSE_SAVE.to_owned(),
            crate::strings::CLOSE_DISCARD.to_owned(),
            crate::strings::CLOSE_CANCEL.to_owned(),
        ));
    if let Some(window) = app.get_webview_window(label) {
        dialog = dialog.parent(&window);
    }
    // The plugin rewrites every button of a custom set to `Custom(label)` before this callback, so
    // matching the labels covers the three answers and the catch-all covers everything else,
    // including the window manager closing the dialog outright.
    dialog.show_with_result(move |result| {
        answer(match result {
            MessageDialogResult::Custom(ref text) if text == crate::strings::CLOSE_SAVE => {
                CloseAnswer::Save
            }
            MessageDialogResult::Custom(ref text) if text == crate::strings::CLOSE_DISCARD => {
                CloseAnswer::Discard
            }
            _ => CloseAnswer::Cancel,
        });
    });
    Ok(())
}

/// Tell the user something went wrong, and do not wait for them to read it.
///
/// Parentless on purpose: both callers reach here while the window they would parent to is either
/// closing or in a state they are refusing to close.
#[cfg(target_os = "linux")]
pub fn report_error(app: &AppHandle, title: &str, body: String) -> tauri::Result<()> {
    use gtk::prelude::*;

    let title = title.to_owned();
    app.run_on_main_thread(move || {
        let dialog = gtk::MessageDialog::new(
            None::<&gtk::ApplicationWindow>,
            gtk::DialogFlags::empty(),
            gtk::MessageType::Error,
            gtk::ButtonsType::Ok,
            &body,
        );
        dialog.set_title(&title);
        dialog.connect_response(|dialog, _| unsafe { dialog.destroy() });
        dialog.show_all();
    })
}

#[cfg(not(target_os = "linux"))]
pub fn report_error(app: &AppHandle, title: &str, body: String) -> tauri::Result<()> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

    app.dialog()
        .message(body)
        .title(title)
        .kind(MessageDialogKind::Error)
        .show(|_| {});
    Ok(())
}
