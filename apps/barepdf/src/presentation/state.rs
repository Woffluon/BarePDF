#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::struct_excessive_bools
)]

use super::ui::{
    normalize_viewing_mode, PAGE_IMAGE_BUDGET, TEXT_GEOMETRY_BUDGET, THUMB_IMAGE_BUDGET,
};
use crate::application::{Application, DocumentController, UpdateController};
use barepdf_core::{
    ContinuousLayout, DocumentId, PageTextGeometry, Rotation, TextSelection, UserPreferences,
    ViewingMode, WindowMode, ZoomFactor, ZoomMode,
};
use barepdf_pdf::OutlineNode;
use barepdf_render::RenderKind;
use lru::LruCache;
use slint::{Image, Rgba8Pixel, Timer};
use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::size_of;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::time::{Duration, Instant};

const MAX_TEXT_GEOMETRIES: usize = 32;

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
    entries: LruCache<(DocumentId, u32, RenderKind), CachedImage>,
    bytes: usize,
    budget: usize,
}

struct CachedTextGeometry {
    geometry: PageTextGeometry,
    bytes: usize,
}

pub(super) struct TextGeometryCache {
    entries: HashMap<(DocumentId, u32), CachedTextGeometry>,
    insertion_order: VecDeque<(DocumentId, u32)>,
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

    pub(super) fn contains_key(&self, document: DocumentId, page_index: u32) -> bool {
        self.entries.contains_key(&(document, page_index))
    }

    pub(super) fn get(
        &mut self,
        document: DocumentId,
        page_index: u32,
    ) -> Option<&PageTextGeometry> {
        let key = (document, page_index);
        if self.entries.contains_key(&key) {
            self.insertion_order.retain(|entry| *entry != key);
            self.insertion_order.push_back(key);
        }
        self.entries.get(&key).map(|entry| &entry.geometry)
    }

    pub(super) fn insert(
        &mut self,
        document: DocumentId,
        page_index: u32,
        geometry: PageTextGeometry,
    ) {
        let key = (document, page_index);
        let bytes = size_of::<PageTextGeometry>().saturating_add(
            geometry
                .glyphs
                .capacity()
                .saturating_mul(size_of::<barepdf_core::GlyphRect>()),
        );
        if bytes > TEXT_GEOMETRY_BUDGET {
            return;
        }

        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
            self.insertion_order.retain(|entry| *entry != key);
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
        self.insertion_order.push_back(key);
        self.entries
            .insert(key, CachedTextGeometry { geometry, bytes });
    }

    pub(super) fn remove_document(&mut self, document: DocumentId) {
        let keys = self
            .entries
            .keys()
            .filter(|(entry_document, _)| *entry_document == document)
            .copied()
            .collect::<Vec<_>>();
        for key in keys {
            if let Some(previous) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(previous.bytes);
            }
            self.insertion_order.retain(|entry| *entry != key);
        }
    }

    pub(super) fn in_page_order(&self, document: DocumentId) -> Vec<&PageTextGeometry> {
        let mut geometries = self
            .entries
            .iter()
            .filter_map(|((entry_document, _), entry)| {
                (*entry_document == document).then_some(&entry.geometry)
            })
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

    pub(super) fn get(
        &mut self,
        document: DocumentId,
        page: u32,
        kind: RenderKind,
    ) -> Option<Image> {
        self.entries
            .get(&(document, page, kind))
            .map(|cached| cached.image.clone())
    }

    pub(super) fn contains_key(&self, document: DocumentId, page: u32, kind: RenderKind) -> bool {
        self.entries.contains(&(document, page, kind))
    }

    pub(super) fn insert(
        &mut self,
        document: DocumentId,
        page: u32,
        kind: RenderKind,
        image: Image,
        bytes: usize,
    ) {
        if bytes > self.budget {
            return;
        }
        if let Some(old) = self
            .entries
            .put((document, page, kind), CachedImage { image, bytes })
        {
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

    pub(super) fn remove_document(&mut self, document: DocumentId) {
        let keys = self
            .entries
            .iter()
            .filter_map(|(key, _)| (key.0 == document).then_some(*key))
            .collect::<Vec<_>>();
        for key in keys {
            if let Some(removed) = self.entries.pop(&key) {
                self.bytes = self.bytes.saturating_sub(removed.bytes);
            }
        }
    }
}

pub(super) struct AppState {
    pub(super) application: Application,
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
    pub(super) update: UpdateController,
    pump_timer: Option<Rc<Timer>>,
    pump_active_until: Option<Instant>,
}

impl AppState {
    pub(super) fn new(mut preferences: UserPreferences) -> Self {
        preferences.viewing_mode = normalize_viewing_mode(preferences.viewing_mode);
        Self {
            application: Application::default(),
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
            update: UpdateController::default(),
            pump_timer: None,
            pump_active_until: None,
        }
    }

    pub(super) fn active_document(&self) -> Option<DocumentId> {
        self.application
            .ready_document()
            .map(crate::application::ReadyDocument::id)
    }

    pub(super) fn page_count(&self) -> u32 {
        self.application
            .ready_document()
            .map_or(0, |document| document.page_count().get())
    }

    pub(super) fn attach_pump_timer(&mut self, timer: Rc<Timer>) {
        self.pump_timer = Some(timer);
    }

    pub(super) fn wake_pump(&mut self) {
        self.pump_active_until = Some(Instant::now() + Duration::from_millis(500));
        if let Some(timer) = self.pump_timer.as_ref() {
            timer.set_interval(super::event_pump::ACTIVE_INTERVAL);
        }
    }

    pub(super) fn pump_requires_active(&self, now: Instant) -> bool {
        self.update.is_busy()
            || self.dimensions_request_pending
            || self.resize_changed_at.is_some()
            || self.active_document().is_some_and(|document| {
                self.visible_page_indices.iter().any(|page| {
                    !self
                        .page_images
                        .contains_key(document, *page, RenderKind::Page)
                })
            })
            || DocumentController::pending_path(&self.application).is_some()
            || self
                .last_user_scroll_at
                .is_some_and(|started| now.duration_since(started) < super::ui::SCROLL_IDLE_DELAY)
            || self
                .pump_active_until
                .is_some_and(|deadline| now < deadline)
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

#[cfg(test)]
mod tests {
    use super::*;
    use barepdf_core::PageIndex;
    use std::path::PathBuf;

    #[test]
    fn update_resize_and_scroll_keep_pump_active() {
        let now = Instant::now();
        let mut app = AppState::new(UserPreferences::default());

        assert!(app.update.begin_check());
        assert!(app.pump_requires_active(now));
        app.update.mark_current();
        app.resize_changed_at = Some(now);
        assert!(app.pump_requires_active(now));
        app.resize_changed_at = None;
        app.last_user_scroll_at = Some(now);
        assert!(app.pump_requires_active(now));
    }

    #[test]
    fn render_command_wake_has_bounded_active_window() {
        let mut app = AppState::new(UserPreferences::default());
        app.wake_pump();

        assert!(app.pump_requires_active(Instant::now()));
        assert!(!app.pump_requires_active(Instant::now() + Duration::from_secs(1)));
    }

    #[test]
    fn missing_visible_render_keeps_pump_active() {
        let mut app = AppState::new(UserPreferences::default());
        let document_id = DocumentId::new(1);
        DocumentController::begin_open(
            &mut app.application,
            document_id,
            PathBuf::from("fixture.pdf"),
            Instant::now(),
        );
        assert!(matches!(
            DocumentController::opened(&mut app.application, document_id, 1, 10_000),
            crate::application::OpenTransition::Ready(_)
        ));
        app.visible_page_indices.push(0);

        assert!(app.pump_requires_active(Instant::now()));
    }

    #[test]
    fn text_geometry_is_isolated_by_document() {
        let mut cache = TextGeometryCache::new();
        let first = DocumentId::new(1);
        let second = DocumentId::new(2);
        cache.insert(
            first,
            0,
            PageTextGeometry {
                page_index: PageIndex::zero(),
                glyphs: Vec::new(),
            },
        );

        assert!(cache.contains_key(first, 0));
        assert!(!cache.contains_key(second, 0));
        assert!(cache.get(second, 0).is_none());
    }
}
