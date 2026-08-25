#![forbid(unsafe_code)]

pub mod error;
pub mod layout;
pub mod limits;
pub mod page_range;
pub mod preferences;
pub mod selection;
pub mod types;

pub use error::PdfError;
pub use layout::{
    calculate_page_pairings, compute_target_dimensions, ContinuousLayout, PageLayoutBox,
    PagePairing, ScrollAnchor,
};
pub use limits::{
    validate_document_page_count, validate_tab_count, ResourceLimitError, MAX_DOCUMENT_PAGES,
    MAX_OPEN_TABS, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS, MAX_PASSWORD_BYTES, MAX_RECENT_FILES,
};
pub use page_range::{
    pages_to_remove_to_retained_pages, validate_page_selection, PageRangeError, PageRangeSelection,
};
pub use preferences::{ThemeMode, UserPreferences};
pub use selection::SelectionEngine;
pub use types::*;
