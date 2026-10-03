use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::types::{GlyphRect, PageIndex, PageTextGeometry};

#[derive(Clone)]
pub struct SearchCancellationToken(Arc<AtomicBool>);

impl SearchCancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl Default for SearchCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SearchQuery {
    pub text: String,
    pub match_case: bool,
    pub whole_word: bool,
}

impl SearchQuery {
    #[must_use]
    pub fn new(text: String, match_case: bool, whole_word: bool) -> Option<Self> {
        if text.is_empty() {
            None
        } else {
            Some(Self {
                text,
                match_case,
                whole_word,
            })
        }
    }

    #[must_use]
    pub fn find_in_geometry(&self, geom: &PageTextGeometry) -> Vec<Range<u32>> {
        let mut matches = Vec::new();

        if geom.glyphs.is_empty() {
            return matches;
        }

        let mut haystack = String::with_capacity(geom.glyphs.len());
        let mut byte_to_glyph = Vec::with_capacity(geom.glyphs.len() + 1);
        let mut encode_buf = [0u8; 4];

        for (i, glyph) in geom.glyphs.iter().enumerate() {
            let glyph_idx = u32::try_from(i).unwrap_or(u32::MAX);
            if self.match_case {
                let encoded = glyph.ch.encode_utf8(&mut encode_buf);
                byte_to_glyph.extend(std::iter::repeat_n(glyph_idx, encoded.len()));
                haystack.push_str(encoded);
            } else {
                let start_len = haystack.len();
                push_lower_char(&mut haystack, glyph.ch, &mut encode_buf);
                let added_bytes = haystack.len() - start_len;
                byte_to_glyph.extend(std::iter::repeat_n(glyph_idx, added_bytes));
            }
        }
        byte_to_glyph.push(u32::try_from(geom.glyphs.len()).unwrap_or(u32::MAX));

        let mut needle_buf = String::new();
        let needle: &str = if self.match_case {
            &self.text
        } else {
            needle_buf.reserve(self.text.len());
            for ch in self.text.chars() {
                push_lower_char(&mut needle_buf, ch, &mut encode_buf);
            }
            &needle_buf
        };

        let mut start_idx = 0;
        while let Some(match_idx) = haystack[start_idx..].find(needle) {
            let absolute_match_idx = start_idx + match_idx;
            let end_match_idx = absolute_match_idx + needle.len();

            let glyph_start = byte_to_glyph[absolute_match_idx];
            let glyph_end = byte_to_glyph[end_match_idx];

            let mut is_whole_word = true;
            if self.whole_word {
                let prev_char_alphanumeric = if glyph_start > 0 {
                    geom.glyphs[(glyph_start - 1) as usize].ch.is_alphanumeric()
                } else {
                    false
                };

                let next_char_alphanumeric = if (glyph_end as usize) < geom.glyphs.len() {
                    geom.glyphs[glyph_end as usize].ch.is_alphanumeric()
                } else {
                    false
                };

                if prev_char_alphanumeric || next_char_alphanumeric {
                    is_whole_word = false;
                }
            }

            if is_whole_word {
                matches.push(glyph_start..glyph_end);
            }

            start_idx = absolute_match_idx + needle.chars().next().map_or(1, char::len_utf8);
        }

        matches
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchMatch {
    pub page_index: PageIndex,
    pub match_index_in_doc: usize,
    pub char_range: Range<u32>,
    pub glyph_boxes: Vec<GlyphRect>,
}

fn push_lower_char(out: &mut String, c: char, buf: &mut [u8; 4]) {
    match c {
        'I' => out.push('ı'),
        'İ' => out.push('i'),
        c => {
            for lower in c.to_lowercase() {
                out.push_str(lower.encode_utf8(buf));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_in_geometry_handles_turkish_and_unicode_without_glyph_string_allocations() {
        let text = "İstanbul Iğdır Straße";
        let glyphs = text
            .chars()
            .enumerate()
            .map(|(i, ch)| GlyphRect {
                x: f32::from(u16::try_from(i).unwrap_or(u16::MAX)),
                y: 0.0,
                width: 1.0,
                height: 1.0,
                ch,
            })
            .collect();
        let geom = PageTextGeometry {
            page_index: PageIndex::zero(),
            glyphs,
            links: Vec::new(),
        };

        let q_tr = SearchQuery::new("ığdır".to_string(), false, true).unwrap();
        assert_eq!(q_tr.find_in_geometry(&geom), vec![9..14]);

        let q_ist = SearchQuery::new("istanbul".to_string(), false, true).unwrap();
        assert_eq!(q_ist.find_in_geometry(&geom), vec![0..8]);
    }
}
