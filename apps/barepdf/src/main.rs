#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use barepdf_core::{
    compute_target_dimensions, default_config_path, selection::SelectionEngine, ContinuousLayout,
    DocumentId, MemoryBudget, PageCount, PageIndex, PageTextGeometry, PdfError, RequestId,
    Rotation, TextPosition, TextSelection, UserPreferences, ViewingMode, WindowMode, ZoomFactor,
    ZoomMode,
};
use barepdf_i18n::{Language, ResolvedLanguage};
use barepdf_pdf::PdfiumEngine;
use barepdf_platform::{ClipboardAccess, FileDialogs};
use barepdf_platform_windows::{WindowsClipboard, WindowsFileDialogs};
use barepdf_render::{Priority, RenderCommand, RenderEvent, RenderJob, RenderScheduler};
use barepdf_ui::{AppWindow, PageItem, SelectionBox, ThumbnailItem};
use lru::LruCache;
use slint::{
    ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode,
    VecModel,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

struct AppState {
    current_doc_id: Option<DocumentId>,
    current_path: Option<PathBuf>,
    page_count: u32,
    current_page: u32,
    viewing_mode: ViewingMode,
    zoom_mode: ZoomMode,
    zoom_factor: ZoomFactor,
    rotation: Rotation,
    first_page_dims: (f32, f32),
    all_page_dims: Vec<(f32, f32)>,
    window_mode: WindowMode,
    preferences: UserPreferences,
    text_geometries: HashMap<u32, PageTextGeometry>,
    selection: Option<TextSelection>,
    is_selecting: bool,
    last_click_time: std::time::Instant,
    click_count: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting BarePDF application...");

    let pdf_engine = match PdfiumEngine::new() {
        Ok(engine) => engine,
        Err(err) => {
            tracing::warn!("Failed to initialize PDFium engine: {}", err);
            PdfiumEngine::new()?
        }
    };

    let prefs_path = default_config_path();
    let preferences = UserPreferences::load_from_file(&prefs_path);
    let scheduler = Rc::new(RenderScheduler::spawn(
        pdf_engine,
        MemoryBudget::new(preferences.memory_budget_bytes),
    ));
    let dialogs = Arc::new(WindowsFileDialogs);
    let clipboard = Arc::new(WindowsClipboard::new());

    let main_window = AppWindow::new()?;
    main_window.set_sidebar_visible(preferences.sidebar_visible);
    main_window.set_view_mode_label(SharedString::from(match preferences.viewing_mode {
        ViewingMode::ContinuousVertical => "Continuous",
        ViewingMode::SinglePage => "Single Page",
        _ => "Continuous",
    }));

    let state = Rc::new(RefCell::new(AppState {
        current_doc_id: None,
        current_path: None,
        page_count: 0,
        current_page: 0,
        viewing_mode: preferences.viewing_mode,
        zoom_mode: preferences.zoom_mode,
        zoom_factor: ZoomFactor::default(),
        rotation: Rotation::Degrees0,
        first_page_dims: (612.0, 792.0),
        all_page_dims: Vec::new(),
        window_mode: WindowMode::Normal,
        preferences: preferences.clone(),
        text_geometries: HashMap::new(),
        selection: None,
        is_selecting: false,
        last_click_time: std::time::Instant::now(),
        click_count: 0,
    }));

    update_ui_strings(&main_window, state.borrow().preferences.language.resolve());

    // Check CLI argument for PDF path
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let initial_path = PathBuf::from(&args[1]);
        if initial_path.exists() {
            open_pdf_document(initial_path, None, &state, &scheduler, &main_window);
        }
    }

    // Connect Slint Callbacks
    let state_open = state.clone();
    let scheduler_open = scheduler.clone();
    let dialogs_open = dialogs.clone();
    let window_open = main_window.as_weak();
    main_window.on_request_open_file(move || {
        if let Some(path) = dialogs_open.pick_file() {
            if let Some(win) = window_open.upgrade() {
                open_pdf_document(path, None, &state_open, &scheduler_open, &win);
            }
        }
    });

    let state_next = state.clone();
    let scheduler_next = scheduler.clone();
    let window_next = main_window.as_weak();
    main_window.on_request_next_page(move || {
        let mut s = state_next.borrow_mut();
        if s.current_page + 1 < s.page_count {
            s.current_page += 1;
            if let Some(win) = window_next.upgrade() {
                update_page_view(&s, &scheduler_next, &win);
                request_thumbnails(&s, &scheduler_next);
            }
        }
    });

    let state_prev = state.clone();
    let scheduler_prev = scheduler.clone();
    let window_prev = main_window.as_weak();
    main_window.on_request_prev_page(move || {
        let mut s = state_prev.borrow_mut();
        if s.current_page > 0 {
            s.current_page -= 1;
            if let Some(win) = window_prev.upgrade() {
                update_page_view(&s, &scheduler_prev, &win);
                request_thumbnails(&s, &scheduler_prev);
            }
        }
    });

    let state_sel = state.clone();
    let scheduler_sel = scheduler.clone();
    let window_sel = main_window.as_weak();
    main_window.on_request_select_page(move |page_idx| {
        let mut s = state_sel.borrow_mut();
        if page_idx >= 0 && (page_idx as u32) < s.page_count {
            s.current_page = page_idx as u32;
            if let Some(win) = window_sel.upgrade() {
                let layout =
                    ContinuousLayout::compute(&s.all_page_dims, 1100, 800, s.zoom_mode, 1.0, 12.0);
                if let Some(box_info) = layout.pages.get(page_idx as usize) {
                    win.set_current_scroll_y(-box_info.y_offset);
                }
                update_page_view(&s, &scheduler_sel, &win);
                request_thumbnails(&s, &scheduler_sel);
            }
        }
    });

    let state_vm = state.clone();
    let scheduler_vm = scheduler.clone();
    let window_vm = main_window.as_weak();
    let prefs_path_vm = prefs_path.clone();
    main_window.on_request_toggle_view_mode(move || {
        let mut s = state_vm.borrow_mut();
        s.viewing_mode = match s.viewing_mode {
            ViewingMode::ContinuousVertical => ViewingMode::SinglePage,
            _ => ViewingMode::ContinuousVertical,
        };
        s.preferences.viewing_mode = s.viewing_mode;
        let _ = s.preferences.save_to_file(&prefs_path_vm);

        if let Some(win) = window_vm.upgrade() {
            win.set_view_mode_label(SharedString::from(match s.viewing_mode {
                ViewingMode::ContinuousVertical => "Continuous",
                ViewingMode::SinglePage => "Single Page",
                _ => "Continuous",
            }));
            update_page_view(&s, &scheduler_vm, &win);
            request_thumbnails(&s, &scheduler_vm);
        }
    });

    let state_zi = state.clone();
    let scheduler_zi = scheduler.clone();
    let window_zi = main_window.as_weak();
    main_window.on_request_zoom_in(move || {
        let mut s = state_zi.borrow_mut();
        s.zoom_factor = s.zoom_factor.zoom_in();
        s.zoom_mode = ZoomMode::Custom(s.zoom_factor);
        if let Some(win) = window_zi.upgrade() {
            update_page_view(&s, &scheduler_zi, &win);
        }
    });

    let state_zo = state.clone();
    let scheduler_zo = scheduler.clone();
    let window_zo = main_window.as_weak();
    main_window.on_request_zoom_out(move || {
        let mut s = state_zo.borrow_mut();
        s.zoom_factor = s.zoom_factor.zoom_out();
        s.zoom_mode = ZoomMode::Custom(s.zoom_factor);
        if let Some(win) = window_zo.upgrade() {
            update_page_view(&s, &scheduler_zo, &win);
        }
    });

    let state_fw = state.clone();
    let scheduler_fw = scheduler.clone();
    let window_fw = main_window.as_weak();
    main_window.on_request_fit_width(move || {
        let mut s = state_fw.borrow_mut();
        s.zoom_mode = ZoomMode::FitWidth;
        if let Some(win) = window_fw.upgrade() {
            update_page_view(&s, &scheduler_fw, &win);
        }
    });

    let state_fp = state.clone();
    let scheduler_fp = scheduler.clone();
    let window_fp = main_window.as_weak();
    main_window.on_request_fit_page(move || {
        let mut s = state_fp.borrow_mut();
        s.zoom_mode = ZoomMode::FitPage;
        if let Some(win) = window_fp.upgrade() {
            update_page_view(&s, &scheduler_fp, &win);
        }
    });

    let state_side = state.clone();
    let window_side = main_window.as_weak();
    let prefs_path_side = prefs_path.clone();
    main_window.on_request_toggle_sidebar(move || {
        if let Some(win) = window_side.upgrade() {
            let next_val = !win.get_sidebar_visible();
            win.set_sidebar_visible(next_val);
            let mut s = state_side.borrow_mut();
            s.preferences.sidebar_visible = next_val;
            let _ = s.preferences.save_to_file(&prefs_path_side);
        }
    });

    let state_fs = state.clone();
    let window_fs = main_window.as_weak();
    main_window.on_request_toggle_fullscreen(move || {
        if let Some(win) = window_fs.upgrade() {
            let mut s = state_fs.borrow_mut();
            if s.window_mode == WindowMode::FullScreen {
                s.window_mode = WindowMode::Normal;
                win.set_window_mode(0);
                win.window().set_fullscreen(false);
            } else {
                s.window_mode = WindowMode::FullScreen;
                win.set_window_mode(1);
                win.window().set_fullscreen(true);
            }
        }
    });

    let state_pres = state.clone();
    let window_pres = main_window.as_weak();
    main_window.on_request_presentation_mode(move || {
        if let Some(win) = window_pres.upgrade() {
            let mut s = state_pres.borrow_mut();
            s.window_mode = WindowMode::Presentation;
            win.set_window_mode(2);
            win.window().set_fullscreen(true);
        }
    });

    let state_exit = state.clone();
    let window_exit = main_window.as_weak();
    main_window.on_request_exit_special_mode(move || {
        if let Some(win) = window_exit.upgrade() {
            let mut s = state_exit.borrow_mut();
            if s.window_mode != WindowMode::Normal {
                s.window_mode = WindowMode::Normal;
                win.set_window_mode(0);
                win.window().set_fullscreen(false);
            }
        }
    });

    let state_pwd = state.clone();
    let scheduler_pwd = scheduler.clone();
    let window_pwd = main_window.as_weak();
    main_window.on_request_unlock_password(move |password| {
        let s = state_pwd.borrow();
        if let Some(ref path) = s.current_path {
            if let Some(win) = window_pwd.upgrade() {
                win.set_password_required(false);
                open_pdf_document(
                    path.clone(),
                    Some(password.to_string()),
                    &state_pwd,
                    &scheduler_pwd,
                    &win,
                );
            }
        }
    });

    // Language switching handler
    let state_lang = state.clone();
    let window_lang = main_window.as_weak();
    let prefs_path_lang = prefs_path.clone();
    main_window.on_request_change_language(move |lang_idx| {
        let mut s = state_lang.borrow_mut();
        let new_lang = match lang_idx {
            1 => Language::English,
            2 => Language::Turkish,
            _ => Language::System,
        };
        s.preferences.language = new_lang;
        let _ = s.preferences.save_to_file(&prefs_path_lang);
        if let Some(win) = window_lang.upgrade() {
            win.set_current_language(lang_idx);
            update_ui_strings(&win, new_lang.resolve());
        }
    });

    // Clipboard copy handler
    let state_copy = state.clone();
    let clipboard_copy = clipboard.clone();
    main_window.on_request_copy(move || {
        let s = state_copy.borrow();
        if let Some(ref sel) = s.selection {
            let geoms: Vec<PageTextGeometry> = s.text_geometries.values().cloned().collect();
            let text = SelectionEngine::get_selected_text(sel, &geoms);
            if !text.is_empty() {
                let _ = clipboard_copy.set_text(&text);
                tracing::info!("Copied {} chars to clipboard", text.len());
            }
        }
    });

    // Select all handler
    let state_sa = state.clone();
    let window_sa = main_window.as_weak();
    main_window.on_request_select_all(move || {
        let mut s = state_sa.borrow_mut();
        if s.page_count > 0 {
            let first = PageIndex::zero();
            let last = PageIndex::from_raw(s.page_count - 1);
            let end_char = s
                .text_geometries
                .get(&(s.page_count - 1))
                .map(|g| g.glyphs.len() as u32)
                .unwrap_or(10000);

            s.selection = Some(TextSelection::new(
                TextPosition::new(first, 0),
                TextPosition::new(last, end_char),
            ));

            if let Some(win) = window_sa.upgrade() {
                win.window().request_redraw();
            }
        }
    });

    // Memory-bounded LRU caches for page and thumbnail bitmaps
    let page_bitmaps = Rc::new(RefCell::new(LruCache::<u32, Image>::new(
        NonZeroUsize::new(10).unwrap(),
    )));
    let thumb_bitmaps = Rc::new(RefCell::new(LruCache::<u32, Image>::new(
        NonZeroUsize::new(30).unwrap(),
    )));

    // Mouse pointer handlers for text selection
    let state_pd = state.clone();
    let window_pd = main_window.as_weak();
    let page_bitmaps_pd = page_bitmaps.clone();
    let thumb_bitmaps_pd = thumb_bitmaps.clone();
    main_window.on_pointer_down(move |page_idx_raw, mx, my, _count| {
        let mut s = state_pd.borrow_mut();
        if page_idx_raw < 0 || (page_idx_raw as u32) >= s.page_count {
            return;
        }

        let p_idx = PageIndex::from_raw(page_idx_raw as u32);
        let (pw, ph) = s
            .all_page_dims
            .get(p_idx.get() as usize)
            .copied()
            .unwrap_or(s.first_page_dims);

        let dims = compute_target_dimensions(pw, ph, 1100, 800, s.zoom_mode, 1.0);
        let scale_x = pw / (dims.width as f32).max(1.0);
        let scale_y = ph / (dims.height as f32).max(1.0);

        let pdf_x = mx * scale_x;
        let pdf_y = ph - (my * scale_y);

        let now = std::time::Instant::now();
        if now.duration_since(s.last_click_time).as_millis() < 400 {
            s.click_count += 1;
        } else {
            s.click_count = 1;
        }
        s.last_click_time = now;

        if let Some(geom) = s.text_geometries.get(&p_idx.get()) {
            let char_idx = SelectionEngine::hit_test(geom, pdf_x, pdf_y);
            let click = s.click_count;
            if click == 2 {
                s.selection = Some(SelectionEngine::select_word(geom, p_idx, char_idx));
            } else if click >= 3 {
                s.selection = Some(SelectionEngine::select_line(geom, p_idx, char_idx));
            } else {
                let pos = TextPosition::new(p_idx, char_idx);
                s.selection = Some(TextSelection::new(pos, pos));
                s.is_selecting = true;
            }
        } else {
            let pos = TextPosition::new(p_idx, 0);
            s.selection = Some(TextSelection::new(pos, pos));
            s.is_selecting = true;
        }

        if let Some(win) = window_pd.upgrade() {
            let has_sel = s
                .selection
                .as_ref()
                .map(|sel| !sel.is_empty())
                .unwrap_or(false);
            win.set_has_selection(has_sel);
            refresh_slint_models(
                &s,
                &win,
                &mut page_bitmaps_pd.borrow_mut(),
                &mut thumb_bitmaps_pd.borrow_mut(),
            );
        }
    });

    let state_pm = state.clone();
    let window_pm = main_window.as_weak();
    let page_bitmaps_pm = page_bitmaps.clone();
    let thumb_bitmaps_pm = thumb_bitmaps.clone();
    main_window.on_pointer_move(move |page_idx_raw, mx, my| {
        let mut s = state_pm.borrow_mut();
        if !s.is_selecting || page_idx_raw < 0 || (page_idx_raw as u32) >= s.page_count {
            return;
        }

        let p_idx = PageIndex::from_raw(page_idx_raw as u32);
        let (pw, ph) = s
            .all_page_dims
            .get(p_idx.get() as usize)
            .copied()
            .unwrap_or(s.first_page_dims);

        let dims = compute_target_dimensions(pw, ph, 1100, 800, s.zoom_mode, 1.0);
        let scale_x = pw / (dims.width as f32).max(1.0);
        let scale_y = ph / (dims.height as f32).max(1.0);

        let pdf_x = mx * scale_x;
        let pdf_y = ph - (my * scale_y);

        let char_idx = if let Some(geom) = s.text_geometries.get(&p_idx.get()) {
            SelectionEngine::hit_test(geom, pdf_x, pdf_y)
        } else {
            0
        };

        if let Some(ref mut sel) = s.selection {
            sel.focus = TextPosition::new(p_idx, char_idx);
        }

        if let Some(win) = window_pm.upgrade() {
            let has_sel = s
                .selection
                .as_ref()
                .map(|sel| !sel.is_empty())
                .unwrap_or(false);
            win.set_has_selection(has_sel);
            refresh_slint_models(
                &s,
                &win,
                &mut page_bitmaps_pm.borrow_mut(),
                &mut thumb_bitmaps_pm.borrow_mut(),
            );
        }
    });

    let state_pu = state.clone();
    let window_pu = main_window.as_weak();
    let page_bitmaps_pu = page_bitmaps.clone();
    let thumb_bitmaps_pu = thumb_bitmaps.clone();
    main_window.on_pointer_up(move |_page_idx_raw, _mx, _my| {
        let mut s = state_pu.borrow_mut();
        s.is_selecting = false;
        if let Some(win) = window_pu.upgrade() {
            let has_sel = s
                .selection
                .as_ref()
                .map(|sel| !sel.is_empty())
                .unwrap_or(false);
            win.set_has_selection(has_sel);
            refresh_slint_models(
                &s,
                &win,
                &mut page_bitmaps_pu.borrow_mut(),
                &mut thumb_bitmaps_pu.borrow_mut(),
            );
        }
    });

    // Event Loop Timer (~60 FPS)
    let timer = Timer::default();
    let scheduler_tick = scheduler.clone();
    let window_tick = main_window.as_weak();
    let state_tick = state.clone();

    let page_bitmaps_tick = page_bitmaps.clone();
    let thumb_bitmaps_tick = thumb_bitmaps.clone();

    timer.start(
        TimerMode::Repeated,
        std::time::Duration::from_millis(16),
        move || {
            while let Some(event) = scheduler_tick.try_recv_event() {
                if let Some(win) = window_tick.upgrade() {
                    match event {
                        RenderEvent::DocumentOpened {
                            document_id,
                            page_count,
                            first_page_dimensions,
                            all_page_dimensions,
                        } => {
                            page_bitmaps_tick.borrow_mut().clear();
                            thumb_bitmaps_tick.borrow_mut().clear();

                            let mut s = state_tick.borrow_mut();
                            s.current_doc_id = Some(document_id);
                            s.page_count = page_count;
                            s.current_page = 0;
                            s.first_page_dims = first_page_dimensions;
                            s.all_page_dims = all_page_dimensions;
                            s.text_geometries.clear();
                            s.selection = None;

                            let doc_name = s
                                .current_path
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .and_then(|n| n.to_str())
                                .unwrap_or("document.pdf");

                            win.set_has_document(true);
                            win.set_has_selection(false);
                            win.set_password_required(false);
                            win.set_document_title(SharedString::from(doc_name));
                            win.set_total_pages_str(SharedString::from(page_count.to_string()));
                            win.set_status_text(SharedString::from(format!(
                                "Opened {} ({} pages)",
                                doc_name, page_count
                            )));

                            update_page_view(&s, &scheduler_tick, &win);
                            request_thumbnails(&s, &scheduler_tick);
                        }
                        RenderEvent::PageRendered {
                            request_id: _,
                            generation: _,
                            document_id: _,
                            page_index,
                            bitmap,
                        } => {
                            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                                &bitmap.pixels,
                                bitmap.width,
                                bitmap.height,
                            );
                            let slint_img = Image::from_rgba8(buffer);

                            if bitmap.width <= 200 {
                                thumb_bitmaps_tick
                                    .borrow_mut()
                                    .put(page_index.get(), slint_img);
                            } else {
                                page_bitmaps_tick
                                    .borrow_mut()
                                    .put(page_index.get(), slint_img.clone());

                                let s = state_tick.borrow();
                                if page_index.get() == s.current_page {
                                    win.set_page_bitmap(slint_img);
                                }
                            }

                            let s = state_tick.borrow();
                            refresh_slint_models(
                                &s,
                                &win,
                                &mut page_bitmaps_tick.borrow_mut(),
                                &mut thumb_bitmaps_tick.borrow_mut(),
                            );
                        }
                        RenderEvent::TextGeometryFetched {
                            document_id: _,
                            page_index,
                            geometry,
                        } => {
                            let mut s = state_tick.borrow_mut();
                            s.text_geometries.insert(page_index.get(), geometry);

                            refresh_slint_models(
                                &s,
                                &win,
                                &mut page_bitmaps_tick.borrow_mut(),
                                &mut thumb_bitmaps_tick.borrow_mut(),
                            );
                        }
                        RenderEvent::Error {
                            request_id: _,
                            error,
                        } => match error {
                            PdfError::PasswordRequired | PdfError::IncorrectPassword => {
                                let s = state_tick.borrow();
                                if let Some(ref path) = s.current_path {
                                    win.set_password_required(true);
                                    win.set_protected_file_name(SharedString::from(
                                        path.file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("document.pdf"),
                                    ));
                                }
                            }
                            other => {
                                win.set_status_text(SharedString::from(format!("Error: {other}")));
                            }
                        },
                        _ => {}
                    }
                }
            }
        },
    );

    main_window.run()?;
    Ok(())
}

fn open_pdf_document(
    path: PathBuf,
    password: Option<String>,
    state: &Rc<RefCell<AppState>>,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    let doc_id = DocumentId::new(rand_id());
    {
        let mut s = state.borrow_mut();
        s.current_path = Some(path.clone());
        s.current_doc_id = Some(doc_id);
    }

    window.set_status_text(SharedString::from("Opening document..."));
    scheduler.send_command(RenderCommand::OpenDocument {
        document_id: doc_id,
        path,
        password,
    });
}

fn update_page_view(state: &AppState, scheduler: &RenderScheduler, window: &AppWindow) {
    let doc_id = match state.current_doc_id {
        Some(id) => id,
        None => return,
    };

    let layout =
        ContinuousLayout::compute(&state.all_page_dims, 1100, 800, state.zoom_mode, 1.0, 12.0);

    window.set_document_total_height(layout.total_height);
    window.set_current_page_str(SharedString::from((state.current_page + 1).to_string()));

    let page_count = PageCount::new(state.page_count.max(1)).expect("non-zero");
    let current_idx =
        PageIndex::new(state.current_page, page_count).unwrap_or_else(PageIndex::zero);

    let (pw, ph) = state
        .all_page_dims
        .get(current_idx.get() as usize)
        .copied()
        .unwrap_or(state.first_page_dims);

    let target_dims = compute_target_dimensions(pw, ph, 1100, 800, state.zoom_mode, 1.0);
    window.set_page_display_width(target_dims.width as f32);
    window.set_page_display_height(target_dims.height as f32);
    window.set_zoom_str(SharedString::from(format!(
        "{}%",
        (target_dims.width as f32 / pw * 100.0) as u32
    )));

    // Request render for visible range based on viewing mode
    let gen = scheduler.bump_generation();

    let (start_idx, end_idx) = match state.viewing_mode {
        ViewingMode::SinglePage => (
            current_idx.get(),
            (current_idx.get() + 1).min(state.page_count),
        ),
        _ => (
            current_idx.get().saturating_sub(1),
            (current_idx.get() + 6).min(state.page_count),
        ),
    };

    for idx in start_idx..end_idx {
        let p_idx = PageIndex::from_raw(idx);
        let (w_pts, h_pts) = state
            .all_page_dims
            .get(idx as usize)
            .copied()
            .unwrap_or((612.0, 792.0));

        let dims = compute_target_dimensions(w_pts, h_pts, 1100, 800, state.zoom_mode, 1.0);

        let priority = if idx == current_idx.get() {
            Priority::Visible
        } else {
            Priority::Prefetch
        };

        scheduler.send_command(RenderCommand::RenderPage(RenderJob {
            request_id: RequestId::new(rand_id()),
            generation: gen,
            document_id: doc_id,
            page_index: p_idx,
            target_width: dims.width,
            target_height: dims.height,
            rotation: state.rotation,
            priority,
        }));

        // Request text geometry on demand if not cached
        if !state.text_geometries.contains_key(&idx) {
            scheduler.send_command(RenderCommand::FetchTextGeometry {
                document_id: doc_id,
                page_index: p_idx,
            });
        }
    }
}

fn request_thumbnails(state: &AppState, scheduler: &RenderScheduler) {
    let doc_id = match state.current_doc_id {
        Some(id) => id,
        None => return,
    };

    let gen = scheduler.current_generation();
    let start_idx = state.current_page.saturating_sub(4);
    let end_idx = (state.current_page + 12).min(state.page_count);

    for idx in start_idx..end_idx {
        let p_idx = PageIndex::from_raw(idx);
        let (w_pts, h_pts) = state
            .all_page_dims
            .get(idx as usize)
            .copied()
            .unwrap_or((612.0, 792.0));

        let aspect = h_pts / w_pts.max(1.0);
        let tw = 140u32;
        let th = ((140.0 * aspect).round() as u32).clamp(100, 220);

        scheduler.send_command(RenderCommand::RenderPage(RenderJob {
            request_id: RequestId::new(rand_id()),
            generation: gen,
            document_id: doc_id,
            page_index: p_idx,
            target_width: tw,
            target_height: th,
            rotation: state.rotation,
            priority: Priority::Thumbnail,
        }));
    }
}

fn refresh_slint_models(
    state: &AppState,
    window: &AppWindow,
    page_bitmaps: &mut LruCache<u32, Image>,
    thumb_bitmaps: &mut LruCache<u32, Image>,
) {
    let layout =
        ContinuousLayout::compute(&state.all_page_dims, 1100, 800, state.zoom_mode, 1.0, 12.0);

    let pages_model = VecModel::<PageItem>::default();
    let current_idx = state.current_page;

    match state.viewing_mode {
        ViewingMode::SinglePage => {
            if let Some(box_info) = layout.pages.get(current_idx as usize) {
                let (bmp, has_bmp) = match page_bitmaps.get(&current_idx).cloned() {
                    Some(b) => (b, true),
                    None => (Image::default(), false),
                };

                let sel_boxes = compute_selection_boxes(
                    state,
                    current_idx,
                    box_info.width as f32,
                    box_info.height as f32,
                );

                pages_model.push(PageItem {
                    page_index: current_idx as i32,
                    page_number: SharedString::from((current_idx + 1).to_string()),
                    width: box_info.width as f32,
                    height: box_info.height as f32,
                    y_offset: box_info.y_offset,
                    bitmap: bmp,
                    has_bitmap: has_bmp,
                    selection_boxes: ModelRc::new(VecModel::from(sel_boxes)),
                });
            }
        }
        _ => {
            for (idx, box_info) in layout.pages.iter().enumerate() {
                let (bmp, has_bmp) = match page_bitmaps.get(&(idx as u32)).cloned() {
                    Some(b) => (b, true),
                    None => (Image::default(), false),
                };

                let sel_boxes = compute_selection_boxes(
                    state,
                    idx as u32,
                    box_info.width as f32,
                    box_info.height as f32,
                );

                pages_model.push(PageItem {
                    page_index: idx as i32,
                    page_number: SharedString::from((idx + 1).to_string()),
                    width: box_info.width as f32,
                    height: box_info.height as f32,
                    y_offset: box_info.y_offset,
                    bitmap: bmp,
                    has_bitmap: has_bmp,
                    selection_boxes: ModelRc::new(VecModel::from(sel_boxes)),
                });
            }
        }
    }
    window.set_visible_pages(ModelRc::new(pages_model));

    // Refresh Sidebar Thumbnails Model (lazy range)
    let thumbs_model = VecModel::<ThumbnailItem>::default();
    let start_idx = state.current_page.saturating_sub(10);
    let end_idx = (state.current_page + 30).min(state.page_count);

    for idx in start_idx..end_idx {
        let bmp = thumb_bitmaps.get(&idx).cloned().unwrap_or_default();
        let (w_pts, h_pts) = state
            .all_page_dims
            .get(idx as usize)
            .copied()
            .unwrap_or((612.0, 792.0));
        let aspect = h_pts / w_pts.max(1.0);
        let tw = 140.0;
        let th = (140.0 * aspect).clamp(100.0, 220.0);

        thumbs_model.push(ThumbnailItem {
            page_index: idx as i32,
            page_number: SharedString::from(format!("Page {}", idx + 1)),
            width: tw,
            height: th,
            bitmap: bmp,
            is_selected: idx == current_idx,
        });
    }
    window.set_thumbnail_items(ModelRc::new(thumbs_model));
}

fn compute_selection_boxes(
    state: &AppState,
    page_idx: u32,
    target_w: f32,
    target_h: f32,
) -> Vec<SelectionBox> {
    let sel = match state.selection {
        Some(ref s) => s,
        None => return Vec::new(),
    };

    let p_idx = PageIndex::from_raw(page_idx);
    let (range_start, range_end) = match sel.range_for_page(p_idx) {
        Some(r) => r,
        None => return Vec::new(),
    };

    let geom = match state.text_geometries.get(&page_idx) {
        Some(g) => g,
        None => return Vec::new(),
    };

    let (pw, ph) = state
        .all_page_dims
        .get(page_idx as usize)
        .copied()
        .unwrap_or(state.first_page_dims);

    let scale_x = target_w / pw.max(1.0);
    let scale_y = target_h / ph.max(1.0);

    let start_idx = (range_start as usize).min(geom.glyphs.len());
    let end_idx = (range_end as usize).min(geom.glyphs.len());

    if start_idx >= end_idx {
        return Vec::new();
    }

    let selected_glyphs = &geom.glyphs[start_idx..end_idx];

    // Merge adjacent glyphs on the same text line into single continuous rectangles (Chrome PDF Viewer style)
    let mut line_rects: Vec<(f32, f32, f32, f32)> = Vec::new(); // (min_x, max_x, min_y, max_y)

    for g in selected_glyphs {
        if g.ch == '\n' || g.ch == '\r' || (g.width <= 0.001 && g.height <= 0.001) {
            continue;
        }

        let gx1 = g.x;
        let gx2 = g.x + g.width;
        let gy1 = g.y;
        let gy2 = g.y + g.height;

        let mut merged = false;
        if let Some(last) = line_rects.last_mut() {
            let y_overlap = (gy2.min(last.3) - gy1.max(last.2)).max(0.0);
            let avg_h = ((gy2 - gy1) + (last.3 - last.2)) / 2.0;

            // Same line if vertical overlap is significant or baselines are close
            if y_overlap > avg_h * 0.3 || (gy1 - last.2).abs() < avg_h * 0.4 {
                last.0 = last.0.min(gx1);
                last.1 = last.1.max(gx2);
                last.2 = last.2.min(gy1);
                last.3 = last.3.max(gy2);
                merged = true;
            }
        }

        if !merged {
            line_rects.push((gx1, gx2, gy1, gy2));
        }
    }

    // Convert merged PDF line bounds to viewport pixel selection boxes
    let mut boxes = Vec::new();
    for (lx1, lx2, ly1, ly2) in line_rects {
        let line_h = (ly2 - ly1).max(1.0);
        // PDFium loose font bounds include ~16% EM ascent leading padding above cap height.
        // Adjusting ly2 and ly1 aligns the selection box rectangle precisely over drawn character ink.
        let adj_ly2 = ly2 - (line_h * 0.16);
        let adj_ly1 = ly1 - (line_h * 0.04);

        let sx = lx1 * scale_x;
        let sy = (ph - adj_ly2) * scale_y;
        let sw = (lx2 - lx1) * scale_x;
        let sh = (adj_ly2 - adj_ly1) * scale_y;

        if sw > 0.5 && sh > 0.5 {
            boxes.push(SelectionBox {
                x: sx,
                y: sy,
                width: sw,
                height: sh,
            });
        }
    }

    boxes
}

fn update_ui_strings(window: &AppWindow, lang: ResolvedLanguage) {
    window.set_text_open(SharedString::from(barepdf_i18n::t(lang, "open.file")));
    window.set_text_sidebar(SharedString::from(barepdf_i18n::t(lang, "sidebar.toggle")));
    window.set_text_thumbnails(SharedString::from(barepdf_i18n::t(
        lang,
        "sidebar.thumbnails",
    )));
    window.set_text_outline(SharedString::from(barepdf_i18n::t(lang, "sidebar.outline")));
    window.set_text_view(SharedString::from(barepdf_i18n::t(lang, "view.mode")));
    window.set_text_zoom_in(SharedString::from(barepdf_i18n::t(lang, "zoom.in")));
    window.set_text_zoom_out(SharedString::from(barepdf_i18n::t(lang, "zoom.out")));
    window.set_text_fit_width(SharedString::from(barepdf_i18n::t(lang, "zoom.fit_width")));
    window.set_text_fit_page(SharedString::from(barepdf_i18n::t(lang, "zoom.fit_page")));
    window.set_text_actual_size(SharedString::from(barepdf_i18n::t(
        lang,
        "zoom.actual_size",
    )));
    window.set_text_fullscreen(SharedString::from(barepdf_i18n::t(lang, "fullscreen")));
    window.set_text_presentation(SharedString::from(barepdf_i18n::t(lang, "presentation")));
    window.set_text_settings(SharedString::from(barepdf_i18n::t(lang, "settings")));
    window.set_text_copy(SharedString::from(barepdf_i18n::t(lang, "context.copy")));
    window.set_text_select_all(SharedString::from(barepdf_i18n::t(
        lang,
        "context.select_all",
    )));
    window.set_text_close(SharedString::from(barepdf_i18n::t(lang, "settings.close")));
    window.set_text_empty_title(SharedString::from(barepdf_i18n::t(lang, "empty.title")));
    window.set_text_empty_desc(SharedString::from(barepdf_i18n::t(lang, "empty.desc")));
}

fn rand_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}
