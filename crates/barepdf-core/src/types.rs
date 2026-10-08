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
    pub const MAX: f32 = 8.0;
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
        let next = if self.0 < 2.0 {
            self.0 + 0.25
        } else if self.0 < 4.0 {
            self.0 + 0.50
        } else {
            self.0 + 1.00
        };
        Self::new(next)
    }

    #[must_use]
    pub fn zoom_out(self) -> Self {
        let next = if self.0 > 4.0 {
            self.0 - 1.00
        } else if self.0 > 2.0 {
            self.0 - 0.50
        } else {
            self.0 - 0.25
        };
        Self::new(next)
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

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AnnotationHistory {
    pub undo_stack: Vec<DocumentAnnotations>,
    pub redo_stack: Vec<DocumentAnnotations>,
}

impl AnnotationHistory {
    pub const MAX_STACK_SIZE: usize = 50;

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_snapshot(&mut self, current: DocumentAnnotations) {
        self.redo_stack.clear();
        self.undo_stack.push(current);
        if self.undo_stack.len() > Self::MAX_STACK_SIZE {
            let excess = self.undo_stack.len() - Self::MAX_STACK_SIZE;
            self.undo_stack.drain(0..excess);
        }
    }

    pub fn undo(&mut self, current: DocumentAnnotations) -> Option<DocumentAnnotations> {
        let previous = self.undo_stack.pop()?;
        self.redo_stack.push(current);
        Some(previous)
    }

    pub fn redo(&mut self, current: DocumentAnnotations) -> Option<DocumentAnnotations> {
        let next = self.redo_stack.pop()?;
        self.undo_stack.push(current);
        Some(next)
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

/// Smooths a polyline using quadratic Bézier curve interpolation through midpoints.
#[must_use]
pub fn smooth_ink_points(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if points.len() < 3 {
        return points.to_vec();
    }

    const SUBDIVISIONS: usize = 4;
    let n = points.len();
    let mut smoothed = Vec::with_capacity(1 + n * SUBDIVISIONS);
    let p0 = points[0];
    smoothed.push(p0);

    let mut midpoints = Vec::with_capacity(n - 1);
    for i in 0..(n - 1) {
        let p_curr = points[i];
        let p_next = points[i + 1];
        midpoints.push(((p_curr.0 + p_next.0) * 0.5, (p_curr.1 + p_next.1) * 0.5));
    }

    let mut curr = p0;
    for (i, &mid) in midpoints.iter().enumerate() {
        let ctrl = points[i];
        for step in 1..=SUBDIVISIONS {
            #[allow(clippy::cast_precision_loss)]
            let t = step as f32 / SUBDIVISIONS as f32;
            let one_minus_t = 1.0 - t;
            let x =
                one_minus_t * one_minus_t * curr.0 + 2.0 * one_minus_t * t * ctrl.0 + t * t * mid.0;
            let y =
                one_minus_t * one_minus_t * curr.1 + 2.0 * one_minus_t * t * ctrl.1 + t * t * mid.1;
            smoothed.push((x, y));
        }
        curr = mid;
    }

    let p_last = points[n - 1];
    for step in 1..=SUBDIVISIONS {
        #[allow(clippy::cast_precision_loss)]
        let t = step as f32 / SUBDIVISIONS as f32;
        let one_minus_t = 1.0 - t;
        let x = one_minus_t * one_minus_t * curr.0
            + 2.0 * one_minus_t * t * p_last.0
            + t * t * p_last.0;
        let y = one_minus_t * one_minus_t * curr.1
            + 2.0 * one_minus_t * t * p_last.1
            + t * t * p_last.1;
        smoothed.push((x, y));
    }

    smoothed
}

#[inline]
fn polyline_length(pts: &[(f32, f32)]) -> f32 {
    if pts.len() < 2 {
        return 0.0;
    }
    pts.windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .sum()
}

type Segment2D = ((f32, f32), (f32, f32));

fn clip_segment_against_ellipse(
    a: (f32, f32),
    b: (f32, f32),
    ex: f32,
    ey: f32,
    rx: f32,
    ry: f32,
) -> (Vec<Segment2D>, bool) {
    let a_prime = ((a.0 - ex) / rx, (a.1 - ey) / ry);
    let b_prime = ((b.0 - ex) / rx, (b.1 - ey) / ry);
    let v = (b_prime.0 - a_prime.0, b_prime.1 - a_prime.1);
    let w = a_prime;

    let a_quad = v.0 * v.0 + v.1 * v.1;
    let b_quad = 2.0 * (v.0 * w.0 + v.1 * w.1);
    let c_quad = w.0 * w.0 + w.1 * w.1 - 1.0;

    if a_quad < 1e-12 {
        if c_quad <= 0.0 {
            return (Vec::new(), true);
        }
        return (vec![(a, b)], false);
    }

    let delta = b_quad * b_quad - 4.0 * a_quad * c_quad;
    if delta <= 0.0 {
        return (vec![(a, b)], false);
    }

    let sqrt_delta = delta.sqrt();
    let u1 = (-b_quad - sqrt_delta) / (2.0 * a_quad);
    let u2 = (-b_quad + sqrt_delta) / (2.0 * a_quad);

    if u2 <= 0.0 || u1 >= 1.0 {
        return (vec![(a, b)], false);
    }

    let mut surviving = Vec::with_capacity(2);

    if u1 > 0.0 {
        let end_t = u1.min(1.0);
        let end_pt = (a.0 + end_t * (b.0 - a.0), a.1 + end_t * (b.1 - a.1));
        surviving.push((a, end_pt));
    }

    if u2 < 1.0 {
        let start_t = u2.max(0.0);
        let start_pt = (a.0 + start_t * (b.0 - a.0), a.1 + start_t * (b.1 - a.1));
        surviving.push((start_pt, b));
    }

    (surviving, true)
}

fn clip_stroke_at_point(
    stroke: &InkStroke,
    ex: f32,
    ey: f32,
    rx: f32,
    ry: f32,
) -> (Vec<Vec<(f32, f32)>>, bool) {
    let mut sub_strokes = Vec::new();
    let mut current_polyline: Vec<(f32, f32)> = Vec::new();
    let mut stroke_modified = false;

    for window in stroke.points.windows(2) {
        let (surviving, modified) =
            clip_segment_against_ellipse(window[0], window[1], ex, ey, rx, ry);
        if modified {
            stroke_modified = true;
        }

        if surviving.is_empty() {
            if !current_polyline.is_empty() {
                sub_strokes.push(std::mem::take(&mut current_polyline));
            }
        } else if surviving.len() == 1 {
            let (start_pt, end_pt) = surviving[0];
            if let Some(&last) = current_polyline.last() {
                if (start_pt.0 - last.0).hypot(start_pt.1 - last.1) < 1e-5 {
                    current_polyline.push(end_pt);
                } else {
                    sub_strokes.push(std::mem::take(&mut current_polyline));
                    current_polyline.push(start_pt);
                    current_polyline.push(end_pt);
                }
            } else {
                current_polyline.push(start_pt);
                current_polyline.push(end_pt);
            }
        } else {
            let (part1_start, part1_end) = surviving[0];
            let (part2_start, part2_end) = surviving[1];

            if let Some(&last) = current_polyline.last() {
                if (part1_start.0 - last.0).hypot(part1_start.1 - last.1) < 1e-5 {
                    current_polyline.push(part1_end);
                } else {
                    sub_strokes.push(std::mem::take(&mut current_polyline));
                    current_polyline.push(part1_start);
                    current_polyline.push(part1_end);
                }
            } else {
                current_polyline.push(part1_start);
                current_polyline.push(part1_end);
            }
            sub_strokes.push(std::mem::take(&mut current_polyline));
            current_polyline.push(part2_start);
            current_polyline.push(part2_end);
        }
    }

    if !current_polyline.is_empty() {
        sub_strokes.push(current_polyline);
    }

    (sub_strokes, stroke_modified)
}

/// Erases ink strokes along the line segment from `from_pt` to `to_pt` using an elliptical eraser footprint.
///
/// Returns `true` if any stroke was modified, split, or deleted.
pub fn erase_ink_strokes_along_segment(
    strokes: &mut Vec<InkStroke>,
    page: PageIndex,
    from_pt: (f32, f32),
    to_pt: (f32, f32),
    radius_x: f32,
    radius_y: f32,
) -> bool {
    if radius_x <= 0.0 || radius_y <= 0.0 || strokes.is_empty() {
        return false;
    }

    let dx = to_pt.0 - from_pt.0;
    let dy = to_pt.1 - from_pt.1;
    let total_dist = dx.hypot(dy);
    let min_r = radius_x.min(radius_y);
    let step_dist = (0.5 * min_r).max(1e-5);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let steps = ((total_dist / step_dist).ceil() as usize).max(1);

    let mut any_modified = false;

    for s in 0..=steps {
        #[allow(clippy::cast_precision_loss)]
        let t = s as f32 / steps as f32;
        let ex = from_pt.0 + t * dx;
        let ey = from_pt.1 + t * dy;

        let mut next_strokes = Vec::with_capacity(strokes.len());
        for stroke in strokes.drain(..) {
            if stroke.page != page {
                next_strokes.push(stroke);
                continue;
            }

            if stroke.points.is_empty() {
                any_modified = true;
                continue;
            }

            if stroke.points.len() == 1 {
                let p = stroke.points[0];
                let nx = (p.0 - ex) / radius_x;
                let ny = (p.1 - ey) / radius_y;
                if nx * nx + ny * ny <= 1.0 {
                    any_modified = true;
                } else {
                    next_strokes.push(stroke);
                }
                continue;
            }

            let (cut_pieces, modified) = clip_stroke_at_point(&stroke, ex, ey, radius_x, radius_y);
            if modified {
                any_modified = true;
            }
            for piece in cut_pieces {
                if polyline_length(&piece) >= 1e-4 {
                    next_strokes.push(InkStroke {
                        page: stroke.page,
                        points: piece,
                        color: stroke.color,
                        width_pts: stroke.width_pts,
                    });
                } else {
                    any_modified = true;
                }
            }
        }
        *strokes = next_strokes;
    }

    any_modified
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
        assert_eq!(ZoomFactor::new(2.0).zoom_in(), ZoomFactor::new(2.5));
        assert_eq!(ZoomFactor::new(3.5).zoom_in(), ZoomFactor::new(4.0));
        assert_eq!(ZoomFactor::new(4.0).zoom_in(), ZoomFactor::new(5.0));
        assert_eq!(ZoomFactor::new(7.0).zoom_in(), ZoomFactor::new(8.0));
        assert_eq!(ZoomFactor::new(8.0).zoom_in(), ZoomFactor::new(8.0));
        assert_eq!(ZoomFactor::new(8.0).zoom_out(), ZoomFactor::new(7.0));
        assert_eq!(ZoomFactor::new(4.5).zoom_out(), ZoomFactor::new(3.5));
        assert_eq!(ZoomFactor::new(4.0).zoom_out(), ZoomFactor::new(3.5));
        assert_eq!(ZoomFactor::new(2.5).zoom_out(), ZoomFactor::new(2.0));
        assert_eq!(ZoomFactor::new(2.0).zoom_out(), ZoomFactor::new(1.75));
    }

    #[test]
    fn persisted_zoom_values_remain_deserializable() {
        let zoom = ZoomFactor::deserialize(serde::de::value::F32Deserializer::<
            serde::de::value::Error,
        >::new(10.0))
        .expect("valid persisted zoom");

        assert_eq!(zoom, ZoomFactor::new(8.0));
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

    #[test]
    fn smooth_ink_points_short_inputs_returned_as_is() {
        assert_eq!(smooth_ink_points(&[]), Vec::<(f32, f32)>::new());
        assert_eq!(smooth_ink_points(&[(1.0, 2.0)]), vec![(1.0, 2.0)]);
        assert_eq!(
            smooth_ink_points(&[(1.0, 2.0), (3.0, 4.0)]),
            vec![(1.0, 2.0), (3.0, 4.0)]
        );
    }

    #[test]
    fn smooth_ink_points_produces_smooth_curve_starting_and_ending_at_endpoints() {
        let raw = vec![(0.0, 0.0), (1.0, 2.0), (2.0, 0.0)];
        let smoothed = smooth_ink_points(&raw);
        assert!(smoothed.len() > raw.len());
        assert_eq!(smoothed.first(), Some(&(0.0, 0.0)));
        assert_eq!(smoothed.last(), Some(&(2.0, 0.0)));
    }

    #[test]
    fn erase_ink_strokes_cuts_stroke_middle_into_two() {
        let page = PageIndex::zero();
        let mut strokes = vec![InkStroke {
            page,
            points: vec![(0.0, 0.5), (1.0, 0.5)],
            color: InkColor::Black,
            width_pts: 2.0,
        }];

        let modified =
            erase_ink_strokes_along_segment(&mut strokes, page, (0.5, 0.5), (0.5, 0.5), 0.1, 0.1);

        assert!(modified);
        assert_eq!(strokes.len(), 2);
        assert_eq!(strokes[0].points[0], (0.0, 0.5));
        assert!((strokes[0].points[1].0 - 0.4).abs() < 1e-4);
        assert!((strokes[1].points[0].0 - 0.6).abs() < 1e-4);
        assert_eq!(strokes[1].points[1], (1.0, 0.5));
    }

    #[test]
    fn erase_ink_strokes_cuts_end_of_stroke() {
        let page = PageIndex::zero();
        let mut strokes = vec![InkStroke {
            page,
            points: vec![(0.0, 0.5), (1.0, 0.5)],
            color: InkColor::Black,
            width_pts: 2.0,
        }];

        let modified =
            erase_ink_strokes_along_segment(&mut strokes, page, (0.95, 0.5), (0.95, 0.5), 0.1, 0.1);

        assert!(modified);
        assert_eq!(strokes.len(), 1);
        assert_eq!(strokes[0].points[0], (0.0, 0.5));
        assert!((strokes[0].points[1].0 - 0.85).abs() < 1e-4);
    }

    #[test]
    fn erase_ink_strokes_fast_drag_interpolates_across_stroke() {
        let page = PageIndex::zero();
        let mut strokes = vec![InkStroke {
            page,
            points: vec![(0.5, 0.0), (0.5, 1.0)],
            color: InkColor::Blue,
            width_pts: 2.0,
        }];

        // Fast drag jumps from x=0.2 to x=0.8 with small radius 0.04
        let modified =
            erase_ink_strokes_along_segment(&mut strokes, page, (0.2, 0.5), (0.8, 0.5), 0.04, 0.04);

        assert!(modified);
        assert_eq!(strokes.len(), 2);
    }

    #[test]
    fn erase_ink_strokes_preserves_strokes_on_other_pages() {
        let page0 = PageIndex::from_raw(0);
        let page1 = PageIndex::from_raw(1);
        let mut strokes = vec![InkStroke {
            page: page1,
            points: vec![(0.0, 0.5), (1.0, 0.5)],
            color: InkColor::Red,
            width_pts: 2.0,
        }];

        let modified =
            erase_ink_strokes_along_segment(&mut strokes, page0, (0.5, 0.5), (0.5, 0.5), 0.2, 0.2);

        assert!(!modified);
        assert_eq!(strokes.len(), 1);
        assert_eq!(strokes[0].points.len(), 2);
    }

    #[test]
    fn annotation_history_push_undo_redo_and_cap() {
        let mut history = AnnotationHistory::default();
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        let state0 = DocumentAnnotations::default();
        let mut state1 = DocumentAnnotations::default();
        state1.strokes.push(InkStroke {
            page: PageIndex::zero(),
            points: vec![(0.1, 0.1)],
            color: InkColor::Black,
            width_pts: 1.0,
        });

        history.push_snapshot(state0.clone());
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // Undo
        let undone = history.undo(state1.clone()).expect("undo succeeds");
        assert_eq!(undone, state0);
        assert!(!history.can_undo());
        assert!(history.can_redo());

        // Redo
        let redone = history.redo(state0).expect("redo succeeds");
        assert_eq!(redone, state1);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // Test cap at 50
        let mut cap_history = AnnotationHistory::default();
        for i in 0..60 {
            let mut s = DocumentAnnotations::default();
            s.strokes.push(InkStroke {
                page: PageIndex::from_raw(i),
                points: vec![],
                color: InkColor::Red,
                width_pts: 1.0,
            });
            cap_history.push_snapshot(s);
        }
        assert_eq!(cap_history.undo_stack.len(), 50);
        assert_eq!(
            cap_history.undo_stack[0].strokes[0].page,
            PageIndex::from_raw(10)
        );
    }
}
