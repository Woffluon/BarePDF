#![forbid(unsafe_code)]

pub mod command;
pub mod error;
pub mod layout;
pub mod preferences;
pub mod types;

pub use command::AppCommand;
pub use error::PdfError;
pub use layout::{calculate_page_pairings, compute_target_dimensions, PagePairing};
pub use preferences::{default_config_path, ThemeMode, UserPreferences};
pub use types::*;
