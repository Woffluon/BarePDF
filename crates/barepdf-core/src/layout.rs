use crate::types::{
    PageCount, PageIndex, ReadingDirection, RenderDimensions, ViewingMode, ZoomMode,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PagePairing {
    pub left: Option<PageIndex>,
    pub right: Option<PageIndex>,
}

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

    let target_w = ((page_width_pts * scale).round() as u32).max(1);
    let target_h = ((page_height_pts * scale).round() as u32).max(1);

    RenderDimensions::new(target_w, target_h).unwrap_or(RenderDimensions {
        width: 1,
        height: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            pairs[1],
            PagePairing {
                left: Some(PageIndex::from_raw(2)),
                right: Some(PageIndex::from_raw(3))
            }
        );
        assert_eq!(
            pairs[2],
            PagePairing {
                left: Some(PageIndex::from_raw(4)),
                right: None
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
                left: Some(PageIndex::from_raw(0)),
                right: None
            }
        );
        assert_eq!(
            pairs[1],
            PagePairing {
                left: Some(PageIndex::from_raw(1)),
                right: Some(PageIndex::from_raw(2))
            }
        );
        assert_eq!(
            pairs[2],
            PagePairing {
                left: Some(PageIndex::from_raw(3)),
                right: Some(PageIndex::from_raw(4))
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
        assert_eq!(
            pairs[1],
            PagePairing {
                left: Some(PageIndex::from_raw(3)),
                right: Some(PageIndex::from_raw(2))
            }
        );
    }
}
