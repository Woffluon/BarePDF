use crate::PageCount;

pub const MAX_DOCUMENT_PAGES: u32 = 10_000;
pub const MAX_OPEN_TABS: usize = 16;
pub const MAX_OUTLINE_DEPTH: usize = 128;
pub const MAX_OUTLINE_ITEMS: usize = 4_096;
pub const MAX_PASSWORD_BYTES: usize = 1_024;
pub const MAX_RECENT_FILES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResourceLimitError {
    #[error("document has {count} pages; maximum is {MAX_DOCUMENT_PAGES}")]
    TooManyDocumentPages { count: u32 },
    #[error(
        "cannot open another tab when {count} tabs are already open; maximum is {MAX_OPEN_TABS}"
    )]
    TooManyTabs { count: usize },
}

/// # Errors
///
/// Returns [`ResourceLimitError::TooManyDocumentPages`] when the document exceeds the product
/// page limit.
pub const fn validate_document_page_count(page_count: PageCount) -> Result<(), ResourceLimitError> {
    if page_count.get() <= MAX_DOCUMENT_PAGES {
        Ok(())
    } else {
        Err(ResourceLimitError::TooManyDocumentPages {
            count: page_count.get(),
        })
    }
}

/// Validates the current number of tabs before another tab is inserted.
///
/// # Errors
///
/// Returns [`ResourceLimitError::TooManyTabs`] when no additional tab may be opened.
pub const fn validate_tab_count(current_count: usize) -> Result<(), ResourceLimitError> {
    if current_count < MAX_OPEN_TABS {
        Ok(())
    } else {
        Err(ResourceLimitError::TooManyTabs {
            count: current_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_page_limit_accepts_boundary_and_rejects_excess() {
        let boundary = PageCount::new(MAX_DOCUMENT_PAGES).expect("limit is non-zero");
        let excess = PageCount::new(MAX_DOCUMENT_PAGES + 1).expect("limit is below u32::MAX");

        assert_eq!(validate_document_page_count(boundary), Ok(()));
        assert_eq!(
            validate_document_page_count(excess),
            Err(ResourceLimitError::TooManyDocumentPages {
                count: MAX_DOCUMENT_PAGES + 1
            })
        );
    }

    #[test]
    fn tab_limit_reserves_space_for_one_new_tab() {
        assert_eq!(validate_tab_count(MAX_OPEN_TABS - 1), Ok(()));
        assert_eq!(
            validate_tab_count(MAX_OPEN_TABS),
            Err(ResourceLimitError::TooManyTabs {
                count: MAX_OPEN_TABS
            })
        );
    }

    #[test]
    fn product_input_limits_are_stable() {
        assert_eq!(MAX_PASSWORD_BYTES, 1_024);
        assert_eq!(MAX_OUTLINE_DEPTH, 128);
        assert_eq!(MAX_OUTLINE_ITEMS, 4_096);
    }
}
