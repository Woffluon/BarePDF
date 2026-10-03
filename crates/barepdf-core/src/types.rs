use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::num::NonZeroU64;

use crate::limits::MAX_DOCUMENT_PAGES;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PageCount(u32);

impl PageCount {
    pub const ONE: Self = Self(1);

    #[must_use]
    pub const fn one() -> Self {
        Self::ONE
    }

    #[must_use]
    pub const fn new(count: u32) -> Option<Self> {
        if count > 0 {
            Some(Self(count))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for PageCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PageIndex(u32);

impl PageIndex {
    #[must_use]
    pub const fn new(index: u32, page_count: PageCount) -> Option<Self> {
        if index < page_count.get() && index < MAX_DOCUMENT_PAGES {
            Some(Self(index))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn checked_new(index: u32) -> Option<Self> {
        if index < MAX_DOCUMENT_PAGES {
            Some(Self(index))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Creates a `PageIndex` directly from a 0-based index, clamping out-of-bounds values to the
    /// maximum supported page index (`MAX_DOCUMENT_PAGES - 1`).
    ///
    /// Prefer [`PageIndex::new`] or [`PageIndex::checked_new`] when validating untrusted input.
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        let max_idx = MAX_DOCUMENT_PAGES - 1;
        if index <= max_idx {
            Self(index)
        } else {
            Self(max_idx)
        }
    }

    #[must_use]
    pub const fn is_within_limit(self) -> bool {
        self.0 < MAX_DOCUMENT_PAGES
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn next(self, page_count: u32) -> Option<Self> {
        let next_idx = self.0.checked_add(1)?;
        if next_idx < page_count && next_idx < MAX_DOCUMENT_PAGES {
            Some(Self(next_idx))
        } else {
            None
        }
    }

    #[must_use]
    pub fn prev(self) -> Option<Self> {
        self.0.checked_sub(1).map(Self)
    }
}

impl fmt::Display for PageIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.checked_add(1) {
            Some(display_index) => write!(f, "{display_index}"),
            None => write!(f, "{}", self.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ZoomFactor(f32);

impl ZoomFactor {
    pub const MIN: f32 = 0.25;
    pub const MAX: f32 = 3.5;
    pub const DEFAULT: f32 = 1.0;
    pub const STEP: f32 = 0.25;

    #[must_use]
    pub fn new(factor: f32) -> Self {
        if factor.is_finite() {
            Self(factor.clamp(Self::MIN, Self::MAX))
        } else {
            Self::default()
        }
    }

    #[must_use]
    pub fn get(self) -> f32 {
        self.0
    }

    #[must_use]
    pub fn factor(self) -> f32 {
        self.0
    }

    #[must_use]
    pub fn zoom_in(self) -> Self {
        Self::new(self.0 + Self::STEP)
    }

    #[must_use]
    pub fn zoom_out(self) -> Self {
        Self::new(self.0 - Self::STEP)
    }
}

impl Default for ZoomFactor {
    fn default() -> Self {
        Self(1.0)
    }
}

impl<'de> Deserialize<'de> for ZoomFactor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(f32::deserialize(deserializer)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Rotation {
    #[default]
    Degrees0,
    Degrees90,
    Degrees180,
    Degrees270,
}

impl Rotation {
    #[must_use]
    pub const fn degrees(self) -> u32 {
        match self {
            Self::Degrees0 => 0,
            Self::Degrees90 => 90,
            Self::Degrees180 => 180,
            Self::Degrees270 => 270,
        }
    }

    #[must_use]
    pub const fn rotate_cw(self) -> Self {
        match self {
            Self::Degrees0 => Self::Degrees90,
            Self::Degrees90 => Self::Degrees180,
            Self::Degrees180 => Self::Degrees270,
            Self::Degrees270 => Self::Degrees0,
        }
    }

    #[must_use]
    pub const fn rotate_ccw(self) -> Self {
        match self {
            Self::Degrees0 => Self::Degrees270,
            Self::Degrees90 => Self::Degrees0,
            Self::Degrees180 => Self::Degrees90,
            Self::Degrees270 => Self::Degrees180,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderDimensions {
    pub width: u32,
    pub height: u32,
}

impl RenderDimensions {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if width > 0 && height > 0 {
            Some(Self { width, height })
        } else {
            None
        }
    }

    #[must_use]
    pub fn estimated_bytes(self) -> Option<usize> {
        let pixels = (self.width as usize).checked_mul(self.height as usize)?;
        pixels.checked_mul(4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryBudget(usize);

impl MemoryBudget {
    pub const DEFAULT_BYTES: usize = 96 * 1024 * 1024; // 96 MB

    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self(Self::DEFAULT_BYTES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DocumentId(u64);

impl DocumentId {
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(u64);

impl RequestId {
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TabId(u64);

impl TabId {
    #[must_use]
    pub const fn new(id: u64) -> Option<Self> {
        if id > 0 {
            Some(Self(id))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn from_non_zero(id: NonZeroU64) -> Self {
        Self(id.get())
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Converts this `TabId` into a positive `i32` suitable for Slint UI models,
    /// returning `None` if the value is zero or exceeds `i32::MAX`.
    #[must_use]
    pub fn to_slint_id(self) -> Option<i32> {
        if self.0 == 0 {
            return None;
        }
        i32::try_from(self.0).ok()
    }

    /// Reconstructs a `TabId` from a Slint `i32` identifier, rejecting zero or negative values.
    #[must_use]
    pub fn from_slint_id(slint_id: i32) -> Option<Self> {
        if slint_id <= 0 {
            return None;
        }
        u64::try_from(slint_id).ok().and_then(Self::new)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ViewingMode {
    SinglePage,
    #[default]
    ContinuousVertical,
    TwoPageSpread,
    BookMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReadingDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum ZoomMode {
    FitPage,
    #[default]
    FitWidth,
    ActualSize,
    Custom(ZoomFactor),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WindowMode {
    #[default]
    Normal,
    FullScreen,
    Presentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SidebarTab {
    #[default]
    Thumbnails,
    Outline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TextPosition {
    pub page: PageIndex,
    pub char_index: u32,
}

impl TextPosition {
    #[must_use]
    pub fn new(page: PageIndex, char_index: u32) -> Self {
        Self { page, char_index }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSelection {
    pub anchor: TextPosition,
    pub focus: TextPosition,
}

impl TextSelection {
    #[must_use]
    pub fn new(anchor: TextPosition, focus: TextPosition) -> Self {
        Self { anchor, focus }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.anchor == self.focus
    }

    #[must_use]
    pub fn start_and_end(&self) -> (TextPosition, TextPosition) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    #[must_use]
    pub fn range_for_page(&self, page: PageIndex) -> Option<(u32, u32)> {
        let (start, end) = self.start_and_end();
        if page < start.page || page > end.page {
            return None;
        }

        let start_idx = if page == start.page {
            start.char_index
        } else {
            0
        };
        let end_idx = if page == end.page {
            end.char_index
        } else {
            u32::MAX
        };

        if start_idx <= end_idx {
            Some((start_idx, end_idx))
        } else {
            Some((end_idx, start_idx))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlyphRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub ch: char,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkTarget {
    Url(String),
    Page(PageIndex),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageLink {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub target: LinkTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageTextGeometry {
    pub page_index: PageIndex,
    pub glyphs: Vec<GlyphRect>,
    #[serde(default)]
    pub links: Vec<PageLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InkColor {
    #[default]
    Black,
    Red,
    Blue,
    Yellow,
}

impl InkColor {
    #[must_use]
    pub const fn rgba(self) -> (u8, u8, u8, u8) {
        match self {
            Self::Black => (20, 20, 20, 255),
            Self::Red => (220, 38, 38, 255),
            Self::Blue => (37, 99, 235, 255),
            Self::Yellow => (250, 204, 21, 140),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InkStroke {
    pub page: PageIndex,
    /// Normalized page coordinates in `[0.0, 1.0]` (top-left origin `(x_norm, y_norm)`).
    pub points: Vec<(f32, f32)>,
    pub color: InkColor,
    pub width_pts: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HighlightQuad {
    pub page: PageIndex,
    /// Normalized top-left origin `(x_norm, y_norm, w_norm, h_norm)` in `[0.0, 1.0]`.
    pub x_norm: f32,
    pub y_norm: f32,
    pub w_norm: f32,
    pub h_norm: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignaturePayload {
    /// Normalized points in `[0.0, 1.0]` within the signature stamp box.
    Drawn(Vec<Vec<(f32, f32)>>),
    /// Raw RGBA pixels + dimensions.
    Image {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignatureStamp {
    pub page: PageIndex,
    /// Normalized top-left origin box `(x_norm, y_norm, w_norm, h_norm)` in `[0.0, 1.0]`.
    pub x_norm: f32,
    pub y_norm: f32,
    pub w_norm: f32,
    pub h_norm: f32,
    pub payload: SignaturePayload,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DocumentAnnotations {
    pub strokes: Vec<InkStroke>,
    pub highlights: Vec<HighlightQuad>,
    pub signatures: Vec<SignatureStamp>,
}

impl DocumentAnnotations {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.highlights.is_empty() && self.signatures.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_factor_normalizes_non_finite_values() {
        assert_eq!(ZoomFactor::new(f32::NAN), ZoomFactor::default());
        assert_eq!(ZoomFactor::new(f32::INFINITY), ZoomFactor::default());
    }

    #[test]
    fn zoom_factor_uses_fixed_steps_and_bounds() {
        assert_eq!(ZoomFactor::new(0.25).zoom_out(), ZoomFactor::new(0.25));
        assert_eq!(ZoomFactor::new(0.25).zoom_in(), ZoomFactor::new(0.5));
        assert_eq!(ZoomFactor::new(1.75).zoom_in(), ZoomFactor::new(2.0));
        assert_eq!(ZoomFactor::new(3.25).zoom_in(), ZoomFactor::new(3.5));
        assert_eq!(ZoomFactor::new(3.5).zoom_in(), ZoomFactor::new(3.5));
    }

    #[test]
    fn persisted_zoom_values_remain_deserializable() {
        let zoom = ZoomFactor::deserialize(serde::de::value::F32Deserializer::<
            serde::de::value::Error,
        >::new(10.0))
        .expect("valid persisted zoom");

        assert_eq!(zoom, ZoomFactor::new(3.5));
    }

    #[test]
    fn page_index_next_and_prev() {
        let page = PageIndex::from_raw(2);
        assert_eq!(page.next(5), Some(PageIndex::from_raw(3)));
        assert_eq!(page.next(3), None);
        assert_eq!(page.prev(), Some(PageIndex::from_raw(1)));

        let first = PageIndex::from_raw(0);
        assert_eq!(first.prev(), None);
    }

    #[test]
    fn page_index_checked_new_and_from_raw_enforce_document_page_limit() {
        assert_eq!(PageIndex::checked_new(0).map(PageIndex::get), Some(0));
        assert_eq!(
            PageIndex::checked_new(MAX_DOCUMENT_PAGES - 1).map(PageIndex::get),
            Some(MAX_DOCUMENT_PAGES - 1)
        );
        assert!(PageIndex::checked_new(MAX_DOCUMENT_PAGES).is_none());
        assert!(PageIndex::checked_new(u32::MAX).is_none());

        let clamped = PageIndex::from_raw(MAX_DOCUMENT_PAGES + 50);
        assert_eq!(clamped.get(), MAX_DOCUMENT_PAGES - 1);
        assert!(clamped.is_within_limit());
        assert!(clamped.next(MAX_DOCUMENT_PAGES + 100).is_none());

        let large_count = PageCount::new(MAX_DOCUMENT_PAGES + 10).unwrap();
        assert!(PageIndex::new(MAX_DOCUMENT_PAGES, large_count).is_none());
        assert_eq!(
            PageIndex::new(MAX_DOCUMENT_PAGES - 1, large_count).map(PageIndex::get),
            Some(MAX_DOCUMENT_PAGES - 1)
        );
    }

    #[test]
    fn tab_id_slint_conversions_prevent_truncation_and_invalid_ids() {
        assert!(TabId::new(0).is_none());
        let valid = TabId::new(42).expect("non-zero tab id");
        assert_eq!(valid.get(), 42);
        assert_eq!(valid.to_slint_id(), Some(42));
        assert_eq!(TabId::from_slint_id(42), Some(valid));

        let max_slint = TabId::new(i32::MAX as u64).unwrap();
        assert_eq!(max_slint.to_slint_id(), Some(i32::MAX));
        assert_eq!(TabId::from_slint_id(i32::MAX), Some(max_slint));

        let overflow = TabId::new((i32::MAX as u64) + 1).unwrap();
        assert_eq!(overflow.to_slint_id(), None);

        assert_eq!(TabId::from_slint_id(0), None);
        assert_eq!(TabId::from_slint_id(-1), None);
        assert_eq!(TabId::from_slint_id(i32::MIN), None);
    }

    #[test]
    fn rotation_ccw_cycles_correctly() {
        assert_eq!(Rotation::Degrees0.rotate_ccw(), Rotation::Degrees270);
        assert_eq!(Rotation::Degrees90.rotate_ccw(), Rotation::Degrees0);
        assert_eq!(Rotation::Degrees180.rotate_ccw(), Rotation::Degrees90);
        assert_eq!(Rotation::Degrees270.rotate_ccw(), Rotation::Degrees180);
    }

    #[test]
    fn document_annotations_is_empty_and_ink_rgba() {
        let mut annotations = DocumentAnnotations::default();
        assert!(annotations.is_empty());
        assert_eq!(InkColor::Black.rgba(), (20, 20, 20, 255));
        assert_eq!(InkColor::Red.rgba(), (220, 38, 38, 255));
        assert_eq!(InkColor::Blue.rgba(), (37, 99, 235, 255));
        assert_eq!(InkColor::Yellow.rgba(), (250, 204, 21, 140));

        annotations.highlights.push(HighlightQuad {
            page: PageIndex::zero(),
            x_norm: 0.1,
            y_norm: 0.2,
            w_norm: 0.3,
            h_norm: 0.05,
        });
        assert!(!annotations.is_empty());
    }
}
