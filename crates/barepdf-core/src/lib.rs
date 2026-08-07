#![forbid(unsafe_code)]

pub mod command;
pub mod error;
pub mod layout;
pub mod preferences;
pub mod selection;
pub mod types;

pub use command::AppCommand;
pub use error::PdfError;
pub use layout::{
    calculate_page_pairings, compute_target_dimensions, ContinuousLayout, PageLayoutBox,
    PagePairing, ScrollAnchor,
};
pub use preferences::{default_config_path, ThemeMode, UserPreferences};
pub use selection::SelectionEngine;
pub use types::*;
