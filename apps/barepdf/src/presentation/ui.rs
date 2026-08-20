#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::map_unwrap_or,
    clippy::redundant_closure_for_method_calls,
    clippy::semicolon_if_nothing_returned,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unchecked_time_subtraction,
    unused_must_use
)]

use crate::app_error::AppError;
use crate::application::{
    DocumentController, OpenTab, OpenTransition, PrintController, RenderController,
};
use crate::diagnostics::{self, DiagnosticEvent};
use crate::infrastructure::{
    default_config_path, save_to_file, start_update_worker, try_load_from_file,
    PreferencesLoadError, CURRENT_VERSION,
};

use barepdf_core::{
    ContinuousLayout, DocumentId, MemoryBudget, PageIndex, PdfError, RequestId, ThemeMode,
    UserPreferences, ViewingMode, WindowMode, ZoomFactor, ZoomMode, MAX_DOCUMENT_PAGES,
    MAX_OPEN_TABS, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS,
};
use barepdf_i18n::{Language, ResolvedLanguage};
use barepdf_pdf::{OutlineNode, PdfiumEngine};
use barepdf_platform_windows::{
    ask_yes_no, install_file_drop, show_fatal_error, WindowsClipboard, WindowsFileDialogs,
};
use barepdf_render::{
    Priority, RenderCommand, RenderError, RenderEvent, RenderJob, RenderKind, RenderScheduler,
};
use barepdf_ui::{AppWindow, OutlineItem, RecentFileItem, SelectionBox, ThemeTokens};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{
    ComponentHandle, Image, LogicalSize, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString,
    Timer, VecModel,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) const RAW_BITMAP_BUDGET: usize = 32 * 1024 * 1024;
pub(super) const PAGE_IMAGE_BUDGET: usize = 16 * 1024 * 1024;
pub(super) const THUMB_IMAGE_BUDGET: usize = 4 * 1024 * 1024;
const THUMB_ROW_HEIGHT: f32 = 188.0;
const PAGE_GAP: f32 = 14.0;
pub(super) const SCROLL_IDLE_DELAY: Duration = Duration::from_millis(180);
const PRESENTATION_MAX_RENDER_EDGE: u32 = 2560;
pub(super) const TEXT_GEOMETRY_BUDGET: usize = 8 * 1024 * 1024;

use super::callbacks::{
    clear_document_transients, close_worker_document, restore_active_view, snapshot_active_view,
};
use super::models::{
    refresh_page_model, refresh_tab_model, refresh_thumbnail_model, refresh_thumbnail_row,
    refresh_thumbnail_selection,
};
use super::state::{fit_bitmap_to_budget, AppState, FlatOutlineEntry, LayoutKey};
use super::update_ui::{
    queue_update_check, render_update_ui, startup_update_check_should_run, unix_timestamp,
};

pub(crate) fn run() -> Result<(), AppError> {
    let pdf_engine = match PdfiumEngine::new() {
        Ok(engine) => engine,
        Err(error) => {
            show_fatal_error(
                "BarePDF",
                &format!("PDF engine could not be loaded.\n\n{error}"),
            );
            return Err(error.into());
        }
    };

    let preferences_path = default_config_path();
    let mut preferences = match try_load_from_file(&preferences_path) {
        Ok(preferences) => preferences,
        Err(PreferencesLoadError::Read(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            UserPreferences::default()
        }
        Err(error) => {
            show_fatal_error(
                "BarePDF",
                &format!("Preferences could not be loaded.\n\n{error}"),
            );
            return Err(error.into());
        }
    };
    if preferences.update_checks_enabled.is_none() {
        let language = preferences.language.resolve();
        preferences.update_checks_enabled = Some(ask_yes_no(
            barepdf_i18n::t(language, "updates.consent.title"),
            barepdf_i18n::t(language, "updates.consent.body"),
        ));
        persist_preferences(&preferences, &preferences_path, None);
    }
    preferences.viewing_mode = normalize_viewing_mode(preferences.viewing_mode);

    let scheduler = Rc::new(RenderScheduler::spawn(
        pdf_engine,
        MemoryBudget::new(RAW_BITMAP_BUDGET),
    ));
    let dialogs = Arc::new(WindowsFileDialogs);
    let clipboard = Arc::new(WindowsClipboard::new());
    let (mut update_worker, update_receiver) = start_update_worker();
    let update_sender = update_worker.command_sender();
    let update_check_canceller = update_worker.check_canceller();
    let window = AppWindow::new()?;
    window.window().set_size(LogicalSize::new(
        preferences.last_window_width.max(760) as f32,
        preferences.last_window_height.max(520) as f32,
    ));

    let state = Rc::new(RefCell::new(AppState::new(preferences)));
    let print_controller = match PrintController::spawn() {
        Ok(controller) => Some(Rc::new(RefCell::new(controller))),
        Err(error) => {
            diagnostics::warn_redacted(DiagnosticEvent::PrintWorkerStart, &error);
            None
        }
    };
    initialize_window(&window, &state.borrow());
    apply_theme(&window, state.borrow().preferences.theme);
    update_ui_strings(&window, state.borrow().preferences.language.resolve());
    refresh_recent_files(&mut state.borrow_mut(), &window);

    super::callbacks::wire_callbacks(
        &window,
        &state,
        &scheduler,
        dialogs,
        clipboard,
        &preferences_path,
        (
            update_sender.clone(),
            update_check_canceller.clone(),
            print_controller.clone(),
        ),
    );

    if startup_update_check_should_run(&state.borrow(), unix_timestamp()) {
        queue_update_check(
            &update_sender,
            &update_check_canceller,
            &state,
            &window,
            &preferences_path,
        );
    }

    if let Some(argument) = env::args_os().nth(1) {
        let path = PathBuf::from(argument);
        if path.is_file() {
            begin_open(path, None, &state, &scheduler, &window);
        }
    }

    let timer = Rc::new(Timer::default());
    state.borrow_mut().attach_pump_timer(timer.clone());
    super::event_pump::start(
        &timer,
        &window,
        &state,
        &scheduler,
        &preferences_path,
        None,
        (update_receiver, print_controller.clone()),
    );
    let run_result = window.run();
    if let Some(controller) = print_controller {
        if let Err(error) = controller.borrow_mut().shutdown() {
            diagnostics::warn_redacted(DiagnosticEvent::PrintShutdown, &error);
        }
    }
    if let Err(error) = scheduler.shutdown() {
        diagnostics::warn_redacted(DiagnosticEvent::RenderShutdown, &error);
    }
    if let Err(error) = update_worker.shutdown() {
        diagnostics::warn_redacted(DiagnosticEvent::UpdaterShutdown, &error);
    }
    run_result?;

    let scale = window.window().scale_factor().max(0.1);
    let size = window.window().size();
    let mut app = state.borrow_mut();
    app.preferences.last_window_width = ((size.width as f32) / scale).round() as u32;
    app.preferences.last_window_height = ((size.height as f32) / scale).round() as u32;
    persist_preferences(&app.preferences, &preferences_path, None);
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
    window.set_current_version(SharedString::from(CURRENT_VERSION));
    window.set_update_checks_enabled(state.preferences.update_checks_enabled == Some(true));
    window.set_zoom_mode(zoom_mode_index(state.zoom_mode));
    window.set_zoom_str(SharedString::from(zoom_percentage(state.zoom_factor)));
    render_update_ui(window, state);
}

pub(super) fn install_native_file_drop(
    window: &AppWindow,
) -> Option<std::sync::mpsc::Receiver<Vec<PathBuf>>> {
    let window_handle = window.window().window_handle();
    install_file_drop(window_handle.window_handle().ok()?)
}

pub(super) fn native_window_handle(window: &AppWindow) -> Option<isize> {
    let window_handle = window.window().window_handle();
    let handle = window_handle.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

pub(super) fn process_view_changes(
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
        && app.page_count() > 0
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
            request_visible_thumbnails(&mut app, scheduler, window);
        } else if previous_page != page {
            if let Some(document_id) = app.active_document() {
                if !app.text_geometries.contains_key(document_id, page) {
                    let generation = app.generation;
                    send_render_command(
                        &mut app,
                        scheduler,
                        RenderCommand::FetchTextGeometry {
                            document_id,
                            generation,
                            page_index: PageIndex::from_raw(page),
                        },
                    );
                }
            }
        }
        refresh_thumbnail_selection(&mut app, window, previous_page);
    }

    if (thumbnail_scroll - app.last_thumbnail_scroll_y).abs() > THUMB_ROW_HEIGHT * 0.5 {
        app.last_thumbnail_scroll_y = thumbnail_scroll;
        request_visible_thumbnails(&mut app, scheduler, window);
    }
}

pub(super) fn handle_render_event(
    event: RenderEvent,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &RenderScheduler,
    _preferences_path: &Path,
) {
    match event {
        RenderEvent::DocumentOpened {
            document_id,
            page_count,
            first_page_dimensions,
        } => {
            let mut app = state.borrow_mut();
            let ready = match DocumentController::opened(
                &mut app.application,
                document_id,
                page_count,
                MAX_DOCUMENT_PAGES,
            ) {
                OpenTransition::Ready(ready) => ready,
                OpenTransition::InvalidPageCount => {
                    show_banner(
                        window,
                        format!("PDF page count must be between 1 and {MAX_DOCUMENT_PAGES}."),
                        true,
                    );
                    return;
                }
                OpenTransition::Stale => return,
            };
            let page_count = ready.page_count().get();
            let path = ready.path().to_path_buf();
            let restored_view = app
                .application
                .tabs
                .active()
                .map(|tab| tab.view.clone())
                .unwrap_or_default();
            app.current_page = restored_view
                .current_page
                .get()
                .min(page_count.saturating_sub(1));
            app.first_page_dimensions = first_page_dimensions;
            app.page_dimensions = vec![first_page_dimensions; page_count as usize];
            app.dimensions_revision += 1;
            app.next_dimensions_start = 1;
            app.dimensions_request_pending = false;
            app.layout_key = None;
            app.visible_page_indices.clear();
            app.first_page_ready = false;
            app.profile_recorded = false;
            app.open_started_at = Some(ready.started_at());
            app.outline.clear();
            app.outline_requested = false;
            app.expanded_outline.clear();
            app.flat_outline.clear();
            app.selection = None;
            app.last_scroll_y = 0.0;
            app.last_thumbnail_scroll_y = 0.0;
            app.last_user_scroll_at = None;
            app.preferences
                .add_recent_file(path.to_string_lossy().into_owned());

            window.set_has_document(true);
            window.set_password_required(false);
            window.set_password_error(SharedString::default());
            window.set_banner_visible(false);
            window.set_has_selection(false);
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("document.pdf");
            window.set_document_title(SharedString::from(file_name));
            window.set_total_pages_str(SharedString::from(page_count.to_string()));
            window.set_status_text(SharedString::from(opened_document_status(
                app.preferences.language.resolve(),
                file_name,
                page_count,
            )));
            refresh_recent_files(&mut app, window);
            refresh_tab_model(&app, window);
            refresh_thumbnail_model(&mut app, window);
            navigate_to_page_inner(app.current_page, &mut app, scheduler, window);
            set_scroll_position(&mut app, window, restored_view.scroll_y);
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
            if !RenderController::accepts(
                &app.application,
                document_id,
                Some(generation),
                app.generation,
            ) {
                return;
            }
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                bitmap.pixels(),
                bitmap.width(),
                bitmap.height(),
            );
            let image = Image::from_rgba8(buffer);
            match kind {
                RenderKind::Page => {
                    app.page_images.insert(
                        document_id,
                        page_index.get(),
                        RenderKind::Page,
                        image.clone(),
                        bitmap.pixels().len(),
                    );
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
                    app.thumbnail_images.insert(
                        document_id,
                        page_index.get(),
                        RenderKind::Thumbnail,
                        image,
                        bitmap.pixels().len(),
                    );
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
            if RenderController::accepts(
                &app.application,
                document_id,
                Some(generation),
                app.generation,
            ) {
                app.text_geometries
                    .insert(document_id, page_index.get(), geometry);
                refresh_page_model(&mut app, window);
            }
        }
        RenderEvent::OutlineFetched {
            document_id,
            outline,
        } => {
            let mut app = state.borrow_mut();
            if RenderController::accepts(&app.application, document_id, None, app.generation) {
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
            if !RenderController::accepts(&app.application, document_id, None, app.generation) {
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
            if DocumentController::is_pending(&app.application, document_id) {
                match error {
                    RenderError::Pdf(PdfError::PasswordRequired) => {
                        let _ =
                            DocumentController::require_password(&mut app.application, document_id);
                        let name = DocumentController::pending_path(&app.application)
                            .and_then(Path::file_name)
                            .and_then(|name| name.to_str())
                            .unwrap_or("document.pdf");
                        window.set_protected_file_name(SharedString::from(name));
                        window.set_password_error(SharedString::default());
                        window.set_password_required(true);
                    }
                    RenderError::Pdf(PdfError::IncorrectPassword) => {
                        window.set_password_error(SharedString::from("Incorrect password."));
                        window.set_password_required(true);
                    }
                    other => {
                        let _ = DocumentController::fail(&mut app.application, document_id);
                        show_banner(window, format!("Could not open PDF: {other}"), true);
                    }
                }
            } else if RenderController::accepts(
                &app.application,
                document_id,
                generation,
                app.generation,
            ) {
                show_banner(window, format!("PDF rendering failed: {error}"), true);
            }
            refresh_tab_model(&app, window);
        }
        RenderEvent::TextExtracted { .. } => {}
    }
}

pub(super) fn begin_open(
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
    let path = std::fs::canonicalize(&path).unwrap_or(path);
    let title = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document.pdf")
        .to_string();
    window.set_password_required(false);
    window.set_password_error(SharedString::default());
    let document_id = {
        let mut app = state.borrow_mut();
        snapshot_active_view(&mut app, window);
        let previous_tab = app.application.tabs.active_id();
        let previous_document = app.active_document();
        if matches!(
            app.application.tabs.open(path.clone(), title),
            OpenTab::Full
        ) {
            show_banner(
                window,
                format!("A maximum of {MAX_OPEN_TABS} tabs can be open."),
                false,
            );
            return;
        }
        if app.application.tabs.active_id() != previous_tab {
            app.generation = scheduler.bump_generation();
            close_worker_document(&mut app, scheduler, previous_document);
            clear_document_transients(&mut app, window);
        }
        restore_active_view(&mut app, window);
        let Some(tab_id) = app.application.tabs.active_id() else {
            return;
        };
        DocumentId::new(tab_id.get())
    };
    let mut app = state.borrow_mut();
    DocumentController::begin_open(
        &mut app.application,
        document_id,
        path.clone(),
        Instant::now(),
    );
    window.set_status_text(SharedString::from("Opening document…"));
    window.set_password_error(SharedString::default());
    if !send_render_command(
        &mut app,
        scheduler,
        RenderCommand::OpenDocument {
            document_id,
            path,
            password,
        },
    ) {
        DocumentController::cancel_open(&mut app.application, document_id);
        show_banner(window, "PDF work queue is unavailable. Try again.", true);
    }
    refresh_tab_model(&app, window);
}

pub(super) fn send_render_command(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    command: RenderCommand,
) -> bool {
    let sent = scheduler.send_command(command);
    if sent {
        app.wake_pump();
    }
    sent
}

pub(super) fn navigate_to_page(
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

pub(super) fn navigate_to_page_inner(
    page: u32,
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    if app.page_count() == 0 {
        return;
    }
    let previous_page = app.current_page;
    let Some(page) = DocumentController::page_index(&app.application, page) else {
        return;
    };
    app.current_page = page.get();
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

pub(super) fn invalidate_layout_and_render(
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
        if let Some(document) = app.active_document() {
            app.page_images.remove_document(document);
        }
    }
    app.generation = scheduler.bump_generation();
    render_visible_pages(app, scheduler, window);
}

pub(super) fn ensure_layout(app: &mut AppState) {
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

pub(super) fn visible_page_indices(app: &AppState, window: &AppWindow) -> Vec<u32> {
    if app.page_count() == 0 {
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
    (first.saturating_sub(1)..=(last + 1).min(app.page_count().saturating_sub(1))).collect()
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

pub(super) fn render_visible_pages(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    let Some(document_id) = app.active_document() else {
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
        let (render_width, render_height) = fit_bitmap_to_budget(
            render_width.clamp(1, 4096),
            render_height.clamp(1, 4096),
            PAGE_IMAGE_BUDGET,
        );
        send_render_command(
            app,
            scheduler,
            RenderCommand::RenderPage(RenderJob {
                request_id: RequestId::new(unique_id()),
                generation: app.generation,
                document_id,
                page_index: PageIndex::from_raw(page),
                target_width: render_width,
                target_height: render_height,
                rotation: app.rotation,
                priority: if page == app.current_page {
                    Priority::Visible
                } else {
                    Priority::Prefetch
                },
                kind: RenderKind::Page,
            }),
        );
    }

    if app.first_page_ready
        && !app
            .text_geometries
            .contains_key(document_id, app.current_page)
    {
        send_render_command(
            app,
            scheduler,
            RenderCommand::FetchTextGeometry {
                document_id,
                generation: app.generation,
                page_index: PageIndex::from_raw(app.current_page),
            },
        );
    }
    refresh_page_model(app, window);
}

pub(super) fn request_visible_thumbnails(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    if !app.first_page_ready
        || !window.get_sidebar_visible()
        || window.get_sidebar_tab() != 0
        || app.page_count() == 0
    {
        return;
    }
    let Some(document_id) = app.active_document() else {
        return;
    };
    let first_visible = ((-window.get_thumbnail_scroll_y()).max(0.0) / THUMB_ROW_HEIGHT) as u32;
    let visible_count =
        (window.get_thumbnail_viewport_height().max(1.0) / THUMB_ROW_HEIGHT).ceil() as u32;
    let start = first_visible.saturating_sub(4);
    let end = (first_visible + visible_count + 4).min(app.page_count());
    for index in start..end {
        let (width, height) = app
            .page_dimensions
            .get(index as usize)
            .copied()
            .unwrap_or(app.first_page_dimensions);
        let target_width = (140.0 * app.scale_factor).round() as u32;
        let target_height = (target_width as f32 * height / width.max(1.0)).round() as u32;
        send_render_command(
            app,
            scheduler,
            RenderCommand::RenderPage(RenderJob {
                request_id: RequestId::new(unique_id()),
                generation: app.generation,
                document_id,
                page_index: PageIndex::from_raw(index),
                target_width: target_width.clamp(1, 512),
                target_height: target_height.clamp(1, 768),
                rotation: app.rotation,
                priority: Priority::Thumbnail,
                kind: RenderKind::Thumbnail,
            }),
        );
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
        if let Some(document_id) = app.active_document() {
            app.outline_requested =
                send_render_command(app, scheduler, RenderCommand::FetchOutline { document_id });
        }
    }
    if let Some(document_id) = app.active_document() {
        send_render_command(
            app,
            scheduler,
            RenderCommand::FetchTextGeometry {
                document_id,
                generation: app.generation,
                page_index: PageIndex::from_raw(app.current_page),
            },
        );
    }
}

fn request_next_dimensions_batch(app: &mut AppState, scheduler: &RenderScheduler) {
    if app.dimensions_request_pending || app.next_dimensions_start >= app.page_count() {
        return;
    }
    if let Some(document_id) = app.active_document() {
        app.dimensions_request_pending = send_render_command(
            app,
            scheduler,
            RenderCommand::FetchPageDimensions {
                document_id,
                start: app.next_dimensions_start,
                count: 32,
            },
        );
    }
}

pub(super) fn refresh_outline_model(app: &mut AppState, window: &AppWindow) {
    let mut items = Vec::new();
    let mut flat = Vec::new();
    flatten_outline(&app.outline, &app.expanded_outline, &mut items, &mut flat);
    app.flat_outline = flat;
    window.set_outline_items(ModelRc::new(VecModel::from(items)));
}

fn flatten_outline(
    nodes: &[OutlineNode],
    expanded: &HashSet<Vec<usize>>,
    items: &mut Vec<OutlineItem>,
    flat: &mut Vec<FlatOutlineEntry>,
) {
    struct Frame<'a> {
        nodes: &'a [OutlineNode],
        next: usize,
        path: Vec<usize>,
    }

    let mut stack = vec![Frame {
        nodes,
        next: 0,
        path: Vec::new(),
    }];
    while let Some(frame) = stack.last_mut() {
        if items.len() >= MAX_OUTLINE_ITEMS {
            tracing::warn!(limit = MAX_OUTLINE_ITEMS, "outline truncated at item limit");
            break;
        }
        if frame.next == frame.nodes.len() {
            stack.pop();
            continue;
        }

        let index = frame.next;
        frame.next += 1;
        let node = &frame.nodes[index];
        let mut path = frame.path.clone();
        path.push(index);
        let has_children = !node.children.is_empty();
        let is_expanded = has_children && expanded.contains(&path);
        items.push(OutlineItem {
            title: SharedString::from(if node.title.is_empty() {
                "Untitled"
            } else {
                &node.title
            }),
            page_index: node.page_index.map(|page| page as i32).unwrap_or(-1),
            depth: i32::try_from(path.len().saturating_sub(1)).unwrap_or(i32::MAX),
            has_children,
            expanded: is_expanded,
        });
        flat.push(FlatOutlineEntry {
            path: path.clone(),
            page_index: node.page_index,
            has_children,
        });
        if is_expanded && path.len() < MAX_OUTLINE_DEPTH {
            stack.push(Frame {
                nodes: &node.children,
                next: 0,
                path,
            });
        } else if is_expanded && has_children {
            tracing::warn!(
                limit = MAX_OUTLINE_DEPTH,
                "outline truncated at depth limit"
            );
        }
    }
}

fn refresh_recent_files(app: &mut AppState, window: &AppWindow) {
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

pub(super) fn pointer_to_pdf(app: &AppState, page: u32, x: f32, y: f32) -> (f32, f32) {
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

pub(super) fn compute_selection_boxes(
    app: &mut AppState,
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
    let Some(document) = app.active_document() else {
        return Vec::new();
    };
    let Some(geometry) = app.text_geometries.get(document, page) else {
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

pub(super) fn sync_effective_zoom(app: &mut AppState) {
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

pub(super) fn zoom_mode_index(mode: ZoomMode) -> i32 {
    match mode {
        ZoomMode::FitWidth => 1,
        ZoomMode::FitPage => 2,
        ZoomMode::ActualSize | ZoomMode::Custom(_) => 0,
    }
}

pub(super) fn zoom_percentage(factor: ZoomFactor) -> String {
    format!("{}%", (factor.get() * 100.0).round())
}

pub(super) fn update_zoom_ui(window: &AppWindow, mode: ZoomMode, factor: ZoomFactor) {
    window.set_zoom_mode(zoom_mode_index(mode));
    window.set_zoom_str(SharedString::from(zoom_percentage(factor)));
}

pub(super) fn save_zoom_preference(app: &mut AppState) {
    app.preferences.zoom_mode = app.zoom_mode;
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

pub(super) fn apply_theme(window: &AppWindow, theme: ThemeMode) {
    window.set_current_theme(theme_index(theme));
    window
        .global::<ThemeTokens>()
        .set_theme_mode(theme_index(theme));
}

pub(super) fn show_banner(window: &AppWindow, message: impl Into<SharedString>, can_retry: bool) {
    window.set_banner_text(message.into());
    window.set_banner_can_retry(can_retry);
    window.set_banner_update_action(false);
    window.set_banner_action_label(SharedString::default());
    window.set_banner_action_enabled(false);
    window.set_banner_visible(true);
}

pub(super) fn persist_preferences(
    preferences: &UserPreferences,
    preferences_path: &Path,
    window: Option<&AppWindow>,
) {
    if let Err(error) = save_to_file(preferences, preferences_path) {
        diagnostics::warn_redacted(DiagnosticEvent::PreferencesSave, &error);
        if let Some(window) = window {
            show_banner(window, "Preferences could not be saved.", false);
        }
    }
}

pub(super) fn parse_drop_paths(text: &str) -> Option<Result<PathBuf, &'static str>> {
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
    let path = paths.into_iter().next()?;
    if !is_pdf_path(&path) {
        return Some(Err("Only PDF files are supported."));
    }
    Some(Ok(path))
}

pub(super) fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

pub(super) fn validated_page_input(input: &str, page_count: u32) -> Option<u32> {
    input
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|page| (1..=page_count).contains(page))
        .map(|page| page - 1)
}

pub(super) fn normalize_viewing_mode(mode: ViewingMode) -> ViewingMode {
    match mode {
        ViewingMode::SinglePage => ViewingMode::SinglePage,
        _ => ViewingMode::ContinuousVertical,
    }
}

pub(super) fn theme_from_index(index: i32) -> ThemeMode {
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

pub(super) fn view_mode_index(mode: ViewingMode) -> i32 {
    i32::from(mode == ViewingMode::SinglePage)
}

pub(super) fn view_mode_label(mode: ViewingMode, language: ResolvedLanguage) -> &'static str {
    barepdf_i18n::t(
        language,
        if mode == ViewingMode::SinglePage {
            "view.mode.single"
        } else {
            "view.mode.continuous"
        },
    )
}

fn opened_document_status(language: ResolvedLanguage, name: &str, pages: u32) -> String {
    barepdf_i18n::t(language, "status.opened")
        .replace("{name}", name)
        .replace("{pages}", &pages.to_string())
}

pub(super) fn update_ui_strings(window: &AppWindow, language: ResolvedLanguage) {
    macro_rules! set_text {
        ($setter:ident, $key:literal) => {
            window.$setter(SharedString::from(barepdf_i18n::t(language, $key)))
        };
    }
    set_text!(set_text_open, "open.file");
    set_text!(set_text_sidebar, "sidebar.toggle");
    set_text!(set_text_thumbnails, "sidebar.thumbnails");
    set_text!(set_text_outline, "sidebar.outline");
    set_text!(set_text_new_tab, "tab.new");
    set_text!(set_text_view, "view.mode");
    set_text!(set_text_zoom_in, "zoom.in");
    set_text!(set_text_zoom_out, "zoom.out");
    set_text!(set_text_zoom, "zoom.label");
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
    set_text!(set_text_updates, "updates");
    set_text!(set_text_update_enabled, "updates.enabled");
    set_text!(set_text_update_disabled, "updates.disabled");
    set_text!(set_text_check_now, "updates.check_now");
    set_text!(set_text_print, "print.action");
    set_text!(set_text_cancel_print, "print.cancel");
    set_text!(set_text_prev_page, "page.previous");
    set_text!(set_text_next_page, "page.next");
    set_text!(set_text_password_title, "password.title");
    set_text!(set_text_password_placeholder, "password.placeholder");
    set_text!(set_text_password_cancel, "password.cancel");
    set_text!(set_text_password_unlock, "password.unlock");
    set_text!(set_text_settings_language, "settings.language");
    set_text!(set_text_settings_theme, "settings.theme");
    set_text!(set_text_settings_system, "settings.theme.system");
    set_text!(set_text_settings_english, "language.english");
    set_text!(set_text_settings_turkish, "language.turkish");
    set_text!(set_text_settings_light, "settings.theme.light");
    set_text!(set_text_settings_dark, "settings.theme.dark");
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
    fn opened_document_status_uses_the_selected_language_template() {
        assert_eq!(
            opened_document_status(ResolvedLanguage::Turkish, "örnek.pdf", 2),
            "örnek.pdf (2 sayfa)"
        );
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
    fn zoom_mode_indices_distinguish_fit_modes() {
        assert_eq!(zoom_mode_index(ZoomMode::Custom(ZoomFactor::default())), 0);
        assert_eq!(zoom_mode_index(ZoomMode::ActualSize), 0);
        assert_eq!(zoom_mode_index(ZoomMode::FitWidth), 1);
        assert_eq!(zoom_mode_index(ZoomMode::FitPage), 2);
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
        flatten_outline(&outline, &expanded, &mut items, &mut flat);
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].depth, 1);
        assert_eq!(flat[1].page_index, Some(3));
    }

    #[test]
    fn outline_flattening_has_a_fixed_item_limit() {
        let outline = (0..=MAX_OUTLINE_ITEMS)
            .map(|index| OutlineNode {
                title: index.to_string(),
                page_index: None,
                children: Vec::new(),
            })
            .collect::<Vec<_>>();
        let mut items = Vec::new();
        let mut flat = Vec::new();

        flatten_outline(&outline, &HashSet::new(), &mut items, &mut flat);

        assert_eq!(items.len(), MAX_OUTLINE_ITEMS);
        assert_eq!(flat.len(), MAX_OUTLINE_ITEMS);
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

    #[test]
    fn page_render_size_fits_image_cache_budget() {
        let (width, height) = fit_bitmap_to_budget(4096, 4096, PAGE_IMAGE_BUDGET);

        assert!(
            usize::try_from(width)
                .unwrap()
                .saturating_mul(usize::try_from(height).unwrap())
                .saturating_mul(std::mem::size_of::<Rgba8Pixel>())
                <= PAGE_IMAGE_BUDGET
        );
    }
}
