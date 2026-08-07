#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use barepdf_core::{
    compute_target_dimensions, default_config_path, ContinuousLayout, DocumentId, MemoryBudget,
    PageCount, PageIndex, PdfError, RequestId, Rotation, UserPreferences, ViewingMode, WindowMode,
    ZoomFactor, ZoomMode,
};
use barepdf_pdf::PdfiumEngine;
use barepdf_platform::FileDialogs;
use barepdf_platform_windows::{WindowsClipboard, WindowsFileDialogs};
use barepdf_render::{Priority, RenderCommand, RenderEvent, RenderJob, RenderScheduler};
use barepdf_ui::{AppWindow, PageItem, ThumbnailItem};
use slint::{
    ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode,
    VecModel,
};
use std::cell::RefCell;
use std::env;
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
    let _clipboard = Arc::new(WindowsClipboard::new());

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
        preferences,
    }));

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
                update_page_view(&s, &scheduler_sel, &win);
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

    let state_act = state.clone();
    let scheduler_act = scheduler.clone();
    let window_act = main_window.as_weak();
    main_window.on_request_actual_size(move || {
        let mut s = state_act.borrow_mut();
        s.zoom_mode = ZoomMode::ActualSize;
        if let Some(win) = window_act.upgrade() {
            update_page_view(&s, &scheduler_act, &win);
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

    // Event Loop Timer (~60 FPS)
    let timer = Timer::default();
    let scheduler_tick = scheduler.clone();
    let window_tick = main_window.as_weak();
    let state_tick = state.clone();

    // Map storing rendered page bitmaps for active document
    let page_bitmaps = Rc::new(RefCell::new(std::collections::HashMap::<u32, Image>::new()));
    let thumb_bitmaps = Rc::new(RefCell::new(std::collections::HashMap::<u32, Image>::new()));

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

                            let doc_name = s
                                .current_path
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .and_then(|n| n.to_str())
                                .unwrap_or("document.pdf");

                            win.set_has_document(true);
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
                                    .insert(page_index.get(), slint_img);
                            } else {
                                page_bitmaps_tick
                                    .borrow_mut()
                                    .insert(page_index.get(), slint_img.clone());
                                win.set_page_bitmap(slint_img);
                            }

                            let s = state_tick.borrow();
                            refresh_slint_models(
                                &s,
                                &win,
                                &page_bitmaps_tick.borrow(),
                                &thumb_bitmaps_tick.borrow(),
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

    // Request render for current page and nearby prefetch pages
    let gen = scheduler.bump_generation();

    let start_idx = current_idx.get().saturating_sub(1);
    let end_idx = (current_idx.get() + 3).min(state.page_count);

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
    }
}

fn request_thumbnails(state: &AppState, scheduler: &RenderScheduler) {
    let doc_id = match state.current_doc_id {
        Some(id) => id,
        None => return,
    };

    let gen = scheduler.current_generation();
    let thumb_count = state.page_count.min(50); // limit eager thumbnail renders

    for idx in 0..thumb_count {
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
    page_bitmaps: &std::collections::HashMap<u32, Image>,
    thumb_bitmaps: &std::collections::HashMap<u32, Image>,
) {
    let layout =
        ContinuousLayout::compute(&state.all_page_dims, 1100, 800, state.zoom_mode, 1.0, 12.0);

    // Refresh Visible Pages Model
    let pages_model = VecModel::<PageItem>::default();
    let current_idx = state.current_page;

    let start_idx = current_idx.saturating_sub(1);
    let end_idx = (current_idx + 4).min(state.page_count);

    for idx in start_idx..end_idx {
        if let Some(box_info) = layout.pages.get(idx as usize) {
            let bmp = page_bitmaps.get(&idx).cloned().unwrap_or_default();
            pages_model.push(PageItem {
                page_index: idx as i32,
                page_number: SharedString::from((idx + 1).to_string()),
                width: box_info.width as f32,
                height: box_info.height as f32,
                y_offset: box_info.y_offset,
                bitmap: bmp,
            });
        }
    }
    window.set_visible_pages(ModelRc::new(pages_model));

    // Refresh Sidebar Thumbnails Model
    let thumbs_model = VecModel::<ThumbnailItem>::default();
    let thumb_count = state.page_count.min(50);

    for idx in 0..thumb_count {
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

fn rand_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}
