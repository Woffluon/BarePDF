#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::struct_excessive_bools
)]

use super::ui::{
    normalize_viewing_mode, TEXT_GEOMETRY_BUDGET, THUMB_IMAGE_BUDGET, UI_IMAGE_CACHE_BUDGET,
};
use crate::application::{Application, DocumentController, UpdateController};
use crate::infrastructure::{ToolJobKey, ToolWorker};
use barepdf_core::{
    ContinuousLayout, DocumentId, PageTextGeometry, Rotation, TextSelection, UserPreferences,
    ViewingMode, WindowMode, ZoomFactor, ZoomMode,
};
use barepdf_pdf::conversion::CancellationToken;
use barepdf_pdf::OutlineNode;
use barepdf_render::RenderKind;
use lru::LruCache;
use slint::{Image, Rgba8Pixel, Timer};
use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::size_of;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

const MAX_TEXT_GEOMETRIES: usize = 32;

#[derive(Clone, PartialEq)]
pub(crate) struct LayoutKey {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) zoom_mode: ZoomMode,
    pub(crate) rotation: Rotation,
    pub(crate) dimensions_revision: u64,
}

#[derive(Clone)]
pub(crate) struct FlatOutlineEntry {
    pub(crate) path: Vec<usize>,
    pub(crate) page_index: Option<u32>,
    pub(crate) has_children: bool,
}

struct CachedImage {
    image: Image,
    bytes: usize,
}

pub(crate) struct UiImageCache {
    entries: LruCache<(DocumentId, u32, RenderKind), CachedImage>,
    bytes: usize,
    budget: usize,
}

struct CachedTextGeometry {
    geometry: PageTextGeometry,
    bytes: usize,
}

pub(crate) struct TextGeometryCache {
    entries: HashMap<(DocumentId, u32), CachedTextGeometry>,
    insertion_order: VecDeque<(DocumentId, u32)>,
    bytes: usize,
}

impl TextGeometryCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            insertion_order: VecDeque::new(),
            bytes: 0,
        }
    }

    pub(crate) fn contains_key(&self, document: DocumentId, page_index: u32) -> bool {
        self.entries.contains_key(&(document, page_index))
    }

    pub(crate) fn get(
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

    pub(crate) fn insert(
        &mut self,
        document: DocumentId,
        page_index: u32,
        geometry: PageTextGeometry,
    ) {
        let key = (document, page_index);
        let bytes = size_of::<PageTextGeometry>()
            .saturating_add(
                geometry
                    .glyphs
                    .capacity()
                    .saturating_mul(size_of::<barepdf_core::GlyphRect>()),
            )
            .saturating_add(
                geometry
                    .links
                    .capacity()
                    .saturating_mul(size_of::<barepdf_core::PageLink>()),
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

    pub(crate) fn remove_document(&mut self, document: DocumentId) {
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

    pub(crate) fn in_page_order(&self, document: DocumentId) -> Vec<&PageTextGeometry> {
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
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            entries: LruCache::new(NonZeroUsize::new(512).unwrap_or(NonZeroUsize::MIN)),
            bytes: 0,
            budget,
        }
    }

    pub(crate) fn get(
        &mut self,
        document: DocumentId,
        page: u32,
        kind: RenderKind,
    ) -> Option<Image> {
        self.entries
            .get(&(document, page, kind))
            .map(|cached| cached.image.clone())
    }

    pub(crate) fn contains_key(&self, document: DocumentId, page: u32, kind: RenderKind) -> bool {
        self.entries.contains(&(document, page, kind))
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn budget(&self) -> usize {
        self.budget
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) fn set_budget(&mut self, new_budget: usize) {
        self.budget = new_budget;
        self.evict_to_budget();
    }

    pub(crate) fn evict_to_budget(&mut self) {
        while self.bytes > self.budget {
            let Some((_, evicted)) = self.entries.pop_lru() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(evicted.bytes);
        }
    }

    pub(crate) fn update_cache_budget_for_zoom(&mut self, zoom: ZoomFactor) {
        self.set_budget(adaptive_cache_budget(zoom));
    }

    pub(crate) fn insert(
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
        self.evict_to_budget();
    }

    pub(crate) fn remove_document(&mut self, document: DocumentId) {
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

    pub(crate) fn remove_page(&mut self, document: DocumentId, page: u32) {
        let keys = self
            .entries
            .iter()
            .filter_map(|(key, _)| (key.0 == document && key.1 == page).then_some(*key))
            .collect::<Vec<_>>();
        for key in keys {
            if let Some(removed) = self.entries.pop(&key) {
                self.bytes = self.bytes.saturating_sub(removed.bytes);
            }
        }
    }
}

pub(crate) struct AppState {
    pub(crate) application: Application,
    pub(crate) current_page: u32,
    pub(crate) viewing_mode: ViewingMode,
    pub(crate) zoom_mode: ZoomMode,
    pub(crate) zoom_factor: ZoomFactor,
    pub(crate) rotation: Rotation,
    pub(crate) first_page_dimensions: (f32, f32),
    pub(crate) page_dimensions: Vec<(f32, f32)>,
    pub(crate) dimensions_revision: u64,
    pub(crate) next_dimensions_start: u32,
    pub(crate) dimensions_request_pending: bool,
    pub(crate) layout: ContinuousLayout,
    pub(crate) layout_key: Option<LayoutKey>,
    pub(crate) visible_page_indices: Vec<u32>,
    pub(crate) generation: u64,
    pub(crate) first_page_ready: bool,
    pub(crate) profile_recorded: bool,
    pub(crate) open_started_at: Option<Instant>,
    pub(crate) window_mode: WindowMode,
    pub(crate) preferences: UserPreferences,
    pub(crate) text_geometries: TextGeometryCache,
    pub(crate) selection: Option<TextSelection>,
    pub(crate) is_selecting: bool,
    pub(crate) last_click_time: Instant,
    pub(crate) click_count: u32,
    pub(crate) last_scroll_y: f32,
    pub(crate) last_thumbnail_scroll_y: f32,
    pub(crate) last_user_scroll_at: Option<Instant>,
    pub(crate) viewport_width: u32,
    pub(crate) viewport_height: u32,
    pub(crate) scale_factor: f32,
    pub(crate) resize_changed_at: Option<Instant>,
    pub(crate) outline: Vec<OutlineNode>,
    pub(crate) outline_requested: bool,
    pub(crate) expanded_outline: HashSet<Vec<usize>>,
    pub(crate) flat_outline: Vec<FlatOutlineEntry>,
    pub(crate) page_images: UiImageCache,
    pub(crate) thumbnail_images: UiImageCache,
    pub(crate) update: UpdateController,
    pub(crate) tools_merge_files: Vec<PathBuf>,
    pub(crate) tools_source_path: Option<PathBuf>,
    pub(crate) tool_password_source: Option<PathBuf>,
    pub(crate) tool_source_token: u64,
    pub(crate) next_tool_job_id: u64,
    pub(crate) active_tool_job: Option<ActiveToolJob>,
    pub(crate) tool_worker: Option<ToolWorker>,
    pub(crate) tool_event_timer: Option<Rc<Timer>>,
    pub(crate) search_query: Option<barepdf_core::search::SearchQuery>,
    pub(crate) search_matches: Vec<barepdf_core::search::SearchMatch>,
    pub(crate) active_search_match: usize,
    pub(crate) annotations: HashMap<DocumentId, barepdf_core::DocumentAnnotations>,
    pub(crate) active_stroke: Option<barepdf_core::InkStroke>,
    pub(crate) drawing_color: barepdf_core::InkColor,
    pub(crate) drawing_width_pts: f32,
    pub(crate) drawing_eraser: bool,
    pub(crate) sign_pad_strokes: Vec<Vec<(f32, f32)>>,
    pub(crate) sign_pad_active_stroke: Option<Vec<(f32, f32)>>,
    pub(crate) sign_uploaded_image: Option<(u32, u32, Vec<u8>)>,
    pump_timer: Option<Rc<Timer>>,
    pump_active_until: Option<Instant>,
}

impl AppState {
    pub(crate) fn new(mut preferences: UserPreferences) -> Self {
        preferences.viewing_mode = normalize_viewing_mode(preferences.viewing_mode);
        let initial_zoom = match preferences.zoom_mode {
            ZoomMode::Custom(factor) => factor,
            _ => ZoomFactor::default(),
        };
        Self {
            application: Application::default(),
            current_page: 0,
            viewing_mode: preferences.viewing_mode,
            zoom_mode: preferences.zoom_mode,
            zoom_factor: initial_zoom,
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
            page_images: UiImageCache::new(adaptive_cache_budget(initial_zoom)),
            thumbnail_images: UiImageCache::new(THUMB_IMAGE_BUDGET),
            update: UpdateController::default(),
            tools_merge_files: Vec::new(),
            tools_source_path: None,
            tool_password_source: None,
            tool_source_token: 1,
            next_tool_job_id: 1,
            active_tool_job: None,
            tool_worker: None,
            tool_event_timer: None,
            search_query: None,
            search_matches: Vec::new(),
            active_search_match: 0,
            annotations: HashMap::new(),
            active_stroke: None,
            drawing_color: barepdf_core::InkColor::Black,
            drawing_width_pts: 4.0,
            drawing_eraser: false,
            sign_pad_strokes: Vec::new(),
            sign_pad_active_stroke: None,
            sign_uploaded_image: None,
            pump_timer: None,
            pump_active_until: None,
        }
    }

    pub(crate) fn active_document(&self) -> Option<DocumentId> {
        self.application
            .ready_document()
            .map(crate::application::ReadyDocument::id)
    }

    pub(crate) fn page_count(&self) -> u32 {
        self.application
            .ready_document()
            .map_or(0, |document| document.page_count().get())
    }

    pub(crate) fn attach_pump_timer(&mut self, timer: Rc<Timer>) {
        self.pump_timer = Some(timer);
    }

    pub(crate) fn wake_pump(&mut self) {
        self.pump_active_until = Some(Instant::now() + Duration::from_millis(500));
        if let Some(timer) = self.pump_timer.as_ref() {
            timer.set_interval(super::event_pump::ACTIVE_INTERVAL);
        }
    }

    pub(crate) fn update_cache_budget_for_zoom(&mut self, zoom: ZoomFactor) {
        self.page_images.update_cache_budget_for_zoom(zoom);
    }

    pub(crate) fn pump_requires_active(&self, now: Instant) -> bool {
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

    pub(crate) fn request_presentation_mode(&mut self) -> bool {
        if self.window_mode != WindowMode::Presentation {
            self.window_mode = WindowMode::Presentation;
            true
        } else {
            false
        }
    }

    pub(crate) fn request_exit_special_mode(&mut self) -> bool {
        if self.window_mode != WindowMode::Normal {
            self.window_mode = WindowMode::Normal;
            true
        } else {
            false
        }
    }

    pub(crate) fn request_toggle_fullscreen(&mut self) -> WindowMode {
        self.window_mode = match self.window_mode {
            WindowMode::Normal => WindowMode::FullScreen,
            WindowMode::FullScreen | WindowMode::Presentation => WindowMode::Normal,
        };
        self.window_mode
    }

    pub(crate) fn snapshot_active_tab_layout(&mut self) {
        let layout = crate::application::TabDocumentLayout {
            page_dimensions: self.page_dimensions.clone(),
            first_page_dimensions: self.first_page_dimensions,
            dimensions_revision: self.dimensions_revision,
            next_dimensions_start: self.next_dimensions_start,
            outline: self.outline.clone(),
        };
        if let Some(tab) = self.application.tabs.active_mut() {
            tab.layout = layout;
        }
    }

    pub(crate) fn restore_active_tab_layout(&mut self) {
        let Some(tab) = self.application.tabs.active() else {
            return;
        };
        let layout = tab.layout.clone();
        self.page_dimensions = layout.page_dimensions;
        self.first_page_dimensions = layout.first_page_dimensions;
        self.dimensions_revision = layout.dimensions_revision;
        self.next_dimensions_start = layout.next_dimensions_start;
        self.outline = layout.outline;
        self.outline_requested = !self.outline.is_empty();
        self.expanded_outline.clear();
        for index in 0..self.outline.len() {
            if !self.outline[index].children.is_empty() {
                self.expanded_outline.insert(vec![index]);
            }
        }
        self.flat_outline.clear();
    }
}

#[must_use]
pub(crate) fn adaptive_cache_budget(zoom: ZoomFactor) -> usize {
    if zoom.factor() <= 1.25 {
        32 * 1024 * 1024
    } else if zoom.factor() <= 2.0 {
        64 * 1024 * 1024
    } else {
        UI_IMAGE_CACHE_BUDGET
    }
}

pub(crate) struct ActiveToolJob {
    pub(crate) key: ToolJobKey,
    pub(crate) cancellation: CancellationToken,
}

pub(crate) fn fit_bitmap_to_budget(width: u32, height: u32, budget: usize) -> (u32, u32) {
    let max_pixels = u64::try_from(budget / size_of::<Rgba8Pixel>()).unwrap_or(u64::MAX);
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if pixels <= max_pixels {
        return (width, height);
    }

    let scale = (max_pixels as f64 / pixels as f64).sqrt();
    let mut fitted_width = (f64::from(width) * scale).round().max(1.0) as u32;
    let mut fitted_height = (f64::from(height) * scale).round().max(1.0) as u32;
    while u64::from(fitted_width).saturating_mul(u64::from(fitted_height)) > max_pixels {
        if fitted_width >= fitted_height && fitted_width > 1 {
            fitted_width -= 1;
        } else if fitted_height > 1 {
            fitted_height -= 1;
        } else {
            break;
        }
    }
    (fitted_width, fitted_height)
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
                links: Vec::new(),
            },
        );

        assert!(cache.contains_key(first, 0));
        assert!(!cache.contains_key(second, 0));
        assert!(cache.get(second, 0).is_none());
    }

    #[test]
    fn thumbnail_cache_can_invalidate_one_page_without_evicting_siblings() {
        let mut cache = UiImageCache::new(32);
        let first = DocumentId::new(1);
        cache.insert(first, 0, RenderKind::Thumbnail, Image::default(), 4);
        cache.insert(first, 1, RenderKind::Thumbnail, Image::default(), 4);

        cache.remove_page(first, 0);

        assert!(!cache.contains_key(first, 0, RenderKind::Thumbnail));
        assert!(cache.contains_key(first, 1, RenderKind::Thumbnail));
    }

    #[test]
    fn page_image_cache_holds_multiple_large_pages_without_thrashing() {
        let mut cache = UiImageCache::new(UI_IMAGE_CACHE_BUDGET);
        let doc = DocumentId::new(1);
        let page_bytes = 24 * 1024 * 1024; // ~24MB per 350% A4 page

        cache.insert(doc, 0, RenderKind::Page, Image::default(), page_bytes);
        cache.insert(doc, 1, RenderKind::Page, Image::default(), page_bytes);
        cache.insert(doc, 2, RenderKind::Page, Image::default(), page_bytes);
        cache.insert(doc, 3, RenderKind::Page, Image::default(), page_bytes);

        assert!(cache.contains_key(doc, 0, RenderKind::Page));
        assert!(cache.contains_key(doc, 1, RenderKind::Page));
        assert!(cache.contains_key(doc, 2, RenderKind::Page));
        assert!(cache.contains_key(doc, 3, RenderKind::Page));
    }

    #[test]
    fn adaptive_budget_scales_correctly_with_zoom() {
        assert_eq!(
            adaptive_cache_budget(ZoomFactor::new(1.0)),
            32 * 1024 * 1024
        );
        assert_eq!(
            adaptive_cache_budget(ZoomFactor::new(1.25)),
            32 * 1024 * 1024
        );
        assert_eq!(
            adaptive_cache_budget(ZoomFactor::new(1.5)),
            64 * 1024 * 1024
        );
        assert_eq!(
            adaptive_cache_budget(ZoomFactor::new(2.0)),
            64 * 1024 * 1024
        );
        assert_eq!(
            adaptive_cache_budget(ZoomFactor::new(2.5)),
            128 * 1024 * 1024
        );
        assert_eq!(
            adaptive_cache_budget(ZoomFactor::new(3.5)),
            128 * 1024 * 1024
        );
    }

    #[test]
    fn cache_evicts_lru_pages_when_zoom_decreases() {
        let mut app = AppState::new(UserPreferences::default());
        let doc = DocumentId::new(1);
        let page_bytes = 20 * 1024 * 1024; // 20 MB

        // Set zoom to 350% -> 128 MB budget
        app.update_cache_budget_for_zoom(ZoomFactor::new(3.5));
        assert_eq!(app.page_images.budget(), 128 * 1024 * 1024);

        // Insert 4 pages (4 * 20 MB = 80 MB, fits in 128 MB)
        app.page_images
            .insert(doc, 0, RenderKind::Page, Image::default(), page_bytes);
        app.page_images
            .insert(doc, 1, RenderKind::Page, Image::default(), page_bytes);
        app.page_images
            .insert(doc, 2, RenderKind::Page, Image::default(), page_bytes);
        app.page_images
            .insert(doc, 3, RenderKind::Page, Image::default(), page_bytes);

        assert_eq!(app.page_images.bytes(), 80 * 1024 * 1024);
        assert!(app.page_images.contains_key(doc, 0, RenderKind::Page));
        assert!(app.page_images.contains_key(doc, 1, RenderKind::Page));
        assert!(app.page_images.contains_key(doc, 2, RenderKind::Page));
        assert!(app.page_images.contains_key(doc, 3, RenderKind::Page));

        // Decrease zoom to 100% -> 32 MB budget
        app.update_cache_budget_for_zoom(ZoomFactor::new(1.0));
        assert_eq!(app.page_images.budget(), 32 * 1024 * 1024);

        // Under 32 MB budget, only 1 page of 20MB can fit (oldest 3 evicted)
        assert!(app.page_images.bytes() <= 32 * 1024 * 1024);
        assert_eq!(app.page_images.bytes(), 20 * 1024 * 1024);
        assert!(!app.page_images.contains_key(doc, 0, RenderKind::Page));
        assert!(!app.page_images.contains_key(doc, 1, RenderKind::Page));
        assert!(!app.page_images.contains_key(doc, 2, RenderKind::Page));
        assert!(app.page_images.contains_key(doc, 3, RenderKind::Page));
    }

    #[test]
    fn tab_switch_evicts_cache_if_exceeding_new_tab_budget() {
        let mut app = AppState::new(UserPreferences::default());
        let doc1 = DocumentId::new(1);
        let doc2 = DocumentId::new(2);

        // Tab 1 at 350% zoom (128MB budget)
        app.update_cache_budget_for_zoom(ZoomFactor::new(3.5));
        let page_bytes = 20 * 1024 * 1024; // 20 MB each
        app.page_images
            .insert(doc1, 0, RenderKind::Page, Image::default(), page_bytes);
        app.page_images
            .insert(doc1, 1, RenderKind::Page, Image::default(), page_bytes);
        app.page_images
            .insert(doc1, 2, RenderKind::Page, Image::default(), page_bytes);
        app.page_images
            .insert(doc1, 3, RenderKind::Page, Image::default(), page_bytes);
        assert_eq!(app.page_images.bytes(), 80 * 1024 * 1024);

        // Switching to Tab 2 which is at 100% zoom (32MB budget)
        let tab2_zoom = ZoomFactor::new(1.0);
        app.update_cache_budget_for_zoom(tab2_zoom);
        assert_eq!(app.page_images.budget(), 32 * 1024 * 1024);
        assert!(app.page_images.bytes() <= 32 * 1024 * 1024);
        assert_eq!(app.page_images.bytes(), 20 * 1024 * 1024);

        // Add page for doc2
        app.page_images.insert(
            doc2,
            0,
            RenderKind::Page,
            Image::default(),
            10 * 1024 * 1024,
        );
        assert_eq!(app.page_images.bytes(), 30 * 1024 * 1024);
        assert!(app.page_images.contains_key(doc2, 0, RenderKind::Page));
    }

    #[test]
    fn multi_tab_switch_preserves_dimensions_and_layout() {
        let mut app = AppState::new(UserPreferences::default());
        let doc1 = DocumentId::new(1);
        DocumentController::begin_open(
            &mut app.application,
            doc1,
            PathBuf::from("large.pdf"),
            Instant::now(),
        );
        let _ = DocumentController::opened(&mut app.application, doc1, 50, 10_000);
        app.page_dimensions = vec![(595.0, 842.0); 50];
        app.first_page_dimensions = (595.0, 842.0);
        app.dimensions_revision = 5;
        app.next_dimensions_start = 51;
        app.outline = vec![OutlineNode {
            title: "Intro".into(),
            page_index: Some(0),
            children: Vec::new(),
        }];

        let tab1_id = app.application.tabs.active_id().unwrap();
        if let Some(tab) = app.application.tabs.active_mut() {
            tab.layout = crate::application::TabDocumentLayout {
                page_dimensions: app.page_dimensions.clone(),
                first_page_dimensions: app.first_page_dimensions,
                dimensions_revision: app.dimensions_revision,
                next_dimensions_start: app.next_dimensions_start,
                outline: app.outline.clone(),
            };
        }

        let doc2 = DocumentId::new(2);
        let _ = app
            .application
            .tabs
            .open(PathBuf::from("small.pdf"), "small".into());
        let tab2_id = app.application.tabs.active_id().unwrap();
        assert_ne!(tab1_id, tab2_id);
        DocumentController::begin_open(
            &mut app.application,
            doc2,
            PathBuf::from("small.pdf"),
            Instant::now(),
        );
        let _ = DocumentController::opened(&mut app.application, doc2, 10, 10_000);
        app.page_dimensions = vec![(612.0, 792.0); 10];
        app.first_page_dimensions = (612.0, 792.0);
        app.dimensions_revision = 1;
        app.next_dimensions_start = 11;
        app.outline.clear();

        if let Some(tab) = app.application.tabs.active_mut() {
            tab.layout = crate::application::TabDocumentLayout {
                page_dimensions: app.page_dimensions.clone(),
                first_page_dimensions: app.first_page_dimensions,
                dimensions_revision: app.dimensions_revision,
                next_dimensions_start: app.next_dimensions_start,
                outline: app.outline.clone(),
            };
        }

        assert_eq!(app.page_count(), 10);
        assert_eq!(app.page_dimensions.len(), 10);

        assert!(app.application.tabs.activate(tab1_id));
        let tab1_layout = app.application.tabs.active().unwrap().layout.clone();
        app.page_dimensions = tab1_layout.page_dimensions;
        app.first_page_dimensions = tab1_layout.first_page_dimensions;
        app.dimensions_revision = tab1_layout.dimensions_revision;
        app.next_dimensions_start = tab1_layout.next_dimensions_start;
        app.outline = tab1_layout.outline;

        assert_eq!(app.page_count(), 50);
        assert_eq!(app.page_dimensions.len(), 50);
        assert_eq!(app.first_page_dimensions, (595.0, 842.0));
        assert_eq!(app.dimensions_revision, 5);
        assert_eq!(app.next_dimensions_start, 51);
        assert_eq!(app.outline.len(), 1);
        assert_eq!(app.outline[0].title, "Intro");

        app.layout =
            ContinuousLayout::compute(&app.page_dimensions, 800, 600, ZoomMode::FitPage, 1.0, 10.0);
        assert_eq!(app.layout.pages.len(), 50);
    }

    #[test]
    fn window_mode_state_transitions_follow_contract() {
        let mut app = AppState::new(UserPreferences::default());
        assert_eq!(app.window_mode, WindowMode::Normal);

        // WindowMode::Normal -> request_presentation_mode -> WindowMode::Presentation
        assert!(app.request_presentation_mode());
        assert_eq!(app.window_mode, WindowMode::Presentation);
        // Redundant call is a no-op returning false
        assert!(!app.request_presentation_mode());

        // WindowMode::Presentation -> request_exit_special_mode -> WindowMode::Normal
        assert!(app.request_exit_special_mode());
        assert_eq!(app.window_mode, WindowMode::Normal);
        // Redundant call is a no-op returning false
        assert!(!app.request_exit_special_mode());

        // WindowMode::Presentation -> request_toggle_fullscreen -> WindowMode::Normal
        // (F11 in presentation mode returns directly to normal mode)
        assert!(app.request_presentation_mode());
        assert_eq!(app.window_mode, WindowMode::Presentation);
        assert_eq!(app.request_toggle_fullscreen(), WindowMode::Normal);
        assert_eq!(app.window_mode, WindowMode::Normal);

        // WindowMode::Normal -> request_toggle_fullscreen -> WindowMode::FullScreen -> request_toggle_fullscreen -> WindowMode::Normal
        assert_eq!(app.request_toggle_fullscreen(), WindowMode::FullScreen);
        assert_eq!(app.window_mode, WindowMode::FullScreen);
        assert_eq!(app.request_toggle_fullscreen(), WindowMode::Normal);
        assert_eq!(app.window_mode, WindowMode::Normal);

        // ESC from FullScreen returns to Normal
        assert_eq!(app.request_toggle_fullscreen(), WindowMode::FullScreen);
        assert!(app.request_exit_special_mode());
        assert_eq!(app.window_mode, WindowMode::Normal);
    }

    #[test]
    fn multi_tab_document_layout_isolation_across_multiple_switches() {
        let mut app = AppState::new(UserPreferences::default());

        // Setup Tab 1: 50 pages A4
        let doc1 = DocumentId::new(101);
        DocumentController::begin_open(
            &mut app.application,
            doc1,
            PathBuf::from("doc1.pdf"),
            Instant::now(),
        );
        let _ = DocumentController::opened(&mut app.application, doc1, 50, 10_000);
        let tab1_id = app.application.tabs.active_id().unwrap();

        app.page_dimensions = vec![(595.0, 842.0); 50];
        app.first_page_dimensions = (595.0, 842.0);
        app.dimensions_revision = 3;
        app.next_dimensions_start = 51;
        app.outline = vec![OutlineNode {
            title: "Doc 1 Intro".into(),
            page_index: Some(0),
            children: Vec::new(),
        }];
        app.snapshot_active_tab_layout();

        // Setup Tab 2: 10 pages Letter
        let doc2 = DocumentId::new(202);
        let _ = app
            .application
            .tabs
            .open(PathBuf::from("doc2.pdf"), "doc2".into());
        let tab2_id = app.application.tabs.active_id().unwrap();
        assert_ne!(tab1_id, tab2_id);

        DocumentController::begin_open(
            &mut app.application,
            doc2,
            PathBuf::from("doc2.pdf"),
            Instant::now(),
        );
        let _ = DocumentController::opened(&mut app.application, doc2, 10, 10_000);

        app.page_dimensions = vec![(612.0, 792.0); 10];
        app.first_page_dimensions = (612.0, 792.0);
        app.dimensions_revision = 1;
        app.next_dimensions_start = 11;
        app.outline = vec![
            OutlineNode {
                title: "Chapter 1".into(),
                page_index: Some(0),
                children: Vec::new(),
            },
            OutlineNode {
                title: "Chapter 2".into(),
                page_index: Some(4),
                children: Vec::new(),
            },
        ];
        app.snapshot_active_tab_layout();

        // Switch to Tab 1
        assert!(app.application.tabs.activate(tab1_id));
        app.restore_active_tab_layout();

        assert_eq!(app.page_count(), 50);
        assert_eq!(app.page_dimensions.len(), 50);
        assert_eq!(app.first_page_dimensions, (595.0, 842.0));
        assert_eq!(app.dimensions_revision, 3);
        assert_eq!(app.next_dimensions_start, 51);
        assert_eq!(app.outline.len(), 1);
        assert_eq!(app.outline[0].title, "Doc 1 Intro");
        app.layout =
            ContinuousLayout::compute(&app.page_dimensions, 800, 600, ZoomMode::FitPage, 1.0, 10.0);
        assert_eq!(app.layout.pages.len(), 50);

        // Switch to Tab 2
        assert!(app.application.tabs.activate(tab2_id));
        app.restore_active_tab_layout();

        assert_eq!(app.page_count(), 10);
        assert_eq!(app.page_dimensions.len(), 10);
        assert_eq!(app.first_page_dimensions, (612.0, 792.0));
        assert_eq!(app.dimensions_revision, 1);
        assert_eq!(app.next_dimensions_start, 11);
        assert_eq!(app.outline.len(), 2);
        assert_eq!(app.outline[0].title, "Chapter 1");
        assert_eq!(app.outline[1].title, "Chapter 2");
        app.layout =
            ContinuousLayout::compute(&app.page_dimensions, 800, 600, ZoomMode::FitPage, 1.0, 10.0);
        assert_eq!(app.layout.pages.len(), 10);

        // Switch back to Tab 1 once more to prove idempotency
        assert!(app.application.tabs.activate(tab1_id));
        app.restore_active_tab_layout();
        assert_eq!(app.page_count(), 50);
        assert_eq!(app.page_dimensions.len(), 50);
        assert_eq!(app.outline.len(), 1);
    }

    #[test]
    fn adaptive_image_cache_eviction_and_oversized_rejection() {
        let budget = 100;
        let mut cache = UiImageCache::new(budget);
        let doc = DocumentId::new(1);

        // 1. Oversized image (> budget) must be rejected without increasing bytes
        cache.insert(doc, 0, RenderKind::Page, Image::default(), 150);
        assert!(!cache.contains_key(doc, 0, RenderKind::Page));
        assert_eq!(cache.bytes, 0);

        // 2. Insert two items within budget: 40 + 40 = 80 <= 100
        cache.insert(doc, 1, RenderKind::Page, Image::default(), 40);
        cache.insert(doc, 2, RenderKind::Page, Image::default(), 40);
        assert_eq!(cache.bytes, 80);
        assert!(cache.contains_key(doc, 1, RenderKind::Page));
        assert!(cache.contains_key(doc, 2, RenderKind::Page));

        // 3. Access page 1 to make it most-recently used
        let _ = cache.get(doc, 1, RenderKind::Page);

        // 4. Insert another item of 40 bytes: total becomes 80 + 40 = 120 > 100
        // LRU entry (page 2, since page 1 was accessed) must be evicted!
        cache.insert(doc, 3, RenderKind::Page, Image::default(), 40);
        assert_eq!(cache.bytes, 80);
        assert!(cache.contains_key(doc, 1, RenderKind::Page));
        assert!(!cache.contains_key(doc, 2, RenderKind::Page));
        assert!(cache.contains_key(doc, 3, RenderKind::Page));

        // 5. Remove document purges all remaining entries and resets byte count
        cache.remove_document(doc);
        assert_eq!(cache.bytes, 0);
        assert!(!cache.contains_key(doc, 1, RenderKind::Page));
        assert!(!cache.contains_key(doc, 3, RenderKind::Page));
    }
}
