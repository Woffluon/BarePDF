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

use crate::application::{DocumentController, PrintController, PrintControllerError};
use crate::diagnostics::{self, DiagnosticEvent};
use crate::infrastructure::{
    PrintEvent, ToolEvent, ToolJobKey, ToolOperation, ToolOutcome, ToolRequest, ToolWorker,
    UpdateCheckCanceller, UpdateCommand,
};

use barepdf_core::{
    page_range::PageRangeSelection, selection::SelectionEngine, DocumentId, PageCount, PageIndex,
    RequestId, Rotation, TextPosition, TextSelection, ViewingMode, WindowMode, ZoomFactor,
    ZoomMode, MAX_OPEN_TABS, MAX_PASSWORD_BYTES,
};
use barepdf_i18n::{Language, ResolvedLanguage};
use barepdf_pdf::conversion::{ConversionDpi, ConversionFormat};
use barepdf_platform::printing::PrintRange;
use barepdf_platform::{ClipboardAccess, FileDialogs};
use barepdf_platform_windows::{
    is_installed_build, open_url, WindowsClipboard, WindowsFileDialogs, WindowsPrinterDialog,
};
use barepdf_render::{Priority, RenderCommand, RenderJob, RenderKind, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, Image, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::models::{refresh_page_model, refresh_tab_model, refresh_thumbnail_model};
use super::state::AppState;
use super::ui::{
    apply_theme, begin_open, invalidate_layout_and_render, native_window_handle, navigate_to_page,
    navigate_to_page_inner, parse_drop_paths, persist_preferences, pointer_to_pdf,
    refresh_outline_model, render_visible_pages, request_visible_thumbnails, save_zoom_preference,
    send_render_command, show_banner, sync_effective_zoom, theme_from_index, update_ui_strings,
    update_zoom_ui, validated_page_input, view_mode_index, view_mode_label, zoom_mode_index,
    zoom_percentage,
};
use super::update_ui::{queue_update_check, render_update_ui};
pub(super) fn wire_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    dialogs: Arc<WindowsFileDialogs>,
    clipboard: Arc<WindowsClipboard>,
    preferences_path: &Path,
    background: (
        std::sync::mpsc::Sender<UpdateCommand>,
        UpdateCheckCanceller,
        Option<Rc<RefCell<PrintController>>>,
    ),
) {
    let (update_sender, update_check_canceller, print_controller) = background;
    let weak = window.as_weak();
    let state_open = state.clone();
    let scheduler_open = scheduler.clone();
    let dialogs_open = dialogs.clone();
    window.on_request_open_file(move || {
        if let (Some(path), Some(window)) = (dialogs_open.pick_file(), weak.upgrade()) {
            begin_open(path, None, &state_open, &scheduler_open, &window);
        }
    });

    connect_navigation_callbacks(window, state, scheduler);
    connect_zoom_callbacks(window, state, scheduler);
    connect_view_callbacks(window, state, scheduler, preferences_path);
    connect_selection_callbacks(window, state, scheduler, clipboard);
    connect_tab_callbacks(window, state, scheduler);
    connect_print_callbacks(window, state, scheduler, print_controller);
    connect_tools_callbacks(window, state, scheduler, dialogs);

    let weak = window.as_weak();
    let state_password = state.clone();
    let scheduler_password = scheduler.clone();
    window.on_request_unlock_password(move |password| {
        if password.as_str().len() > MAX_PASSWORD_BYTES {
            let language = state_password.borrow().preferences.language.resolve();
            if let Some(window) = weak.upgrade() {
                window.set_password_error(SharedString::from(barepdf_i18n::t(
                    language,
                    "password.error.too_long",
                )));
                window.set_password_required(true);
            }
            return;
        }
        let path = DocumentController::pending_path(&state_password.borrow().application)
            .map(Path::to_path_buf);
        if let (Some(path), Some(window)) = (path, weak.upgrade()) {
            let password = password.to_string();
            begin_open(
                path,
                Some(password),
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
        let window = weak.upgrade();
        persist_preferences(
            &app.preferences,
            &preferences_path_language,
            window.as_ref(),
        );
        if let Some(window) = window {
            window.set_current_language(index);
            window.set_view_mode_label(SharedString::from(view_mode_label(
                app.viewing_mode,
                language.resolve(),
            )));
            update_ui_strings(&window, language.resolve());
            refresh_thumbnail_model(&mut app, &window);
            render_update_ui(&window, &app);
        }
    });

    let weak = window.as_weak();
    let state_theme = state.clone();
    let preferences_path_theme = preferences_path.to_path_buf();
    window.on_request_change_theme(move |index| {
        let theme = theme_from_index(index);
        let mut app = state_theme.borrow_mut();
        app.preferences.theme = theme;
        let window = weak.upgrade();
        persist_preferences(&app.preferences, &preferences_path_theme, window.as_ref());
        if let Some(window) = window {
            apply_theme(&window, theme);
        }
    });

    let weak = window.as_weak();
    let state_update_consent = state.clone();
    let update_check_canceller_consent = update_check_canceller.clone();
    let preferences_path_updates = preferences_path.to_path_buf();
    window.on_request_change_update_checks(move |enabled| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if !enabled {
            update_check_canceller_consent.cancel_pending_check();
        }
        {
            let mut app = state_update_consent.borrow_mut();
            app.preferences.update_checks_enabled = Some(enabled);
            persist_preferences(&app.preferences, &preferences_path_updates, Some(&window));
        }
        window.set_update_checks_enabled(enabled);
    });

    let weak = window.as_weak();
    let state_update_check = state.clone();
    let preferences_path_check = preferences_path.to_path_buf();
    let update_sender_check = update_sender.clone();
    let update_check_canceller_check = update_check_canceller.clone();
    window.on_request_check_update(move || {
        if let Some(window) = weak.upgrade() {
            queue_update_check(
                &update_sender_check,
                &update_check_canceller_check,
                &state_update_check,
                &window,
                &preferences_path_check,
            );
        }
    });

    let weak = window.as_weak();
    let state_update_action = state.clone();
    window.on_request_update_action(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_update_action.borrow_mut();
        if app.update.is_busy() {
            return;
        }
        if let Some((path, update)) = app.update.begin_install() {
            app.wake_pump();
            render_update_ui(&window, &app);
            if update_sender
                .send(UpdateCommand::Install { path, update })
                .is_err()
            {
                app.update.mark_failed();
                render_update_ui(&window, &app);
            }
            return;
        }
        if !is_installed_build() {
            let Some(release_url) = app.update.release_url().map(str::to_owned) else {
                return;
            };
            if let Err(error) = open_url(&release_url) {
                diagnostics::warn_redacted(DiagnosticEvent::ReleasePageOpen, &error);
                app.update.mark_failed();
                render_update_ui(&window, &app);
            }
            return;
        }
        let Some(update) = app.update.begin_download() else {
            return;
        };
        app.wake_pump();
        render_update_ui(&window, &app);
        if update_sender.send(UpdateCommand::Download(update)).is_err() {
            app.update.mark_failed();
            render_update_ui(&window, &app);
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
            window.set_banner_update_action(false);
            window.set_banner_action_label(SharedString::default());
            window.set_banner_action_enabled(false);
        }
    });

    let weak = window.as_weak();
    let state_retry = state.clone();
    let scheduler_retry = scheduler.clone();
    window.on_request_retry(move || {
        let path = DocumentController::failed_path(&state_retry.borrow().application)
            .map(Path::to_path_buf);
        if let (Some(path), Some(window)) = (path, weak.upgrade()) {
            begin_open(path, None, &state_retry, &scheduler_retry, &window);
        }
    });
}

const PRINT_PREVIEW_REQUEST_MASK: u64 = 1 << 63;
const PRINT_PREVIEW_MAX_EDGE: u32 = 960;
static PRINT_PREVIEW_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
struct PendingPreviewRender {
    request_id: RequestId,
    page_index: PageIndex,
}

#[derive(Debug)]
struct PrintPreviewState {
    open: bool,
    document_id: DocumentId,
    generation: u64,
    page_count: PageCount,
    page_index: PageIndex,
    orientation: i32,
    range: String,
    pending: Option<PendingPreviewRender>,
}

impl PrintPreviewState {
    fn open(
        document_id: DocumentId,
        generation: u64,
        page_count: PageCount,
        page_index: PageIndex,
    ) -> Self {
        Self {
            open: true,
            document_id,
            generation,
            page_count,
            page_index: PageIndex::from_raw(page_index.get().min(page_count.get() - 1)),
            orientation: 0,
            range: default_print_preview_range(page_count),
            pending: None,
        }
    }

    fn expect_render(&mut self, request_id: RequestId, page_index: PageIndex) {
        self.pending = Some(PendingPreviewRender {
            request_id,
            page_index,
        });
    }

    fn accept_render(
        &mut self,
        request_id: RequestId,
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
    ) -> bool {
        let matches = self.open
            && self.document_id == document_id
            && self.generation == generation
            && self.pending.is_some_and(|pending| {
                pending.request_id == request_id
                    && pending.page_index == page_index
                    && self.page_index == page_index
            });
        if matches {
            self.pending = None;
        }
        matches
    }

    fn close(&mut self) {
        self.open = false;
        self.pending = None;
    }

    fn set_page(&mut self, page: i32) -> PageIndex {
        let maximum = self.page_count.get().saturating_sub(1);
        let page = u32::try_from(page).unwrap_or(0).min(maximum);
        self.page_index = PageIndex::from_raw(page);
        self.page_index
    }
}

thread_local! {
    static PRINT_PREVIEW: RefCell<Option<PrintPreviewState>> = const { RefCell::new(None) };
}

fn default_print_preview_range(page_count: PageCount) -> String {
    if page_count.get() == 1 {
        "1".into()
    } else {
        format!("1-{}", page_count.get())
    }
}

fn parse_print_preview_range(input: &str, page_count: PageCount) -> Option<(PageIndex, PageIndex)> {
    let input = input.trim();
    if input.is_empty() {
        return Some((PageIndex::zero(), PageIndex::from_raw(page_count.get() - 1)));
    }
    let mut first_page: Option<u32> = None;
    let mut previous_last: Option<u32> = None;
    for segment in input.split(',') {
        let segment = segment.trim();
        let (first, last) = segment
            .split_once('-')
            .map_or((segment, segment), |parts| parts);
        if first.contains('-') || last.contains('-') {
            return None;
        }
        let first = first.trim().parse::<u32>().ok()?;
        let last = last.trim().parse::<u32>().ok()?;
        if first < 1 || first > last || last > page_count.get() {
            return None;
        }
        if previous_last.is_some_and(|previous| first != previous.saturating_add(1)) {
            return None;
        }
        first_page.get_or_insert(first);
        previous_last = Some(last);
    }
    Some((
        PageIndex::from_raw(first_page? - 1),
        PageIndex::from_raw(previous_last? - 1),
    ))
}

fn next_print_preview_request_id() -> RequestId {
    let sequence = PRINT_PREVIEW_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    RequestId::new(PRINT_PREVIEW_REQUEST_MASK | (sequence & !PRINT_PREVIEW_REQUEST_MASK).max(1))
}

fn print_preview_dimensions(dimensions: (f32, f32), rotation: Rotation) -> (u32, u32) {
    let (mut width, mut height) = dimensions;
    if matches!(rotation, Rotation::Degrees90 | Rotation::Degrees270) {
        std::mem::swap(&mut width, &mut height);
    }
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return (PRINT_PREVIEW_MAX_EDGE, PRINT_PREVIEW_MAX_EDGE);
    }
    let scale = PRINT_PREVIEW_MAX_EDGE as f32 / width.max(height);
    (
        (width * scale)
            .round()
            .clamp(1.0, PRINT_PREVIEW_MAX_EDGE as f32) as u32,
        (height * scale)
            .round()
            .clamp(1.0, PRINT_PREVIEW_MAX_EDGE as f32) as u32,
    )
}

fn request_print_preview_render(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    let Some(document_id) = app.active_document() else {
        return;
    };
    let request = PRINT_PREVIEW.with(|preview| {
        let mut preview = preview.borrow_mut();
        let preview = preview.as_mut()?;
        if !preview.open
            || preview.document_id != document_id
            || preview.generation != app.generation
        {
            return None;
        }
        let page_index = preview.page_index;
        let dimensions = app
            .page_dimensions
            .get(page_index.get() as usize)
            .copied()
            .unwrap_or(app.first_page_dimensions);
        let (target_width, target_height) = print_preview_dimensions(dimensions, app.rotation);
        let request_id = next_print_preview_request_id();
        preview.expect_render(request_id, page_index);
        Some((
            request_id,
            RenderCommand::RenderPage(RenderJob {
                request_id,
                generation: app.generation,
                document_id,
                page_index,
                target_width,
                target_height,
                rotation: app.rotation,
                priority: Priority::Visible,
                kind: RenderKind::Page,
            }),
        ))
    });
    let Some((request_id, command)) = request else {
        return;
    };
    window.set_print_preview_has_image(false);
    if !send_render_command(app, scheduler, command) {
        PRINT_PREVIEW.with(|preview| {
            let mut preview = preview.borrow_mut();
            if preview
                .as_ref()
                .and_then(|preview| preview.pending)
                .is_some_and(|pending| pending.request_id == request_id)
            {
                if let Some(preview) = preview.as_mut() {
                    preview.pending = None;
                }
            }
        });
    }
}

pub(super) fn consume_print_preview_render(
    request_id: RequestId,
    document_id: DocumentId,
    generation: u64,
    page_index: PageIndex,
) -> Option<bool> {
    if request_id.get() & PRINT_PREVIEW_REQUEST_MASK == 0 {
        return None;
    }
    Some(PRINT_PREVIEW.with(|preview| {
        preview.borrow_mut().as_mut().is_some_and(|preview| {
            preview.accept_render(request_id, document_id, generation, page_index)
        })
    }))
}

pub(super) fn requeue_print_preview_for_generation(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    let document_id = app.active_document();
    let should_close = PRINT_PREVIEW.with(|preview| {
        let mut should_close = false;
        if let Some(preview) = preview.borrow_mut().as_mut() {
            if preview.open && Some(preview.document_id) == document_id {
                preview.generation = app.generation;
                preview.pending = None;
            } else if preview.open {
                should_close = true;
            }
        }
        should_close
    });
    if should_close {
        close_print_preview(window);
        return;
    }
    request_print_preview_render(app, scheduler, window);
}

fn close_print_preview(window: &AppWindow) {
    PRINT_PREVIEW.with(|preview| {
        if let Some(preview) = preview.borrow_mut().as_mut() {
            preview.close();
        }
        *preview.borrow_mut() = None;
    });
    window.set_print_preview_open(false);
    window.set_print_preview_has_image(false);
    window.set_print_preview_image(Image::default());
}

fn connect_print_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    controller: Option<Rc<RefCell<PrintController>>>,
) {
    let weak = window.as_weak();
    let state_print = state.clone();
    let controller_request = controller.clone();
    let scheduler_print = scheduler.clone();
    window.on_request_print(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let language = window_language(&window);
        let Some(_controller) = controller_request.as_ref() else {
            show_banner(
                &window,
                barepdf_i18n::t(language, "print.unavailable"),
                false,
            );
            return;
        };
        let mut app = state_print.borrow_mut();
        let Some(document) = app.application.ready_document() else {
            show_banner(
                &window,
                barepdf_i18n::t(language, "print.open_document"),
                false,
            );
            return;
        };
        let preview = PrintPreviewState::open(
            document.id(),
            app.generation,
            document.page_count(),
            PageIndex::from_raw(app.current_page),
        );
        window.set_print_preview_page(i32::try_from(preview.page_index.get()).unwrap_or(i32::MAX));
        window.set_print_preview_range(SharedString::from(preview.range.clone()));
        window.set_print_preview_orientation(preview.orientation);
        window.set_print_preview_has_image(false);
        window.set_print_preview_image(Image::default());
        window.set_print_preview_open(true);
        PRINT_PREVIEW.with(|state| *state.borrow_mut() = Some(preview));
        request_print_preview_render(&mut app, &scheduler_print, &window);
    });

    let weak = window.as_weak();
    let state_page = state.clone();
    let scheduler_page = scheduler.clone();
    window.on_request_print_preview_page(move |page| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let page = PRINT_PREVIEW.with(|preview| {
            preview
                .borrow_mut()
                .as_mut()
                .filter(|preview| preview.open)
                .map(|preview| preview.set_page(page))
        });
        let Some(page) = page else {
            return;
        };
        window.set_print_preview_page(i32::try_from(page.get()).unwrap_or(i32::MAX));
        request_print_preview_render(&mut state_page.borrow_mut(), &scheduler_page, &window);
    });

    window.on_request_print_preview_range(move |range| {
        PRINT_PREVIEW.with(|preview| {
            if let Some(preview) = preview.borrow_mut().as_mut() {
                if preview.open {
                    preview.range = range.to_string();
                }
            }
        });
    });

    window.on_request_print_preview_orientation(move |orientation| {
        PRINT_PREVIEW.with(|preview| {
            if let Some(preview) = preview.borrow_mut().as_mut() {
                if preview.open {
                    preview.orientation = orientation.clamp(0, 2);
                }
            }
        });
    });

    let weak = window.as_weak();
    window.on_request_close_print_preview(move || {
        if let Some(window) = weak.upgrade() {
            close_print_preview(&window);
        }
    });

    let weak = window.as_weak();
    let state_confirm = state.clone();
    let controller_confirm = controller.clone();
    window.on_request_confirm_print(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let language = window_language(&window);
        let Some(controller) = controller_confirm.as_ref() else {
            show_banner(
                &window,
                barepdf_i18n::t(language, "print.unavailable"),
                false,
            );
            return;
        };
        let preview_target = PRINT_PREVIEW.with(|preview| {
            preview
                .borrow()
                .as_ref()
                .filter(|preview| preview.open)
                .map(|preview| {
                    (
                        preview.document_id,
                        preview.generation,
                        preview.page_count,
                        preview.orientation,
                        preview.range.clone(),
                    )
                })
        });
        let Some((document_id, generation, page_count, orientation, range_input)) = preview_target
        else {
            return;
        };
        let target = {
            let app = state_confirm.borrow();
            app.application
                .ready_document()
                .filter(|document| document.id() == document_id && app.generation == generation)
                .map(|document| {
                    let path = document.path().to_path_buf();
                    let title = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(barepdf_i18n::t(language, "print.default_document"))
                        .to_string();
                    (path, title)
                })
        };
        let Some((path, title)) = target else {
            close_print_preview(&window);
            return;
        };
        let Some((first, last)) = parse_print_preview_range(&range_input, page_count) else {
            show_banner(
                &window,
                barepdf_i18n::t(language, "print.start_failed"),
                false,
            );
            return;
        };
        let Ok(range) = PrintRange::new(first, last, page_count) else {
            show_banner(
                &window,
                barepdf_i18n::t(language, "print.start_failed"),
                false,
            );
            return;
        };
        let job_id = match controller.borrow_mut().reserve_job() {
            Ok(job_id) => job_id,
            Err(PrintControllerError::Busy) => {
                show_banner(&window, barepdf_i18n::t(language, "print.busy"), false);
                return;
            }
            Err(_) => {
                show_banner(
                    &window,
                    barepdf_i18n::t(language, "print.start_failed"),
                    false,
                );
                return;
            }
        };
        let Some(hwnd) = native_window_handle(&window) else {
            controller.borrow_mut().release_reservation(job_id);
            show_banner(
                &window,
                barepdf_i18n::t(language, "print.dialog_unavailable"),
                false,
            );
            return;
        };
        let selection = WindowsPrinterDialog::new(hwnd as _).select_with_defaults(
            job_id,
            page_count,
            range,
            orientation,
        );
        let selection = match selection {
            Ok(Some(selection)) => selection,
            Ok(None) => {
                controller.borrow_mut().release_reservation(job_id);
                return;
            }
            Err(_) => {
                controller.borrow_mut().release_reservation(job_id);
                show_banner(
                    &window,
                    barepdf_i18n::t(language, "print.dialog_failed"),
                    false,
                );
                return;
            }
        };
        close_print_preview(&window);
        match controller.borrow_mut().submit(
            job_id,
            path,
            title,
            selection.range,
            selection.copies,
            Box::new(selection.sink),
        ) {
            Ok(()) => {
                window.set_print_active(true);
                window.set_print_progress(0.0);
                window.set_print_status(SharedString::from(barepdf_i18n::t(
                    language,
                    "print.status.preparing",
                )));
                state_confirm.borrow_mut().wake_pump();
            }
            Err(_) => {
                show_banner(
                    &window,
                    barepdf_i18n::t(language, "print.queue_failed"),
                    false,
                );
            }
        }
    });

    let weak = window.as_weak();
    let state_cancel = state.clone();
    window.on_request_cancel_print(move || {
        let Some(controller) = controller.as_ref() else {
            return;
        };
        if controller.borrow().cancel() {
            if let Some(window) = weak.upgrade() {
                window.set_print_status(SharedString::from(barepdf_i18n::t(
                    window_language(&window),
                    "print.status.cancelling",
                )));
                state_cancel.borrow_mut().wake_pump();
            }
        }
    });
}

pub(super) fn handle_print_event(event: PrintEvent, window: &AppWindow) {
    match event {
        PrintEvent::Progress {
            completed, total, ..
        } => {
            let progress = if total == 0 {
                0.0
            } else {
                completed as f32 / total as f32
            };
            window.set_print_progress(progress);
            window.set_print_status(SharedString::from(format!(
                "{} {completed} / {total}…",
                barepdf_i18n::t(window_language(window), "print.status.progress")
            )));
        }
        PrintEvent::Finished { .. } => {
            window.set_print_active(false);
            window.set_print_progress(1.0);
            window.set_print_status(SharedString::from(barepdf_i18n::t(
                window_language(window),
                "print.status.complete",
            )));
        }
        PrintEvent::Cancelled { .. } => {
            window.set_print_active(false);
            window.set_print_progress(0.0);
            window.set_print_status(SharedString::from(barepdf_i18n::t(
                window_language(window),
                "print.status.cancelled",
            )));
        }
        PrintEvent::Failed { message, .. } => {
            window.set_print_active(false);
            window.set_print_progress(0.0);
            drop(message);
            let message = barepdf_i18n::t(window_language(window), "print.status.failed");
            window.set_print_status(SharedString::from(message));
            show_banner(window, message, false);
        }
    }
}

fn window_language(window: &AppWindow) -> ResolvedLanguage {
    match window.get_current_language() {
        1 => Language::English.resolve(),
        2 => Language::Turkish.resolve(),
        _ => Language::System.resolve(),
    }
}

fn connect_tab_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let weak = window.as_weak();
    let state_activate = state.clone();
    let scheduler_activate = scheduler.clone();
    window.on_request_activate_tab(move |raw_id| {
        let Ok(raw_id) = u64::try_from(raw_id) else {
            return;
        };
        let Some(window) = weak.upgrade() else {
            return;
        };
        let path = {
            let mut app = state_activate.borrow_mut();
            let Some(id) = app.application.tabs.find_id(raw_id) else {
                return;
            };
            if app.application.tabs.active_id() == Some(id) {
                return;
            }
            snapshot_active_view(&mut app, &window);
            let previous_document = app.active_document();
            app.generation = scheduler_activate.bump_generation();
            close_worker_document(&mut app, &scheduler_activate, previous_document);
            clear_document_transients(&mut app, &window);
            if !app.application.tabs.activate(id) {
                return;
            }
            restore_active_view(&mut app, &window);
            app.application.tabs.path(id).map(Path::to_path_buf)
        };
        if let Some(path) = path {
            if path.is_file() {
                begin_open(path, None, &state_activate, &scheduler_activate, &window);
            } else {
                let mut app = state_activate.borrow_mut();
                if let Some(document) = app.active_document() {
                    app.page_images.remove_document(document);
                    app.thumbnail_images.remove_document(document);
                    app.text_geometries.remove_document(document);
                }
                DocumentController::fail_active_path(&mut app.application, path);
                reset_empty_document(&mut app, &window);
                show_banner(&window, "This PDF no longer exists.", true);
            }
        } else {
            reset_empty_document(&mut state_activate.borrow_mut(), &window);
        }
        refresh_tab_model(&state_activate.borrow(), &window);
    });

    let weak = window.as_weak();
    let state_close = state.clone();
    let scheduler_close = scheduler.clone();
    window.on_request_close_tab(move |raw_id| {
        let Ok(raw_id) = u64::try_from(raw_id) else {
            return;
        };
        let Some(window) = weak.upgrade() else {
            return;
        };
        let path = {
            let mut app = state_close.borrow_mut();
            let Some(id) = app.application.tabs.find_id(raw_id) else {
                return;
            };
            let was_active = app.application.tabs.active_id() == Some(id);
            if was_active {
                snapshot_active_view(&mut app, &window);
                let previous_document = app.active_document();
                app.generation = scheduler_close.bump_generation();
                close_worker_document(&mut app, &scheduler_close, previous_document);
                clear_document_transients(&mut app, &window);
            }
            if !app.application.tabs.close(id) {
                return;
            }
            if was_active {
                restore_active_view(&mut app, &window);
            }
            app.application
                .tabs
                .active()
                .and_then(|tab| tab.path.clone())
                .filter(|_| was_active)
        };
        if let Some(path) = path {
            begin_open(path, None, &state_close, &scheduler_close, &window);
        } else if state_close.borrow().application.tabs.active_id().is_none() {
            reset_empty_document(&mut state_close.borrow_mut(), &window);
        }
        refresh_tab_model(&state_close.borrow(), &window);
    });

    let weak = window.as_weak();
    let state_new = state.clone();
    let scheduler_new = scheduler.clone();
    window.on_request_new_tab(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_new.borrow_mut();
        snapshot_active_view(&mut app, &window);
        let previous_document = app.active_document();
        app.generation = scheduler_new.bump_generation();
        close_worker_document(&mut app, &scheduler_new, previous_document);
        clear_document_transients(&mut app, &window);
        if app.application.tabs.new_empty().is_none() {
            show_banner(
                &window,
                format!("A maximum of {MAX_OPEN_TABS} tabs can be open."),
                false,
            );
            return;
        }
        restore_active_view(&mut app, &window);
        reset_empty_document(&mut app, &window);
        refresh_tab_model(&app, &window);
    });
}

pub(super) fn snapshot_active_view(app: &mut AppState, window: &AppWindow) {
    let view = crate::application::ViewState {
        current_page: PageIndex::from_raw(app.current_page),
        zoom_mode: app.zoom_mode,
        zoom_factor: app.zoom_factor,
        rotation: app.rotation,
        scroll_y: window.get_current_scroll_y(),
        sidebar_visible: window.get_sidebar_visible(),
        sidebar_tab: window.get_sidebar_tab(),
    };
    if let Some(tab) = app.application.tabs.active_mut() {
        tab.view = view;
    }
}

pub(super) fn restore_active_view(app: &mut AppState, window: &AppWindow) {
    let Some(view) = app.application.tabs.active().map(|tab| tab.view.clone()) else {
        return;
    };
    app.current_page = view.current_page.get();
    app.zoom_mode = view.zoom_mode;
    app.zoom_factor = view.zoom_factor;
    app.rotation = view.rotation;
    app.last_scroll_y = view.scroll_y;
    window.set_current_scroll_y(view.scroll_y);
    window.set_sidebar_visible(view.sidebar_visible);
    window.set_sidebar_tab(view.sidebar_tab);
    update_zoom_ui(window, app.zoom_mode, app.zoom_factor);
}

fn reset_empty_document(app: &mut AppState, window: &AppWindow) {
    app.current_page = 0;
    app.visible_page_indices.clear();
    app.page_dimensions.clear();
    app.selection = None;
    window.set_has_document(false);
    window.set_password_required(false);
    window.set_has_selection(false);
    window.set_document_title(SharedString::default());
    window.set_total_pages_str(SharedString::from("0"));
    window.set_page_bitmap(Image::default());
    window.set_visible_pages(ModelRc::new(VecModel::default()));
    window.set_thumbnail_items(ModelRc::new(VecModel::default()));
}

pub(super) fn close_worker_document(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    document: Option<DocumentId>,
) {
    if let Some(document) = document {
        send_render_command(app, scheduler, RenderCommand::CloseDocument(document));
    }
}

pub(super) fn clear_document_transients(app: &mut AppState, window: &AppWindow) {
    close_print_preview(window);
    app.selection = None;
    app.is_selecting = false;
    app.outline.clear();
    app.outline_requested = false;
    app.expanded_outline.clear();
    app.flat_outline.clear();
    app.visible_page_indices.clear();
    window.set_has_selection(false);
    window.set_password_required(false);
    window.set_password_error(SharedString::default());
    window.set_outline_items(ModelRc::new(VecModel::default()));
    window.set_visible_pages(ModelRc::new(VecModel::default()));
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
                                (app.current_page + 1).min(app.page_count().saturating_sub(1))
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

fn connect_zoom_callbacks(
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
            app.zoom_factor = app.zoom_factor.zoom_in();
            app.zoom_mode = ZoomMode::Custom(app.zoom_factor);
            save_zoom_preference(&mut app);
            invalidate_layout_and_render(&mut app, &scheduler_in, &window, true);
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
            app.zoom_factor = app.zoom_factor.zoom_out();
            app.zoom_mode = ZoomMode::Custom(app.zoom_factor);
            save_zoom_preference(&mut app);
            invalidate_layout_and_render(&mut app, &scheduler_out, &window, true);
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
        app.zoom_factor = ZoomFactor::new(percent as f32 / 100.0);
        app.zoom_mode = ZoomMode::Custom(app.zoom_factor);
        save_zoom_preference(&mut app);
        invalidate_layout_and_render(&mut app, &scheduler_set, &window, true);
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
                invalidate_layout_and_render(&mut app, &scheduler, &window, true);
                window.set_zoom_mode(zoom_mode_index(app.zoom_mode));
            }
        }),
    );
}

fn parse_zoom_percent(input: &str) -> Option<i32> {
    let input = input.trim();
    let value = input
        .strip_suffix('%')
        .map_or(input, |without_percent| without_percent.trim());
    value
        .parse::<i32>()
        .ok()
        .map(|percent| percent.clamp(25, 200))
}

fn connect_view_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    _preferences_path: &Path,
) {
    let weak = window.as_weak();
    let state_view = state.clone();
    let scheduler_view = scheduler.clone();
    window.on_request_toggle_view_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_view.borrow_mut();
            app.viewing_mode = match app.viewing_mode {
                ViewingMode::ContinuousVertical => ViewingMode::SinglePage,
                _ => ViewingMode::ContinuousVertical,
            };
            app.preferences.viewing_mode = app.viewing_mode;
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
    let state_tab = state.clone();
    let scheduler_tab = scheduler.clone();
    window.on_request_sidebar_tab(move |tab| {
        if let Some(window) = weak.upgrade() {
            let mut app = state_tab.borrow_mut();
            if tab == 1 && !app.outline_requested {
                if let Some(document_id) = app.active_document() {
                    app.outline_requested = send_render_command(
                        &mut app,
                        &scheduler_tab,
                        RenderCommand::FetchOutline { document_id },
                    );
                }
            } else if tab == 0 {
                request_visible_thumbnails(&mut app, &scheduler_tab, &window);
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
        if let (Some(selection), Some(document)) = (app.selection, app.active_document()) {
            let geometries = app.text_geometries.in_page_order(document);
            let text = SelectionEngine::get_selected_text_in_page_order(&selection, &geometries);
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
            .and_then(|document| app.text_geometries.get(document, page).cloned());
        if let Some(geometry) = geometry.as_ref() {
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
            .map(|geometry| SelectionEngine::hit_test(geometry, pdf_x, pdf_y))
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
    window.on_pointer_up(move |_, _, _| {
        let mut app = state_up.borrow_mut();
        app.is_selecting = false;
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(app.selection.is_some_and(|selection| !selection.is_empty()));
        }
    });
}

fn refresh_merge_files(window: &AppWindow, app: &mut AppState) {
    let files = app
        .tools_merge_files
        .iter()
        .map(|path| {
            SharedString::from(
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string()),
            )
        })
        .collect::<Vec<_>>();
    let active = app
        .application
        .ready_document()
        .map(|document| (document.id(), document.path().to_path_buf()));
    let previews = app
        .tools_merge_files
        .iter()
        .map(|path| {
            active
                .as_ref()
                .filter(|(_, active_path)| active_path == path)
                .and_then(|(document, _)| {
                    app.thumbnail_images
                        .get(*document, 0, RenderKind::Thumbnail)
                })
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    window.set_merge_files(ModelRc::new(VecModel::from(files)));
    window.set_merge_first_page_images(ModelRc::new(VecModel::from(previews)));
}

fn current_tool_source(app: &AppState) -> Option<PathBuf> {
    app.tools_source_path.clone().or_else(|| {
        app.application
            .ready_document()
            .map(|document| document.path().to_path_buf())
    })
}

fn set_tool_source(app: &mut AppState, source: PathBuf) {
    if app.tools_source_path.as_ref() != Some(&source) {
        app.tools_source_path = Some(source);
        app.tool_source_token = app.tool_source_token.wrapping_add(1).max(1);
    }
}

fn tool_drop_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(PathBuf::from)
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
        })
        .collect()
}

fn selected_tool_pages(input: &str, total: u32) -> Vec<u32> {
    let Some(page_count) = PageCount::new(total) else {
        return Vec::new();
    };
    PageRangeSelection::parse(input, page_count)
        .map(|pages| pages.into_iter().map(|page| page.get() + 1).collect())
        .unwrap_or_default()
}

fn next_tool_job_key(app: &mut AppState) -> ToolJobKey {
    let id = app.next_tool_job_id;
    app.next_tool_job_id = app.next_tool_job_id.wrapping_add(1).max(1);
    ToolJobKey::new(id, app.generation, app.tool_source_token)
}

fn queue_tool_operation(
    operation: ToolOperation,
    state: &Rc<RefCell<AppState>>,
    window: &AppWindow,
) {
    let result = (|| -> Result<(), String> {
        let mut app = state.borrow_mut();
        if app.active_tool_job.is_some() {
            Err(barepdf_i18n::t(app.preferences.language.resolve(), "tools.error.busy").to_owned())
        } else {
            if app.tool_worker.is_none() {
                app.tool_worker = Some(ToolWorker::spawn().map_err(|error| error.to_string())?);
            }
            let key = next_tool_job_key(&mut app);
            let request = ToolRequest::new(key, operation);
            let Some(worker) = app.tool_worker.as_ref() else {
                return Err("PDF tool worker is unavailable".to_owned());
            };
            let cancellation = worker.submit(request).map_err(|error| error.to_string())?;
            app.active_tool_job = Some(super::state::ActiveToolJob { key, cancellation });
            app.tool_password_source = None;
            app.wake_pump();
            Ok(())
        }
    })();
    match result {
        Ok(()) => {
            window.set_tools_error(SharedString::default());
            window.set_tools_working(true);
        }
        Err(error) => window.set_tools_error(SharedString::from(error)),
    }
}

fn cancel_active_tool(state: &Rc<RefCell<AppState>>, window: &AppWindow) {
    let mut app = state.borrow_mut();
    let Some(active) = app.active_tool_job.as_ref() else {
        return;
    };
    if let Some(worker) = app.tool_worker.as_ref() {
        worker.cancel(active.key, &active.cancellation);
    }
    window.set_tool_password_prompt_open(false);
    app.tool_password_source = None;
    window.set_tools_working(false);
}

fn handle_tool_event(
    event: ToolEvent,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let key = event.key();
    let current = {
        let app = state.borrow();
        app.active_tool_job.as_ref().is_some_and(|active| {
            active.key == key && key.is_current(app.generation, app.tool_source_token)
        })
    };
    if !current {
        if event.is_terminal() {
            let mut app = state.borrow_mut();
            if app
                .active_tool_job
                .as_ref()
                .is_some_and(|active| active.key == key)
            {
                app.active_tool_job = None;
                window.set_tools_working(false);
                window.set_tool_password_prompt_open(false);
            }
        }
        return;
    }
    match event {
        ToolEvent::PasswordRequired {
            source,
            wrong_password,
            ..
        } => {
            let error = barepdf_i18n::t(
                window_language(window),
                if wrong_password {
                    "tools.password.incorrect"
                } else {
                    "tools.password.required"
                },
            );
            state.borrow_mut().tool_password_source = Some(source);
            window.set_tool_password_error(SharedString::from(error));
            window.set_tool_password_prompt_open(true);
        }
        ToolEvent::Completed { outcome, .. } => {
            state.borrow_mut().active_tool_job = None;
            state.borrow_mut().tool_password_source = None;
            window.set_tools_working(false);
            window.set_tool_password_prompt_open(false);
            window.set_tools_open(false);
            window.set_current_tool(-1);
            match outcome {
                ToolOutcome::Pdf { output } => {
                    show_banner(
                        window,
                        barepdf_i18n::t(window_language(window), "tools.status.success"),
                        false,
                    );
                    begin_open(output, None, state, scheduler, window);
                }
                ToolOutcome::Split {
                    output_directory,
                    file_count,
                } => show_banner(
                    window,
                    format!(
                        "Created {file_count} PDF files in {}.",
                        output_directory.display()
                    ),
                    false,
                ),
                ToolOutcome::Conversion(report) => show_banner(
                    window,
                    format!(
                        "Converted {} file(s) in {}.",
                        report.files.len(),
                        report.output_directory.display()
                    ),
                    false,
                ),
                #[cfg(test)]
                ToolOutcome::Test => {}
            }
        }
        ToolEvent::Cancelled { .. } => {
            state.borrow_mut().active_tool_job = None;
            state.borrow_mut().tool_password_source = None;
            window.set_tools_working(false);
            window.set_tool_password_prompt_open(false);
        }
        ToolEvent::Failed { message, .. } => {
            state.borrow_mut().active_tool_job = None;
            state.borrow_mut().tool_password_source = None;
            window.set_tools_working(false);
            window.set_tool_password_prompt_open(false);
            window.set_tools_error(SharedString::from(message));
        }
    }
}

fn ensure_tool_event_timer(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    if state.borrow().tool_event_timer.is_some() {
        return;
    }
    let timer = Rc::new(Timer::default());
    let weak = window.as_weak();
    let state_for_timer = state.clone();
    let scheduler_for_timer = scheduler.clone();
    let timer_for_callback = timer.clone();
    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let event = state_for_timer
            .borrow()
            .tool_worker
            .as_ref()
            .and_then(ToolWorker::try_recv_event);
        if let Some(event) = event {
            handle_tool_event(event, &window, &state_for_timer, &scheduler_for_timer);
        }
        let interval = if state_for_timer.borrow().active_tool_job.is_some() {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(250)
        };
        timer_for_callback.set_interval(interval);
    });
    state.borrow_mut().tool_event_timer = Some(timer);
}

fn connect_tools_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    dialogs: Arc<WindowsFileDialogs>,
) {
    ensure_tool_event_timer(window, state, scheduler);
    let weak = window.as_weak();
    let state_toggle_tools = state.clone();
    window.on_request_toggle_tools(move || {
        if let Some(window) = weak.upgrade() {
            let next_open = !window.get_tools_open();
            if !next_open {
                cancel_active_tool(&state_toggle_tools, &window);
            }
            window.set_tools_open(next_open);
            if next_open {
                window.set_current_tool(-1);
                window.set_tools_error(SharedString::new());
                window.set_tools_working(false);
            }
        }
    });

    let weak = window.as_weak();
    let state_open_tool = state.clone();
    window.on_request_open_tool(move |tool_id| {
        if let Some(window) = weak.upgrade() {
            {
                let mut app = state_open_tool.borrow_mut();
                if app.active_tool_job.is_none() {
                    app.tools_source_path = None;
                    app.tool_source_token = app.tool_source_token.wrapping_add(1).max(1);
                }
            }
            window.set_current_tool(tool_id);
            window.set_tools_error(SharedString::new());
            window.set_tools_working(false);

            if tool_id == 0 {
                let mut app = state_open_tool.borrow_mut();
                refresh_merge_files(&window, &mut app);
                window.set_selected_merge_index(-1);
            } else if tool_id == 1 || tool_id == 2 || tool_id == 3 {
                let app = state_open_tool.borrow();
                let has_doc = app.application.ready_document().is_some();
                if has_doc {
                    let page_num = app.current_page + 1;
                    window.set_tools_page_range(SharedString::from(page_num.to_string()));
                } else {
                    window.set_tools_page_range(SharedString::new());
                }
                window.set_tools_split_mode(0);
                window.set_tools_rotation(1);
            }
        }
    });

    let weak = window.as_weak();
    let state_close_tools = state.clone();
    window.on_request_close_tools(move || {
        if let Some(window) = weak.upgrade() {
            cancel_active_tool(&state_close_tools, &window);
            window.set_tools_open(false);
            window.set_current_tool(-1);
            window.set_tools_error(SharedString::new());
            window.set_tools_working(false);
        }
    });

    let weak = window.as_weak();
    let state_merge_add = state.clone();
    let dialogs_merge_add = dialogs.clone();
    window.on_request_merge_add_files(move || {
        let picked = dialogs_merge_add.pick_multiple_files();
        if !picked.is_empty() {
            let mut app = state_merge_add.borrow_mut();
            app.tools_merge_files.extend(picked);
            if let Some(window) = weak.upgrade() {
                refresh_merge_files(&window, &mut app);
                window.set_tools_error(SharedString::new());
            }
        }
    });

    let weak = window.as_weak();
    window.on_request_merge_select_file(move |idx| {
        if let Some(window) = weak.upgrade() {
            window.set_selected_merge_index(idx);
        }
    });

    let weak = window.as_weak();
    let state_move_up = state.clone();
    window.on_request_merge_move_up(move |idx| {
        if idx > 0 {
            let idx = idx as usize;
            let mut app = state_move_up.borrow_mut();
            if idx < app.tools_merge_files.len() {
                app.tools_merge_files.swap(idx, idx - 1);
                if let Some(window) = weak.upgrade() {
                    refresh_merge_files(&window, &mut app);
                    window.set_selected_merge_index((idx - 1) as i32);
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_move_down = state.clone();
    window.on_request_merge_move_down(move |idx| {
        if idx >= 0 {
            let idx = idx as usize;
            let mut app = state_move_down.borrow_mut();
            if idx + 1 < app.tools_merge_files.len() {
                app.tools_merge_files.swap(idx, idx + 1);
                if let Some(window) = weak.upgrade() {
                    refresh_merge_files(&window, &mut app);
                    window.set_selected_merge_index((idx + 1) as i32);
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_remove = state.clone();
    window.on_request_merge_remove_file(move |idx| {
        if idx >= 0 {
            let idx = idx as usize;
            let mut app = state_remove.borrow_mut();
            if idx < app.tools_merge_files.len() {
                app.tools_merge_files.remove(idx);
                let new_selected = if app.tools_merge_files.is_empty() {
                    -1
                } else if idx >= app.tools_merge_files.len() {
                    (app.tools_merge_files.len() - 1) as i32
                } else {
                    idx as i32
                };
                if let Some(window) = weak.upgrade() {
                    refresh_merge_files(&window, &mut app);
                    window.set_selected_merge_index(new_selected);
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_clear = state.clone();
    window.on_request_merge_clear(move || {
        let mut app = state_clear.borrow_mut();
        app.tools_merge_files.clear();
        if let Some(window) = weak.upgrade() {
            refresh_merge_files(&window, &mut app);
            window.set_selected_merge_index(-1);
        }
    });

    let weak = window.as_weak();
    let state_merge_exec = state.clone();
    let dialogs_merge_exec = dialogs.clone();
    window.on_request_merge_execute(move || {
        let files = state_merge_exec.borrow().tools_merge_files.clone();
        let language = state_merge_exec.borrow().preferences.language.resolve();
        if files.len() < 2 {
            if let Some(window) = weak.upgrade() {
                window.set_tools_error(SharedString::from(barepdf_i18n::t(
                    language,
                    "tools.error.no_files",
                )));
            }
            return;
        }

        let Some(output_path) = dialogs_merge_exec.save_file("merged.pdf") else {
            return;
        };

        if let Some(window) = weak.upgrade() {
            queue_tool_operation(
                ToolOperation::Merge {
                    inputs: files,
                    output: output_path,
                },
                &state_merge_exec,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_split_exec = state.clone();
    let dialogs_split_exec = dialogs.clone();
    window.on_request_split_execute(move |range_str, mode| {
        let source_path = {
            let app = state_split_exec.borrow();
            let lang = app.preferences.language.resolve();
            let Some(source) = current_tool_source(&app) else {
                if let Some(window) = weak.upgrade() {
                    window.set_tools_error(SharedString::from(barepdf_i18n::t(
                        lang,
                        "tools.error.no_files",
                    )));
                }
                return;
            };
            let _ = lang;
            source
        };

        if mode == 0 {
            let default_name = format!(
                "{}_extracted.pdf",
                source_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "document".to_string())
            );
            let Some(output_path) = dialogs_split_exec.save_file(&default_name) else {
                return;
            };

            if let Some(window) = weak.upgrade() {
                set_tool_source(&mut state_split_exec.borrow_mut(), source_path.clone());
                queue_tool_operation(
                    ToolOperation::Extract {
                        source: source_path,
                        range: range_str.to_string(),
                        output: output_path,
                    },
                    &state_split_exec,
                    &window,
                );
            }
        } else {
            let Some(output_dir) = dialogs_split_exec.pick_directory() else {
                return;
            };

            let base_name = source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".to_string());

            if let Some(window) = weak.upgrade() {
                set_tool_source(&mut state_split_exec.borrow_mut(), source_path.clone());
                queue_tool_operation(
                    ToolOperation::SplitAll {
                        source: source_path,
                        output_parent: output_dir,
                        base_name,
                    },
                    &state_split_exec,
                    &window,
                );
            }
        }
    });

    let weak = window.as_weak();
    let state_del_exec = state.clone();
    let dialogs_del_exec = dialogs.clone();
    window.on_request_delete_pages_execute(move |range_str| {
        let source_path = {
            let app = state_del_exec.borrow();
            let lang = app.preferences.language.resolve();
            let Some(source) = current_tool_source(&app) else {
                if let Some(window) = weak.upgrade() {
                    window.set_tools_error(SharedString::from(barepdf_i18n::t(
                        lang,
                        "tools.error.no_files",
                    )));
                }
                return;
            };
            source
        };

        let default_name = format!(
            "{}_modified.pdf",
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".to_string())
        );
        let Some(output_path) = dialogs_del_exec.save_file(&default_name) else {
            return;
        };

        if let Some(window) = weak.upgrade() {
            set_tool_source(&mut state_del_exec.borrow_mut(), source_path.clone());
            queue_tool_operation(
                ToolOperation::Delete {
                    source: source_path,
                    range: range_str.to_string(),
                    output: output_path,
                },
                &state_del_exec,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_rot_exec = state.clone();
    let dialogs_rot_exec = dialogs.clone();
    window.on_request_rotate_pages_execute(move |range_str, rot_val| {
        let source_path = {
            let app = state_rot_exec.borrow();
            let lang = app.preferences.language.resolve();
            let Some(source) = current_tool_source(&app) else {
                if let Some(window) = weak.upgrade() {
                    window.set_tools_error(SharedString::from(barepdf_i18n::t(
                        lang,
                        "tools.error.no_files",
                    )));
                }
                return;
            };
            source
        };

        let target_rot = match rot_val {
            1 => Rotation::Degrees90,
            2 => Rotation::Degrees180,
            3 => Rotation::Degrees270,
            _ => Rotation::Degrees90,
        };

        let default_name = format!(
            "{}_rotated.pdf",
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".to_string())
        );
        let Some(output_path) = dialogs_rot_exec.save_file(&default_name) else {
            return;
        };

        if let Some(window) = weak.upgrade() {
            set_tool_source(&mut state_rot_exec.borrow_mut(), source_path.clone());
            queue_tool_operation(
                ToolOperation::Rotate {
                    source: source_path,
                    range: range_str.to_string(),
                    rotation: target_rot,
                    output: output_path,
                },
                &state_rot_exec,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_reorder = state.clone();
    window.on_request_merge_reorder(move |from, to| {
        if from < 0 || to < 0 {
            return;
        }
        let mut app = state_reorder.borrow_mut();
        let (from, to) = (from as usize, to as usize);
        if from >= app.tools_merge_files.len() || to >= app.tools_merge_files.len() || from == to {
            return;
        }
        let moved = app.tools_merge_files.remove(from);
        app.tools_merge_files.insert(to, moved);
        if let Some(window) = weak.upgrade() {
            refresh_merge_files(&window, &mut app);
            window.set_selected_merge_index(to as i32);
        }
    });

    let weak = window.as_weak();
    let state_drop = state.clone();
    window.on_request_tool_drop(move |transfer, tool_id| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let paths = transfer
            .plain_text()
            .ok()
            .map_or_else(Vec::new, |text| tool_drop_paths(text.as_str()));
        if paths.is_empty() {
            window.set_tools_error(SharedString::from(barepdf_i18n::t(
                window_language(&window),
                "tools.error.drop_pdf",
            )));
            return;
        }
        let mut app = state_drop.borrow_mut();
        if app.active_tool_job.is_some() {
            window.set_tools_error(SharedString::from(barepdf_i18n::t(
                window_language(&window),
                "tools.error.busy",
            )));
            return;
        }
        if tool_id == 0 {
            app.tools_merge_files.extend(paths);
            refresh_merge_files(&window, &mut app);
            window.set_tools_error(SharedString::default());
        } else if let [source] = paths.as_slice() {
            set_tool_source(&mut app, source.clone());
            window.set_tools_error(SharedString::default());
        } else {
            window.set_tools_error(SharedString::from(barepdf_i18n::t(
                window_language(&window),
                "tools.error.single_source",
            )));
        }
    });

    let weak = window.as_weak();
    let state_selection = state.clone();
    window.on_request_select_page_range(move |page, ctrl, shift| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_selection.borrow_mut();
        let page = u32::try_from(page).unwrap_or(0).saturating_add(1);
        let total = app.page_count();
        if page == 0 || page > total {
            return;
        }
        let existing = window.get_tools_page_range().to_string();
        let range = if shift {
            let anchor = existing
                .split([',', '-'])
                .find_map(|item| item.trim().parse::<u32>().ok())
                .unwrap_or(page);
            format!("{}-{}", anchor.min(page), anchor.max(page))
        } else if ctrl {
            let mut pages = selected_tool_pages(&existing, total);
            if let Some(index) = pages.iter().position(|selected| *selected == page) {
                pages.remove(index);
            } else {
                pages.push(page);
                pages.sort_unstable();
            }
            pages
                .into_iter()
                .map(|selected| selected.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            page.to_string()
        };
        window.set_tools_page_range(SharedString::from(range));
        app.wake_pump();
    });

    let weak = window.as_weak();
    let state_convert = state.clone();
    let dialogs_convert = dialogs.clone();
    window.on_request_convert_execute(move |format, dpi, _quality, range| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let format = match format {
            0 => ConversionFormat::Text,
            1 => ConversionFormat::Markdown,
            2 => ConversionFormat::Png,
            3 => ConversionFormat::Jpeg,
            _ => {
                window.set_tools_error(SharedString::from(barepdf_i18n::t(
                    window_language(&window),
                    "tools.error.format",
                )));
                return;
            }
        };
        let dpi = match dpi {
            150 => ConversionDpi::Dpi150,
            300 => ConversionDpi::Dpi300,
            _ => {
                window.set_tools_error(SharedString::from(barepdf_i18n::t(
                    window_language(&window),
                    "tools.error.resolution",
                )));
                return;
            }
        };
        let source = {
            let app = state_convert.borrow();
            current_tool_source(&app)
        };
        let Some(source) = source else {
            window.set_tools_error(SharedString::from(barepdf_i18n::t(
                window_language(&window),
                "tools.error.source",
            )));
            return;
        };
        let Some(output_parent) = dialogs_convert.pick_directory() else {
            return;
        };
        set_tool_source(&mut state_convert.borrow_mut(), source.clone());
        queue_tool_operation(
            ToolOperation::Convert {
                source,
                output_parent,
                range: range.to_string(),
                format,
                dpi,
            },
            &state_convert,
            &window,
        );
    });

    let weak = window.as_weak();
    let state_password = state.clone();
    window.on_request_submit_tool_password(move |password| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if password.len() > MAX_PASSWORD_BYTES {
            window.set_tool_password_error(SharedString::from(barepdf_i18n::t(
                window_language(&window),
                "password.error.too_long",
            )));
            return;
        }
        let password = password.to_string();
        let result = {
            let app = state_password.borrow();
            let Some(active) = app.active_tool_job.as_ref() else {
                return;
            };
            let Some(source) = app
                .tool_password_source
                .clone()
                .or_else(|| app.tools_source_path.clone())
            else {
                return;
            };
            let Some(worker) = app.tool_worker.as_ref() else {
                return;
            };
            worker.provide_password(active.key, source, password)
        };
        match result {
            Ok(()) => {
                window.set_tool_password_error(SharedString::default());
                window.set_tool_password_prompt_open(false);
            }
            Err(error) => window.set_tool_password_error(SharedString::from(error.to_string())),
        }
    });

    let weak = window.as_weak();
    let state_cancel_password = state.clone();
    window.on_request_cancel_tool_password(move || {
        if let Some(window) = weak.upgrade() {
            cancel_active_tool(&state_cancel_password, &window);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        parse_print_preview_range, parse_zoom_percent, print_preview_dimensions,
        selected_tool_pages, tool_drop_paths, PrintPreviewState, PRINT_PREVIEW_REQUEST_MASK,
    };
    use barepdf_core::{DocumentId, PageCount, PageIndex, RequestId, Rotation};
    use std::path::PathBuf;

    fn page_count(value: u32) -> PageCount {
        PageCount::new(value).expect("test page count")
    }

    #[test]
    fn print_preview_accepts_only_the_latest_matching_render_once() {
        let document = DocumentId::new(7);
        let mut preview = PrintPreviewState::open(document, 11, page_count(4), PageIndex::zero());
        let first = RequestId::new(PRINT_PREVIEW_REQUEST_MASK | 1);
        let latest = RequestId::new(PRINT_PREVIEW_REQUEST_MASK | 2);
        preview.expect_render(first, PageIndex::zero());
        preview.set_page(1);
        preview.expect_render(latest, PageIndex::from_raw(1));

        assert!(!preview.accept_render(first, document, 11, PageIndex::zero()));
        assert!(!preview.accept_render(latest, DocumentId::new(8), 11, PageIndex::from_raw(1)));
        assert!(!preview.accept_render(latest, document, 12, PageIndex::from_raw(1)));
        assert!(preview.accept_render(latest, document, 11, PageIndex::from_raw(1)));
        assert!(!preview.accept_render(latest, document, 11, PageIndex::from_raw(1)));
    }

    #[test]
    fn closing_print_preview_rejects_an_in_flight_render() {
        let document = DocumentId::new(7);
        let mut preview = PrintPreviewState::open(document, 11, page_count(2), PageIndex::zero());
        let request = RequestId::new(PRINT_PREVIEW_REQUEST_MASK | 3);
        preview.expect_render(request, PageIndex::zero());

        preview.close();

        assert!(!preview.accept_render(request, document, 11, PageIndex::zero()));
    }

    #[test]
    fn print_preview_range_is_contiguous_and_bounded() {
        assert_eq!(
            parse_print_preview_range("", page_count(5)),
            Some((PageIndex::zero(), PageIndex::from_raw(4)))
        );
        assert_eq!(
            parse_print_preview_range("2-4", page_count(5)),
            Some((PageIndex::from_raw(1), PageIndex::from_raw(3)))
        );
        assert_eq!(
            parse_print_preview_range("3", page_count(5)),
            Some((PageIndex::from_raw(2), PageIndex::from_raw(2)))
        );
        assert_eq!(
            parse_print_preview_range("1-2, 3-4", page_count(5)),
            Some((PageIndex::zero(), PageIndex::from_raw(3)))
        );
        assert_eq!(parse_print_preview_range("4-2", page_count(5)), None);
        assert_eq!(parse_print_preview_range("1,3", page_count(5)), None);
        assert_eq!(parse_print_preview_range("1-3,5", page_count(5)), None);
        assert_eq!(parse_print_preview_range("6", page_count(5)), None);
    }

    #[test]
    fn print_preview_bitmap_fits_the_existing_thumbnail_budget() {
        let (width, height) = print_preview_dimensions((2_000.0, 1_000.0), Rotation::Degrees90);
        let bytes = u64::from(width) * u64::from(height) * 4;

        assert_eq!((width, height), (480, 960));
        assert!(bytes <= super::super::ui::THUMB_IMAGE_BUDGET as u64);
    }

    #[test]
    fn parses_and_clamps_integer_zoom_percentages() {
        for (input, expected) in [
            ("125", Some(125)),
            (" 125% ", Some(125)),
            ("125 %", Some(125)),
            ("+125", Some(125)),
            ("0", Some(25)),
            ("-25", Some(25)),
            ("250", Some(200)),
        ] {
            assert_eq!(parse_zoom_percent(input), expected, "{input}");
        }
    }

    #[test]
    fn rejects_non_integer_or_malformed_zoom_percentages() {
        for input in ["", "%", "125.0", "125%%", "abc", "+ 125"] {
            assert_eq!(parse_zoom_percent(input), None, "{input}");
        }
    }

    #[test]
    fn tool_drop_uses_only_pdf_sources_without_opening_them() {
        assert_eq!(
            tool_drop_paths("C:/docs/one.pdf\nC:/docs/two.PDF\nC:/docs/note.txt"),
            vec![
                PathBuf::from("C:/docs/one.pdf"),
                PathBuf::from("C:/docs/two.PDF")
            ]
        );
    }

    #[test]
    fn ctrl_selection_expands_existing_ranges_before_toggling_a_page() {
        assert_eq!(selected_tool_pages("1-3, 5", 5), vec![1, 2, 3, 5]);
        assert!(selected_tool_pages("6", 5).is_empty());
    }
}
