#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use barepdf_core::{
    default_config_path, selection::SelectionEngine, ContinuousLayout, DocumentId, MemoryBudget,
    PageIndex, PageTextGeometry, PdfError, RequestId, Rotation, TextPosition, TextSelection,
    ThemeMode, UserPreferences, ViewingMode, WindowMode, ZoomFactor, ZoomMode,
};
use barepdf_i18n::{Language, ResolvedLanguage};
use barepdf_pdf::{OutlineNode, PdfiumEngine};
use barepdf_platform::{ClipboardAccess, FileDialogs};
use barepdf_platform_windows::{
    install_file_drop, show_fatal_error, WindowsClipboard, WindowsFileDialogs,
};
use barepdf_render::{
    Priority, RenderCommand, RenderEvent, RenderJob, RenderKind, RenderScheduler,
};
use barepdf_ui::{
    AppWindow, OutlineItem, PageItem, RecentFileItem, SelectionBox, ThemeTokens, ThumbnailItem,
};
use lru::LruCache;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{
    ComponentHandle, Image, LogicalSize, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer,
    SharedString, Timer, TimerMode, VecModel,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const RAW_BITMAP_BUDGET: usize = 32 * 1024 * 1024;
const PAGE_IMAGE_BUDGET: usize = 16 * 1024 * 1024;
const THUMB_IMAGE_BUDGET: usize = 4 * 1024 * 1024;
const THUMB_ROW_HEIGHT: f32 = 188.0;
const PAGE_GAP: f32 = 14.0;
const SCROLL_IDLE_DELAY: Duration = Duration::from_millis(180);
const PRESENTATION_MAX_RENDER_EDGE: u32 = 2560;

#[derive(Clone)]
struct PendingDocument {
    id: DocumentId,
    path: PathBuf,
    started_at: Instant,
}

#[derive(Clone, PartialEq)]
struct LayoutKey {
    width: u32,
    height: u32,
    zoom_mode: ZoomMode,
    dimensions_revision: u64,
}

#[derive(Clone)]
struct FlatOutlineEntry {
    path: Vec<usize>,
    page_index: Option<u32>,
    has_children: bool,
}

struct CachedImage {
    image: Image,
    bytes: usize,
}

struct UiImageCache {
    entries: LruCache<u32, CachedImage>,
    bytes: usize,
    budget: usize,
}

impl UiImageCache {
    fn new(budget: usize) -> Self {
        Self {
            entries: LruCache::new(NonZeroUsize::new(512).expect("non-zero")),
            bytes: 0,
            budget,
        }
    }

    fn get(&mut self, page: u32) -> Option<Image> {
        self.entries.get(&page).map(|cached| cached.image.clone())
    }

    fn insert(&mut self, page: u32, image: Image, bytes: usize) {
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

    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

struct AppState {
    active_document: Option<DocumentId>,
    pending_document: Option<PendingDocument>,
    current_path: Option<PathBuf>,
    last_failed_path: Option<PathBuf>,
    page_count: u32,
    current_page: u32,
    viewing_mode: ViewingMode,
    zoom_mode: ZoomMode,
    zoom_factor: ZoomFactor,
    rotation: Rotation,
    first_page_dimensions: (f32, f32),
    page_dimensions: Vec<(f32, f32)>,
    dimensions_revision: u64,
    next_dimensions_start: u32,
    dimensions_request_pending: bool,
    layout: ContinuousLayout,
    layout_key: Option<LayoutKey>,
    visible_page_indices: Vec<u32>,
    generation: u64,
    first_page_ready: bool,
    profile_recorded: bool,
    open_started_at: Option<Instant>,
    window_mode: WindowMode,
    preferences: UserPreferences,
    text_geometries: HashMap<u32, PageTextGeometry>,
    selection: Option<TextSelection>,
    is_selecting: bool,
    last_click_time: Instant,
    click_count: u32,
    last_scroll_y: f32,
    last_thumbnail_scroll_y: f32,
    last_user_scroll_at: Option<Instant>,
    viewport_width: u32,
    viewport_height: u32,
    scale_factor: f32,
    resize_changed_at: Option<Instant>,
    outline: Vec<OutlineNode>,
    outline_requested: bool,
    expanded_outline: HashSet<Vec<usize>>,
    flat_outline: Vec<FlatOutlineEntry>,
    page_images: UiImageCache,
    thumbnail_images: UiImageCache,
}

impl AppState {
    fn new(mut preferences: UserPreferences) -> Self {
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
            text_geometries: HashMap::new(),
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
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let pdf_engine = match PdfiumEngine::new() {
        Ok(engine) => engine,
        Err(error) => {
            show_fatal_error(
                "BarePDF",
                &format!("PDF engine could not be loaded.\n\n{error}"),
            );
            return Err(Box::new(error));
        }
    };

    let preferences_path = default_config_path();
    let mut preferences = UserPreferences::load_from_file(&preferences_path);
    let original_recent_count = preferences.recent_files.len();
    preferences
        .recent_files
        .retain(|path| Path::new(path).is_file());
    preferences.viewing_mode = normalize_viewing_mode(preferences.viewing_mode);
    if original_recent_count != preferences.recent_files.len() {
        let _ = preferences.save_to_file(&preferences_path);
    }

    let scheduler = Rc::new(RenderScheduler::spawn(
        pdf_engine,
        MemoryBudget::new(RAW_BITMAP_BUDGET),
    ));
    let dialogs = Arc::new(WindowsFileDialogs);
    let clipboard = Arc::new(WindowsClipboard::new());
    let window = AppWindow::new()?;
    window.window().set_size(LogicalSize::new(
        preferences.last_window_width.max(760) as f32,
        preferences.last_window_height.max(520) as f32,
    ));

    let state = Rc::new(RefCell::new(AppState::new(preferences)));
    initialize_window(&window, &state.borrow());
    apply_theme(&window, state.borrow().preferences.theme);
    update_ui_strings(&window, state.borrow().preferences.language.resolve());
    refresh_recent_files(&mut state.borrow_mut(), &window, &preferences_path);

    wire_callbacks(
        &window,
        &state,
        &scheduler,
        dialogs,
        clipboard,
        &preferences_path,
    );

    if let Some(argument) = env::args_os().nth(1) {
        let path = PathBuf::from(argument);
        if path.is_file() {
            begin_open(path, None, &state, &scheduler, &window);
        }
    }

    let timer = Timer::default();
    start_event_timer(&timer, &window, &state, &scheduler, &preferences_path, None);
    window.run()?;

    let scale = window.window().scale_factor().max(0.1);
    let size = window.window().size();
    let mut app = state.borrow_mut();
    app.preferences.last_window_width = ((size.width as f32) / scale).round() as u32;
    app.preferences.last_window_height = ((size.height as f32) / scale).round() as u32;
    let _ = app.preferences.save_to_file(&preferences_path);
    Ok(())
}

fn initialize_window(window: &AppWindow, state: &AppState) {
    window.set_sidebar_visible(state.preferences.sidebar_visible);
    window.set_current_language(language_index(state.preferences.language));
    window.set_view_mode(view_mode_index(state.viewing_mode));
    window.set_view_mode_label(SharedString::from(view_mode_label(
        state.viewing_mode,
        state.preferences.language.resolve(),
    )));
    window.set_status_text(SharedString::from(barepdf_i18n::t(
        state.preferences.language.resolve(),
        "status.ready",
    )));
}

fn wire_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    dialogs: Arc<WindowsFileDialogs>,
    clipboard: Arc<WindowsClipboard>,
    preferences_path: &Path,
) {
    let weak = window.as_weak();
    let state_open = state.clone();
    let scheduler_open = scheduler.clone();
    window.on_request_open_file(move || {
        if let (Some(path), Some(window)) = (dialogs.pick_file(), weak.upgrade()) {
            begin_open(path, None, &state_open, &scheduler_open, &window);
        }
    });

    connect_navigation_callbacks(window, state, scheduler);
    connect_zoom_callbacks(window, state, scheduler, preferences_path);
    connect_view_callbacks(window, state, scheduler, preferences_path);
    connect_selection_callbacks(window, state, scheduler, clipboard);

    let weak = window.as_weak();
    let state_password = state.clone();
    let scheduler_password = scheduler.clone();
    window.on_request_unlock_password(move |password| {
        let path = state_password
            .borrow()
            .pending_document
            .as_ref()
            .map(|pending| pending.path.clone());
        if let (Some(path), Some(window)) = (path, weak.upgrade()) {
            begin_open(
                path,
                Some(password.to_string()),
                &state_password,
                &scheduler_password,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_language = state.clone();
    let preferences_path_language = preferences_path.to_path_buf();
    window.on_request_change_language(move |index| {
        let language = match index {
            1 => Language::English,
            2 => Language::Turkish,
            _ => Language::System,
        };
        let mut app = state_language.borrow_mut();
        app.preferences.language = language;
        let _ = app.preferences.save_to_file(&preferences_path_language);
        if let Some(window) = weak.upgrade() {
            window.set_current_language(index);
            window.set_view_mode_label(SharedString::from(view_mode_label(
                app.viewing_mode,
                language.resolve(),
            )));
            update_ui_strings(&window, language.resolve());
        }
    });

    let weak = window.as_weak();
    let state_theme = state.clone();
    let preferences_path_theme = preferences_path.to_path_buf();
    window.on_request_change_theme(move |index| {
        let theme = theme_from_index(index);
        let mut app = state_theme.borrow_mut();
        app.preferences.theme = theme;
        let _ = app.preferences.save_to_file(&preferences_path_theme);
        if let Some(window) = weak.upgrade() {
            apply_theme(&window, theme);
        }
    });

    let weak = window.as_weak();
    let state_recent = state.clone();
    let scheduler_recent = scheduler.clone();
    window.on_request_open_recent(move |path| {
        if let Some(window) = weak.upgrade() {
            begin_open(
                PathBuf::from(path.as_str()),
                None,
                &state_recent,
                &scheduler_recent,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_drop = state.clone();
    let scheduler_drop = scheduler.clone();
    window.on_request_drop(move |transfer| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        match transfer
            .plain_text()
            .ok()
            .and_then(|text| parse_drop_paths(text.as_str()))
        {
            Some(Ok(path)) => begin_open(path, None, &state_drop, &scheduler_drop, &window),
            Some(Err(message)) => show_banner(&window, message, false),
            None => show_banner(&window, "The dropped item is not a PDF file.", false),
        }
    });

    let weak = window.as_weak();
    window.on_request_dismiss_banner(move || {
        if let Some(window) = weak.upgrade() {
            window.set_banner_visible(false);
        }
    });

    let weak = window.as_weak();
    let state_retry = state.clone();
    let scheduler_retry = scheduler.clone();
    window.on_request_retry(move || {
        let path = state_retry.borrow().last_failed_path.clone();
        if let (Some(path), Some(window)) = (path, weak.upgrade()) {
            begin_open(path, None, &state_retry, &scheduler_retry, &window);
        }
    });
}

fn connect_navigation_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let connect = |register: fn(&AppWindow, Box<dyn Fn()>), target: NavigationTarget| {
        let weak = window.as_weak();
        let state = state.clone();
        let scheduler = scheduler.clone();
        register(
            window,
            Box::new(move || {
                if let Some(window) = weak.upgrade() {
                    let page = {
                        let app = state.borrow();
                        match target {
                            NavigationTarget::Previous => app.current_page.saturating_sub(1),
                            NavigationTarget::Next => {
                                (app.current_page + 1).min(app.page_count.saturating_sub(1))
                            }
                            NavigationTarget::First => 0,
                            NavigationTarget::Last => app.page_count.saturating_sub(1),
                        }
                    };
                    navigate_to_page(page, &state, &scheduler, &window);
                }
            }),
        );
    };
    connect(
        |w, cb| w.on_request_prev_page(cb),
        NavigationTarget::Previous,
    );
    connect(|w, cb| w.on_request_next_page(cb), NavigationTarget::Next);
    connect(|w, cb| w.on_request_first_page(cb), NavigationTarget::First);
    connect(|w, cb| w.on_request_last_page(cb), NavigationTarget::Last);

    let weak = window.as_weak();
    let state_select = state.clone();
    let scheduler_select = scheduler.clone();
    window.on_request_select_page(move |page| {
        if page >= 0 {
            if let Some(window) = weak.upgrade() {
                navigate_to_page(page as u32, &state_select, &scheduler_select, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_entry = state.clone();
    let scheduler_entry = scheduler.clone();
    window.on_request_go_to_page(move |text| {
        let count = state_entry.borrow().page_count;
        if let Some(page) = validated_page_input(text.as_str(), count) {
            if let Some(window) = weak.upgrade() {
                navigate_to_page(page, &state_entry, &scheduler_entry, &window);
            }
        } else if let Some(window) = weak.upgrade() {
            let current = state_entry.borrow().current_page + 1;
            window.set_current_page_str(SharedString::from(current.to_string()));
        }
    });
}

#[derive(Clone, Copy)]
enum NavigationTarget {
    Previous,
    Next,
    First,
    Last,
}

fn connect_zoom_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    preferences_path: &Path,
) {
    let weak = window.as_weak();
    let state_in = state.clone();
    let scheduler_in = scheduler.clone();
    let preferences_path_in = preferences_path.to_path_buf();
    window.on_request_zoom_in(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_in.borrow_mut();
            sync_effective_zoom(&mut app);
            app.zoom_factor = app.zoom_factor.zoom_in();
            app.zoom_mode = ZoomMode::Custom(app.zoom_factor);
            save_zoom_preference(&mut app, &preferences_path_in);
            invalidate_layout_and_render(&mut app, &scheduler_in, &window, true);
        }
    });

    let weak = window.as_weak();
    let state_out = state.clone();
    let scheduler_out = scheduler.clone();
    let preferences_path_out = preferences_path.to_path_buf();
    window.on_request_zoom_out(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_out.borrow_mut();
            sync_effective_zoom(&mut app);
            app.zoom_factor = app.zoom_factor.zoom_out();
            app.zoom_mode = ZoomMode::Custom(app.zoom_factor);
            save_zoom_preference(&mut app, &preferences_path_out);
            invalidate_layout_and_render(&mut app, &scheduler_out, &window, true);
        }
    });

    connect_zoom_mode(
        window,
        state,
        scheduler,
        preferences_path,
        ZoomMode::FitWidth,
        |w, cb| w.on_request_fit_width(cb),
    );
    connect_zoom_mode(
        window,
        state,
        scheduler,
        preferences_path,
        ZoomMode::FitPage,
        |w, cb| w.on_request_fit_page(cb),
    );
    connect_zoom_mode(
        window,
        state,
        scheduler,
        preferences_path,
        ZoomMode::ActualSize,
        |w, cb| w.on_request_actual_size(cb),
    );
}

fn connect_zoom_mode<F>(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    preferences_path: &Path,
    mode: ZoomMode,
    register: F,
) where
    F: FnOnce(&AppWindow, Box<dyn Fn()>),
{
    let weak = window.as_weak();
    let state = state.clone();
    let scheduler = scheduler.clone();
    let preferences_path = preferences_path.to_path_buf();
    register(
        window,
        Box::new(move || {
            if let Some(window) = weak.upgrade() {
                let mut app = state.borrow_mut();
                app.zoom_mode = mode;
                save_zoom_preference(&mut app, &preferences_path);
                invalidate_layout_and_render(&mut app, &scheduler, &window, true);
            }
        }),
    );
}

fn connect_view_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    preferences_path: &Path,
) {
    let weak = window.as_weak();
    let state_view = state.clone();
    let scheduler_view = scheduler.clone();
    let preferences_path_view = preferences_path.to_path_buf();
    window.on_request_toggle_view_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_view.borrow_mut();
            app.viewing_mode = match app.viewing_mode {
                ViewingMode::ContinuousVertical => ViewingMode::SinglePage,
                _ => ViewingMode::ContinuousVertical,
            };
            app.preferences.viewing_mode = app.viewing_mode;
            let _ = app.preferences.save_to_file(&preferences_path_view);
            window.set_view_mode(view_mode_index(app.viewing_mode));
            window.set_view_mode_label(SharedString::from(view_mode_label(
                app.viewing_mode,
                app.preferences.language.resolve(),
            )));
            navigate_to_page_inner(app.current_page, &mut app, &scheduler_view, &window);
        }
    });

    let weak = window.as_weak();
    let state_sidebar = state.clone();
    let scheduler_sidebar = scheduler.clone();
    let preferences_path_sidebar = preferences_path.to_path_buf();
    window.on_request_toggle_sidebar(move || {
        if let Some(window) = weak.upgrade() {
            let visible = !window.get_sidebar_visible();
            window.set_sidebar_visible(visible);
            let mut app = state_sidebar.borrow_mut();
            app.preferences.sidebar_visible = visible;
            app.layout_key = None;
            let _ = app.preferences.save_to_file(&preferences_path_sidebar);
            if visible {
                request_visible_thumbnails(&app, &scheduler_sidebar, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_tab = state.clone();
    let scheduler_tab = scheduler.clone();
    window.on_request_sidebar_tab(move |tab| {
        if let Some(window) = weak.upgrade() {
            let mut app = state_tab.borrow_mut();
            if tab == 1 && !app.outline_requested {
                if let Some(document_id) = app.active_document {
                    app.outline_requested =
                        scheduler_tab.send_command(RenderCommand::FetchOutline { document_id });
                }
            } else if tab == 0 {
                request_visible_thumbnails(&app, &scheduler_tab, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_outline = state.clone();
    let scheduler_outline = scheduler.clone();
    window.on_request_toggle_outline(move |index| {
        let mut target = None;
        {
            let mut app = state_outline.borrow_mut();
            if let Some(entry) = app.flat_outline.get(index as usize).cloned() {
                if entry.has_children {
                    if !app.expanded_outline.remove(&entry.path) {
                        app.expanded_outline.insert(entry.path.clone());
                    }
                    if let Some(window) = weak.upgrade() {
                        refresh_outline_model(&mut app, &window);
                    }
                }
                target = entry.page_index;
            }
        }
        if let (Some(page), Some(window)) = (target, weak.upgrade()) {
            navigate_to_page(page, &state_outline, &scheduler_outline, &window);
        }
    });

    let weak = window.as_weak();
    let state_fullscreen = state.clone();
    window.on_request_toggle_fullscreen(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_fullscreen.borrow_mut();
            let enabled = app.window_mode != WindowMode::FullScreen;
            app.window_mode = if enabled {
                WindowMode::FullScreen
            } else {
                WindowMode::Normal
            };
            window.set_window_mode(if enabled { 1 } else { 0 });
            window.window().set_fullscreen(enabled);
        }
    });

    let weak = window.as_weak();
    let state_presentation = state.clone();
    let scheduler_presentation = scheduler.clone();
    window.on_request_presentation_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_presentation.borrow_mut();
            app.window_mode = WindowMode::Presentation;
            window.set_window_mode(2);
            window.window().set_fullscreen(true);
            app.generation = scheduler_presentation.bump_generation();
            render_visible_pages(&mut app, &scheduler_presentation, &window);
        }
    });

    let weak = window.as_weak();
    let state_exit = state.clone();
    window.on_request_exit_special_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_exit.borrow_mut();
            if app.window_mode != WindowMode::Normal {
                app.window_mode = WindowMode::Normal;
                window.set_window_mode(0);
                window.window().set_fullscreen(false);
            }
        }
    });
}

fn connect_selection_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    clipboard: Arc<WindowsClipboard>,
) {
    let state_copy = state.clone();
    window.on_request_copy(move || {
        let app = state_copy.borrow();
        if let Some(selection) = app.selection {
            let geometries: Vec<_> = app.text_geometries.values().cloned().collect();
            let text = SelectionEngine::get_selected_text(&selection, &geometries);
            if !text.is_empty() {
                let _ = clipboard.set_text(&text);
            }
        }
    });

    let weak = window.as_weak();
    let state_all = state.clone();
    window.on_request_select_all(move || {
        let mut app = state_all.borrow_mut();
        if app.page_count == 0 {
            return;
        }
        let last_page = app.page_count - 1;
        let last_character = app
            .text_geometries
            .get(&last_page)
            .map(|geometry| geometry.glyphs.len() as u32)
            .unwrap_or(u32::MAX);
        app.selection = Some(TextSelection::new(
            TextPosition::new(PageIndex::zero(), 0),
            TextPosition::new(PageIndex::from_raw(last_page), last_character),
        ));
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(true);
            refresh_page_model(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_down = state.clone();
    let scheduler_down = scheduler.clone();
    window.on_pointer_down(move |page, x, y, _| {
        if page < 0 {
            return;
        }
        let mut app = state_down.borrow_mut();
        let page = page as u32;
        if page >= app.page_count {
            return;
        }
        if !app.text_geometries.contains_key(&page) {
            if let Some(document_id) = app.active_document {
                scheduler_down.send_command(RenderCommand::FetchTextGeometry {
                    document_id,
                    generation: app.generation,
                    page_index: PageIndex::from_raw(page),
                });
            }
        }
        let (pdf_x, pdf_y) = pointer_to_pdf(&app, page, x, y);
        let now = Instant::now();
        app.click_count = if now.duration_since(app.last_click_time) < Duration::from_millis(400) {
            app.click_count + 1
        } else {
            1
        };
        app.last_click_time = now;
        if let Some(geometry) = app.text_geometries.get(&page) {
            let page_index = PageIndex::from_raw(page);
            let character = SelectionEngine::hit_test(geometry, pdf_x, pdf_y);
            app.selection = Some(match app.click_count {
                2 => SelectionEngine::select_word(geometry, page_index, character),
                count if count >= 3 => {
                    SelectionEngine::select_line(geometry, page_index, character)
                }
                _ => {
                    app.is_selecting = true;
                    let position = TextPosition::new(page_index, character);
                    TextSelection::new(position, position)
                }
            });
        }
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(app.selection.is_some_and(|selection| !selection.is_empty()));
            refresh_page_model(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_move = state.clone();
    window.on_pointer_move(move |page, x, y| {
        if page < 0 {
            return;
        }
        let mut app = state_move.borrow_mut();
        if !app.is_selecting {
            return;
        }
        let page = page as u32;
        let (pdf_x, pdf_y) = pointer_to_pdf(&app, page, x, y);
        let character = app
            .text_geometries
            .get(&page)
            .map(|geometry| SelectionEngine::hit_test(geometry, pdf_x, pdf_y))
            .unwrap_or(0);
        if let Some(selection) = app.selection.as_mut() {
            selection.focus = TextPosition::new(PageIndex::from_raw(page), character);
        }
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(app.selection.is_some_and(|selection| !selection.is_empty()));
            refresh_page_model(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_up = state.clone();
    window.on_pointer_up(move |_, _, _| {
        let mut app = state_up.borrow_mut();
        app.is_selecting = false;
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(app.selection.is_some_and(|selection| !selection.is_empty()));
        }
    });
}

fn start_event_timer(
    timer: &Timer,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    preferences_path: &Path,
    mut native_drop_receiver: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
) {
    let weak = window.as_weak();
    let state = state.clone();
    let scheduler = scheduler.clone();
    let preferences_path = preferences_path.to_path_buf();
    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if native_drop_receiver.is_none() {
            native_drop_receiver = install_native_file_drop(&window);
        }
        if let Some(receiver) = native_drop_receiver.as_ref() {
            while let Ok(paths) = receiver.try_recv() {
                match paths.as_slice() {
                    [path] if is_pdf_path(path) => {
                        begin_open(path.clone(), None, &state, &scheduler, &window)
                    }
                    [_] => show_banner(&window, "Only PDF files are supported.", false),
                    _ => show_banner(&window, "Drop exactly one PDF file.", false),
                }
            }
        }
        process_view_changes(&window, &state, &scheduler);
        while let Some(event) = scheduler.try_recv_event() {
            handle_render_event(event, &window, &state, &scheduler, &preferences_path);
        }
    });
}

fn install_native_file_drop(window: &AppWindow) -> Option<std::sync::mpsc::Receiver<Vec<PathBuf>>> {
    let window_handle = window.window().window_handle();
    let handle = window_handle.window_handle().ok()?;
    match handle.as_raw() {
        // SAFETY: Slint supplies the live HWND for this window while the event loop owns it.
        RawWindowHandle::Win32(handle) => unsafe { install_file_drop(handle.hwnd.get() as _) },
        _ => None,
    }
}

fn process_view_changes(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &RenderScheduler,
) {
    let width = window.get_pdf_viewport_width().max(240.0).round() as u32;
    let height = window.get_pdf_viewport_height().max(240.0).round() as u32;
    let scale = window.window().scale_factor().max(0.5);
    let scroll = window.get_current_scroll_y();
    let thumbnail_scroll = window.get_thumbnail_scroll_y();
    let now = Instant::now();

    let mut app = state.borrow_mut();
    if width != app.viewport_width
        || height != app.viewport_height
        || (scale - app.scale_factor).abs() > 0.01
    {
        app.viewport_width = width;
        app.viewport_height = height;
        app.scale_factor = scale;
        app.resize_changed_at = Some(now);
    }

    if app
        .resize_changed_at
        .is_some_and(|changed| now.duration_since(changed) >= Duration::from_millis(100))
    {
        app.resize_changed_at = None;
        invalidate_layout_and_render(&mut app, scheduler, window, true);
    }

    if app.viewing_mode == ViewingMode::ContinuousVertical
        && app.page_count > 0
        && (scroll - app.last_scroll_y).abs() > 12.0
    {
        app.last_scroll_y = scroll;
        app.last_user_scroll_at = Some(now);
        ensure_layout(&mut app);
        let page = app
            .layout
            .primary_page((-scroll).max(0.0), app.viewport_height as f32)
            .get();
        let previous_page = app.current_page;
        app.current_page = page;
        window.set_current_page_str(SharedString::from((page + 1).to_string()));
        request_next_dimensions_batch(&mut app, scheduler);
        let pages = visible_page_indices(&app, window);
        if pages != app.visible_page_indices {
            app.generation = scheduler.bump_generation();
            render_visible_pages(&mut app, scheduler, window);
            request_visible_thumbnails(&app, scheduler, window);
        } else if previous_page != page && !app.text_geometries.contains_key(&page) {
            if let Some(document_id) = app.active_document {
                scheduler.send_command(RenderCommand::FetchTextGeometry {
                    document_id,
                    generation: app.generation,
                    page_index: PageIndex::from_raw(page),
                });
            }
        }
        refresh_thumbnail_selection(&mut app, window, previous_page);
    }

    if (thumbnail_scroll - app.last_thumbnail_scroll_y).abs() > THUMB_ROW_HEIGHT * 0.5 {
        app.last_thumbnail_scroll_y = thumbnail_scroll;
        request_visible_thumbnails(&app, scheduler, window);
    }
}

fn handle_render_event(
    event: RenderEvent,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &RenderScheduler,
    preferences_path: &Path,
) {
    match event {
        RenderEvent::DocumentOpened {
            document_id,
            page_count,
            first_page_dimensions,
        } => {
            let mut app = state.borrow_mut();
            let Some(pending) = app.pending_document.as_ref() else {
                return;
            };
            if pending.id != document_id {
                return;
            }
            let pending = app.pending_document.take().expect("pending checked");
            app.active_document = Some(document_id);
            app.current_path = Some(pending.path.clone());
            app.current_page = 0;
            app.page_count = page_count;
            app.first_page_dimensions = first_page_dimensions;
            app.page_dimensions = vec![first_page_dimensions; page_count as usize];
            app.dimensions_revision += 1;
            app.next_dimensions_start = 1;
            app.dimensions_request_pending = false;
            app.layout_key = None;
            app.visible_page_indices.clear();
            app.first_page_ready = false;
            app.profile_recorded = false;
            app.open_started_at = Some(pending.started_at);
            app.outline.clear();
            app.outline_requested = false;
            app.expanded_outline.clear();
            app.flat_outline.clear();
            app.text_geometries.clear();
            app.selection = None;
            app.last_scroll_y = 0.0;
            app.last_thumbnail_scroll_y = 0.0;
            app.last_user_scroll_at = None;
            app.page_images.clear();
            app.thumbnail_images.clear();
            app.preferences
                .add_recent_file(pending.path.to_string_lossy().into_owned());
            let _ = app.preferences.save_to_file(preferences_path);

            window.set_has_document(true);
            window.set_password_required(false);
            window.set_password_error(SharedString::default());
            window.set_banner_visible(false);
            window.set_has_selection(false);
            let file_name = pending
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("document.pdf");
            window.set_document_title(SharedString::from(file_name));
            window.set_total_pages_str(SharedString::from(page_count.to_string()));
            window.set_status_text(SharedString::from(format!(
                "{} ({} pages)",
                file_name, page_count
            )));
            refresh_recent_files(&mut app, window, preferences_path);
            refresh_thumbnail_model(&mut app, window);
            navigate_to_page_inner(0, &mut app, scheduler, window);
        }
        RenderEvent::PageRendered {
            generation,
            document_id,
            page_index,
            kind,
            bitmap,
            ..
        } => {
            let mut app = state.borrow_mut();
            if app.active_document != Some(document_id) || app.generation != generation {
                return;
            }
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                &bitmap.pixels,
                bitmap.width,
                bitmap.height,
            );
            let image = Image::from_rgba8(buffer);
            match kind {
                RenderKind::Page => {
                    app.page_images
                        .insert(page_index.get(), image.clone(), bitmap.pixels.len());
                    if page_index.get() == app.current_page {
                        window.set_page_bitmap(image);
                    }
                    if !app.first_page_ready {
                        app.first_page_ready = true;
                        record_first_page_profile(&mut app);
                        start_deferred_document_work(&mut app, scheduler, window);
                        render_visible_pages(&mut app, scheduler, window);
                    }
                    refresh_page_model(&mut app, window);
                }
                RenderKind::Thumbnail => {
                    app.thumbnail_images
                        .insert(page_index.get(), image, bitmap.pixels.len());
                    refresh_thumbnail_row(&mut app, window, page_index.get());
                }
            }
        }
        RenderEvent::TextGeometryFetched {
            document_id,
            generation,
            page_index,
            geometry,
        } => {
            let mut app = state.borrow_mut();
            if app.active_document == Some(document_id) && app.generation == generation {
                app.text_geometries.insert(page_index.get(), geometry);
                refresh_page_model(&mut app, window);
            }
        }
        RenderEvent::OutlineFetched {
            document_id,
            outline,
        } => {
            let mut app = state.borrow_mut();
            if app.active_document == Some(document_id) {
                app.outline = outline;
                for index in 0..app.outline.len() {
                    if !app.outline[index].children.is_empty() {
                        app.expanded_outline.insert(vec![index]);
                    }
                }
                refresh_outline_model(&mut app, window);
            }
        }
        RenderEvent::PageDimensionsFetched {
            document_id,
            start,
            dimensions,
        } => {
            let mut app = state.borrow_mut();
            if app.active_document != Some(document_id) {
                return;
            }
            app.dimensions_request_pending = false;
            if dimensions.is_empty() {
                return;
            }
            let restore_scroll = !user_is_scrolling(app.last_user_scroll_at, Instant::now());
            ensure_layout(&mut app);
            let anchor = app.layout.compute_anchor(
                (-window.get_current_scroll_y()).max(0.0),
                app.viewport_height as f32,
            );
            for (offset, dimensions) in dimensions.iter().copied().enumerate() {
                if let Some(slot) = app.page_dimensions.get_mut(start as usize + offset) {
                    *slot = dimensions;
                }
            }
            app.dimensions_revision += 1;
            app.layout_key = None;
            ensure_layout(&mut app);
            if restore_scroll && app.viewing_mode == ViewingMode::ContinuousVertical {
                let scroll_y = -app.layout.restore_anchor(anchor);
                set_scroll_position(&mut app, window, scroll_y);
            }
            for index in start..start.saturating_add(dimensions.len() as u32) {
                refresh_thumbnail_row(&mut app, window, index);
            }
            app.next_dimensions_start = start + dimensions.len() as u32;
            if app.current_page.saturating_add(16) >= app.next_dimensions_start {
                request_next_dimensions_batch(&mut app, scheduler);
            }
            app.generation = scheduler.bump_generation();
            render_visible_pages(&mut app, scheduler, window);
        }
        RenderEvent::Error {
            document_id,
            generation,
            error,
            ..
        } => {
            let mut app = state.borrow_mut();
            if app
                .pending_document
                .as_ref()
                .is_some_and(|pending| pending.id == document_id)
            {
                match error {
                    PdfError::PasswordRequired => {
                        let name = app
                            .pending_document
                            .as_ref()
                            .and_then(|pending| pending.path.file_name())
                            .and_then(|name| name.to_str())
                            .unwrap_or("document.pdf");
                        window.set_protected_file_name(SharedString::from(name));
                        window.set_password_error(SharedString::default());
                        window.set_password_required(true);
                    }
                    PdfError::IncorrectPassword => {
                        window.set_password_error(SharedString::from("Incorrect password."));
                        window.set_password_required(true);
                    }
                    other => {
                        app.last_failed_path =
                            app.pending_document.take().map(|pending| pending.path);
                        show_banner(window, format!("Could not open PDF: {other}"), true);
                    }
                }
            } else if app.active_document == Some(document_id)
                && generation.is_none_or(|generation| generation == app.generation)
            {
                show_banner(window, format!("PDF rendering failed: {error}"), true);
            }
        }
        RenderEvent::TextExtracted { .. } => {}
    }
}

fn begin_open(
    path: PathBuf,
    password: Option<String>,
    state: &Rc<RefCell<AppState>>,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    if !path.is_file() || !is_pdf_path(&path) {
        show_banner(window, "Choose an existing PDF file.", false);
        return;
    }
    let pending = PendingDocument {
        id: DocumentId::new(unique_id()),
        path: path.clone(),
        started_at: Instant::now(),
    };
    let document_id = pending.id;
    state.borrow_mut().pending_document = Some(pending);
    window.set_status_text(SharedString::from("Opening document…"));
    window.set_password_error(SharedString::default());
    scheduler.send_command(RenderCommand::OpenDocument {
        document_id,
        path,
        password,
    });
}

fn navigate_to_page(
    page: u32,
    state: &Rc<RefCell<AppState>>,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    let mut app = state.borrow_mut();
    navigate_to_page_inner(page, &mut app, scheduler, window);
}

fn set_scroll_position(app: &mut AppState, window: &AppWindow, scroll_y: f32) {
    app.last_scroll_y = scroll_y;
    window.set_current_scroll_y(scroll_y);
}

fn navigate_to_page_inner(
    page: u32,
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    if app.page_count == 0 {
        return;
    }
    let previous_page = app.current_page;
    app.current_page = page.min(app.page_count - 1);
    ensure_layout(app);
    if app.viewing_mode == ViewingMode::ContinuousVertical
        && app.window_mode != WindowMode::Presentation
    {
        if let Some(scroll_y) = app
            .layout
            .pages
            .get(app.current_page as usize)
            .map(|page| -page.y_offset)
        {
            set_scroll_position(app, window, scroll_y);
        }
    }
    app.generation = scheduler.bump_generation();
    window.set_current_page_str(SharedString::from((app.current_page + 1).to_string()));
    request_next_dimensions_batch(app, scheduler);
    render_visible_pages(app, scheduler, window);
    refresh_thumbnail_selection(app, window, previous_page);
    request_visible_thumbnails(app, scheduler, window);
}

fn invalidate_layout_and_render(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
    clear_images: bool,
) {
    let old_anchor =
        if app.viewing_mode == ViewingMode::ContinuousVertical && !app.layout.pages.is_empty() {
            Some(app.layout.compute_anchor(
                (-window.get_current_scroll_y()).max(0.0),
                app.viewport_height as f32,
            ))
        } else {
            None
        };
    app.layout_key = None;
    ensure_layout(app);
    if let Some(anchor) = old_anchor {
        set_scroll_position(app, window, -app.layout.restore_anchor(anchor));
    }
    if clear_images {
        app.page_images.clear();
    }
    app.generation = scheduler.bump_generation();
    render_visible_pages(app, scheduler, window);
}

fn ensure_layout(app: &mut AppState) {
    let key = LayoutKey {
        width: app.viewport_width,
        height: app.viewport_height,
        zoom_mode: app.zoom_mode,
        dimensions_revision: app.dimensions_revision,
    };
    if app.layout_key.as_ref() == Some(&key) {
        return;
    }
    app.layout = ContinuousLayout::compute(
        &app.page_dimensions,
        app.viewport_width.saturating_sub(32).max(1),
        app.viewport_height.saturating_sub(32).max(1),
        app.zoom_mode,
        1.0,
        PAGE_GAP,
    );
    app.layout_key = Some(key);
}

fn visible_page_indices(app: &AppState, window: &AppWindow) -> Vec<u32> {
    if app.page_count == 0 {
        return Vec::new();
    }
    if app.window_mode == WindowMode::Presentation || app.viewing_mode == ViewingMode::SinglePage {
        return vec![app.current_page];
    }
    let visible = app.layout.visible_pages(
        (-window.get_current_scroll_y()).max(0.0),
        app.viewport_height as f32,
    );
    let first = visible
        .first()
        .map(|page| page.get())
        .unwrap_or(app.current_page);
    let last = visible
        .last()
        .map(|page| page.get())
        .unwrap_or(app.current_page);
    (first.saturating_sub(1)..=(last + 1).min(app.page_count.saturating_sub(1))).collect()
}

fn presentation_render_size(page: (f32, f32), viewport: (u32, u32), dpi_scale: f32) -> (u32, u32) {
    let page_width = page.0.max(1.0);
    let page_height = page.1.max(1.0);
    let fit_scale = (viewport.0 as f32 / page_width)
        .min(viewport.1 as f32 / page_height)
        .max(0.01);
    let mut width = page_width * fit_scale * dpi_scale;
    let mut height = page_height * fit_scale * dpi_scale;
    let edge_scale = (PRESENTATION_MAX_RENDER_EDGE as f32 / width.max(height)).min(1.0);
    width *= edge_scale;
    height *= edge_scale;
    (
        width.round().max(1.0) as u32,
        height.round().max(1.0) as u32,
    )
}

fn user_is_scrolling(last_scroll: Option<Instant>, now: Instant) -> bool {
    last_scroll.is_some_and(|last| now.saturating_duration_since(last) < SCROLL_IDLE_DELAY)
}

fn render_visible_pages(app: &mut AppState, scheduler: &RenderScheduler, window: &AppWindow) {
    let Some(document_id) = app.active_document else {
        return;
    };
    ensure_layout(app);
    window.set_document_total_height(app.layout.total_height);

    let pages = visible_page_indices(app, window);

    for page in pages.iter().copied() {
        let Some(layout_page) = app.layout.pages.get(page as usize) else {
            continue;
        };
        // Show the first bitmap at logical resolution, then immediately replace it with the
        // native-DPI render. This keeps startup responsive without leaving high-DPI displays soft.
        let render_scale = if app.first_page_ready {
            app.scale_factor
        } else {
            0.45
        };
        let (render_width, render_height) = if app.window_mode == WindowMode::Presentation {
            presentation_render_size(
                app.page_dimensions
                    .get(page as usize)
                    .copied()
                    .unwrap_or(app.first_page_dimensions),
                (app.viewport_width, app.viewport_height),
                render_scale,
            )
        } else {
            (
                ((layout_page.width as f32) * render_scale).round() as u32,
                ((layout_page.height as f32) * render_scale).round() as u32,
            )
        };
        scheduler.send_command(RenderCommand::RenderPage(RenderJob {
            request_id: RequestId::new(unique_id()),
            generation: app.generation,
            document_id,
            page_index: PageIndex::from_raw(page),
            target_width: render_width.clamp(1, 4096),
            target_height: render_height.clamp(1, 4096),
            rotation: app.rotation,
            priority: if page == app.current_page {
                Priority::Visible
            } else {
                Priority::Prefetch
            },
            kind: RenderKind::Page,
        }));
    }

    if app.first_page_ready && !app.text_geometries.contains_key(&app.current_page) {
        scheduler.send_command(RenderCommand::FetchTextGeometry {
            document_id,
            generation: app.generation,
            page_index: PageIndex::from_raw(app.current_page),
        });
    }
    refresh_page_model(app, window);
}

fn refresh_page_model(app: &mut AppState, window: &AppWindow) {
    ensure_layout(app);
    let indices = visible_page_indices(app, window);
    app.visible_page_indices.clone_from(&indices);

    let model = VecModel::default();
    for index in indices {
        let Some(layout_page) = app.layout.pages.get(index as usize).cloned() else {
            continue;
        };
        let image = app.page_images.get(index);
        let has_bitmap = image.is_some();
        let selection_boxes = compute_selection_boxes(
            app,
            index,
            layout_page.width as f32,
            layout_page.height as f32,
        );
        model.push(PageItem {
            page_index: index as i32,
            page_number: SharedString::from((index + 1).to_string()),
            width: layout_page.width as f32,
            height: layout_page.height as f32,
            y_offset: layout_page.y_offset,
            bitmap: image.unwrap_or_default(),
            has_bitmap,
            selection_boxes: ModelRc::new(VecModel::from(selection_boxes)),
        });
    }
    window.set_visible_pages(ModelRc::new(model));

    if let Some(page) = app.layout.pages.get(app.current_page as usize) {
        window.set_page_display_width(page.width as f32);
        window.set_page_display_height(page.height as f32);
        let page_width = app
            .page_dimensions
            .get(app.current_page as usize)
            .map(|dimensions| dimensions.0)
            .unwrap_or(app.first_page_dimensions.0)
            .max(1.0);
        let effective_zoom = page.width as f32 / page_width;
        window.set_zoom_str(SharedString::from(format!(
            "{}%",
            (effective_zoom * 100.0).round()
        )));
    }
}

fn refresh_thumbnail_model(app: &mut AppState, window: &AppWindow) {
    let model = VecModel::default();
    for index in 0..app.page_count {
        model.push(thumbnail_item(app, index));
    }
    window.set_thumbnail_items(ModelRc::new(model));
}

fn refresh_thumbnail_row(app: &mut AppState, window: &AppWindow, index: u32) {
    let model = window.get_thumbnail_items();
    if index < app.page_count && (index as usize) < model.row_count() {
        model.set_row_data(index as usize, thumbnail_item(app, index));
    }
}

fn refresh_thumbnail_selection(app: &mut AppState, window: &AppWindow, previous_page: u32) {
    refresh_thumbnail_row(app, window, previous_page);
    if previous_page != app.current_page {
        refresh_thumbnail_row(app, window, app.current_page);
    }
}

fn thumbnail_item(app: &mut AppState, index: u32) -> ThumbnailItem {
    let (width, height) = app
        .page_dimensions
        .get(index as usize)
        .copied()
        .unwrap_or(app.first_page_dimensions);
    let display_width = 140.0;
    let display_height = (display_width * height / width.max(1.0)).min(150.0);
    let image = app.thumbnail_images.get(index);
    ThumbnailItem {
        page_index: index as i32,
        page_number: SharedString::from(format!("Page {}", index + 1)),
        width: display_width,
        height: display_height,
        bitmap: image.clone().unwrap_or_default(),
        has_bitmap: image.is_some(),
        is_selected: index == app.current_page,
    }
}

fn request_visible_thumbnails(app: &AppState, scheduler: &RenderScheduler, window: &AppWindow) {
    if !app.first_page_ready
        || !window.get_sidebar_visible()
        || window.get_sidebar_tab() != 0
        || app.page_count == 0
    {
        return;
    }
    let Some(document_id) = app.active_document else {
        return;
    };
    let first_visible = ((-window.get_thumbnail_scroll_y()).max(0.0) / THUMB_ROW_HEIGHT) as u32;
    let visible_count =
        (window.get_thumbnail_viewport_height().max(1.0) / THUMB_ROW_HEIGHT).ceil() as u32;
    let start = first_visible.saturating_sub(4);
    let end = (first_visible + visible_count + 4).min(app.page_count);
    for index in start..end {
        let (width, height) = app
            .page_dimensions
            .get(index as usize)
            .copied()
            .unwrap_or(app.first_page_dimensions);
        let target_width = (140.0 * app.scale_factor).round() as u32;
        let target_height = (target_width as f32 * height / width.max(1.0)).round() as u32;
        scheduler.send_command(RenderCommand::RenderPage(RenderJob {
            request_id: RequestId::new(unique_id()),
            generation: app.generation,
            document_id,
            page_index: PageIndex::from_raw(index),
            target_width: target_width.clamp(1, 512),
            target_height: target_height.clamp(1, 768),
            rotation: app.rotation,
            priority: Priority::Thumbnail,
            kind: RenderKind::Thumbnail,
        }));
    }
}

fn start_deferred_document_work(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    request_next_dimensions_batch(app, scheduler);
    request_visible_thumbnails(app, scheduler, window);
    if window.get_sidebar_tab() == 1 && !app.outline_requested {
        if let Some(document_id) = app.active_document {
            app.outline_requested =
                scheduler.send_command(RenderCommand::FetchOutline { document_id });
        }
    }
    if let Some(document_id) = app.active_document {
        scheduler.send_command(RenderCommand::FetchTextGeometry {
            document_id,
            generation: app.generation,
            page_index: PageIndex::from_raw(app.current_page),
        });
    }
}

fn request_next_dimensions_batch(app: &mut AppState, scheduler: &RenderScheduler) {
    if app.dimensions_request_pending || app.next_dimensions_start >= app.page_count {
        return;
    }
    if let Some(document_id) = app.active_document {
        app.dimensions_request_pending =
            scheduler.send_command(RenderCommand::FetchPageDimensions {
                document_id,
                start: app.next_dimensions_start,
                count: 32,
            });
    }
}

fn refresh_outline_model(app: &mut AppState, window: &AppWindow) {
    let mut items = Vec::new();
    let mut flat = Vec::new();
    flatten_outline(
        &app.outline,
        &app.expanded_outline,
        &mut Vec::new(),
        0,
        &mut items,
        &mut flat,
    );
    app.flat_outline = flat;
    window.set_outline_items(ModelRc::new(VecModel::from(items)));
}

fn flatten_outline(
    nodes: &[OutlineNode],
    expanded: &HashSet<Vec<usize>>,
    path: &mut Vec<usize>,
    depth: i32,
    items: &mut Vec<OutlineItem>,
    flat: &mut Vec<FlatOutlineEntry>,
) {
    for (index, node) in nodes.iter().enumerate() {
        path.push(index);
        let has_children = !node.children.is_empty();
        let is_expanded = has_children && expanded.contains(path);
        items.push(OutlineItem {
            title: SharedString::from(if node.title.is_empty() {
                "Untitled"
            } else {
                &node.title
            }),
            page_index: node.page_index.map(|page| page as i32).unwrap_or(-1),
            depth,
            has_children,
            expanded: is_expanded,
        });
        flat.push(FlatOutlineEntry {
            path: path.clone(),
            page_index: node.page_index,
            has_children,
        });
        if is_expanded {
            flatten_outline(&node.children, expanded, path, depth + 1, items, flat);
        }
        path.pop();
    }
}

fn refresh_recent_files(app: &mut AppState, window: &AppWindow, preferences_path: &Path) {
    let before = app.preferences.recent_files.len();
    app.preferences
        .recent_files
        .retain(|path| Path::new(path).is_file());
    if before != app.preferences.recent_files.len() {
        let _ = app.preferences.save_to_file(preferences_path);
    }
    let items = app
        .preferences
        .recent_files
        .iter()
        .take(5)
        .map(|path| {
            let name = Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path);
            RecentFileItem {
                name: SharedString::from(name),
                path: SharedString::from(path),
            }
        })
        .collect::<Vec<_>>();
    window.set_recent_files(ModelRc::new(VecModel::from(items)));
}

fn pointer_to_pdf(app: &AppState, page: u32, x: f32, y: f32) -> (f32, f32) {
    let (page_width, page_height) = app
        .page_dimensions
        .get(page as usize)
        .copied()
        .unwrap_or(app.first_page_dimensions);
    let (display_width, display_height) = app
        .layout
        .pages
        .get(page as usize)
        .map(|page| (page.width as f32, page.height as f32))
        .unwrap_or((page_width, page_height));
    (
        x * page_width / display_width.max(1.0),
        page_height - y * page_height / display_height.max(1.0),
    )
}

fn compute_selection_boxes(
    app: &AppState,
    page: u32,
    target_width: f32,
    target_height: f32,
) -> Vec<SelectionBox> {
    let Some(selection) = app.selection else {
        return Vec::new();
    };
    let Some((start, end)) = selection.range_for_page(PageIndex::from_raw(page)) else {
        return Vec::new();
    };
    let Some(geometry) = app.text_geometries.get(&page) else {
        return Vec::new();
    };
    let (page_width, page_height) = app
        .page_dimensions
        .get(page as usize)
        .copied()
        .unwrap_or(app.first_page_dimensions);
    let start = (start as usize).min(geometry.glyphs.len());
    let end = (end as usize).min(geometry.glyphs.len());
    geometry.glyphs[start..end]
        .iter()
        .filter(|glyph| glyph.width > 0.0 && glyph.height > 0.0)
        .map(|glyph| SelectionBox {
            x: glyph.x * target_width / page_width.max(1.0),
            y: (page_height - glyph.y - glyph.height) * target_height / page_height.max(1.0),
            width: glyph.width * target_width / page_width.max(1.0),
            height: glyph.height * target_height / page_height.max(1.0),
        })
        .collect()
}

fn sync_effective_zoom(app: &mut AppState) {
    if matches!(app.zoom_mode, ZoomMode::Custom(_)) {
        return;
    }
    ensure_layout(app);
    let Some(layout_page) = app.layout.pages.get(app.current_page as usize) else {
        return;
    };
    let page_width = app
        .page_dimensions
        .get(app.current_page as usize)
        .map(|dimensions| dimensions.0)
        .unwrap_or(app.first_page_dimensions.0)
        .max(1.0);
    app.zoom_factor = ZoomFactor::new(layout_page.width as f32 / page_width);
}

fn save_zoom_preference(app: &mut AppState, preferences_path: &Path) {
    app.preferences.zoom_mode = app.zoom_mode;
    let _ = app.preferences.save_to_file(preferences_path);
}

fn record_first_page_profile(app: &mut AppState) {
    if app.profile_recorded {
        return;
    }
    app.profile_recorded = true;
    let Some(started) = app.open_started_at else {
        return;
    };
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    if let Some(path) = env::var_os("BAREPDF_PROFILE_FILE") {
        let payload = format!("{{\"first_bitmap_ms\":{elapsed_ms:.2}}}\n");
        let _ = std::fs::write(path, payload);
    }
}

fn apply_theme(window: &AppWindow, theme: ThemeMode) {
    window.set_current_theme(theme_index(theme));
    window
        .global::<ThemeTokens>()
        .set_theme_mode(theme_index(theme));
}

fn show_banner(window: &AppWindow, message: impl Into<SharedString>, can_retry: bool) {
    window.set_banner_text(message.into());
    window.set_banner_can_retry(can_retry);
    window.set_banner_visible(true);
}

fn parse_drop_paths(text: &str) -> Option<Result<PathBuf, &'static str>> {
    let paths = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let decoded = line
                .strip_prefix("file:///")
                .unwrap_or(line)
                .replace("%20", " ")
                .replace('/', "\\");
            PathBuf::from(decoded)
        })
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return None;
    }
    if paths.len() != 1 {
        return Some(Err("Drop exactly one PDF file."));
    }
    let path = paths.into_iter().next().expect("one path");
    if !is_pdf_path(&path) {
        return Some(Err("Only PDF files are supported."));
    }
    Some(Ok(path))
}

fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

fn validated_page_input(input: &str, page_count: u32) -> Option<u32> {
    input
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|page| (1..=page_count).contains(page))
        .map(|page| page - 1)
}

fn normalize_viewing_mode(mode: ViewingMode) -> ViewingMode {
    match mode {
        ViewingMode::SinglePage => ViewingMode::SinglePage,
        _ => ViewingMode::ContinuousVertical,
    }
}

fn theme_from_index(index: i32) -> ThemeMode {
    match index {
        1 => ThemeMode::Light,
        2 => ThemeMode::Dark,
        _ => ThemeMode::System,
    }
}

fn theme_index(theme: ThemeMode) -> i32 {
    match theme {
        ThemeMode::System => 0,
        ThemeMode::Light => 1,
        ThemeMode::Dark => 2,
    }
}

fn language_index(language: Language) -> i32 {
    match language {
        Language::System => 0,
        Language::English => 1,
        Language::Turkish => 2,
    }
}

fn view_mode_index(mode: ViewingMode) -> i32 {
    i32::from(mode == ViewingMode::SinglePage)
}

fn view_mode_label(mode: ViewingMode, language: ResolvedLanguage) -> &'static str {
    barepdf_i18n::t(
        language,
        if mode == ViewingMode::SinglePage {
            "view.mode.single"
        } else {
            "view.mode.continuous"
        },
    )
}

fn update_ui_strings(window: &AppWindow, language: ResolvedLanguage) {
    macro_rules! set_text {
        ($setter:ident, $key:literal) => {
            window.$setter(SharedString::from(barepdf_i18n::t(language, $key)))
        };
    }
    set_text!(set_text_open, "open.file");
    set_text!(set_text_sidebar, "sidebar.toggle");
    set_text!(set_text_thumbnails, "sidebar.thumbnails");
    set_text!(set_text_outline, "sidebar.outline");
    set_text!(set_text_view, "view.mode");
    set_text!(set_text_zoom_in, "zoom.in");
    set_text!(set_text_zoom_out, "zoom.out");
    set_text!(set_text_fit_width, "zoom.fit_width");
    set_text!(set_text_fit_page, "zoom.fit_page");
    set_text!(set_text_actual_size, "zoom.actual_size");
    set_text!(set_text_fullscreen, "fullscreen");
    set_text!(set_text_presentation, "presentation");
    set_text!(set_text_settings, "settings");
    set_text!(set_text_copy, "context.copy");
    set_text!(set_text_select_all, "context.select_all");
    set_text!(set_text_close, "settings.close");
    set_text!(set_text_empty_title, "empty.title");
    set_text!(set_text_empty_desc, "empty.desc");
    set_text!(set_text_no_outline, "outline.empty");
    set_text!(set_text_recent, "recent.title");
    set_text!(set_text_retry, "action.retry");
    set_text!(set_text_dismiss, "action.dismiss");
    set_text!(set_text_loading, "status.loading");
}

fn unique_id() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_input_is_one_based_and_bounded() {
        assert_eq!(validated_page_input("1", 12), Some(0));
        assert_eq!(validated_page_input("12", 12), Some(11));
        assert_eq!(validated_page_input("0", 12), None);
        assert_eq!(validated_page_input("13", 12), None);
        assert_eq!(validated_page_input("abc", 12), None);
    }

    #[test]
    fn unsupported_view_modes_normalize_to_continuous() {
        assert_eq!(
            normalize_viewing_mode(ViewingMode::BookMode),
            ViewingMode::ContinuousVertical
        );
        assert_eq!(
            normalize_viewing_mode(ViewingMode::TwoPageSpread),
            ViewingMode::ContinuousVertical
        );
        assert_eq!(
            normalize_viewing_mode(ViewingMode::SinglePage),
            ViewingMode::SinglePage
        );
    }

    #[test]
    fn dropped_file_must_be_one_pdf() {
        assert!(matches!(parse_drop_paths("C:/book.pdf"), Some(Ok(_))));
        assert!(matches!(
            parse_drop_paths("C:/one.pdf\nC:/two.pdf"),
            Some(Err(_))
        ));
        assert!(matches!(parse_drop_paths("C:/note.txt"), Some(Err(_))));
    }

    #[test]
    fn theme_indices_map_to_all_supported_modes() {
        assert_eq!(theme_from_index(0), ThemeMode::System);
        assert_eq!(theme_from_index(1), ThemeMode::Light);
        assert_eq!(theme_from_index(2), ThemeMode::Dark);
    }

    #[test]
    fn outline_flattens_nested_targets_and_respects_expansion() {
        let outline = vec![OutlineNode {
            title: "Chapter".into(),
            page_index: None,
            children: vec![OutlineNode {
                title: "Section".into(),
                page_index: Some(3),
                children: Vec::new(),
            }],
        }];
        let mut expanded = HashSet::new();
        expanded.insert(vec![0]);
        let mut items = Vec::new();
        let mut flat = Vec::new();
        flatten_outline(
            &outline,
            &expanded,
            &mut Vec::new(),
            0,
            &mut items,
            &mut flat,
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].depth, 1);
        assert_eq!(flat[1].page_index, Some(3));
    }

    #[test]
    fn active_scroll_does_not_allow_anchor_restore() {
        let now = Instant::now();
        assert!(user_is_scrolling(
            Some(now - Duration::from_millis(50)),
            now
        ));
        assert!(!user_is_scrolling(
            Some(now - Duration::from_millis(250)),
            now
        ));
        assert!(!user_is_scrolling(None, now));
    }

    #[test]
    fn presentation_render_is_bounded_and_keeps_aspect_ratio() {
        let (width, height) = presentation_render_size((612.0, 792.0), (3840, 2160), 2.0);
        assert!(width <= PRESENTATION_MAX_RENDER_EDGE);
        assert!(height <= PRESENTATION_MAX_RENDER_EDGE);
        assert!((width as f32 / height as f32 - 612.0 / 792.0).abs() < 0.01);
    }
}
