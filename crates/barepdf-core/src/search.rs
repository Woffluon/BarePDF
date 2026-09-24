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

        let mut haystack = String::new();
        let mut byte_to_glyph = Vec::new();

        for (i, glyph) in geom.glyphs.iter().enumerate() {
            let ch_str = if self.match_case {
                glyph.ch.to_string()
            } else {
                char_to_lower(glyph.ch)
            };

            for _ in 0..ch_str.len() {
                byte_to_glyph.push(i as u32);
            }
            haystack.push_str(&ch_str);
        }
        byte_to_glyph.push(geom.glyphs.len() as u32);

        let needle = if self.match_case {
            self.text.clone()
        } else {
            self.text.chars().map(char_to_lower).collect::<String>()
        };

        let mut start_idx = 0;
        while let Some(match_idx) = haystack[start_idx..].find(&needle) {
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

            start_idx = absolute_match_idx + needle.chars().next().map_or(1, |c| c.len_utf8());
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

fn char_to_lower(c: char) -> String {
    match c {
        'I' => "ı".to_string(),
        'İ' => "i".to_string(),
        c => c.to_lowercase().collect(),
    }
}
