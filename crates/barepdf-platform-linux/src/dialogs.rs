use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxFileDialogs;

pub fn show_fatal_error(title: &str, message: &str) {
    MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_level(MessageLevel::Error)
        .set_buttons(MessageButtons::Ok)
        .show();
}

#[must_use]
pub fn ask_yes_no(title: &str, message: &str) -> bool {
    MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_level(MessageLevel::Info)
        .set_buttons(MessageButtons::YesNo)
        .show()
        == MessageDialogResult::Yes
}

impl LinuxFileDialogs {
    #[must_use]
    pub fn pick_file(&self) -> Option<PathBuf> {
        FileDialog::new()
            .add_filter("PDF Document", &["pdf"])
            .pick_file()
    }

    #[must_use]
    pub fn pick_multiple_files(&self) -> Vec<PathBuf> {
        FileDialog::new()
            .add_filter("PDF Document", &["pdf"])
            .pick_files()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn save_file(&self, default_name: &str) -> Option<PathBuf> {
        FileDialog::new()
            .add_filter("PDF Document", &["pdf"])
            .set_file_name(default_name)
            .save_file()
    }

    #[must_use]
    pub fn pick_directory(&self) -> Option<PathBuf> {
        FileDialog::new().pick_folder()
    }

    #[must_use]
    pub fn pick_image_file(&self) -> Option<PathBuf> {
        FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg"])
            .pick_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn test_linux_file_dialogs_construction() {
        let dialogs = LinuxFileDialogs;
        let default_dialogs = LinuxFileDialogs::default();
        let cloned = dialogs;
        assert_eq!(format!("{dialogs:?}"), format!("{default_dialogs:?}"));
        assert_eq!(format!("{dialogs:?}"), format!("{cloned:?}"));
    }
}
