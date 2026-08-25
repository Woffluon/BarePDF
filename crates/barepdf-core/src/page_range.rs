use std::collections::BTreeSet;
use thiserror::Error;

use crate::types::{PageCount, PageIndex};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PageRangeError {
    #[error("Page range input cannot be empty")]
    EmptyInput,

    #[error("Invalid page range syntax: {0}")]
    InvalidSyntax(String),

    #[error("Page numbers are 1-based; page 0 is invalid")]
    ZeroPageNumber,

    #[error("Page number {requested} is out of range (document has {max_pages} pages)")]
    PageOutOfRange { requested: u32, max_pages: u32 },

    #[error("Invalid page range order: start page {start} is greater than end page {end}")]
    InvalidRangeOrder { start: u32, end: u32 },

    #[error("Cannot delete all pages from document")]
    CannotDeleteAllPages,
}

pub struct PageRangeSelection;

impl PageRangeSelection {
    /// Parses a page range string (e.g. "1", "1-5", "1, 3, 5-8") into sorted, deduplicated `PageIndex`es.
    pub fn parse(input: &str, total_pages: PageCount) -> Result<Vec<PageIndex>, PageRangeError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(PageRangeError::EmptyInput);
        }

        let max_pages = total_pages.get();
        let mut page_indices = BTreeSet::new();

        for segment in trimmed.split(',') {
            let seg = segment.trim();
            if seg.is_empty() {
                return Err(PageRangeError::InvalidSyntax(
                    "Empty range segment".to_string(),
                ));
            }

            let dash_count = seg.chars().filter(|&c| c == '-').count();
            match dash_count {
                0 => {
                    let page_num = parse_page_number(seg, max_pages)?;
                    page_indices.insert(PageIndex::from_raw(page_num - 1));
                }
                1 => {
                    let mut parts = seg.split('-');
                    let start_str = parts.next().unwrap_or("").trim();
                    let end_str = parts.next().unwrap_or("").trim();

                    if start_str.is_empty() || end_str.is_empty() {
                        return Err(PageRangeError::InvalidSyntax(seg.to_string()));
                    }

                    let start = parse_page_number(start_str, max_pages)?;
                    let end = parse_page_number(end_str, max_pages)?;

                    if start > end {
                        return Err(PageRangeError::InvalidRangeOrder { start, end });
                    }

                    for page_num in start..=end {
                        page_indices.insert(PageIndex::from_raw(page_num - 1));
                    }
                }
                _ => {
                    return Err(PageRangeError::InvalidSyntax(seg.to_string()));
                }
            }
        }

        Ok(page_indices.into_iter().collect())
    }
}

fn parse_page_number(s: &str, max_pages: u32) -> Result<u32, PageRangeError> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
        return Err(PageRangeError::InvalidSyntax(s.to_string()));
    }

    let n: u32 = match s.parse() {
        Ok(val) => val,
        Err(_) => {
            return Err(PageRangeError::PageOutOfRange {
                requested: u32::MAX,
                max_pages,
            });
        }
    };

    if n == 0 {
        return Err(PageRangeError::ZeroPageNumber);
    }

    if n > max_pages {
        return Err(PageRangeError::PageOutOfRange {
            requested: n,
            max_pages,
        });
    }

    Ok(n)
}

/// Validates that a list of page indices are non-empty and within bounds for `total_pages`.
pub fn validate_page_selection(
    indices: &[PageIndex],
    total_pages: PageCount,
) -> Result<(), PageRangeError> {
    if indices.is_empty() {
        return Err(PageRangeError::EmptyInput);
    }

    let max_pages = total_pages.get();
    for &page in indices {
        if page.get() >= max_pages {
            return Err(PageRangeError::PageOutOfRange {
                requested: page.get() + 1,
                max_pages,
            });
        }
    }

    Ok(())
}

/// Calculates remaining pages after removing `pages_to_remove`.
/// Returns `Err(PageRangeError::CannotDeleteAllPages)` if all pages would be removed.
pub fn pages_to_remove_to_retained_pages(
    total_pages: PageCount,
    pages_to_remove: &[PageIndex],
) -> Result<Vec<PageIndex>, PageRangeError> {
    let max_pages = total_pages.get();
    let mut remove_set = BTreeSet::new();

    for &page in pages_to_remove {
        if page.get() >= max_pages {
            return Err(PageRangeError::PageOutOfRange {
                requested: page.get() + 1,
                max_pages,
            });
        }
        remove_set.insert(page.get());
    }

    if remove_set.len() == max_pages as usize {
        return Err(PageRangeError::CannotDeleteAllPages);
    }

    let mut retained = Vec::with_capacity((max_pages as usize).saturating_sub(remove_set.len()));
    for i in 0..max_pages {
        if !remove_set.contains(&i) {
            retained.push(PageIndex::from_raw(i));
        }
    }

    Ok(retained)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(n: u32) -> PageCount {
        PageCount::new(n).expect("valid page count")
    }

    fn idx(i: u32) -> PageIndex {
        PageIndex::from_raw(i)
    }

    #[test]
    fn test_parse_single_pages() {
        let total = count(10);
        assert_eq!(PageRangeSelection::parse("1", total).unwrap(), vec![idx(0)]);
        assert_eq!(PageRangeSelection::parse("5", total).unwrap(), vec![idx(4)]);
        assert_eq!(
            PageRangeSelection::parse("10", total).unwrap(),
            vec![idx(9)]
        );
    }

    #[test]
    fn test_parse_ranges() {
        let total = count(10);
        assert_eq!(
            PageRangeSelection::parse("1-5", total).unwrap(),
            vec![idx(0), idx(1), idx(2), idx(3), idx(4)]
        );
        assert_eq!(
            PageRangeSelection::parse("3-5", total).unwrap(),
            vec![idx(2), idx(3), idx(4)]
        );
        assert_eq!(
            PageRangeSelection::parse("4-4", total).unwrap(),
            vec![idx(3)]
        );
    }

    #[test]
    fn test_parse_mixed() {
        let total = count(15);
        assert_eq!(
            PageRangeSelection::parse("1, 3-5, 8, 10-12", total).unwrap(),
            vec![
                idx(0),
                idx(2),
                idx(3),
                idx(4),
                idx(7),
                idx(9),
                idx(10),
                idx(11)
            ]
        );
    }

    #[test]
    fn test_parse_whitespace_tolerance() {
        let total = count(10);
        assert_eq!(
            PageRangeSelection::parse("  1 ,  3 - 5 , 8  ", total).unwrap(),
            vec![idx(0), idx(2), idx(3), idx(4), idx(7)]
        );
        assert_eq!(
            PageRangeSelection::parse("\t 2 - 4 \n", total).unwrap(),
            vec![idx(1), idx(2), idx(3)]
        );
    }

    #[test]
    fn test_parse_dedup_and_sort() {
        let total = count(10);
        assert_eq!(
            PageRangeSelection::parse("5, 1, 3-5, 2, 2", total).unwrap(),
            vec![idx(0), idx(1), idx(2), idx(3), idx(4)]
        );
    }

    #[test]
    fn test_parse_empty_input() {
        let total = count(10);
        assert_eq!(
            PageRangeSelection::parse("", total),
            Err(PageRangeError::EmptyInput)
        );
        assert_eq!(
            PageRangeSelection::parse("   ", total),
            Err(PageRangeError::EmptyInput)
        );
    }

    #[test]
    fn test_parse_zero_page() {
        let total = count(10);
        assert_eq!(
            PageRangeSelection::parse("0", total),
            Err(PageRangeError::ZeroPageNumber)
        );
        assert_eq!(
            PageRangeSelection::parse("0-3", total),
            Err(PageRangeError::ZeroPageNumber)
        );
        assert_eq!(
            PageRangeSelection::parse("3-0", total),
            Err(PageRangeError::ZeroPageNumber)
        );
        assert_eq!(
            PageRangeSelection::parse("1, 0, 3", total),
            Err(PageRangeError::ZeroPageNumber)
        );
    }

    #[test]
    fn test_parse_invalid_syntax() {
        let total = count(10);
        assert!(matches!(
            PageRangeSelection::parse("-1", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
        assert!(matches!(
            PageRangeSelection::parse("abc", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
        assert!(matches!(
            PageRangeSelection::parse("1a", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
        assert!(matches!(
            PageRangeSelection::parse("1,,2", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
        assert!(matches!(
            PageRangeSelection::parse(",1", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
        assert!(matches!(
            PageRangeSelection::parse("1,", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
        assert!(matches!(
            PageRangeSelection::parse("1-3-5", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
        assert!(matches!(
            PageRangeSelection::parse("-", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
        assert!(matches!(
            PageRangeSelection::parse("1-", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
        assert!(matches!(
            PageRangeSelection::parse("-5", total),
            Err(PageRangeError::InvalidSyntax(_))
        ));
    }

    #[test]
    fn test_parse_invalid_range_order() {
        let total = count(10);
        assert_eq!(
            PageRangeSelection::parse("5-2", total),
            Err(PageRangeError::InvalidRangeOrder { start: 5, end: 2 })
        );
    }

    #[test]
    fn test_parse_out_of_range() {
        let total = count(10);
        assert_eq!(
            PageRangeSelection::parse("1-99999", total),
            Err(PageRangeError::PageOutOfRange {
                requested: 99999,
                max_pages: 10
            })
        );
        assert_eq!(
            PageRangeSelection::parse("11", total),
            Err(PageRangeError::PageOutOfRange {
                requested: 11,
                max_pages: 10
            })
        );
        assert_eq!(
            PageRangeSelection::parse("20-25", total),
            Err(PageRangeError::PageOutOfRange {
                requested: 20,
                max_pages: 10
            })
        );
    }

    #[test]
    fn test_validate_page_selection() {
        let total = count(10);
        assert_eq!(
            validate_page_selection(&[], total),
            Err(PageRangeError::EmptyInput)
        );
        assert_eq!(
            validate_page_selection(&[idx(0), idx(4), idx(9)], total),
            Ok(())
        );
        assert_eq!(
            validate_page_selection(&[idx(0), idx(10)], total),
            Err(PageRangeError::PageOutOfRange {
                requested: 11,
                max_pages: 10
            })
        );
    }

    #[test]
    fn test_pages_to_remove_to_retained_pages() {
        let total = count(5);

        // Delete all pages
        assert_eq!(
            pages_to_remove_to_retained_pages(total, &[idx(0), idx(1), idx(2), idx(3), idx(4)]),
            Err(PageRangeError::CannotDeleteAllPages)
        );
        assert_eq!(
            pages_to_remove_to_retained_pages(total, &[idx(4), idx(3), idx(2), idx(1), idx(0)]),
            Err(PageRangeError::CannotDeleteAllPages)
        );

        // Delete some pages
        assert_eq!(
            pages_to_remove_to_retained_pages(total, &[idx(0), idx(2), idx(4)]).unwrap(),
            vec![idx(1), idx(3)]
        );

        // Delete with duplicates in remove list
        assert_eq!(
            pages_to_remove_to_retained_pages(total, &[idx(1), idx(1), idx(3)]).unwrap(),
            vec![idx(0), idx(2), idx(4)]
        );

        // Delete no pages (empty remove list)
        assert_eq!(
            pages_to_remove_to_retained_pages(total, &[]).unwrap(),
            vec![idx(0), idx(1), idx(2), idx(3), idx(4)]
        );

        // Out of range remove index
        assert_eq!(
            pages_to_remove_to_retained_pages(total, &[idx(5)]),
            Err(PageRangeError::PageOutOfRange {
                requested: 6,
                max_pages: 5
            })
        );
    }
}
