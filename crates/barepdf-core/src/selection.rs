use crate::types::{PageIndex, PageTextGeometry, TextPosition, TextSelection};

pub struct SelectionEngine;

impl SelectionEngine {
    /// Hit-tests a point `(x, y)` in PDF page coordinates against page text glyphs.
    /// Returns the 0-based character index closest to the point.
    pub fn hit_test(geom: &PageTextGeometry, x: f32, y: f32) -> u32 {
        if geom.glyphs.is_empty() {
            return 0;
        }

        let mut closest_idx = 0u32;
        let mut min_dist_sq = f32::MAX;

        for (idx, g) in geom.glyphs.iter().enumerate() {
            // Check if point is inside bounding box
            if x >= g.x && x <= g.x + g.width && y >= g.y && y <= g.y + g.height {
                return idx as u32;
            }

            // Calculate center distance
            let cx = g.x + g.width * 0.5;
            let cy = g.y + g.height * 0.5;
            let dx = x - cx;
            let dy = y - cy;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest_idx = idx as u32;
            }
        }

        closest_idx
    }

    /// Extends selection to word boundaries around `char_index` (Unicode & Turkish aware).
    pub fn select_word(geom: &PageTextGeometry, page: PageIndex, char_index: u32) -> TextSelection {
        if geom.glyphs.is_empty() {
            let pos = TextPosition::new(page, 0);
            return TextSelection::new(pos, pos);
        }

        let len = geom.glyphs.len() as u32;
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
    pub fn select_line(geom: &PageTextGeometry, page: PageIndex, char_index: u32) -> TextSelection {
        if geom.glyphs.is_empty() {
            let pos = TextPosition::new(page, 0);
            return TextSelection::new(pos, pos);
        }

        let target_glyph = &geom.glyphs[char_index.min(geom.glyphs.len() as u32 - 1) as usize];
        let line_y = target_glyph.y;
        let line_h = target_glyph.height.max(1.0);

        let mut start = char_index;
        while start > 0 {
            let prev = &geom.glyphs[start as usize - 1];
            if (prev.y - line_y).abs() > line_h * 0.8 || prev.ch == '\n' || prev.ch == '\r' {
                break;
            }
            start -= 1;
        }

        let mut end = char_index;
        while (end as usize) < geom.glyphs.len() {
            let curr = &geom.glyphs[end as usize];
            if (curr.y - line_y).abs() > line_h * 0.8 || curr.ch == '\n' || curr.ch == '\r' {
                break;
            }
            end += 1;
        }

        TextSelection::new(TextPosition::new(page, start), TextPosition::new(page, end))
    }

    /// Formats selected text across pages into clipboard string.
    pub fn get_selected_text(selection: &TextSelection, geometries: &[PageTextGeometry]) -> String {
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
                geom.glyphs.len() as u32
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
    use crate::types::{GlyphRect, PageIndex};

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
}
