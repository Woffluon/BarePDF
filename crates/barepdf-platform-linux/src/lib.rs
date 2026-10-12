#![forbid(unsafe_code)]

mod clipboard;
mod dialogs;
mod shell;

pub use clipboard::LinuxClipboard;
pub use dialogs::{ask_yes_no, show_fatal_error, LinuxFileDialogs};
pub use shell::{open_url, trigger_virtual_keyboard};
