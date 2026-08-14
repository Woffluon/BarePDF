use barepdf_platform::FileDialogs;
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::path::PathBuf;

pub struct WindowsFileDialogs;

pub fn show_fatal_error(title: &str, description: &str) {
    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title(title)
        .set_description(description)
        .set_buttons(MessageButtons::Ok)
        .show();
}

#[must_use]
pub fn ask_yes_no(title: &str, description: &str) -> bool {
    MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title(title)
        .set_description(description)
        .set_buttons(MessageButtons::YesNo)
        .show()
        == MessageDialogResult::Yes
}

impl FileDialogs for WindowsFileDialogs {
    fn pick_file(&self) -> Option<PathBuf> {
        FileDialog::new()
            .add_filter("PDF Document", &["pdf"])
            .pick_file()
    }
}
