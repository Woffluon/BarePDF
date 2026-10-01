use crate::types::{LinkTarget, PageIndex, PageTextGeometry, TextPosition, TextSelection};

/// Hit-tests normalized top-left page coordinates `(norm_x, norm_y)` against `geometry.links`.
#[must_use]
pub fn hit_test_link(
    geometry: &PageTextGeometry,
    page_width_pts: f32,
    page_height_pts: f32,
    norm_x: f32,
    norm_y: f32,
) -> Option<&LinkTarget> {
    let pdf_x = norm_x * page_width_pts;
    let pdf_y = (1.0 - norm_y) * page_height_pts;
    geometry
        .links
        .iter()
        .find(|link| {
            pdf_x >= link.x
                && pdf_x <= link.x + link.width
                && pdf_y >= link.y
                && pdf_y <= link.y + link.height
        })
        .map(|link| &link.target)
}

pub struct SelectionEngine;

impl SelectionEngine {
    /// Hit-tests normalized top-left page coordinates `(norm_x, norm_y)` against `geometry.links`.
    #[must_use]
    pub fn hit_test_link(
        geometry: &PageTextGeometry,
        page_width_pts: f32,
        page_height_pts: f32,
        norm_x: f32,
        norm_y: f32,
    ) -> Option<&LinkTarget> {
        hit_test_link(geometry, page_width_pts, page_height_pts, norm_x, norm_y)
    }

    /// Hit-tests a point `(x, y)` in PDF page coordinates against page text glyphs.
    /// Returns the 0-based character index closest to the point.
    #[must_use]
    pub fn hit_test(geom: &PageTextGeometry, x: f32, y: f32) -> u32 {
        if geom.glyphs.is_empty() {
            return 0;
        }

        let mut closest_idx = 0u32;
        let mut min_dist_sq = f32::MAX;

        for (idx, g) in geom.glyphs.iter().enumerate() {
            // Check if point is inside bounding box
            if x >= g.x && x <= g.x + g.width && y >= g.y && y <= g.y + g.height {
                return u32::try_from(idx).unwrap_or(u32::MAX);
            }

            // Calculate center distance
            let cx = g.x + g.width * 0.5;
            let cy = g.y + g.height * 0.5;
            let dx = x - cx;
            let dy = y - cy;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest_idx = u32::try_from(idx).unwrap_or(u32::MAX);
            }
        }

        closest_idx
    }

    /// Extends selection to word boundaries around `char_index` (Unicode & Turkish aware).
    #[must_use]
    pub fn select_word(geom: &PageTextGeometry, page: PageIndex, char_index: u32) -> TextSelection {
        if geom.glyphs.is_empty() {
            let pos = TextPosition::new(page, 0);
            return TextSelection::new(pos, pos);
        }

        let len = u32::try_from(geom.glyphs.len()).unwrap_or(u32::MAX);
        let target = char_index.min(len.saturating_sub(1));

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '\'' || c == '’';

        let mut start = target;
        while start > 0 {
            let ch = geom.glyphs[start as usize - 1].ch;
            if !is_word_char(ch) {
                break;
            }
            start -= 1;
        }

        let mut end = target;
        while (end as usize) < geom.glyphs.len() {
            let ch = geom.glyphs[end as usize].ch;
            if !is_word_char(ch) {
                break;
            }
            end += 1;
        }

        if start == end && (end as usize) < geom.glyphs.len() {
            end += 1;
        }

        TextSelection::new(TextPosition::new(page, start), TextPosition::new(page, end))
    }

    /// Selects the line containing `char_index`.
    #[must_use]
    pub fn select_line(geom: &PageTextGeometry, page: PageIndex, char_index: u32) -> TextSelection {
        if geom.glyphs.is_empty() {
            let pos = TextPosition::new(page, 0);
            return TextSelection::new(pos, pos);
        }

        let last = u32::try_from(geom.glyphs.len() - 1).unwrap_or(u32::MAX);
        let target = char_index.min(last);
        let target_glyph = &geom.glyphs[target as usize];
        let line_y = target_glyph.y;
        let line_h = target_glyph.height.max(1.0);

        let mut start = target;
        while start > 0 {
            let prev = &geom.glyphs[start as usize - 1];
            if (prev.y - line_y).abs() > line_h * 0.8 || prev.ch == '\n' || prev.ch == '\r' {
                break;
            }
            start -= 1;
        }

        let mut end = target;
        while (end as usize) < geom.glyphs.len() {
            let curr = &geom.glyphs[end as usize];
            if (curr.y - line_y).abs() > line_h * 0.8 || curr.ch == '\n' || curr.ch == '\r' {
                break;
            }
            end += 1;
        }

        TextSelection::new(TextPosition::new(page, start), TextPosition::new(page, end))
    }

    /// Formats selected text across pages into clipboard string in page order.
    #[must_use]
    pub fn get_selected_text(selection: &TextSelection, geometries: &[PageTextGeometry]) -> String {
        let mut ordered_geometries: Vec<_> = geometries.iter().collect();
        ordered_geometries.sort_unstable_by_key(|geom| geom.page_index);
        Self::get_selected_text_in_page_order(selection, &ordered_geometries)
    }

    /// Formats selected text from page-ordered borrowed geometries without cloning them.
    #[must_use]
    pub fn get_selected_text_in_page_order(
        selection: &TextSelection,
        geometries: &[&PageTextGeometry],
    ) -> String {
        if selection.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        let (start_pos, end_pos) = selection.start_and_end();

        for geom in geometries {
            if geom.page_index < start_pos.page || geom.page_index > end_pos.page {
                continue;
            }

            let start_idx = if geom.page_index == start_pos.page {
                start_pos.char_index
            } else {
                0
            };

            let end_idx = if geom.page_index == end_pos.page {
                end_pos.char_index
            } else {
                u32::try_from(geom.glyphs.len()).unwrap_or(u32::MAX)
            };

            let page_glyphs = &geom.glyphs;
            let slice_end = (end_idx as usize).min(page_glyphs.len());
            let slice_start = (start_idx as usize).min(slice_end);

            if !result.is_empty() && slice_start < slice_end {
                result.push('\n');
            }

            for g in &page_glyphs[slice_start..slice_end] {
                result.push(g.ch);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GlyphRect, PageIndex, PageLink};

    fn sample_geometry() -> PageTextGeometry {
        let text = "Hello BarePDF! Türkçe metin testi.";
        let mut glyphs = Vec::new();
        let mut x = 10.0f32;
        for ch in text.chars() {
            glyphs.push(GlyphRect {
                x,
                y: 100.0,
                width: 8.0,
                height: 12.0,
                ch,
            });
            x += 8.0;
        }
        PageTextGeometry {
            page_index: PageIndex::zero(),
            glyphs,
            links: Vec::new(),
        }
    }

    #[test]
    fn test_hit_test() {
        let geom = sample_geometry();
        let idx = SelectionEngine::hit_test(&geom, 12.0, 102.0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_select_word_turkish() {
        let geom = sample_geometry();
        // "Türkçe" starts around index 15
        let sel = SelectionEngine::select_word(&geom, PageIndex::zero(), 16);
        let txt = SelectionEngine::get_selected_text(&sel, &[geom]);
        assert_eq!(txt, "Türkçe");
    }

    #[test]
    fn select_line_clamps_out_of_range_index() {
        let geom = sample_geometry();

        let selection = SelectionEngine::select_line(&geom, PageIndex::zero(), u32::MAX);

        let (_, end) = selection.start_and_end();
        assert!(end.char_index <= u32::try_from(geom.glyphs.len()).unwrap());
    }

    #[test]
    fn selected_text_is_sorted_by_page_index() {
        let page_one = PageTextGeometry {
            page_index: PageIndex::from_raw(1),
            glyphs: vec![GlyphRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                ch: 'B',
            }],
            links: Vec::new(),
        };
        let page_zero = PageTextGeometry {
            page_index: PageIndex::zero(),
            glyphs: vec![GlyphRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                ch: 'A',
            }],
            links: Vec::new(),
        };
        let selection = TextSelection::new(
            TextPosition::new(PageIndex::zero(), 0),
            TextPosition::new(PageIndex::from_raw(1), 1),
        );

        assert_eq!(
            SelectionEngine::get_selected_text(&selection, &[page_one, page_zero]),
            "A\nB"
        );
    }

    #[test]
    fn hit_test_link_converts_top_left_normalized_to_pdf_bottom_left() {
        let mut geom = sample_geometry();
        geom.links.push(PageLink {
            x: 100.0,
            y: 600.0,
            width: 50.0,
            height: 20.0,
            target: LinkTarget::Url("https://example.com".into()),
        });
        geom.links.push(PageLink {
            x: 200.0,
            y: 100.0,
            width: 40.0,
            height: 30.0,
            target: LinkTarget::Page(PageIndex::from_raw(3)),
        });

        let page_w = 500.0;
        let page_h = 800.0;

        // (norm_x = 0.25 -> pdf_x = 125.0, norm_y = 0.2375 -> pdf_y = 610.0)
        assert_eq!(
            hit_test_link(&geom, page_w, page_h, 0.25, 0.2375),
            Some(&LinkTarget::Url("https://example.com".into()))
        );
        // (norm_x = 0.44 -> pdf_x = 220.0, norm_y = 0.85 -> pdf_y = 120.0)
        assert_eq!(
            hit_test_link(&geom, page_w, page_h, 0.44, 0.85),
            Some(&LinkTarget::Page(PageIndex::from_raw(3)))
        );
        // Miss
        assert_eq!(hit_test_link(&geom, page_w, page_h, 0.9, 0.9), None);
    }
}
