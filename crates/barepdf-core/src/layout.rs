use crate::types::{
    PageCount, PageIndex, ReadingDirection, RenderDimensions, ViewingMode, ZoomMode,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PagePairing {
    pub left: Option<PageIndex>,
    pub right: Option<PageIndex>,
}

#[must_use]
pub fn calculate_page_pairings(
    viewing_mode: ViewingMode,
    reading_direction: ReadingDirection,
    page_count: PageCount,
) -> Vec<PagePairing> {
    let count = page_count.get();
    let mut pairings = Vec::new();

    match viewing_mode {
        ViewingMode::SinglePage | ViewingMode::ContinuousVertical => {
            for i in 0..count {
                pairings.push(PagePairing {
                    left: Some(PageIndex::from_raw(i)),
                    right: None,
                });
            }
        }
        ViewingMode::TwoPageSpread => {
            let mut i = 0;
            while i < count {
                let p1 = PageIndex::from_raw(i);
                let p2 = if i + 1 < count {
                    Some(PageIndex::from_raw(i + 1))
                } else {
                    None
                };

                let (left, right) = match reading_direction {
                    ReadingDirection::LeftToRight => (Some(p1), p2),
                    ReadingDirection::RightToLeft => (p2, Some(p1)),
                };

                pairings.push(PagePairing { left, right });
                i += 2;
            }
        }
        ViewingMode::BookMode => {
            // First page is cover page standing alone
            pairings.push(PagePairing {
                left: Some(PageIndex::from_raw(0)),
                right: None,
            });

            let mut i = 1;
            while i < count {
                let p1 = PageIndex::from_raw(i);
                let p2 = if i + 1 < count {
                    Some(PageIndex::from_raw(i + 1))
                } else {
                    None
                };

                let (left, right) = match reading_direction {
                    ReadingDirection::LeftToRight => (Some(p1), p2),
                    ReadingDirection::RightToLeft => (p2, Some(p1)),
                };

                pairings.push(PagePairing { left, right });
                i += 2;
            }
        }
    }

    pairings
}

#[must_use]
#[allow(clippy::cast_precision_loss)] // Viewport pixels are bounded by the UI and converted for PDF point math.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Finite positive values are clamped to 1..=4096.
pub fn compute_target_dimensions(
    page_width_pts: f32,
    page_height_pts: f32,
    viewport_width: u32,
    viewport_height: u32,
    zoom_mode: ZoomMode,
    dpi_scale: f32,
) -> RenderDimensions {
    let scale = match zoom_mode {
        ZoomMode::ActualSize => dpi_scale,
        ZoomMode::FitWidth => {
            if page_width_pts > 0.0 {
                (viewport_width as f32) / page_width_pts
            } else {
                1.0
            }
        }
        ZoomMode::FitPage => {
            if page_width_pts > 0.0 && page_height_pts > 0.0 {
                let scale_w = (viewport_width as f32) / page_width_pts;
                let scale_h = (viewport_height as f32) / page_height_pts;
                scale_w.min(scale_h)
            } else {
                1.0
            }
        }
        ZoomMode::Custom(factor) => factor.get() * dpi_scale,
    };

    let target_w = ((page_width_pts * scale).round() as u32).clamp(1, 4096);
    let target_h = ((page_height_pts * scale).round() as u32).clamp(1, 4096);

    RenderDimensions::new(target_w, target_h).unwrap_or(RenderDimensions {
        width: 1,
        height: 1,
    })
}

pub const DEFAULT_PAGE_GAP: f32 = 12.0;

#[derive(Debug, Clone, PartialEq)]
pub struct PageLayoutBox {
    pub page_index: PageIndex,
    pub y_offset: f32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ContinuousLayout {
    pub pages: Vec<PageLayoutBox>,
    pub total_height: f32,
    pub max_width: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollAnchor {
    pub page_index: PageIndex,
    pub relative_y_ratio: f32,
}

impl ContinuousLayout {
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Layout coordinates use f32 throughout.
    pub fn compute(
        page_dimensions: &[(f32, f32)],
        viewport_width: u32,
        viewport_height: u32,
        zoom_mode: ZoomMode,
        dpi_scale: f32,
        gap: f32,
    ) -> Self {
        if page_dimensions.is_empty() {
            return Self::default();
        }

        let mut pages = Vec::with_capacity(page_dimensions.len());
        let mut current_y = gap;
        let mut max_w = 0u32;

        for (idx, &(pw, ph)) in page_dimensions.iter().enumerate() {
            let Ok(page_index) = u32::try_from(idx) else {
                break;
            };
            let dims = compute_target_dimensions(
                pw,
                ph,
                viewport_width.saturating_sub(24), // account for margin
                viewport_height,
                zoom_mode,
                dpi_scale,
            );

            pages.push(PageLayoutBox {
                page_index: PageIndex::from_raw(page_index),
                y_offset: current_y,
                width: dims.width,
                height: dims.height,
            });

            current_y += dims.height as f32 + gap;
            if dims.width > max_w {
                max_w = dims.width;
            }
        }

        Self {
            pages,
            total_height: current_y,
            max_width: max_w,
        }
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Layout coordinates use f32 throughout.
    pub fn visible_pages(&self, viewport_top: f32, viewport_height: f32) -> Vec<PageIndex> {
        let viewport_bottom = viewport_top + viewport_height;
        let first = self
            .pages
            .partition_point(|page| page.y_offset + page.height as f32 <= viewport_top);
        self.pages[first..]
            .iter()
            .take_while(|page| page.y_offset <= viewport_bottom)
            .map(|p| p.page_index)
            .collect()
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Layout coordinates use f32 throughout.
    pub fn primary_page(&self, viewport_top: f32, viewport_height: f32) -> PageIndex {
        let viewport_center = viewport_top + viewport_height * 0.5;
        let mut best_page = PageIndex::zero();
        let mut best_dist = f32::MAX;

        let first = self
            .pages
            .partition_point(|page| page.y_offset + page.height as f32 <= viewport_top);
        for page in self.pages[first..]
            .iter()
            .take_while(|page| page.y_offset <= viewport_top + viewport_height)
        {
            let page_center = page.y_offset + page.height as f32 * 0.5;
            let dist = (page_center - viewport_center).abs();
            if dist < best_dist {
                best_dist = dist;
                best_page = page.page_index;
            }
        }

        best_page
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Layout coordinates use f32 throughout.
    pub fn compute_anchor(&self, viewport_top: f32, viewport_height: f32) -> ScrollAnchor {
        let primary = self.primary_page(viewport_top, viewport_height);
        if let Some(page) = self.pages.iter().find(|p| p.page_index == primary) {
            let page_h = (page.height as f32).max(1.0);
            let rel_y = (viewport_top - page.y_offset).clamp(0.0, page_h);
            ScrollAnchor {
                page_index: primary,
                relative_y_ratio: rel_y / page_h,
            }
        } else {
            ScrollAnchor {
                page_index: PageIndex::zero(),
                relative_y_ratio: 0.0,
            }
        }
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Layout coordinates use f32 throughout.
    pub fn restore_anchor(&self, anchor: ScrollAnchor) -> f32 {
        if let Some(page) = self
            .pages
            .iter()
            .find(|p| p.page_index == anchor.page_index)
        {
            page.y_offset + page.height as f32 * anchor.relative_y_ratio.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::float_cmp)] // Fixed small test fixtures are exactly representable.
    fn test_continuous_layout_compute() {
        let dims = vec![(600.0, 800.0), (600.0, 800.0)];
        let layout = ContinuousLayout::compute(&dims, 800, 1000, ZoomMode::FitWidth, 1.0, 10.0);
        assert_eq!(layout.pages.len(), 2);
        assert_eq!(layout.pages[0].y_offset, 10.0);
        assert_eq!(
            layout.pages[1].y_offset,
            10.0 + layout.pages[0].height as f32 + 10.0
        );
    }

    #[test]
    fn test_visible_pages() {
        let dims = vec![(600.0, 800.0), (600.0, 800.0), (600.0, 800.0)];
        let layout = ContinuousLayout::compute(&dims, 800, 1000, ZoomMode::FitWidth, 1.0, 10.0);
        let visible = layout.visible_pages(0.0, 900.0);
        assert!(!visible.is_empty());
        assert_eq!(visible[0], PageIndex::from_raw(0));
    }

    #[test]
    fn test_two_page_spread_ltr() {
        let count = PageCount::new(5).unwrap();
        let pairs = calculate_page_pairings(
            ViewingMode::TwoPageSpread,
            ReadingDirection::LeftToRight,
            count,
        );
        assert_eq!(pairs.len(), 3);
        assert_eq!(
            pairs[0],
            PagePairing {
                left: Some(PageIndex::from_raw(0)),
                right: Some(PageIndex::from_raw(1))
            }
        );
    }

    #[test]
    fn test_book_mode_ltr() {
        let count = PageCount::new(5).unwrap();
        let pairs =
            calculate_page_pairings(ViewingMode::BookMode, ReadingDirection::LeftToRight, count);
        assert_eq!(pairs.len(), 3);
    }

    #[test]
    fn test_two_page_spread_rtl() {
        let count = PageCount::new(4).unwrap();
        let pairs = calculate_page_pairings(
            ViewingMode::TwoPageSpread,
            ReadingDirection::RightToLeft,
            count,
        );
        assert_eq!(
            pairs[0],
            PagePairing {
                left: Some(PageIndex::from_raw(1)),
                right: Some(PageIndex::from_raw(0))
            }
        );
    }
}
