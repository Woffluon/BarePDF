#![forbid(unsafe_code)]

mod clipboard;
mod dialogs;
pub mod printing;
mod shell;

pub use clipboard::LinuxClipboard;
pub use dialogs::{ask_yes_no, show_fatal_error, LinuxFileDialogs};
pub use printing::{
    enumerate_installed_printers, CommandOutput, CommandRunner, LinuxPrinterDialog,
    LinuxPrinterSink, SystemCommandRunner,
};
pub use shell::{launch_package_installer, open_url, trigger_virtual_keyboard};

pub use barepdf_platform::printing::{InstalledPrinter, PrintOrientation};
