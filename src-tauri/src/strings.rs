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
