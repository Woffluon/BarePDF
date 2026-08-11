use super::{
    normalize_viewing_mode, update::VerifiedUpdate, PAGE_IMAGE_BUDGET, RAW_BITMAP_BUDGET,
    TEXT_GEOMETRY_BUDGET, THUMB_IMAGE_BUDGET,
};
use barepdf_core::{
    ContinuousLayout, DocumentId, PageTextGeometry, Rotation, TextSelection, UserPreferences,
    ViewingMode, WindowMode, ZoomFactor, ZoomMode,
};
use barepdf_pdf::OutlineNode;
use lru::LruCache;
use slint::{Image, Rgba8Pixel};
use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::size_of;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Instant;

const MAX_TEXT_GEOMETRIES: usize = 32;

#[derive(Clone, Copy)]
pub(super) enum UpdateUiState {
    Ready,
    Checking,
    Current,
    Available,
    Downloading,
    Verified,
    Error,
}

#[derive(Clone)]
pub(super) struct PendingDocument {
    pub(super) id: DocumentId,
    pub(super) path: PathBuf,
    pub(super) started_at: Instant,
}

#[derive(Clone, PartialEq)]
pub(super) struct LayoutKey {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) zoom_mode: ZoomMode,
    pub(super) dimensions_revision: u64,
}

#[derive(Clone)]
pub(super) struct FlatOutlineEntry {
    pub(super) path: Vec<usize>,
    pub(super) page_index: Option<u32>,
    pub(super) has_children: bool,
}

struct CachedImage {
    image: Image,
    bytes: usize,
}

pub(super) struct UiImageCache {
    entries: LruCache<u32, CachedImage>,
    bytes: usize,
    budget: usize,
}

struct CachedTextGeometry {
    geometry: PageTextGeometry,
    bytes: usize,
}

pub(super) struct TextGeometryCache {
    entries: HashMap<u32, CachedTextGeometry>,
    insertion_order: VecDeque<u32>,
    bytes: usize,
}

impl TextGeometryCache {
    pub(super) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            insertion_order: VecDeque::new(),
            bytes: 0,
        }
    }

    pub(super) fn contains_key(&self, page_index: u32) -> bool {
        self.entries.contains_key(&page_index)
    }

    pub(super) fn get(&mut self, page_index: u32) -> Option<&PageTextGeometry> {
        if self.entries.contains_key(&page_index) {
            self.insertion_order.retain(|index| *index != page_index);
            self.insertion_order.push_back(page_index);
        }
        self.entries.get(&page_index).map(|entry| &entry.geometry)
    }

    pub(super) fn insert(&mut self, page_index: u32, geometry: PageTextGeometry) {
        let bytes = size_of::<PageTextGeometry>().saturating_add(
            geometry
                .glyphs
                .capacity()
                .saturating_mul(size_of::<barepdf_core::GlyphRect>()),
        );
        if bytes > TEXT_GEOMETRY_BUDGET {
            return;
        }

        if let Some(previous) = self.entries.remove(&page_index) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
            self.insertion_order.retain(|index| *index != page_index);
        }
        while self.entries.len() >= MAX_TEXT_GEOMETRIES
            || self.bytes.saturating_add(bytes) > TEXT_GEOMETRY_BUDGET
        {
            let Some(oldest) = self.insertion_order.pop_front() else {
                break;
            };
            if let Some(previous) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(previous.bytes);
            }
        }

        self.bytes = self.bytes.saturating_add(bytes);
        self.insertion_order.push_back(page_index);
        self.entries
            .insert(page_index, CachedTextGeometry { geometry, bytes });
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.insertion_order.clear();
        self.bytes = 0;
    }

    pub(super) fn in_page_order(&self) -> Vec<&PageTextGeometry> {
        let mut geometries = self
            .entries
            .values()
            .map(|entry| &entry.geometry)
            .collect::<Vec<_>>();
        geometries.sort_unstable_by_key(|geometry| geometry.page_index);
        geometries
    }
}

impl UiImageCache {
    pub(super) fn new(budget: usize) -> Self {
        Self {
            entries: LruCache::new(NonZeroUsize::new(512).unwrap_or(NonZeroUsize::MIN)),
            bytes: 0,
            budget,
        }
    }

    pub(super) fn get(&mut self, page: u32) -> Option<Image> {
        self.entries.get(&page).map(|cached| cached.image.clone())
    }

    pub(super) fn insert(&mut self, page: u32, image: Image, bytes: usize) {
        if bytes > self.budget {
            return;
        }
        if let Some(old) = self.entries.put(page, CachedImage { image, bytes }) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        self.bytes = self.bytes.saturating_add(bytes);
        while self.bytes > self.budget {
            let Some((_, evicted)) = self.entries.pop_lru() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(evicted.bytes);
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

pub(super) struct AppState {
    pub(super) active_document: Option<DocumentId>,
    pub(super) pending_document: Option<PendingDocument>,
    pub(super) current_path: Option<PathBuf>,
    pub(super) last_failed_path: Option<PathBuf>,
    pub(super) page_count: u32,
    pub(super) current_page: u32,
    pub(super) viewing_mode: ViewingMode,
    pub(super) zoom_mode: ZoomMode,
    pub(super) zoom_factor: ZoomFactor,
    pub(super) rotation: Rotation,
    pub(super) first_page_dimensions: (f32, f32),
    pub(super) page_dimensions: Vec<(f32, f32)>,
    pub(super) dimensions_revision: u64,
    pub(super) next_dimensions_start: u32,
    pub(super) dimensions_request_pending: bool,
    pub(super) layout: ContinuousLayout,
    pub(super) layout_key: Option<LayoutKey>,
    pub(super) visible_page_indices: Vec<u32>,
    pub(super) generation: u64,
    pub(super) first_page_ready: bool,
    pub(super) profile_recorded: bool,
    pub(super) open_started_at: Option<Instant>,
    pub(super) window_mode: WindowMode,
    pub(super) preferences: UserPreferences,
    pub(super) text_geometries: TextGeometryCache,
    pub(super) selection: Option<TextSelection>,
    pub(super) is_selecting: bool,
    pub(super) last_click_time: Instant,
    pub(super) click_count: u32,
    pub(super) last_scroll_y: f32,
    pub(super) last_thumbnail_scroll_y: f32,
    pub(super) last_user_scroll_at: Option<Instant>,
    pub(super) viewport_width: u32,
    pub(super) viewport_height: u32,
    pub(super) scale_factor: f32,
    pub(super) resize_changed_at: Option<Instant>,
    pub(super) outline: Vec<OutlineNode>,
    pub(super) outline_requested: bool,
    pub(super) expanded_outline: HashSet<Vec<usize>>,
    pub(super) flat_outline: Vec<FlatOutlineEntry>,
    pub(super) page_images: UiImageCache,
    pub(super) thumbnail_images: UiImageCache,
    pub(super) available_update: Option<VerifiedUpdate>,
    pub(super) verified_update: Option<PathBuf>,
    pub(super) update_busy: bool,
    pub(super) update_ui_state: UpdateUiState,
}

impl AppState {
    pub(super) fn new(mut preferences: UserPreferences) -> Self {
        preferences.viewing_mode = normalize_viewing_mode(preferences.viewing_mode);
        preferences.memory_budget_bytes = RAW_BITMAP_BUDGET;
        Self {
            active_document: None,
            pending_document: None,
            current_path: None,
            last_failed_path: None,
            page_count: 0,
            current_page: 0,
            viewing_mode: preferences.viewing_mode,
            zoom_mode: preferences.zoom_mode,
            zoom_factor: ZoomFactor::default(),
            rotation: Rotation::Degrees0,
            first_page_dimensions: (612.0, 792.0),
            page_dimensions: Vec::new(),
            dimensions_revision: 0,
            next_dimensions_start: 1,
            dimensions_request_pending: false,
            layout: ContinuousLayout::default(),
            layout_key: None,
            visible_page_indices: Vec::new(),
            generation: 1,
            first_page_ready: false,
            profile_recorded: false,
            open_started_at: None,
            window_mode: WindowMode::Normal,
            preferences,
            text_geometries: TextGeometryCache::new(),
            selection: None,
            is_selecting: false,
            last_click_time: Instant::now(),
            click_count: 0,
            last_scroll_y: 0.0,
            last_thumbnail_scroll_y: 0.0,
            last_user_scroll_at: None,
            viewport_width: 900,
            viewport_height: 700,
            scale_factor: 1.0,
            resize_changed_at: None,
            outline: Vec::new(),
            outline_requested: false,
            expanded_outline: HashSet::new(),
            flat_outline: Vec::new(),
            page_images: UiImageCache::new(PAGE_IMAGE_BUDGET),
            thumbnail_images: UiImageCache::new(THUMB_IMAGE_BUDGET),
            available_update: None,
            verified_update: None,
            update_busy: false,
            update_ui_state: UpdateUiState::Ready,
        }
    }
}

pub(super) fn fit_bitmap_to_budget(width: u32, height: u32, budget: usize) -> (u32, u32) {
    let max_pixels = u64::try_from(budget / size_of::<Rgba8Pixel>()).unwrap_or(u64::MAX);
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if pixels <= max_pixels {
        return (width, height);
    }

    let scale = (max_pixels as f64 / pixels as f64).sqrt();
    (
        (f64::from(width) * scale).round().max(1.0) as u32,
        (f64::from(height) * scale).round().max(1.0) as u32,
    )
}
