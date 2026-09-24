use barepdf_core::limits::sanitize_render_dimensions;
use barepdf_core::search::{SearchCancellationToken, SearchQuery};
use barepdf_core::types::{GlyphRect, PageIndex, PageTextGeometry};

#[test]
fn test_search_query_new() {
    assert!(SearchQuery::new(String::new(), false, false).is_none());
    assert!(SearchQuery::new("test".to_string(), false, false).is_some());
}

#[test]
fn test_search_cancellation() {
    let token = SearchCancellationToken::new();
    assert!(!token.is_cancelled());
    token.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn test_sanitize_render_dimensions() {
    assert!(sanitize_render_dimensions(800, 600).is_ok());
    assert!(sanitize_render_dimensions(0, 600).is_err());
    assert!(sanitize_render_dimensions(800, 0).is_err());
    assert!(sanitize_render_dimensions(8193, 600).is_err());
    assert!(sanitize_render_dimensions(800, 8193).is_err());
}

#[test]
fn test_find_in_geometry() {
    let text = "İstanbul'da bir Kedi ve şeker yedim, KATKI değil kat!";
    let mut glyphs = Vec::new();
    for (i, ch) in text.chars().enumerate() {
        glyphs.push(GlyphRect {
            x: i as f32,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            ch,
        });
    }

    let geom = PageTextGeometry {
        page_index: PageIndex::zero(),
        glyphs,
    };

    // Case insensitive match
    let q = SearchQuery::new("istanbul".to_string(), false, false).unwrap();
    let matches = q.find_in_geometry(&geom);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], 0..8);

    // Turkish special chars
    let q = SearchQuery::new("ŞEKER".to_string(), false, false).unwrap();
    let matches = q.find_in_geometry(&geom);
    assert_eq!(matches.len(), 1);

    // Case sensitive match
    let q = SearchQuery::new("Kedi".to_string(), true, false).unwrap();
    let matches = q.find_in_geometry(&geom);
    assert_eq!(matches.len(), 1);

    let q = SearchQuery::new("kedi".to_string(), true, false).unwrap();
    let matches = q.find_in_geometry(&geom);
    assert_eq!(matches.len(), 0);

    // Whole word test
    let q = SearchQuery::new("kat".to_string(), false, true).unwrap();
    let matches = q.find_in_geometry(&geom);
    assert_eq!(matches.len(), 1);

    // Check empty geometry
    let geom_empty = PageTextGeometry {
        page_index: PageIndex::zero(),
        glyphs: vec![],
    };
    let q = SearchQuery::new("a".to_string(), false, false).unwrap();
    assert_eq!(q.find_in_geometry(&geom_empty).len(), 0);
}
