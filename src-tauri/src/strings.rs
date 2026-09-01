//! English source strings for the native dialogs. The webview's copy lives in src/i18n/en.ts;
//! these are the ones the webview cannot render, because at crash time it may be what broke.

pub const CRASH_TITLE: &str = "Sublore has stopped";

pub const CRASH_BODY_NO_REPORT: &str = "Sublore hit an unexpected error and has to close.\n\n\
     The crash report could not be saved, so the details are lost. Reopen Sublore to continue.";

pub fn crash_body(report_path: &str) -> String {
    format!(
        "Sublore hit an unexpected error and has to close.\n\n\
         A crash report was saved to:\n{report_path}\n\n\
         Reopen Sublore to continue. If this keeps happening, attach that file to a bug report."
    )
}

/// Titles for the two native file dialogs the project panel opens.
pub const CHOOSE_PROJECT_FOLDER: &str = "Choose a project folder";
pub const CHOOSE_PROJECT_FILE: &str = "Choose a video or subtitle file";
/// The chooser's own buttons on Linux, where Sublore builds it rather than the plugin (N1c). The
/// underscore marks the mnemonic, as in the close gate below; "Open" is what GTK's own file chooser
/// calls the accept button in both modes, and Alt+S is taken by the chooser's search.
pub const CHOOSE_VIDEO: &str = "Choose a video";
pub const CHOOSE_SUBTITLE: &str = "Choose a subtitle";
pub const CHOOSE_SUBTITLE_SAVE: &str = "Save a copy of the subtitle";
pub const CHOOSE_ACCEPT: &str = "_Open";
/// Naming a file to write, not picking one that exists, so the button says what it does.
pub const CHOOSE_SAVE: &str = "Sa_ve";
pub const CHOOSE_CANCEL: &str = "_Cancel";

/// The close gate. Native, not webview: the answer decides whether the window survives, and the
/// video surface sits above the webview until decision 1 lands (BACKLOG N1).
pub const CLOSE_UNSAVED_TITLE: &str = "Unsaved changes";
pub const CLOSE_UNSAVED_BODY: &str =
    "The subtitle file has edits that are not on disk.\n\nSave them before closing?";
/// The underscore marks the mnemonic letter: GTK shows "Save" and answers Alt+S. It is part of the
/// translated string on purpose — which letter is free depends on the language — and it is what
/// gives the dialog keyboard access, for a user whose hands are on the keyboard and for a harness
/// that would otherwise have to guess where a button sits.
pub const CLOSE_SAVE: &str = "_Save";
pub const CLOSE_DISCARD: &str = "_Discard";
pub const CLOSE_CANCEL: &str = "_Cancel";

/// The same labels without the marker, for the platforms whose dialogs take plain text.
pub const CLOSE_SAVE_PLAIN: &str = "Save";
pub const CLOSE_DISCARD_PLAIN: &str = "Discard";
pub const CLOSE_CANCEL_PLAIN: &str = "Cancel";

/// A save that fails on the way out leaves the window open. Saying so is the difference between
/// a refusal the user understands and one that looks like a stuck button (CONTRIBUTING.md §6).
pub const CLOSE_FAILED_TITLE: &str = "Could not close";
pub fn close_failed(reason: &str) -> String {
    format!(
        "Sublore could not finish closing, so the window is still open and may no longer be showing \
         what is in memory.\n\n{reason}\n\nSave a copy from the toolbar before trying again."
    )
}

pub const CLOSE_SAVE_FAILED_TITLE: &str = "Could not save";
pub fn close_save_failed(reason: &str) -> String {
    format!(
        "The file was not saved, so Sublore stayed open and your edits are still here.\n\n\
         {reason}\n\nUse Save copy to write them somewhere else. If Save keeps refusing, copy your \
         work out before closing: this window is the only place it exists."
    )
}
