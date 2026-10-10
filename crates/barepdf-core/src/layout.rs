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
            // First page is cover page standing alone (on the right in LTR, on the left in RTL)
            let (cover_left, cover_right) = match reading_direction {
                ReadingDirection::LeftToRight => (None, Some(PageIndex::from_raw(0))),
                ReadingDirection::RightToLeft => (Some(PageIndex::from_raw(0)), None),
            };
            pairings.push(PagePairing {
                left: cover_left,
                right: cover_right,
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

pub const MAX_LAYOUT_DIMENSION: f32 = 16_384.0;

#[must_use]
#[allow(clippy::cast_precision_loss)] // Viewport pixels are bounded by the UI and converted for PDF point math.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Finite positive values are bounded by MAX_LAYOUT_DIMENSION.
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

    let raw_w = (page_width_pts * scale).max(1.0);
    let raw_h = (page_height_pts * scale).max(1.0);
    let max_edge = raw_w.max(raw_h);
    let clamp_scale = if max_edge > MAX_LAYOUT_DIMENSION {
        MAX_LAYOUT_DIMENSION / max_edge
    } else {
        1.0
    };
    let target_w = (raw_w * clamp_scale).round().max(1.0) as u32;
    let target_h = (raw_h * clamp_scale).round().max(1.0) as u32;

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

pub type DocumentLayout = ContinuousLayout;

impl ContinuousLayout {
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // y-coordinates are accumulated in f64 and narrowed to f32 for UI layout.
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
        let gap_f64 = f64::from(gap);
        let mut current_y = gap_f64;
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
                y_offset: current_y as f32,
                width: dims.width,
                height: dims.height,
            });

            current_y += f64::from(dims.height) + gap_f64;
            if dims.width > max_w {
                max_w = dims.width;
            }
        }

        Self {
            pages,
            total_height: current_y as f32,
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
        assert_eq!(
            pairs[0],
            PagePairing {
                left: None,
                right: Some(PageIndex::from_raw(0)),
            }
        );
        assert_eq!(
            pairs[1],
            PagePairing {
                left: Some(PageIndex::from_raw(1)),
                right: Some(PageIndex::from_raw(2)),
            }
        );
        assert_eq!(
            pairs[2],
            PagePairing {
                left: Some(PageIndex::from_raw(3)),
                right: Some(PageIndex::from_raw(4)),
            }
        );
    }

    #[test]
    fn test_book_mode_rtl() {
        let count = PageCount::new(5).unwrap();
        let pairs =
            calculate_page_pairings(ViewingMode::BookMode, ReadingDirection::RightToLeft, count);
        assert_eq!(pairs.len(), 3);
        assert_eq!(
            pairs[0],
            PagePairing {
                left: Some(PageIndex::from_raw(0)),
                right: None,
            }
        );
        assert_eq!(
            pairs[1],
            PagePairing {
                left: Some(PageIndex::from_raw(2)),
                right: Some(PageIndex::from_raw(1)),
            }
        );
        assert_eq!(
            pairs[2],
            PagePairing {
                left: Some(PageIndex::from_raw(4)),
                right: Some(PageIndex::from_raw(3)),
            }
        );
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

    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::float_cmp)]
    fn continuous_layout_f64_accumulation_avoids_drift_across_many_pages() {
        let page_count = 8_000u32;
        let dims = vec![(612.0f32, 792.0f32); page_count as usize];
        let gap = 12.1f32;
        let layout = DocumentLayout::compute(&dims, 800, 1000, ZoomMode::ActualSize, 3.0, gap);
        assert_eq!(layout.pages.len(), page_count as usize);

        let page_h = f64::from(layout.pages[0].height);
        let gap_f64 = f64::from(gap);
        let expected_last_y = (gap_f64 + (f64::from(page_count) - 1.0) * (page_h + gap_f64)) as f32;
        let expected_total = (gap_f64 + f64::from(page_count) * (page_h + gap_f64)) as f32;

        assert_eq!(
            layout.pages[(page_count - 1) as usize].y_offset,
            expected_last_y
        );
        assert_eq!(layout.total_height, expected_total);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn compute_target_dimensions_preserves_aspect_ratio_at_high_zoom() {
        let a4_w = 595.28_f32;
        let a4_h = 841.89_f32;
        let expected_ratio = a4_w / a4_h;

        for zoom in [5.0_f32, 6.0, 8.0] {
            let dims = compute_target_dimensions(
                a4_w,
                a4_h,
                1920,
                1080,
                ZoomMode::Custom(crate::types::ZoomFactor::new(zoom)),
                1.0,
            );
            let actual_ratio = dims.width as f32 / dims.height as f32;
            assert!(
                (actual_ratio - expected_ratio).abs() < 1e-3,
                "Aspect ratio distorted at zoom {zoom}x: got {actual_ratio}, expected {expected_ratio}"
            );
        }
    }
}
