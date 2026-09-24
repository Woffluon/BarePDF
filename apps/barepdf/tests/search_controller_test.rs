#[path = "../src/controllers/search_controller.rs"]
mod search_controller;

use std::collections::HashMap;

use barepdf_core::layout::{ContinuousLayout, PageLayoutBox};
use barepdf_core::search::{SearchMatch, SearchQuery};
use barepdf_core::types::{GlyphRect, PageIndex, PageTextGeometry};
use search_controller::SearchController;

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
        },
    );

    let query = SearchQuery::new("app".to_string(), false, false).unwrap();
    let matches = SearchController::execute_search(&query, &geometries, 1);

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
    let matches = SearchController::execute_search(&query, &geometries, 1);
    assert!(matches.is_empty());
}

#[test]
fn test_match_rotation() {
    assert_eq!(SearchController::next_match(0, 5), 1);
    assert_eq!(SearchController::next_match(4, 5), 0); // circular
    assert_eq!(SearchController::next_match(0, 0), 0);

    assert_eq!(SearchController::prev_match(1, 5), 0);
    assert_eq!(SearchController::prev_match(0, 5), 4); // circular
    assert_eq!(SearchController::prev_match(0, 0), 0);
}

#[test]
fn test_match_summary() {
    assert_eq!(SearchController::match_summary(0, 0), "0 / 0");
    assert_eq!(SearchController::match_summary(0, 5), "1 / 5");
    assert_eq!(SearchController::match_summary(4, 5), "5 / 5");
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

    let target = SearchController::get_scroll_target_for_match(&match_item, &layout);
    assert_eq!(target, Some(160.0)); // 110.0 + 50.0
}
