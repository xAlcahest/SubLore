//! The app's native message dialogs.
//!
//! On Linux the close gate is built on GTK directly, on the main thread, instead of going through
//! tauri-plugin-dialog. The plugin uses rfd, which starts a second thread the first time any dialog
//! is shown and iterates GTK on it for the rest of the process's life, which GTK3 is not built for.
//! Keeping the close gate off that thread is worth doing on its own, and it lets the dialog be
//! modal and transient for the window it asks about, which rfd cannot be because it builds with a
//! null parent.
//!
//! It removes that thread from the close gate only, not from the process: `project::choose_path`
//! and `crash::show_dialog` still raise plugin dialogs, so any session that reaches one still has
//! rfd's GTK thread in it. That is BACKLOG N1c, and it is not fixed here.
//!
//! It is **not** the fix for N1b's exit crash. That crash survived this change, and a core from
//! this binary shows the same crashing frame with no rfd thread in the process
//! (`docs/reports/n1b-segfault-uscita.md`). Every other platform keeps the plugin.

use std::sync::mpsc::{self, Sender};

use tauri::AppHandle;

use crate::log;

/// What the user answered when asked about unsaved work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseAnswer {
    Save,
    Discard,
    Cancel,
}

/// The thread the answer is acted on. Named because a panic there is reported as `thread: <name>`
/// and `<unnamed>` would not say which path it came from (`crash::format_report`).
const ANSWER_THREAD: &str = "sublore-close-answer";

/// Holds the answer callback so that it is answered exactly once, on every path there is.
///
/// Dropping it unanswered answers `Cancel`, which is the safe direction: the window stays and the
/// next close request asks again. A dialog can go away without responding — its parent destroyed
/// under it, its task never dispatched, its callback dropped by the plugin — and a dropped question
/// leaves the gate standing open forever (gate 2, `dialog.rs:46` and `:120`).
struct Delivery<F: FnOnce(CloseAnswer)> {
    answer: Option<F>,
}

impl<F: FnOnce(CloseAnswer)> Delivery<F> {
    fn new(answer: F) -> Self {
        Self {
            answer: Some(answer),
        }
    }

    /// The first answer wins. GTK can respond more than once — a button press followed by the
    /// window manager closing the dialog — and acting twice would close the window twice.
    fn deliver(&mut self, answer: CloseAnswer) {
        if let Some(deliver) = self.answer.take() {
            deliver(answer);
        }
    }
}

impl<F: FnOnce(CloseAnswer)> Drop for Delivery<F> {
    fn drop(&mut self) {
        // Silence here is a window that can never be closed again, so it is said out loud.
        if self.answer.is_some() {
            log::warn!("close gate: the dialog went away without an answer, taking it as Cancel");
        }
        self.deliver(CloseAnswer::Cancel);
    }
}

/// Start the thread that will act on the answer, and hand back the channel it listens on.
///
/// Started before anything is asked, because it can fail: a thread the OS refuses has to fail the
/// question, never strand an answer the user has already given (gate 2, `dialog.rs:77`). Acting on
/// the answer takes a blocking lock and writes a file, which is the one thing the main loop must
/// not do (CLAUDE.md §7), so it never runs on the thread that asks.
///
/// Closing the channel without sending answers `Cancel`.
fn answer_worker<F>(answer: F) -> std::io::Result<Sender<CloseAnswer>>
where
    F: FnOnce(CloseAnswer) + Send + 'static,
{
    let (send, receive) = mpsc::channel();
    let mut carrier = Delivery::new(answer);
    std::thread::Builder::new()
        .name(ANSWER_THREAD.to_owned())
        .spawn(move || {
            // A closed channel is a dialog that went away unanswered; dropping the carrier answers
            // Cancel for it.
            if let Ok(answered) = receive.recv() {
                carrier.deliver(answered);
            }
        })?;
    Ok(send)
}

/// Ask what to do with unsaved edits, and deliver the answer exactly once.
///
/// The answer arrives later, on `ANSWER_THREAD`, never on the thread that asks. Every way of losing
/// the dialog answers `Cancel` rather than dropping the question. An error means nobody was asked
/// and nobody will be, so the caller has to keep the window open and say so.
///
/// On Linux the dialog is up before this returns: the only caller is already the main thread, and
/// `run_on_main_thread` runs the task inline there rather than posting it.
#[cfg(target_os = "linux")]
pub fn ask_close<F>(app: &AppHandle, label: &str, answer: F) -> tauri::Result<()>
where
    F: FnOnce(CloseAnswer) + Send + 'static,
{
    use gtk::prelude::*;
    use tauri::Manager;

    let send = answer_worker(answer)?;
    let handle = app.clone();
    let label = label.to_owned();
    app.run_on_main_thread(move || {
        // Transient and modal, which is what the rfd dialog could not be: its GTK backend built the
        // dialog with a null parent, so on Linux it could end up behind the window it was asking
        // about (BACKLOG N1).
        let parent = handle
            .get_webview_window(&label)
            .and_then(|window| window.gtk_window().ok());
        if parent.is_none() {
            // A null parent is the rfd behaviour this module exists to escape, so losing it is not
            // something to discover from a screenshot.
            log::warn!("close gate: no GTK parent for {label}, the dialog cannot be transient");
        }
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
        // Mnemonics, so the dialog can be answered from the keyboard: Alt+S, Alt+D, and Escape for
        // cancel, which GTK gives for free. A button reachable only by aiming a pointer at it is a
        // button some users cannot press, and one the harness had to locate by arithmetic.
        for (label, response) in [
            (crate::strings::CLOSE_SAVE, gtk::ResponseType::Yes),
            (crate::strings::CLOSE_DISCARD, gtk::ResponseType::No),
            (crate::strings::CLOSE_CANCEL, gtk::ResponseType::Cancel),
        ] {
            // `add_button` hands back a Widget; the underline is a Button property, and GTK3
            // leaves it off unless it is asked for.
            if let Ok(button) = dialog.add_button(label, response).downcast::<gtk::Button>() {
                button.set_use_underline(true);
            }
        }

        // `connect_response` takes an `Fn` and the dialog can answer more than once, while only the
        // first answer may be acted on, because acting on it destroys the window.
        let send = std::cell::RefCell::new(Some(send));
        dialog.connect_response(move |dialog, response| {
            let Some(send) = send.borrow_mut().take() else {
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
            // The worker is only ever gone if it panicked, which the panic hook turns into an
            // exit; logged rather than assumed impossible.
            if send.send(answered).is_err() {
                log::error!("close gate: the answer thread is gone, {answered:?} was not acted on");
            }
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

    let send = answer_worker(answer)?;
    let mut dialog = app
        .dialog()
        .message(crate::strings::CLOSE_UNSAVED_BODY)
        .title(crate::strings::CLOSE_UNSAVED_TITLE)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            crate::strings::CLOSE_SAVE_PLAIN.to_owned(),
            crate::strings::CLOSE_DISCARD_PLAIN.to_owned(),
            crate::strings::CLOSE_CANCEL_PLAIN.to_owned(),
        ));
    match app.get_webview_window(label) {
        Some(window) => dialog = dialog.parent(&window),
        // Same silent degradation as the Linux branch, one layer down: the plugin swallows both
        // handle errors of its own `parent()` too.
        None => log::warn!("close gate: no window {label}, the dialog cannot be transient"),
    }
    // The plugin rewrites every button of a custom set to `Custom(label)` before this callback, so
    // matching the labels covers the three answers and the catch-all covers everything else,
    // including the window manager closing the dialog outright.
    dialog.show_with_result(move |result| {
        let answered = match result {
            MessageDialogResult::Custom(ref text) if text == crate::strings::CLOSE_SAVE_PLAIN => {
                CloseAnswer::Save
            }
            MessageDialogResult::Custom(ref text)
                if text == crate::strings::CLOSE_DISCARD_PLAIN =>
            {
                CloseAnswer::Discard
            }
            _ => CloseAnswer::Cancel,
        };
        // Same as the Linux branch: a missing worker means it panicked, and that ends the process.
        if send.send(answered).is_err() {
            log::error!("close gate: the answer thread is gone, {answered:?} was not acted on");
        }
    });
    // The plugin discards its own post to the main thread, so the dialog may never be raised. That
    // is no longer silent: dropping the callback drops the channel, which answers Cancel.
    Ok(())
}

/// Tell the user something went wrong, and do not wait for them to read it.
///
/// Parentless on purpose: both callers reach here while the window they would parent to is either
/// closing or in a state they are refusing to close. One of the two is on the main thread
/// (`report_close_failure`), where the post below runs inline instead of waiting on anything.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    /// Long enough that a loaded machine cannot fail these, short enough that a lost answer ends
    /// the test instead of hanging the suite.
    const WAIT: Duration = Duration::from_secs(5);
    /// How long a second answer is waited for before it is agreed there is none.
    const QUIET: Duration = Duration::from_millis(200);

    #[test]
    fn the_answer_reaches_the_callback() {
        let (report, answered) = channel();
        let send = answer_worker(move |answer| report.send(answer).expect("the test is listening"))
            .expect("a worker thread");

        send.send(CloseAnswer::Save)
            .expect("the worker is listening");

        assert_eq!(
            answered.recv_timeout(WAIT).expect("an answer"),
            CloseAnswer::Save
        );
    }

    #[test]
    fn a_dialog_that_goes_away_unanswered_answers_cancel() {
        let (report, answered) = channel();
        let send = answer_worker(move |answer| report.send(answer).expect("the test is listening"))
            .expect("a worker thread");

        // The dialog destroyed with its parent, or its task never dispatched: the channel closes
        // with nothing sent.
        drop(send);

        assert_eq!(
            answered.recv_timeout(WAIT).expect("an answer"),
            CloseAnswer::Cancel
        );
    }

    #[test]
    fn a_callback_dropped_before_any_dialog_answers_cancel() {
        let (report, answered) = channel();

        drop(Delivery::new(move |answer| {
            report.send(answer).expect("the test is listening")
        }));

        assert_eq!(
            answered.recv_timeout(WAIT).expect("an answer"),
            CloseAnswer::Cancel
        );
    }

    #[test]
    fn an_answered_gate_is_never_answered_a_second_time() {
        let (report, answered) = channel();
        let mut carrier =
            Delivery::new(move |answer| report.send(answer).expect("the test is listening"));

        carrier.deliver(CloseAnswer::Discard);
        // A second GTK response, then the drop that follows every dialog.
        carrier.deliver(CloseAnswer::Save);
        drop(carrier);

        assert_eq!(
            answered.recv_timeout(WAIT).expect("an answer"),
            CloseAnswer::Discard
        );
        assert!(
            answered.recv_timeout(QUIET).is_err(),
            "the gate was answered more than once"
        );
    }

    #[test]
    fn the_answer_is_acted_on_by_the_named_worker_and_not_by_the_asking_thread() {
        let (report, answered) = channel();
        let asking = std::thread::current().id();
        let send = answer_worker(move |_| {
            let acting = std::thread::current();
            report
                .send((acting.id(), acting.name().map(str::to_owned)))
                .expect("the test is listening");
        })
        .expect("a worker thread");

        send.send(CloseAnswer::Save)
            .expect("the worker is listening");

        let (acting, name) = answered.recv_timeout(WAIT).expect("an answer");
        assert_ne!(
            acting, asking,
            "the answer was acted on by the asking thread"
        );
        assert_eq!(name.as_deref(), Some(ANSWER_THREAD));
    }
}
