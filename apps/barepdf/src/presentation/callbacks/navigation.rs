use crate::application::DocumentController;
use crate::diagnostics::{self, DiagnosticEvent};
use barepdf_core::{
    selection::SelectionEngine, PageIndex, TextPosition, TextSelection, ViewingMode, WindowMode,
    ZoomFactor, ZoomMode,
};
use barepdf_platform_windows::{open_url, WindowsClipboard};
use barepdf_render::{RenderCommand, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, SharedString};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::models::refresh_page_model;
use super::super::state::AppState;
use super::super::ui::{
    invalidate_layout_and_render, navigate_to_page, navigate_to_page_inner, pointer_to_pdf,
    render_visible_pages, request_visible_thumbnails, save_zoom_preference, send_render_command,
    sync_effective_zoom, update_zoom_ui, validated_page_input, view_mode_index, view_mode_label,
    zoom_mode_index, zoom_percentage,
};

pub(super) fn connect_navigation_callbacks(
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
                            NavigationTarget::Previous => {
                                if app.viewing_mode == ViewingMode::TwoPageSpread {
                                    app.current_page.saturating_sub(2)
                                } else if app.viewing_mode == ViewingMode::BookMode {
                                    if app.current_page <= 2 {
                                        0
                                    } else {
                                        app.current_page.saturating_sub(2)
                                    }
                                } else {
                                    app.current_page.saturating_sub(1)
                                }
                            }
                            NavigationTarget::Next => {
                                if app.viewing_mode == ViewingMode::TwoPageSpread {
                                    (app.current_page + 2).min(app.page_count().saturating_sub(1))
                                } else if app.viewing_mode == ViewingMode::BookMode {
                                    if app.current_page == 0 {
                                        1.min(app.page_count().saturating_sub(1))
                                    } else {
                                        (app.current_page + 2)
                                            .min(app.page_count().saturating_sub(1))
                                    }
                                } else {
                                    (app.current_page + 1).min(app.page_count().saturating_sub(1))
                                }
                            }
                            NavigationTarget::First => 0,
                            NavigationTarget::Last => app.page_count().saturating_sub(1),
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
        let count = state_entry.borrow().page_count();
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

pub(super) fn connect_zoom_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let weak = window.as_weak();
    let state_in = state.clone();
    let scheduler_in = scheduler.clone();
    window.on_request_zoom_in(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_in.borrow_mut();
            sync_effective_zoom(&mut app);
            let new_zoom = app.zoom_factor.zoom_in();
            app.zoom_factor = new_zoom;
            app.zoom_mode = ZoomMode::Custom(new_zoom);
            app.update_cache_budget_for_zoom(new_zoom);
            save_zoom_preference(&mut app);
            invalidate_layout_and_render(&mut app, &scheduler_in, &window, false);
            update_zoom_ui(&window, app.zoom_mode, app.zoom_factor);
        }
    });

    let weak = window.as_weak();
    let state_out = state.clone();
    let scheduler_out = scheduler.clone();
    window.on_request_zoom_out(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_out.borrow_mut();
            sync_effective_zoom(&mut app);
            let new_zoom = app.zoom_factor.zoom_out();
            app.zoom_factor = new_zoom;
            app.zoom_mode = ZoomMode::Custom(new_zoom);
            app.update_cache_budget_for_zoom(new_zoom);
            save_zoom_preference(&mut app);
            invalidate_layout_and_render(&mut app, &scheduler_out, &window, false);
            update_zoom_ui(&window, app.zoom_mode, app.zoom_factor);
        }
    });

    let weak = window.as_weak();
    let state_set = state.clone();
    let scheduler_set = scheduler.clone();
    window.on_request_set_zoom(move |input| {
        let Some(window) = weak.upgrade() else {
            return SharedString::default();
        };
        let mut app = state_set.borrow_mut();
        sync_effective_zoom(&mut app);
        let current = zoom_percentage(app.zoom_factor);
        let Some(percent) = parse_zoom_percent(input.as_str()) else {
            return SharedString::from(current);
        };
        let new_zoom = ZoomFactor::new(percent as f32 / 100.0);
        app.zoom_factor = new_zoom;
        app.zoom_mode = ZoomMode::Custom(new_zoom);
        app.update_cache_budget_for_zoom(new_zoom);
        save_zoom_preference(&mut app);
        invalidate_layout_and_render(&mut app, &scheduler_set, &window, false);
        update_zoom_ui(&window, app.zoom_mode, app.zoom_factor);
        SharedString::from(zoom_percentage(app.zoom_factor))
    });

    connect_zoom_mode(window, state, scheduler, ZoomMode::FitWidth, |w, cb| {
        w.on_request_fit_width(cb)
    });
    connect_zoom_mode(window, state, scheduler, ZoomMode::FitPage, |w, cb| {
        w.on_request_fit_page(cb)
    });
    connect_zoom_mode(window, state, scheduler, ZoomMode::ActualSize, |w, cb| {
        w.on_request_actual_size(cb)
    });
}

fn connect_zoom_mode<F>(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    mode: ZoomMode,
    register: F,
) where
    F: FnOnce(&AppWindow, Box<dyn Fn()>),
{
    let weak = window.as_weak();
    let state = state.clone();
    let scheduler = scheduler.clone();
    register(
        window,
        Box::new(move || {
            if let Some(window) = weak.upgrade() {
                let mut app = state.borrow_mut();
                app.zoom_mode = mode;
                save_zoom_preference(&mut app);
                invalidate_layout_and_render(&mut app, &scheduler, &window, false);
                window.set_zoom_mode(zoom_mode_index(app.zoom_mode));
            }
        }),
    );
}

pub(super) fn parse_zoom_percent(input: &str) -> Option<i32> {
    let input = input.trim();
    let value = input
        .strip_suffix('%')
        .map_or(input, |without_percent| without_percent.trim());
    value
        .parse::<i32>()
        .ok()
        .map(|percent| percent.clamp(25, 800))
}

pub(super) fn connect_view_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let weak = window.as_weak();
    let state_view = state.clone();
    let scheduler_view = scheduler.clone();
    window.on_request_toggle_view_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_view.borrow_mut();
            app.viewing_mode = match app.viewing_mode {
                ViewingMode::ContinuousVertical => ViewingMode::SinglePage,
                ViewingMode::SinglePage => ViewingMode::TwoPageSpread,
                ViewingMode::TwoPageSpread => ViewingMode::BookMode,
                ViewingMode::BookMode => ViewingMode::ContinuousVertical,
            };
            app.preferences.viewing_mode = app.viewing_mode;
            app.layout_key = None;
            window.set_view_mode(view_mode_index(app.viewing_mode));
            window.set_view_mode_label(SharedString::from(view_mode_label(
                app.viewing_mode,
                app.preferences.language.resolve(),
            )));
            navigate_to_page_inner(app.current_page, &mut app, &scheduler_view, &window);
        }
    });

    let weak = window.as_weak();
    let state_rot_cw = state.clone();
    let scheduler_rot_cw = scheduler.clone();
    window.on_rotate_view_cw(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_rot_cw.borrow_mut();
            app.rotation = app.rotation.rotate_cw();
            invalidate_layout_and_render(&mut app, &scheduler_rot_cw, &window, true);
        }
    });

    let weak = window.as_weak();
    let state_rot_ccw = state.clone();
    let scheduler_rot_ccw = scheduler.clone();
    window.on_rotate_view_ccw(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_rot_ccw.borrow_mut();
            app.rotation = app.rotation.rotate_ccw();
            invalidate_layout_and_render(&mut app, &scheduler_rot_ccw, &window, true);
        }
    });

    let weak = window.as_weak();
    let state_sidebar = state.clone();
    let scheduler_sidebar = scheduler.clone();
    window.on_request_toggle_sidebar(move || {
        if let Some(window) = weak.upgrade() {
            let visible = !window.get_sidebar_visible();
            window.set_sidebar_visible(visible);
            let mut app = state_sidebar.borrow_mut();
            app.preferences.sidebar_visible = visible;
            app.layout_key = None;
            if visible {
                request_visible_thumbnails(&mut app, &scheduler_sidebar, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_fullscreen = state.clone();
    window.on_request_toggle_fullscreen(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_fullscreen.borrow_mut();
            if app.window_mode == WindowMode::Presentation {
                app.window_mode = WindowMode::Normal;
                window.set_window_mode(0);
                window.window().set_fullscreen(false);
            } else {
                let new_mode = app.request_toggle_fullscreen();
                let enabled = new_mode == WindowMode::FullScreen;
                window.set_window_mode(if enabled { 1 } else { 0 });
                window.window().set_fullscreen(enabled);
            }
            super::super::window_chrome::sync_window_maximized(&window);
            window.invoke_focus_main();
        }
    });

    let weak = window.as_weak();
    let state_presentation = state.clone();
    let scheduler_presentation = scheduler.clone();
    window.on_request_presentation_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_presentation.borrow_mut();
            if app.request_presentation_mode() {
                window.set_window_mode(2);
                window.window().set_fullscreen(true);
                super::super::window_chrome::sync_window_maximized(&window);
                app.generation = scheduler_presentation.bump_generation();
                render_visible_pages(&mut app, &scheduler_presentation, &window);
            }
            window.invoke_focus_main();
        }
    });

    let weak = window.as_weak();
    let state_exit = state.clone();
    window.on_request_exit_special_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_exit.borrow_mut();
            if app.request_exit_special_mode() {
                window.set_window_mode(0);
                window.window().set_fullscreen(false);
                super::super::window_chrome::sync_window_maximized(&window);
            }
            window.invoke_focus_main();
        }
    });
}

pub(super) fn connect_selection_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    clipboard: Arc<WindowsClipboard>,
) {
    let state_copy = state.clone();
    window.on_request_copy(move || {
        let app = state_copy.borrow();
        if let (Some(selection), Some(document)) = (app.selection, app.active_document()) {
            let geometries = app.text_geometries.in_page_order(document);
            let geom_refs: Vec<&barepdf_core::types::PageTextGeometry> =
                geometries.iter().map(AsRef::as_ref).collect();
            let text = SelectionEngine::get_selected_text_in_page_order(&selection, &geom_refs);
            if !text.is_empty() {
                if let Err(error) = clipboard.set_text(&text) {
                    diagnostics::warn_redacted(DiagnosticEvent::ClipboardWrite, &error);
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_all = state.clone();
    window.on_request_select_all(move || {
        let mut app = state_all.borrow_mut();
        if app.page_count() == 0 {
            return;
        }
        let Some(document) = app.active_document() else {
            return;
        };
        let last_page = app.page_count() - 1;
        let last_character = app
            .text_geometries
            .get(document, last_page)
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
        if weak.upgrade().is_some_and(|w| w.get_drawing_mode_active()) {
            return;
        }
        let mut app = state_down.borrow_mut();
        let page = page as u32;
        let Some(page_index) = DocumentController::page_index(&app.application, page) else {
            return;
        };
        if let Some(document_id) = app.active_document() {
            if !app.text_geometries.contains_key(document_id, page) {
                let generation = app.generation;
                send_render_command(
                    &mut app,
                    &scheduler_down,
                    RenderCommand::FetchTextGeometry {
                        document_id,
                        generation,
                        page_index,
                    },
                );
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
        let click_count = app.click_count;
        let geometry = app
            .active_document()
            .and_then(|document| app.text_geometries.get(document, page));
        if let Some(geometry) = geometry.as_deref() {
            let character = SelectionEngine::hit_test(geometry, pdf_x, pdf_y);
            app.selection = Some(match click_count {
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
        if weak.upgrade().is_some_and(|w| w.get_drawing_mode_active()) {
            return;
        }
        let mut app = state_move.borrow_mut();
        if !app.is_selecting {
            return;
        }
        let page = page as u32;
        let Some(page_index) = DocumentController::page_index(&app.application, page) else {
            return;
        };
        let Some(document) = app.active_document() else {
            return;
        };
        let (pdf_x, pdf_y) = pointer_to_pdf(&app, page, x, y);
        let character = app
            .text_geometries
            .get(document, page)
            .map(|geometry| SelectionEngine::hit_test(&geometry, pdf_x, pdf_y))
            .unwrap_or(0);
        if let Some(selection) = app.selection.as_mut() {
            selection.focus = TextPosition::new(page_index, character);
        }
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(app.selection.is_some_and(|selection| !selection.is_empty()));
            refresh_page_model(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_up = state.clone();
    let scheduler_up = scheduler.clone();
    window.on_pointer_up(move |page, x, y| {
        if weak.upgrade().is_some_and(|w| w.get_drawing_mode_active()) {
            return;
        }
        let mut app = state_up.borrow_mut();
        app.is_selecting = false;
        let has_non_empty_selection = app.selection.is_some_and(|selection| !selection.is_empty());
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(has_non_empty_selection);
        }

        if !has_non_empty_selection && page >= 0 && app.click_count <= 1 {
            let page_u32 = page as u32;
            let (page_width, page_height) = app
                .page_dimensions
                .get(page_u32 as usize)
                .copied()
                .unwrap_or(app.first_page_dimensions);
            let (display_width, display_height) = app
                .layout
                .pages
                .get(page_u32 as usize)
                .map(|p| (p.width as f32, p.height as f32))
                .unwrap_or((page_width, page_height));
            let norm_x = (x / display_width.max(1.0)).clamp(0.0, 1.0);
            let norm_y = (y / display_height.max(1.0)).clamp(0.0, 1.0);
            let link_target = app
                .active_document()
                .and_then(|doc| app.text_geometries.get(doc, page_u32))
                .and_then(|geom| {
                    barepdf_core::hit_test_link(&geom, page_width, page_height, norm_x, norm_y)
                        .cloned()
                });
            if let Some(target) = link_target {
                match target {
                    barepdf_core::LinkTarget::Page(dest) => {
                        let dest_page = dest.get();
                        drop(app);
                        if let Some(window) = weak.upgrade() {
                            navigate_to_page(dest_page, &state_up, &scheduler_up, &window);
                        }
                    }
                    barepdf_core::LinkTarget::Url(url) => {
                        if is_safe_external_link_url(&url) {
                            let _ = open_url(&url);
                        }
                    }
                }
            }
        }
    });
}

pub(super) fn is_safe_external_link_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}
