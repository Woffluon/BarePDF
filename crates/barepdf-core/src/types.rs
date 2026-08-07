use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PageCount(u32);

impl PageCount {
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
    pub fn new(index: u32, page_count: PageCount) -> Option<Self> {
        if index < page_count.get() {
            Some(Self(index))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn zero() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for PageIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0 + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ZoomFactor(f32);

impl ZoomFactor {
    pub const MIN: f32 = 0.1;
    pub const MAX: f32 = 10.0;
    pub const DEFAULT: f32 = 1.0;

    pub fn new(factor: f32) -> Self {
        Self(factor.clamp(Self::MIN, Self::MAX))
    }

    #[must_use]
    pub fn get(self) -> f32 {
        self.0
    }

    #[must_use]
    pub fn zoom_in(self) -> Self {
        Self::new(self.0 * 1.2)
    }

    #[must_use]
    pub fn zoom_out(self) -> Self {
        Self::new(self.0 / 1.2)
    }
}

impl Default for ZoomFactor {
    fn default() -> Self {
        Self(1.0)
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderDimensions {
    pub width: u32,
    pub height: u32,
}

impl RenderDimensions {
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
    pub const DEFAULT_BYTES: usize = 256 * 1024 * 1024; // 256 MB

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
