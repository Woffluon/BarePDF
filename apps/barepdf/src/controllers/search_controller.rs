use std::collections::HashMap;

use barepdf_core::layout::ContinuousLayout;
use barepdf_core::search::{SearchMatch, SearchQuery};
use barepdf_core::types::{PageIndex, PageTextGeometry};

#[must_use]
pub fn execute_search(
    query: &SearchQuery,
    geometries: &HashMap<u32, PageTextGeometry>,
    page_count: u32,
) -> Vec<SearchMatch> {
    let mut all_matches = Vec::new();
    let mut global_index = 0;

    for page_idx in 0..page_count {
        if let Some(geom) = geometries.get(&page_idx) {
            let ranges = query.find_in_geometry(geom);
            for range in ranges {
                let mut glyph_boxes = Vec::new();
                let start = range.start as usize;
                let end = range.end as usize;

                if start <= geom.glyphs.len() && end <= geom.glyphs.len() {
                    glyph_boxes.extend_from_slice(&geom.glyphs[start..end]);
                }

                all_matches.push(SearchMatch {
                    page_index: PageIndex::from_raw(page_idx),
                    match_index_in_doc: global_index,
                    char_range: range,
                    glyph_boxes,
                });
                global_index += 1;
            }
        }
    }
    all_matches
}

#[must_use]
pub fn next_match(current: usize, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        (current + 1) % total
    }
}

#[must_use]
pub fn prev_match(current: usize, total: usize) -> usize {
    if total == 0 {
        0
    } else if current == 0 {
        total - 1
    } else {
        current - 1
    }
}

#[must_use]
pub fn match_summary(current: usize, total: usize) -> String {
    if total == 0 {
        "0 / 0".to_string()
    } else {
        format!("{} / {}", current + 1, total)
    }
}

#[must_use]
pub fn get_scroll_target_for_match(
    match_item: &SearchMatch,
    layout: &ContinuousLayout,
) -> Option<f32> {
    if let Some(first_glyph) = match_item.glyph_boxes.first() {
        let page_offset = layout
            .pages
            .iter()
            .find(|p| p.page_index == match_item.page_index)
            .map(|p| p.y_offset)
            .unwrap_or(0.0);

        Some(page_offset + first_glyph.y)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use barepdf_core::layout::PageLayoutBox;
    use barepdf_core::types::GlyphRect;

    #[test]
    fn execute_search_finds_matches_across_pages() {
        let mut geometries = HashMap::new();
        geometries.insert(
            0,
            PageTextGeometry {
                page_index: PageIndex::from_raw(0),
                glyphs: vec![
                    GlyphRect {
                        x: 0.0,
                        y: 10.0,
                        width: 5.0,
                        height: 10.0,
                        ch: 'a',
                    },
                    GlyphRect {
                        x: 5.0,
                        y: 10.0,
                        width: 5.0,
                        height: 10.0,
                        ch: 'p',
                    },
                    GlyphRect {
                        x: 10.0,
                        y: 10.0,
                        width: 5.0,
                        height: 10.0,
                        ch: 'p',
                    },
                    GlyphRect {
                        x: 15.0,
                        y: 10.0,
                        width: 5.0,
                        height: 10.0,
                        ch: 'l',
                    },
                    GlyphRect {
                        x: 20.0,
                        y: 10.0,
                        width: 5.0,
                        height: 10.0,
                        ch: 'e',
                    },
                ],
                links: Vec::new(),
            },
        );

        let query = SearchQuery::new("app".to_string(), false, false).unwrap();
        let matches = execute_search(&query, &geometries, 1);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].page_index.get(), 0);
        assert_eq!(matches[0].match_index_in_doc, 0);
        assert_eq!(matches[0].char_range, 0..3);
        assert_eq!(matches[0].glyph_boxes.len(), 3);
    }

    #[test]
    fn execute_search_handles_empty_geometry() {
        let geometries = HashMap::new();
        let query = SearchQuery::new("app".to_string(), false, false).unwrap();
        let matches = execute_search(&query, &geometries, 1);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_match_rotation() {
        assert_eq!(next_match(0, 5), 1);
        assert_eq!(next_match(4, 5), 0); // circular
        assert_eq!(next_match(0, 0), 0);

        assert_eq!(prev_match(1, 5), 0);
        assert_eq!(prev_match(0, 5), 4); // circular
        assert_eq!(prev_match(0, 0), 0);
    }

    #[test]
    fn test_match_summary() {
        assert_eq!(match_summary(0, 0), "0 / 0");
        assert_eq!(match_summary(0, 5), "1 / 5");
        assert_eq!(match_summary(4, 5), "5 / 5");
    }

    #[test]
    fn get_scroll_target_computes_correct_offset() {
        let match_item = SearchMatch {
            page_index: PageIndex::from_raw(1),
            match_index_in_doc: 0,
            char_range: 0..1,
            glyph_boxes: vec![GlyphRect {
                x: 0.0,
                y: 50.0,
                width: 10.0,
                height: 10.0,
                ch: 't',
            }],
        };

        let layout = ContinuousLayout {
            pages: vec![
                PageLayoutBox {
                    page_index: PageIndex::from_raw(0),
                    y_offset: 0.0,
                    width: 100,
                    height: 100,
                },
                PageLayoutBox {
                    page_index: PageIndex::from_raw(1),
                    y_offset: 110.0,
                    width: 100,
                    height: 100,
                },
            ],
            total_height: 210.0,
            max_width: 100,
        };

        let target = get_scroll_target_for_match(&match_item, &layout);
        assert_eq!(target, Some(160.0)); // 110.0 + 50.0
    }
}
