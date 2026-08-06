#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use barepdf_core::{
    DocumentId, MemoryBudget, PageCount, PageIndex, PdfError, RequestId, Rotation, UserPreferences,
    ZoomFactor, ZoomMode,
};
use barepdf_pdf::PdfiumEngine;
use barepdf_platform::FileDialogs;
use barepdf_platform_windows::{WindowsClipboard, WindowsFileDialogs};
use barepdf_render::{Priority, RenderCommand, RenderEvent, RenderJob, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{
    ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode,
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
    zoom_mode: ZoomMode,
    zoom_factor: ZoomFactor,
    rotation: Rotation,
    first_page_dims: (f32, f32),
    _preferences: UserPreferences,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting BarePDF application...");

    let pdf_engine = match PdfiumEngine::new() {
        Ok(engine) => engine,
        Err(err) => {
            tracing::warn!("Failed to initialize PDFium system engine: {}", err);
            PdfiumEngine::new()?
        }
    };

    let preferences = UserPreferences::default();
    let scheduler = Rc::new(RenderScheduler::spawn(
        pdf_engine,
        MemoryBudget::new(preferences.memory_budget_bytes),
    ));
    let dialogs = Arc::new(WindowsFileDialogs);
    let _clipboard = Arc::new(WindowsClipboard::new());

    let main_window = AppWindow::new()?;
    let state = Rc::new(RefCell::new(AppState {
        current_doc_id: None,
        current_path: None,
        page_count: 0,
        current_page: 0,
        zoom_mode: ZoomMode::FitWidth,
        zoom_factor: ZoomFactor::default(),
        rotation: Rotation::Degrees0,
        first_page_dims: (612.0, 792.0),
        _preferences: preferences,
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

    // Render Event Processing Loop (Tick timer ~60 FPS)
    let timer = Timer::default();
    let scheduler_tick = scheduler.clone();
    let window_tick = main_window.as_weak();
    let state_tick = state.clone();

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
                        } => {
                            let mut s = state_tick.borrow_mut();
                            s.current_doc_id = Some(document_id);
                            s.page_count = page_count;
                            s.current_page = 0;
                            s.first_page_dims = first_page_dimensions;

                            win.set_has_document(true);
                            win.set_password_required(false);
                            win.set_total_pages_str(SharedString::from(page_count.to_string()));
                            win.set_status_text(SharedString::from(format!(
                                "Opened document ({} pages)",
                                page_count
                            )));

                            update_page_view(&s, &scheduler_tick, &win);
                        }
                        RenderEvent::PageRendered {
                            request_id: _,
                            generation,
                            document_id: _,
                            page_index,
                            bitmap,
                        } if generation == scheduler_tick.current_generation() => {
                            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                                &bitmap.pixels,
                                bitmap.width,
                                bitmap.height,
                            );
                            let slint_img = Image::from_rgba8(buffer);

                            win.set_page_bitmap(slint_img);
                            win.set_current_page_str(SharedString::from(
                                (page_index.get() + 1).to_string(),
                            ));
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

    let page_count = PageCount::new(state.page_count.max(1)).expect("non-zero");
    let page_index = PageIndex::new(state.current_page, page_count).unwrap_or_else(PageIndex::zero);

    let gen = scheduler.bump_generation();
    let (pw, ph) = state.first_page_dims;
    let target_dims =
        barepdf_core::compute_target_dimensions(pw, ph, 1100, 800, state.zoom_mode, 1.0);

    let job = RenderJob {
        request_id: RequestId::new(rand_id()),
        generation: gen,
        document_id: doc_id,
        page_index,
        target_width: target_dims.width,
        target_height: target_dims.height,
        rotation: state.rotation,
        priority: Priority::Visible,
    };

    window.set_zoom_str(SharedString::from(format!(
        "{}%",
        (target_dims.width as f32 / pw * 100.0) as u32
    )));
    scheduler.send_command(RenderCommand::RenderPage(job));
}

fn rand_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}
