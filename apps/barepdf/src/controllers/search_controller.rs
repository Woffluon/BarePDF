use std::collections::HashMap;

use barepdf_core::layout::ContinuousLayout;
use barepdf_core::search::{SearchMatch, SearchQuery};
use barepdf_core::types::{PageIndex, PageTextGeometry};

#[allow(dead_code)]
#[derive(Default)]
pub struct SearchState {
    pub query: Option<SearchQuery>,
    pub matches: Vec<SearchMatch>,
    pub active_match_index: usize,
    pub is_bar_visible: bool,
}

pub struct SearchController;

impl SearchController {
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
}
