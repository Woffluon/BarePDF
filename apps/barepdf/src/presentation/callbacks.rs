#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::map_unwrap_or,
    clippy::redundant_closure_for_method_calls,
    clippy::semicolon_if_nothing_returned,
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
    Rotation, SecretPassword, TextPosition, TextSelection, ViewingMode, WindowMode, ZoomFactor,
    ZoomMode, MAX_OPEN_TABS, MAX_PASSWORD_BYTES,
};
use barepdf_i18n::{Language, ResolvedLanguage};
use barepdf_pdf::conversion::{ConversionDpi, ConversionFormat};
use barepdf_platform::printing::{
    Copies, InstalledPrinter, PrintDuplex, PrintError, PrintJobId, PrintOrientation, PrintPage,
    PrintRange, PrinterSink,
};
use barepdf_platform::{ClipboardAccess, FileDialogs};
use barepdf_platform_windows::{
    enumerate_installed_printers, is_installed_build, open_url, WindowsClipboard,
    WindowsFileDialogs, WindowsPrinterSink,
};
use barepdf_render::{Priority, RenderCommand, RenderJob, RenderKind, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, Image, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::models::{
    refresh_annotation_overlays, refresh_bookmark_model, refresh_page_model, refresh_tab_model,
    refresh_thumbnail_model, refresh_tool_thumbnails, render_signature_pad_preview,
};
use super::state::{
    next_print_preview_request_id, AppState, BackgroundUiEvent, ERASER_RADII,
    PRINT_PREVIEW_MAX_EDGE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolKind {
    Merge,
    SingleSource,
}

impl ToolKind {
    #[must_use]
    pub(crate) fn from_slint_id(id: i32) -> Self {
        match id {
            0 => Self::Merge,
            _ => Self::SingleSource,
        }
    }
}
use super::ui::{
    apply_theme, begin_open, ensure_layout, invalidate_layout_and_render, navigate_to_page,
    navigate_to_page_inner, parse_drop_paths, persist_preferences, pointer_to_pdf,
    refresh_generation_bound_views, refresh_outline_model, render_visible_pages,
    request_next_dimensions_batch, request_visible_thumbnails, save_zoom_preference,
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
    connect_tools_callbacks(window, state, scheduler, dialogs.clone());
    connect_niche_feature_callbacks(window, state, scheduler, preferences_path);
    connect_annotation_and_signature_callbacks(window, state, scheduler, dialogs);
    super::window_chrome::connect_window_chrome_callbacks(window);

    let weak = window.as_weak();
    let state_password = state.clone();
    let scheduler_password = scheduler.clone();
    window.on_request_unlock_password(move |password| {
        if let Some(window) = weak.upgrade() {
            clear_document_password_ui(&window);
        }
        let Ok(mut password) = consume_ui_password(password) else {
            let language = state_password.borrow().preferences.language.resolve();
            if let Some(window) = weak.upgrade() {
                window.set_password_error(SharedString::from(barepdf_i18n::t(
                    language,
                    "password.error.too_long",
                )));
                window.set_password_required(true);
            }
            return;
        };
        let path = DocumentController::pending_path(&state_password.borrow().application)
            .map(Path::to_path_buf);
        if let (Some(path), Some(window)) = (path, weak.upgrade()) {
            begin_open(
                path,
                Some(password),
                &state_password,
                &scheduler_password,
                &window,
            );
        } else {
            password.clear();
        }
    });

    let weak = window.as_weak();
    window.on_request_cancel_unlock_password(move || {
        if let Some(window) = weak.upgrade() {
            clear_document_password_ui(&window);
            window.set_password_error(SharedString::default());
            window.set_password_required(false);
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
    if !app.print_preview.open
        || app.print_preview.document_id != Some(document_id)
        || app.print_preview.generation != app.generation
    {
        return;
    }
    let page_index = app.print_preview.page_index;
    let dimensions = app
        .page_dimensions
        .get(page_index.get() as usize)
        .copied()
        .unwrap_or(app.first_page_dimensions);
    let (target_width, target_height) = print_preview_dimensions(dimensions, app.rotation);
    let request_id = next_print_preview_request_id();
    app.print_preview.expect_render(request_id, page_index);
    let command = RenderCommand::RenderPage(RenderJob {
        request_id,
        generation: app.generation,
        document_id,
        page_index,
        target_width,
        target_height,
        rotation: app.rotation,
        priority: Priority::Visible,
        kind: RenderKind::Page,
    });
    window.set_print_preview_has_image(false);
    if !send_render_command(app, scheduler, command)
        && app
            .print_preview
            .pending
            .is_some_and(|p| p.request_id == request_id)
    {
        app.print_preview.pending = None;
    }
}

pub(super) fn requeue_print_preview_for_generation(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    let document_id = app.active_document();
    if app.print_preview.open && app.print_preview.document_id == document_id {
        app.print_preview.generation = app.generation;
        app.print_preview.pending = None;
    } else if app.print_preview.open {
        close_print_preview(app, window);
        return;
    }
    request_print_preview_render(app, scheduler, window);
}

fn close_print_preview(app: &mut AppState, window: &AppWindow) {
    app.print_preview.close();
    window.set_print_preview_open(false);
    window.set_print_preview_has_image(false);
    window.set_print_preview_image(Image::default());
}

fn populate_print_preview_printers(window: &AppWindow, installed: &[InstalledPrinter]) {
    let mut names = Vec::new();
    let mut default_idx = 0;
    for (i, p) in installed.iter().enumerate() {
        names.push(SharedString::from(p.name.clone()));
        if p.is_default {
            default_idx = i;
        }
    }
    if names.is_empty() {
        names.push(SharedString::from("Microsoft Print to PDF"));
    }
    window.set_print_preview_printers(ModelRc::new(VecModel::from(names)));
    window.set_print_preview_selected_printer(default_idx as i32);
}

struct DeferredPrinterSink<F> {
    job_id: PrintJobId,
    target_dpi: u16,
    factory: Option<F>,
    inner: Option<Box<dyn PrinterSink>>,
}

impl<F> DeferredPrinterSink<F>
where
    F: FnOnce() -> Result<Box<dyn PrinterSink>, PrintError> + Send,
{
    fn new(job_id: PrintJobId, target_dpi: u16, factory: F) -> Self {
        Self {
            job_id,
            target_dpi,
            factory: Some(factory),
            inner: None,
        }
    }
}

impl<F> PrinterSink for DeferredPrinterSink<F>
where
    F: FnOnce() -> Result<Box<dyn PrinterSink>, PrintError> + Send,
{
    fn job_id(&self) -> PrintJobId {
        self.inner.as_ref().map_or(self.job_id, |s| s.job_id())
    }

    fn target_dpi(&self) -> u16 {
        self.inner
            .as_ref()
            .map_or(self.target_dpi, |s| s.target_dpi())
    }

    fn begin(&mut self, title: &str) -> Result<(), PrintError> {
        if self.inner.is_some() {
            return Err(PrintError::InvalidState);
        }
        let factory = self.factory.take().ok_or(PrintError::InvalidState)?;
        let mut sink = factory()?;
        sink.begin(title)?;
        self.inner = Some(sink);
        Ok(())
    }

    fn write_page(&mut self, page: PrintPage<'_>) -> Result<(), PrintError> {
        self.inner
            .as_mut()
            .ok_or(PrintError::InvalidState)?
            .write_page(page)
    }

    fn finish(mut self: Box<Self>) -> Result<(), PrintError> {
        self.inner.take().ok_or(PrintError::InvalidState)?.finish()
    }
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
        let doc_id = document.id();
        let page_count = document.page_count();
        let generation = app.generation;
        let current_page = PageIndex::from_raw(app.current_page);
        app.print_preview
            .open(doc_id, generation, page_count, current_page);
        window.set_print_preview_page(
            i32::try_from(app.print_preview.page_index.get()).unwrap_or(i32::MAX),
        );
        window.set_print_preview_range(SharedString::from(app.print_preview.range.clone()));
        window.set_print_preview_orientation(app.print_preview.orientation);
        window.set_print_preview_duplex(app.print_preview.duplex);
        window.set_print_preview_has_image(false);
        window.set_print_preview_image(Image::default());
        window.set_print_preview_open(true);

        populate_print_preview_printers(&window, &app.cached_printers);
        if !app.in_flight.printer_enum {
            app.in_flight.printer_enum = true;
            app.spawn_background_io(|| {
                BackgroundUiEvent::PrintersEnumerated(enumerate_installed_printers())
            });
        }
        window.set_print_preview_copies(1);
        window.set_print_preview_range_mode(0);

        request_print_preview_render(&mut app, &scheduler_print, &window);
    });

    let weak = window.as_weak();
    let state_page = state.clone();
    let scheduler_page = scheduler.clone();
    window.on_request_print_preview_page(move |page| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let page = {
            let mut app = state_page.borrow_mut();
            if app.print_preview.open {
                Some(app.print_preview.set_page(page))
            } else {
                None
            }
        };
        let Some(page) = page else {
            return;
        };
        window.set_print_preview_page(i32::try_from(page.get()).unwrap_or(i32::MAX));
        request_print_preview_render(&mut state_page.borrow_mut(), &scheduler_page, &window);
    });

    let state_range = state.clone();
    window.on_request_print_preview_range(move |range| {
        let mut app = state_range.borrow_mut();
        if app.print_preview.open {
            app.print_preview.range = range.to_string();
        }
    });

    let state_orientation = state.clone();
    window.on_request_print_preview_orientation(move |orientation| {
        let mut app = state_orientation.borrow_mut();
        if app.print_preview.open {
            app.print_preview.orientation = orientation.clamp(0, 2);
        }
    });

    let state_duplex = state.clone();
    window.on_request_print_preview_duplex(move |duplex| {
        let mut app = state_duplex.borrow_mut();
        if app.print_preview.open {
            app.print_preview.duplex = duplex.clamp(0, 2);
        }
    });

    let weak = window.as_weak();
    let state_close = state.clone();
    window.on_request_close_print_preview(move || {
        if let Some(window) = weak.upgrade() {
            close_print_preview(&mut state_close.borrow_mut(), &window);
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
        let preview_target = {
            let app = state_confirm.borrow();
            if app.print_preview.open {
                app.print_preview.document_id.map(|document_id| {
                    (
                        document_id,
                        app.print_preview.generation,
                        app.print_preview.page_count,
                        app.print_preview.orientation,
                        app.print_preview.range.clone(),
                    )
                })
            } else {
                None
            }
        };
        let Some((document_id, generation, page_count, _orientation, range_input)) = preview_target
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
            close_print_preview(&mut state_confirm.borrow_mut(), &window);
            return;
        };
        let selected_printer_idx = window.get_print_preview_selected_printer().max(0) as usize;
        let printer_model = window.get_print_preview_printers();
        let printer_name = printer_model
            .row_data(selected_printer_idx)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Microsoft Print to PDF".to_string());

        let range_mode = window.get_print_preview_range_mode();
        let range = match range_mode {
            1 => {
                let cur = {
                    let app = state_confirm.borrow();
                    PageIndex::from_raw(app.current_page.min(page_count.get().saturating_sub(1)))
                };
                PrintRange::new(cur, cur, page_count)
                    .unwrap_or_else(|_| PrintRange::all(page_count))
            }
            2 => {
                let Some((first, last)) = parse_print_preview_range(&range_input, page_count)
                else {
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
                range
            }
            _ => PrintRange::all(page_count),
        };

        let copies_num = (window.get_print_preview_copies() as u16).clamp(1, 99);
        let copies = Copies::new(copies_num).unwrap_or_default();
        let orientation = PrintOrientation::from_index(window.get_print_preview_orientation());
        let duplex = PrintDuplex::from_index(window.get_print_preview_duplex());

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

        let target_dpi = 300;
        let sink = DeferredPrinterSink::new(job_id, target_dpi, move || {
            let sink = WindowsPrinterSink::direct(
                job_id,
                target_dpi,
                &printer_name,
                orientation,
                duplex,
                copies_num,
            )?;
            Ok(Box::new(sink))
        });
        close_print_preview(&mut state_confirm.borrow_mut(), &window);
        match controller
            .borrow_mut()
            .submit(job_id, path, title, range, copies, Box::new(sink))
        {
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
                controller.borrow_mut().release_reservation(job_id);
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
        let Some(window) = weak.upgrade() else {
            return;
        };
        let (path, already_ready) = {
            let mut app = state_activate.borrow_mut();
            let Some(id) = app.application.tabs.find_slint_id(raw_id) else {
                return;
            };
            if app.application.tabs.active_id() == Some(id) {
                return;
            }
            snapshot_active_view(&mut app, &window);
            app.generation = scheduler_activate.bump_generation();
            clear_document_transients(&mut app, &window);
            if !app.application.tabs.activate(id) {
                return;
            }
            restore_active_view(&mut app, &window);
            let already_ready = matches!(
                app.application
                    .tabs
                    .active()
                    .and_then(|t| t.document.as_ref()),
                Some(crate::application::DocumentState::Ready(_))
            );
            (
                app.application.tabs.path(id).map(Path::to_path_buf),
                already_ready,
            )
        };
        if already_ready {
            let mut app = state_activate.borrow_mut();
            window.set_has_document(true);
            let title = app
                .application
                .tabs
                .active()
                .map(|tab| tab.title.clone())
                .unwrap_or_default();
            window.set_document_title(SharedString::from(title));
            let page_count = app.page_count();
            window.set_total_pages_str(SharedString::from(page_count.to_string()));
            app.layout_key = None;
            ensure_layout(&mut app);
            refresh_thumbnail_model(&mut app, &window);
            refresh_page_model(&mut app, &window);
            refresh_tab_model(&app, &window);
            refresh_generation_bound_views(&mut app, &scheduler_activate, &window);
            refresh_bookmark_model(&app, &window);
            if !app.outline.is_empty() {
                refresh_outline_model(&mut app, &window);
            }
            let current_page = app.current_page;
            let active_doc = app.active_document();
            if let Some(image) = active_doc.and_then(|document| {
                app.page_images
                    .get(document, current_page, RenderKind::Page)
            }) {
                window.set_page_bitmap(image);
            }
            request_next_dimensions_batch(&mut app, &scheduler_activate);
        } else if let Some(path) = path {
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
        let Some(window) = weak.upgrade() else {
            return;
        };
        let path = {
            let mut app = state_close.borrow_mut();
            let Some(id) = app.application.tabs.find_slint_id(raw_id) else {
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
        app.generation = scheduler_new.bump_generation();
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
    app.snapshot_active_tab_layout();
    if let Some(tab) = app.application.tabs.active_mut() {
        tab.view = view;
    }
}

pub(super) fn restore_active_view(app: &mut AppState, window: &AppWindow) {
    let Some(tab) = app.application.tabs.active() else {
        return;
    };
    let view = tab.view.clone();

    app.current_page = view.current_page.get();
    app.zoom_mode = view.zoom_mode;
    app.zoom_factor = view.zoom_factor;
    app.update_cache_budget_for_zoom(app.zoom_factor);
    app.rotation = view.rotation;
    app.last_scroll_y = view.scroll_y;

    app.restore_active_tab_layout();

    window.set_current_scroll_y(view.scroll_y);
    window.set_sidebar_visible(view.sidebar_visible);
    window.set_sidebar_tab(view.sidebar_tab);
    update_zoom_ui(window, app.zoom_mode, app.zoom_factor);
}

fn consume_ui_password(raw: SharedString) -> Result<SecretPassword, ()> {
    let mut password = SecretPassword::new(raw.to_string());
    if password.expose().len() > MAX_PASSWORD_BYTES {
        password.clear();
        return Err(());
    }
    Ok(password)
}

fn clear_document_password_ui(window: &AppWindow) {
    window.set_document_password_input(SharedString::default());
}

fn clear_tool_password_ui(window: &AppWindow) {
    window.set_tool_password_input(SharedString::default());
}

fn reset_empty_document(app: &mut AppState, window: &AppWindow) {
    app.current_page = 0;
    app.visible_page_indices.clear();
    app.page_dimensions = Arc::new(Vec::new());
    app.selection = None;
    window.set_has_document(false);
    clear_document_password_ui(window);
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
    close_print_preview(app, window);
    app.selection = None;
    app.is_selecting = false;
    app.outline = Arc::new(Vec::new());
    app.in_flight.outline_requested = false;
    app.expanded_outline.clear();
    app.flat_outline.clear();
    app.visible_page_indices.clear();
    app.failed_pages.clear();
    window.set_has_selection(false);
    clear_document_password_ui(window);
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
                        let step = if app.viewing_mode == ViewingMode::TwoPageSpread {
                            2
                        } else {
                            1
                        };
                        match target {
                            NavigationTarget::Previous => app.current_page.saturating_sub(step),
                            NavigationTarget::Next => {
                                (app.current_page + step).min(app.page_count().saturating_sub(1))
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

fn parse_zoom_percent(input: &str) -> Option<i32> {
    let input = input.trim();
    let value = input
        .strip_suffix('%')
        .map_or(input, |without_percent| without_percent.trim());
    value
        .parse::<i32>()
        .ok()
        .map(|percent| percent.clamp(25, 800))
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
    window.on_request_toggle_view_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_view.borrow_mut();
            app.viewing_mode = match app.viewing_mode {
                ViewingMode::ContinuousVertical => ViewingMode::SinglePage,
                ViewingMode::SinglePage => ViewingMode::TwoPageSpread,
                _ => ViewingMode::ContinuousVertical,
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
    let state_tab = state.clone();
    let scheduler_tab = scheduler.clone();
    window.on_request_sidebar_tab(move |tab| {
        if let Some(window) = weak.upgrade() {
            let mut app = state_tab.borrow_mut();
            if tab == 1 && !app.in_flight.outline_requested {
                if let Some(document_id) = app.active_document() {
                    app.in_flight.outline_requested = send_render_command(
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
    let state_bookmark = state.clone();
    let scheduler_bookmark = scheduler.clone();
    window.on_bookmark_selected(move |index| {
        let page = {
            let app = state_bookmark.borrow();
            let mut target_page = None;
            if let Some(active_tab) = app.application.tabs.active() {
                if let Some(path) = &active_tab.path {
                    if let Some(session) =
                        app.preferences.open_tabs.iter().find(|s| &s.path == path)
                    {
                        if let Some(bookmark) = session.bookmarks.get(index as usize) {
                            target_page = Some(bookmark.page_index);
                        }
                    }
                }
            }
            target_page
        };
        if let (Some(page), Some(window)) = (page, weak.upgrade()) {
            navigate_to_page(page, &state_bookmark, &scheduler_bookmark, &window);
        }
    });

    let weak = window.as_weak();
    let state_toggle_bm = state.clone();
    let preferences_path_bm = preferences_path.to_path_buf();
    window.on_request_toggle_bookmark(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_toggle_bm.borrow_mut();
        let current_page = app.current_page;
        let active_path = app.application.tabs.active().and_then(|t| t.path.clone());
        let Some(path) = active_path else {
            return;
        };

        let zoom_mode = app.zoom_mode;
        let pos = if let Some(pos) = app
            .preferences
            .open_tabs
            .iter()
            .position(|s| s.path == path)
        {
            pos
        } else {
            app.preferences
                .open_tabs
                .push(barepdf_core::DocumentSession {
                    path: path.clone(),
                    page_index: current_page,
                    scroll_y: 0.0,
                    zoom_mode,
                    bookmarks: Vec::new(),
                });
            app.preferences.open_tabs.len() - 1
        };
        let session = &mut app.preferences.open_tabs[pos];

        let added = crate::controllers::bookmark_controller::BookmarkController::toggle_bookmark(
            &mut session.bookmarks,
            barepdf_core::types::PageIndex::from_raw(current_page),
            None,
        );

        refresh_bookmark_model(&app, &window);
        persist_preferences(&app.preferences, &preferences_path_bm, Some(&window));

        let msg = if added {
            barepdf_i18n::t(app.preferences.language.resolve(), "bookmark.added")
        } else {
            barepdf_i18n::t(app.preferences.language.resolve(), "bookmark.removed")
        };
        show_banner(&window, msg, false);
    });

    let weak = window.as_weak();
    let state_search = state.clone();
    let scheduler_search = scheduler.clone();
    window.on_request_search_query(move |query_str| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_search.borrow_mut();
        let query_text = query_str.as_str().trim();
        if query_text.is_empty() {
            app.search_query = None;
            app.search_matches.clear();
            app.active_search_match = 0;
            window.set_search_has_matches(false);
            window.set_search_match_counter(SharedString::from("0 / 0"));
            refresh_page_model(&mut app, &window);
            return;
        }

        let case_sensitive = window.get_search_case_sensitive();
        let whole_word = window.get_search_whole_word();
        let query = barepdf_core::search::SearchQuery::new(
            query_text.to_string(),
            case_sensitive,
            whole_word,
        );
        let Some(query) = query else {
            app.search_query = None;
            app.search_matches.clear();
            app.active_search_match = 0;
            window.set_search_has_matches(false);
            window.set_search_match_counter(SharedString::from("0 / 0"));
            refresh_page_model(&mut app, &window);
            return;
        };

        let matches = if let Some(doc) = app.active_document() {
            let mut all_matches = Vec::new();
            let mut global_index = 0;
            for geom in app.text_geometries.in_page_order(doc) {
                let page_idx = geom.page_index.get();
                let ranges = query.find_in_geometry(&geom);
                for range in ranges {
                    let mut glyph_boxes = Vec::new();
                    let start = range.start as usize;
                    let end = range.end as usize;

                    if start <= geom.glyphs.len() && end <= geom.glyphs.len() {
                        glyph_boxes.extend_from_slice(&geom.glyphs[start..end]);
                    }

                    all_matches.push(barepdf_core::search::SearchMatch {
                        page_index: PageIndex::from_raw(page_idx),
                        match_index_in_doc: global_index,
                        char_range: range,
                        glyph_boxes,
                    });
                    global_index += 1;
                }
            }
            all_matches
        } else {
            Vec::new()
        };

        let total = matches.len();
        let has_matches = total > 0;
        app.search_query = Some(query);
        app.search_matches = matches;
        app.active_search_match = 0;

        window.set_search_has_matches(has_matches);
        window.set_search_match_counter(SharedString::from(
            crate::controllers::search_controller::SearchController::match_summary(0, total),
        ));
        refresh_page_model(&mut app, &window);

        if has_matches {
            if let Some(target_match) = app.search_matches.first() {
                let target_page = target_match.page_index.get();
                drop(app);
                navigate_to_page(target_page, &state_search, &scheduler_search, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_search_next = state.clone();
    let scheduler_search_next = scheduler.clone();
    window.on_request_search_next(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_search_next.borrow_mut();
        let total = app.search_matches.len();
        if total == 0 {
            return;
        }
        app.active_search_match =
            crate::controllers::search_controller::SearchController::next_match(
                app.active_search_match,
                total,
            );
        let current = app.active_search_match;
        window.set_search_match_counter(SharedString::from(
            crate::controllers::search_controller::SearchController::match_summary(current, total),
        ));
        let target_page = app.search_matches.get(current).map(|m| m.page_index.get());
        if let Some(target_page) = target_page {
            drop(app);
            navigate_to_page(
                target_page,
                &state_search_next,
                &scheduler_search_next,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_search_prev = state.clone();
    let scheduler_search_prev = scheduler.clone();
    window.on_request_search_prev(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_search_prev.borrow_mut();
        let total = app.search_matches.len();
        if total == 0 {
            return;
        }
        app.active_search_match =
            crate::controllers::search_controller::SearchController::prev_match(
                app.active_search_match,
                total,
            );
        let current = app.active_search_match;
        window.set_search_match_counter(SharedString::from(
            crate::controllers::search_controller::SearchController::match_summary(current, total),
        ));
        let target_page = app.search_matches.get(current).map(|m| m.page_index.get());
        if let Some(target_page) = target_page {
            drop(app);
            navigate_to_page(
                target_page,
                &state_search_prev,
                &scheduler_search_prev,
                &window,
            );
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
            super::window_chrome::sync_window_maximized(&window);
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
                super::window_chrome::sync_window_maximized(&window);
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
                super::window_chrome::sync_window_maximized(&window);
            }
            window.invoke_focus_main();
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

fn is_safe_external_link_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}

pub(super) fn refresh_merge_files(window: &AppWindow, app: &mut AppState) {
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

pub(super) fn set_tool_source(app: &mut AppState, source: PathBuf) {
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

pub(super) fn selected_tool_pages(input: &str, total: u32) -> Vec<u32> {
    let Some(page_count) = PageCount::new(total) else {
        return Vec::new();
    };
    PageRangeSelection::parse(input, page_count)
        .map(|pages| pages.into_iter().map(|page| page.get() + 1).collect())
        .unwrap_or_default()
}

pub(super) fn toggle_tool_page_selection(
    existing: &str,
    page: u32,
    total: u32,
    shift: bool,
) -> String {
    if shift {
        let anchor = existing
            .split([',', '-'])
            .find_map(|item| item.trim().parse::<u32>().ok())
            .unwrap_or(page);
        format!("{}-{}", anchor.min(page), anchor.max(page))
    } else {
        let mut pages = selected_tool_pages(existing, total);
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
    }
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
    clear_tool_password_ui(window);
    window.set_tool_password_prompt_open(false);
    app.tool_password_source = None;
    window.set_tools_working(false);
    app.wake_pump();
}

pub(super) fn handle_tool_event(
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
                clear_tool_password_ui(window);
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
            clear_tool_password_ui(window);
            window.set_tool_password_error(SharedString::from(error));
            window.set_tool_password_prompt_open(true);
        }
        ToolEvent::Completed { outcome, .. } => {
            state.borrow_mut().active_tool_job = None;
            state.borrow_mut().tool_password_source = None;
            window.set_tools_working(false);
            clear_tool_password_ui(window);
            window.set_tool_password_prompt_open(false);
            window.set_tools_open(false);
            window.set_current_tool(-1);
            refresh_thumbnail_model(&mut state.borrow_mut(), window);
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
            clear_tool_password_ui(window);
            window.set_tool_password_prompt_open(false);
        }
        ToolEvent::Failed { message, .. } => {
            state.borrow_mut().active_tool_job = None;
            state.borrow_mut().tool_password_source = None;
            window.set_tools_working(false);
            clear_tool_password_ui(window);
            window.set_tool_password_prompt_open(false);
            window.set_tools_error(SharedString::from(message));
        }
    }
}

pub(super) fn handle_background_ui_event(
    event: BackgroundUiEvent,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    match event {
        BackgroundUiEvent::PrintersEnumerated(printers) => {
            let mut app = state.borrow_mut();
            app.in_flight.printer_enum = false;
            app.cached_printers = printers;
            if window.get_print_preview_open() {
                populate_print_preview_printers(window, &app.cached_printers);
            }
        }
        BackgroundUiEvent::AnnotationsSaved {
            doc_id,
            output_path,
            result,
        } => {
            let lang = {
                let mut app = state.borrow_mut();
                app.in_flight.annotation_save = false;
                app.preferences.language.resolve()
            };
            match result {
                Ok(()) => {
                    {
                        let mut app = state.borrow_mut();
                        app.annotations.remove(&doc_id);
                        app.active_stroke = None;
                        app.page_images.remove_document(doc_id);
                        app.thumbnail_images.remove_document(doc_id);
                        app.text_geometries.remove_document(doc_id);
                        refresh_annotation_overlays(&mut app, window);
                    }
                    window.set_drawing_mode_active(false);
                    let name = output_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("document.pdf");
                    let msg = barepdf_i18n::t(lang, "status.saved").replace("{name}", name);
                    window.set_status_text(SharedString::from(msg.as_str()));
                    show_banner(window, msg, false);
                    begin_open(output_path, None, state, scheduler, window);
                }
                Err(err) => {
                    show_banner(window, format!("Failed to save annotations: {err}"), false);
                }
            }
        }
        BackgroundUiEvent::SignatureImageDecoded { result } => match result {
            Ok((w, h, pixels)) => {
                let mut app = state.borrow_mut();
                app.sign_uploaded_image = Some((w, h, pixels));
                let preview =
                    render_signature_pad_preview(&[], None, app.sign_uploaded_image.as_ref());
                window.set_sign_pad_preview(preview);
                window.set_sign_has_preview(true);
            }
            Err(err) => {
                show_banner(
                    window,
                    format!("Could not load signature image: {err}"),
                    false,
                );
            }
        },
    }
}

fn connect_tools_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    _scheduler: &Rc<RenderScheduler>,
    dialogs: Arc<WindowsFileDialogs>,
) {
    window.on_request_open_url(move |url| {
        let _ = open_url(url.as_str());
    });

    let weak = window.as_weak();
    let state_toggle_tools = state.clone();
    window.on_request_toggle_tools(move || {
        if let Some(window) = weak.upgrade() {
            let next_open = !window.get_tools_open();
            if !next_open {
                cancel_active_tool(&state_toggle_tools, &window);
                refresh_thumbnail_model(&mut state_toggle_tools.borrow_mut(), &window);
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
                let mut app = state_open_tool.borrow_mut();
                let has_doc = app.application.ready_document().is_some();
                let range = if has_doc {
                    let page_num = app.current_page + 1;
                    page_num.to_string()
                } else {
                    String::new()
                };
                window.set_tools_page_range(SharedString::from(range.clone()));
                window.set_tools_split_mode(0);
                window.set_tools_rotation(1);
                refresh_tool_thumbnails(&window, &mut app, &range);
            } else if tool_id == 4 {
                let mut app = state_open_tool.borrow_mut();
                let count = app.page_count();
                let range = if count > 0 {
                    if count == 1 {
                        "1".to_string()
                    } else {
                        format!("1-{}", count)
                    }
                } else {
                    String::new()
                };
                window.set_tools_page_range(SharedString::from(range.clone()));
                refresh_tool_thumbnails(&window, &mut app, &range);
            } else if tool_id == -1 {
                let mut app = state_open_tool.borrow_mut();
                refresh_thumbnail_model(&mut app, &window);
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
            refresh_thumbnail_model(&mut state_close_tools.borrow_mut(), &window);
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
        match ToolKind::from_slint_id(tool_id) {
            ToolKind::Merge => {
                app.tools_merge_files.extend(paths);
                refresh_merge_files(&window, &mut app);
                window.set_tools_error(SharedString::default());
            }
            ToolKind::SingleSource => {
                if let [source] = paths.as_slice() {
                    set_tool_source(&mut app, source.clone());
                    window.set_tools_error(SharedString::default());
                } else {
                    window.set_tools_error(SharedString::from(barepdf_i18n::t(
                        window_language(&window),
                        "tools.error.single_source",
                    )));
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_selection = state.clone();
    window.on_request_select_page_range(move |page, _ctrl, shift| {
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
        let range = toggle_tool_page_selection(&existing, page, total, shift);
        window.set_tools_page_range(SharedString::from(range.clone()));
        refresh_tool_thumbnails(&window, &mut app, &range);
        app.wake_pump();
    });

    let weak = window.as_weak();
    let state_select_all = state.clone();
    window.on_request_select_all_tool_pages(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_select_all.borrow_mut();
        let count = app.page_count();
        if count > 0 {
            let range = if count == 1 {
                "1".to_string()
            } else {
                format!("1-{}", count)
            };
            window.set_tools_page_range(SharedString::from(range.clone()));
            refresh_tool_thumbnails(&window, &mut app, &range);
            app.wake_pump();
        }
    });

    let weak = window.as_weak();
    let state_clear_pages = state.clone();
    window.on_request_clear_tool_pages(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_clear_pages.borrow_mut();
        window.set_tools_page_range(SharedString::default());
        refresh_tool_thumbnails(&window, &mut app, "");
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
        let Ok(mut password) = consume_ui_password(password) else {
            if let Some(window) = weak.upgrade() {
                clear_tool_password_ui(&window);
                window.set_tool_password_error(SharedString::from(barepdf_i18n::t(
                    window_language(&window),
                    "password.error.too_long",
                )));
            }
            return;
        };
        let Some(window) = weak.upgrade() else {
            password.clear();
            return;
        };
        clear_tool_password_ui(&window);
        let result = {
            let app = state_password.borrow();
            let Some(active) = app.active_tool_job.as_ref() else {
                password.clear();
                return;
            };
            let Some(source) = app
                .tool_password_source
                .clone()
                .or_else(|| app.tools_source_path.clone())
            else {
                password.clear();
                return;
            };
            let Some(worker) = app.tool_worker.as_ref() else {
                password.clear();
                return;
            };
            worker.provide_password(active.key, source, password)
        };
        match result {
            Ok(()) => {
                window.set_tool_password_error(SharedString::default());
                window.set_tool_password_prompt_open(false);
                state_password.borrow_mut().wake_pump();
            }
            Err(error) => window.set_tool_password_error(SharedString::from(error.to_string())),
        }
    });

    let weak = window.as_weak();
    let state_cancel_password = state.clone();
    window.on_request_cancel_tool_password(move || {
        if let Some(window) = weak.upgrade() {
            clear_tool_password_ui(&window);
            window.set_tool_password_error(SharedString::default());
            cancel_active_tool(&state_cancel_password, &window);
        }
    });
}

fn connect_niche_feature_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    _preferences_path: &Path,
) {
    let weak = window.as_weak();
    window.on_request_toggle_command_palette(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let open = !window.get_command_palette_open();
        window.set_command_palette_open(open);
        if open {
            window.set_command_palette_query(slint::SharedString::from(""));
        }
    });

    let weak = window.as_weak();
    let state_cmd = state.clone();
    let scheduler_cmd = scheduler.clone();
    window.on_request_execute_command(move |query| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let action = {
            let mut app = state_cmd.borrow_mut();
            super::hud_commands::execute_hud_command(&mut app, &scheduler_cmd, &window, &query)
        };
        match action {
            super::hud_commands::HudAction::RequestPrint => {
                window.invoke_request_print();
            }
            super::hud_commands::HudAction::None => {}
        }
    });

    let weak = window.as_weak();
    let state_sel = state.clone();
    let scheduler_sel = scheduler.clone();
    window.on_request_command_selected(move |idx| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let matching =
            super::hud_commands::filter_hud_commands(window.get_command_palette_query().as_str());
        let action = if let Some(item) = matching.get(idx as usize) {
            let mut app = state_sel.borrow_mut();
            super::hud_commands::execute_hud_command(&mut app, &scheduler_sel, &window, item.id)
        } else {
            super::hud_commands::HudAction::None
        };
        window.set_command_palette_open(false);
        match action {
            super::hud_commands::HudAction::RequestPrint => {
                window.invoke_request_print();
            }
            super::hud_commands::HudAction::None => {}
        }
    });

    let weak = window.as_weak();
    let state_tint = state.clone();
    let scheduler_tint = scheduler.clone();
    window.on_request_set_paper_tint(move |tint| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_tint.borrow_mut();
        app.preferences.paper_tint = tint as u8;
        window.set_paper_tint(tint);
        super::ui::invalidate_layout_and_render(&mut app, &scheduler_tint, &window, false);
    });

    let weak = window.as_weak();
    let state_invert = state.clone();
    let scheduler_invert = scheduler.clone();
    window.on_request_toggle_invert_colors(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_invert.borrow_mut();
        app.preferences.invert_colors = !app.preferences.invert_colors;
        window.set_invert_page_colors(app.preferences.invert_colors);
        super::ui::invalidate_layout_and_render(&mut app, &scheduler_invert, &window, true);
    });
}

fn update_drawing_undo_redo_ui(app: &AppState, window: &AppWindow) {
    if let Some(doc_id) = app.active_document() {
        if let Some(history) = app.annotation_history.get(&doc_id) {
            window.set_drawing_can_undo(history.can_undo());
            window.set_drawing_can_redo(history.can_redo());
            return;
        }
    }
    window.set_drawing_can_undo(false);
    window.set_drawing_can_redo(false);
}

fn connect_annotation_and_signature_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    dialogs: Arc<WindowsFileDialogs>,
) {
    {
        let app = state.borrow();
        window.set_pan_mode_active(app.pan_mode);
        window.set_drawing_eraser_size_index(app.drawing_eraser_size_index as i32);
        window.set_drawing_eraser_diameter_norm(ERASER_RADII[app.drawing_eraser_size_index] * 2.0);
        window.set_drawing_toolbar_at_bottom(app.drawing_toolbar_at_bottom);
        update_drawing_undo_redo_ui(&app, window);
    }
    let weak = window.as_weak();
    let state_ctx = state.clone();
    window.on_page_right_clicked(move |page_idx, _norm_x, _norm_y, win_x, win_y| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_ctx.borrow_mut();
        if page_idx >= 0 && app.page_count() > 0 {
            app.current_page = (page_idx as u32).min(app.page_count().saturating_sub(1));
        }
        let has_sel = app.selection.is_some_and(|s| !s.is_empty());
        window.set_context_menu_has_selection(has_sel);
        window.set_context_menu_x(win_x);
        window.set_context_menu_y(win_y);
        window.set_context_menu_open(true);
    });

    let weak = window.as_weak();
    let state_find_sel = state.clone();
    window.on_context_find_selection(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let selected_text = {
            let app = state_find_sel.borrow();
            if let (Some(selection), Some(document)) = (app.selection, app.active_document()) {
                let geometries = app.text_geometries.in_page_order(document);
                let geom_refs: Vec<&barepdf_core::types::PageTextGeometry> =
                    geometries.iter().map(AsRef::as_ref).collect();
                SelectionEngine::get_selected_text_in_page_order(&selection, &geom_refs)
            } else {
                String::new()
            }
        };
        let query = selected_text.trim().to_string();
        if !query.is_empty() {
            window.set_search_open(true);
            window.set_search_query(SharedString::from(query.as_str()));
            window.invoke_request_search_query(SharedString::from(query));
        }
    });

    let weak = window.as_weak();
    let state_hl = state.clone();
    window.on_context_highlight_selection(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_hl.borrow_mut();
        let Some(selection) = app.selection else {
            return;
        };
        if selection.is_empty() {
            return;
        }
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let (first_page, last_page) = selection.start_and_end();
        let mut new_quads = Vec::new();
        for p in first_page.page.get()..=last_page.page.get() {
            let page_index = PageIndex::from_raw(p);
            let Some((start, end)) = selection.range_for_page(page_index) else {
                continue;
            };
            let (pw, ph) = app
                .page_dimensions
                .get(p as usize)
                .copied()
                .unwrap_or(app.first_page_dimensions);
            if let Some(geom) = app.text_geometries.get(doc_id, p) {
                new_quads.extend(selection_to_highlight_quads(
                    &geom, page_index, start, end, pw, ph,
                ));
            }
        }
        if !new_quads.is_empty() {
            let before = app.annotations.get(&doc_id).cloned().unwrap_or_default();
            app.annotations
                .entry(doc_id)
                .or_default()
                .highlights
                .extend(new_quads);
            let history = app.annotation_history.entry(doc_id).or_default();
            history.push_snapshot(before);
            app.selection = None;
            window.set_has_selection(false);
            app.committed_overlay_cache = None;
            update_drawing_undo_redo_ui(&app, &window);
            refresh_page_model(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_draw_mode = state.clone();
    window.on_toggle_drawing_mode(move || {
        if let Some(window) = weak.upgrade() {
            let next = !window.get_drawing_mode_active();
            window.set_drawing_mode_active(next);
            let app = state_draw_mode.borrow();
            window.set_pan_mode_active(app.pan_mode);
            window.set_drawing_eraser_size_index(app.drawing_eraser_size_index as i32);
            window.set_drawing_eraser_diameter_norm(
                ERASER_RADII[app.drawing_eraser_size_index] * 2.0,
            );
            window.set_drawing_toolbar_at_bottom(app.drawing_toolbar_at_bottom);
            update_drawing_undo_redo_ui(&app, &window);
        }
    });

    let weak = window.as_weak();
    let state_pan = state.clone();
    window.on_toggle_pan_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_pan.borrow_mut();
            app.pan_mode = !app.pan_mode;
            window.set_pan_mode_active(app.pan_mode);
        }
    });

    let weak = window.as_weak();
    let state_eraser = state.clone();
    window.on_set_drawing_eraser(move |active| {
        let mut app = state_eraser.borrow_mut();
        app.drawing_eraser = active;
        if let Some(window) = weak.upgrade() {
            window.set_drawing_eraser_active(active);
            let idx = app.drawing_eraser_size_index;
            let radius = ERASER_RADII[idx];
            window.set_drawing_eraser_size_index(idx as i32);
            window.set_drawing_eraser_diameter_norm(radius * 2.0);
        }
    });

    let weak = window.as_weak();
    let state_eraser_size = state.clone();
    window.on_set_drawing_eraser_size(move |idx| {
        let mut app = state_eraser_size.borrow_mut();
        let idx = (idx.max(0) as usize).min(ERASER_RADII.len() - 1);
        app.drawing_eraser_size_index = idx;
        let radius = ERASER_RADII[idx];
        if let Some(window) = weak.upgrade() {
            window.set_drawing_eraser_size_index(idx as i32);
            window.set_drawing_eraser_diameter_norm(radius * 2.0);
        }
    });

    let weak = window.as_weak();
    let state_toolbar_pos = state.clone();
    window.on_toggle_drawing_toolbar_position(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_toolbar_pos.borrow_mut();
            app.drawing_toolbar_at_bottom = !app.drawing_toolbar_at_bottom;
            window.set_drawing_toolbar_at_bottom(app.drawing_toolbar_at_bottom);
        }
    });

    let weak = window.as_weak();
    let state_color = state.clone();
    window.on_set_drawing_color(move |idx| {
        let mut app = state_color.borrow_mut();
        app.drawing_color = match idx {
            1 => barepdf_core::InkColor::Red,
            2 => barepdf_core::InkColor::Blue,
            3 => barepdf_core::InkColor::Yellow,
            _ => barepdf_core::InkColor::Black,
        };
        app.drawing_eraser = false;
        if let Some(window) = weak.upgrade() {
            window.set_drawing_color_index(idx.clamp(0, 3));
            window.set_drawing_eraser_active(false);
        }
    });

    let weak = window.as_weak();
    let state_width = state.clone();
    window.on_set_drawing_width(move |idx| {
        let mut app = state_width.borrow_mut();
        app.drawing_width_pts = match idx {
            0 => 2.0,
            1 => 4.0,
            _ => 8.0,
        };
        if let Some(window) = weak.upgrade() {
            window.set_drawing_width_index(idx.clamp(0, 2));
        }
    });

    let weak = window.as_weak();
    let state_draw_down = state.clone();
    window.on_drawing_pointer_down(move |page, nx, ny| {
        if page < 0 {
            return;
        }
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_draw_down.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let page_idx = PageIndex::from_raw(page as u32);
        let pt = (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0));
        if app.drawing_eraser {
            app.last_eraser_point = Some((page_idx, pt.0, pt.1));
            let radius = ERASER_RADII[app.drawing_eraser_size_index];
            let mut modified = false;
            let mut before = None;
            if let Some(ann) = app.annotations.get_mut(&doc_id) {
                before = Some(ann.clone());
                modified = barepdf_core::erase_ink_strokes_along_segment(
                    &mut ann.strokes,
                    page_idx,
                    pt,
                    pt,
                    radius,
                    radius,
                );
            }
            if let Some(before) = before {
                let history = app.annotation_history.entry(doc_id).or_default();
                history.push_snapshot(before);
            }
            if modified {
                app.committed_overlay_cache = None;
                update_drawing_undo_redo_ui(&app, &window);
                refresh_annotation_overlays(&mut app, &window);
            }
        } else {
            app.active_stroke = Some(barepdf_core::InkStroke {
                page: page_idx,
                points: vec![pt],
                color: app.drawing_color,
                width_pts: app.drawing_width_pts,
            });
            refresh_annotation_overlays(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_draw_move = state.clone();
    window.on_drawing_pointer_move(move |page, nx, ny| {
        if page < 0 {
            return;
        }
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_draw_move.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let page_idx = PageIndex::from_raw(page as u32);
        let pt = (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0));
        if app.drawing_eraser {
            let prev_pt = app
                .last_eraser_point
                .filter(|(p, _, _)| *p == page_idx)
                .map(|(_, x, y)| (x, y))
                .unwrap_or(pt);
            app.last_eraser_point = Some((page_idx, pt.0, pt.1));
            let radius = ERASER_RADII[app.drawing_eraser_size_index];
            if let Some(ann) = app.annotations.get_mut(&doc_id) {
                let modified = barepdf_core::erase_ink_strokes_along_segment(
                    &mut ann.strokes,
                    page_idx,
                    prev_pt,
                    pt,
                    radius,
                    radius,
                );
                if modified {
                    app.committed_overlay_cache = None;
                    update_drawing_undo_redo_ui(&app, &window);
                    refresh_annotation_overlays(&mut app, &window);
                }
            }
        } else if let Some(stroke) = app.active_stroke.as_mut() {
            if stroke.page == page_idx && stroke.points.len() < 4096 {
                stroke.points.push(pt);
                refresh_annotation_overlays(&mut app, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_draw_up = state.clone();
    window.on_drawing_pointer_up(move |_page, nx, ny| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_draw_up.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        app.last_eraser_point = None;
        if app.drawing_eraser {
            let current_ann = app.annotations.get(&doc_id).cloned();
            if let Some(ann) = current_ann {
                if let Some(history) = app.annotation_history.get_mut(&doc_id) {
                    if history.undo_stack.last() == Some(&ann) {
                        history.undo_stack.pop();
                    }
                }
            }
            update_drawing_undo_redo_ui(&app, &window);
            return;
        }
        if let Some(mut stroke) = app.active_stroke.take() {
            if stroke.points.is_empty() {
                stroke.points.push((nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0)));
            }
            stroke.points = barepdf_core::smooth_ink_points(&stroke.points);
            let ann = app.annotations.entry(doc_id).or_default();
            let before = ann.clone();
            ann.strokes.push(stroke);
            let history = app.annotation_history.entry(doc_id).or_default();
            history.push_snapshot(before);
            app.committed_overlay_cache = None;
            update_drawing_undo_redo_ui(&app, &window);
            refresh_annotation_overlays(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_undo = state.clone();
    window.on_drawing_undo(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_undo.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let current_ann = app.annotations.get(&doc_id).cloned().unwrap_or_default();
        if let Some(history) = app.annotation_history.get_mut(&doc_id) {
            if let Some(prev) = history.undo(current_ann) {
                app.annotations.insert(doc_id, prev);
                app.committed_overlay_cache = None;
                update_drawing_undo_redo_ui(&app, &window);
                refresh_annotation_overlays(&mut app, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_redo = state.clone();
    window.on_drawing_redo(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_redo.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let current_ann = app.annotations.get(&doc_id).cloned().unwrap_or_default();
        if let Some(history) = app.annotation_history.get_mut(&doc_id) {
            if let Some(next) = history.redo(current_ann) {
                app.annotations.insert(doc_id, next);
                app.committed_overlay_cache = None;
                update_drawing_undo_redo_ui(&app, &window);
                refresh_annotation_overlays(&mut app, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_clear_page = state.clone();
    window.on_drawing_clear_page(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_clear_page.borrow_mut();
        let current_page = app.current_page;
        let Some(doc_id) = app.active_document() else {
            return;
        };
        if let Some(ann) = app.annotations.get_mut(&doc_id) {
            let before = ann.clone();
            ann.strokes.retain(|s| s.page.get() != current_page);
            ann.highlights.retain(|h| h.page.get() != current_page);
            ann.signatures.retain(|s| s.page.get() != current_page);
            if *ann != before {
                let history = app.annotation_history.entry(doc_id).or_default();
                history.push_snapshot(before);
                app.committed_overlay_cache = None;
                update_drawing_undo_redo_ui(&app, &window);
                refresh_annotation_overlays(&mut app, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_discard = state.clone();
    window.on_discard_annotations(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_discard.borrow_mut();
        if let Some(doc_id) = app.active_document() {
            app.annotations.remove(&doc_id);
            app.annotation_history.remove(&doc_id);
        }
        app.active_stroke = None;
        app.committed_overlay_cache = None;
        window.set_drawing_mode_active(false);
        update_drawing_undo_redo_ui(&app, &window);
        refresh_annotation_overlays(&mut app, &window);
    });

    let weak = window.as_weak();
    let state_save = state.clone();
    let scheduler_save = scheduler.clone();
    window.on_save_annotations(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        save_active_annotations(&state_save, &scheduler_save, &window, None);
    });

    let weak = window.as_weak();
    let state_save_as = state.clone();
    let scheduler_save_as = scheduler.clone();
    let dialogs_save_as = dialogs.clone();
    window.on_save_annotations_as(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let default_name = {
            let app = state_save_as.borrow();
            let Some(doc) = app.application.ready_document() else {
                return;
            };
            format!(
                "{}_annotated.pdf",
                doc.path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("document")
            )
        };
        let Some(output_path) = dialogs_save_as.save_file(&default_name) else {
            return;
        };
        save_active_annotations(
            &state_save_as,
            &scheduler_save_as,
            &window,
            Some(output_path),
        );
    });

    // Signature creation & placement callbacks
    let weak = window.as_weak();
    let state_open_sign = state.clone();
    window.on_open_sign_modal(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_open_sign.borrow_mut();
        app.sign_pad_strokes.clear();
        app.sign_pad_active_stroke = None;
        app.sign_uploaded_image = None;
        window.set_sign_pad_preview(Image::default());
        window.set_sign_has_preview(false);
        window.set_sign_tab_index(0);
        window.set_sign_modal_open(true);
    });

    let weak = window.as_weak();
    let state_sign_down = state.clone();
    window.on_sign_pad_pointer_down(move |nx, ny| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_down.borrow_mut();
        app.sign_uploaded_image = None;
        app.sign_pad_active_stroke = Some(vec![(nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0))]);
        let preview = render_signature_pad_preview(
            &app.sign_pad_strokes,
            app.sign_pad_active_stroke.as_ref(),
            None,
        );
        window.set_sign_pad_preview(preview);
        window.set_sign_has_preview(true);
    });

    let weak = window.as_weak();
    let state_sign_move = state.clone();
    window.on_sign_pad_pointer_move(move |nx, ny| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_move.borrow_mut();
        if let Some(stroke) = app.sign_pad_active_stroke.as_mut() {
            if stroke.len() < 2048 {
                stroke.push((nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0)));
                let preview = render_signature_pad_preview(
                    &app.sign_pad_strokes,
                    app.sign_pad_active_stroke.as_ref(),
                    None,
                );
                window.set_sign_pad_preview(preview);
            }
        }
    });

    let weak = window.as_weak();
    let state_sign_up = state.clone();
    window.on_sign_pad_pointer_up(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_up.borrow_mut();
        if let Some(stroke) = app.sign_pad_active_stroke.take() {
            if !stroke.is_empty() {
                app.sign_pad_strokes.push(stroke);
            }
            let preview = render_signature_pad_preview(&app.sign_pad_strokes, None, None);
            window.set_sign_pad_preview(preview);
            window.set_sign_has_preview(!app.sign_pad_strokes.is_empty());
        }
    });

    let weak = window.as_weak();
    let state_sign_clear = state.clone();
    window.on_sign_pad_clear(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_clear.borrow_mut();
        app.sign_pad_strokes.clear();
        app.sign_pad_active_stroke = None;
        app.sign_uploaded_image = None;
        window.set_sign_pad_preview(Image::default());
        window.set_sign_has_preview(false);
    });

    let weak = window.as_weak();
    let state_sign_img = state.clone();
    let dialogs_sign_img = dialogs;
    window.on_sign_pick_image(move || {
        if weak.upgrade().is_none() {
            return;
        }
        let Some(path) = dialogs_sign_img.pick_image_file() else {
            return;
        };
        state_sign_img.borrow_mut().spawn_background_io(move || {
            let result = barepdf_platform_windows::decode_image_rgba(&path)
                .map(|bmp| bmp.into_parts())
                .map_err(|err| err.to_string());
            BackgroundUiEvent::SignatureImageDecoded { result }
        });
    });

    let weak = window.as_weak();
    let state_sign_place = state.clone();
    window.on_sign_start_placement(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let current_page = state_sign_place.borrow().current_page;
        window.set_sign_modal_open(false);
        window.set_signature_placement_page(current_page as i32);
        window.set_signature_box_x(0.35);
        window.set_signature_box_y(0.72);
        window.set_signature_box_w(0.28);
        window.set_signature_box_h(0.10);
        window.set_signature_placement_active(true);
    });

    let weak = window.as_weak();
    let state_sign_apply = state.clone();
    window.on_signature_apply(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_apply.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let payload = if let Some((width, height, rgba)) = app.sign_uploaded_image.clone() {
            barepdf_core::SignaturePayload::Image {
                width,
                height,
                rgba,
            }
        } else if !app.sign_pad_strokes.is_empty() {
            barepdf_core::SignaturePayload::Drawn(app.sign_pad_strokes.clone())
        } else {
            window.set_signature_placement_active(false);
            return;
        };
        let page = window.get_signature_placement_page().max(0) as u32;
        let stamp = barepdf_core::SignatureStamp {
            page: PageIndex::from_raw(page),
            x_norm: window.get_signature_box_x().clamp(0.0, 0.95),
            y_norm: window.get_signature_box_y().clamp(0.0, 0.95),
            w_norm: window.get_signature_box_w().clamp(0.05, 1.0),
            h_norm: window.get_signature_box_h().clamp(0.03, 1.0),
            payload,
        };
        let ann = app.annotations.entry(doc_id).or_default();
        let before = ann.clone();
        ann.signatures.push(stamp);
        let history = app.annotation_history.entry(doc_id).or_default();
        history.push_snapshot(before);
        app.committed_overlay_cache = None;
        window.set_signature_placement_active(false);
        update_drawing_undo_redo_ui(&app, &window);
        refresh_annotation_overlays(&mut app, &window);
    });

    let weak = window.as_weak();
    window.on_signature_cancel(move || {
        if let Some(window) = weak.upgrade() {
            window.set_signature_placement_active(false);
        }
    });
}

fn save_active_annotations(
    state: &Rc<RefCell<AppState>>,
    _scheduler: &Rc<RenderScheduler>,
    _window: &AppWindow,
    save_as_path: Option<PathBuf>,
) {
    let (doc_id, source_path, annotations) = {
        let mut app = state.borrow_mut();
        if app.in_flight.annotation_save {
            return;
        }
        let Some(doc) = app.application.ready_document() else {
            return;
        };
        let doc_id = doc.id();
        let Some(annotations) = app.annotations.get(&doc_id).cloned() else {
            return;
        };
        if annotations.is_empty() {
            return;
        }
        let source_path = doc.path().to_path_buf();
        app.in_flight.annotation_save = true;
        (doc_id, source_path, annotations)
    };
    let output_path = save_as_path.unwrap_or_else(|| source_path.clone());
    state.borrow_mut().spawn_background_io(move || {
        let result = barepdf_pdf::PdfOperations::save_with_annotations(
            &source_path,
            &annotations,
            &output_path,
        )
        .map_err(|err| err.to_string());
        BackgroundUiEvent::AnnotationsSaved {
            doc_id,
            output_path,
            result,
        }
    });
}

fn selection_to_highlight_quads(
    geometry: &barepdf_core::PageTextGeometry,
    page_index: PageIndex,
    start: u32,
    end: u32,
    page_width: f32,
    page_height: f32,
) -> Vec<barepdf_core::HighlightQuad> {
    let pw = page_width.max(1.0);
    let ph = page_height.max(1.0);
    let start = (start as usize).min(geometry.glyphs.len());
    let end = (end as usize).min(geometry.glyphs.len());
    let mut quads: Vec<barepdf_core::HighlightQuad> = Vec::new();

    for glyph in &geometry.glyphs[start..end] {
        if glyph.width <= 0.0 || glyph.height <= 0.0 {
            continue;
        }
        let x_norm = (glyph.x / pw).clamp(0.0, 1.0);
        let y_norm = ((ph - glyph.y - glyph.height) / ph).clamp(0.0, 1.0);
        let w_norm = (glyph.width / pw).clamp(0.001, 1.0 - x_norm);
        let h_norm = (glyph.height / ph).clamp(0.001, 1.0 - y_norm);

        if let Some(last) = quads.last_mut() {
            if (last.y_norm - y_norm).abs() < 0.015
                && (last.h_norm - h_norm).abs() < 0.015
                && (x_norm - (last.x_norm + last.w_norm)).abs() < 0.025
            {
                let new_right = (x_norm + w_norm).max(last.x_norm + last.w_norm);
                last.w_norm = (new_right - last.x_norm).clamp(0.001, 1.0 - last.x_norm);
                continue;
            }
        }
        quads.push(barepdf_core::HighlightQuad {
            page: page_index,
            x_norm,
            y_norm,
            w_norm,
            h_norm,
        });
    }
    quads
}

#[cfg(test)]
fn erase_strokes_near(
    strokes: &mut Vec<barepdf_core::InkStroke>,
    page: PageIndex,
    nx: f32,
    ny: f32,
    radius: f32,
) -> bool {
    let r2 = radius * radius;
    let before = strokes.len();
    strokes.retain(|stroke| {
        if stroke.page != page {
            return true;
        }
        !stroke
            .points
            .iter()
            .any(|&(px, py)| (px - nx) * (px - nx) + (py - ny) * (py - ny) <= r2)
    });
    strokes.len() != before
}

#[cfg(test)]
mod tests {
    use super::{
        consume_ui_password, erase_strokes_near, is_safe_external_link_url,
        parse_print_preview_range, parse_zoom_percent, print_preview_dimensions,
        selected_tool_pages, selection_to_highlight_quads, toggle_tool_page_selection,
        tool_drop_paths, MAX_PASSWORD_BYTES,
    };
    use crate::presentation::state::{PrintPreviewState, PRINT_PREVIEW_REQUEST_MASK};
    use barepdf_core::{
        DocumentId, GlyphRect, InkColor, InkStroke, PageCount, PageIndex, PageTextGeometry,
        RequestId, Rotation,
    };
    use slint::SharedString;
    use std::path::PathBuf;

    fn page_count(value: u32) -> PageCount {
        PageCount::new(value).expect("test page count")
    }

    #[test]
    fn print_preview_initializes_duplex_to_single_sided() {
        let document = DocumentId::new(7);
        let mut preview = PrintPreviewState::default();
        preview.open(document, 11, page_count(4), PageIndex::zero());
        assert_eq!(preview.duplex, 0);
    }

    #[test]
    fn print_preview_accepts_only_the_latest_matching_render_once() {
        let document = DocumentId::new(7);
        let mut preview = PrintPreviewState::default();
        preview.open(document, 11, page_count(4), PageIndex::zero());
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
        let mut preview = PrintPreviewState::default();
        preview.open(document, 11, page_count(2), PageIndex::zero());
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
            ("250", Some(250)),
            ("800", Some(800)),
            ("999", Some(800)),
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

    #[test]
    fn toggle_tool_page_selection_toggles_pages_without_ctrl() {
        // Toggle page in
        assert_eq!(toggle_tool_page_selection("1, 3", 2, 5, false), "1, 2, 3");
        assert_eq!(toggle_tool_page_selection("", 1, 5, false), "1");
        assert_eq!(toggle_tool_page_selection("1-3", 5, 5, false), "1, 2, 3, 5");

        // Toggle page out
        assert_eq!(toggle_tool_page_selection("1-3, 5", 2, 5, false), "1, 3, 5");
        assert_eq!(toggle_tool_page_selection("2", 2, 5, false), "");
    }

    #[test]
    fn toggle_tool_page_selection_handles_shift_range_selection() {
        assert_eq!(toggle_tool_page_selection("2", 5, 5, true), "2-5");
        assert_eq!(toggle_tool_page_selection("5", 2, 5, true), "2-5");
        assert_eq!(toggle_tool_page_selection("2-4", 6, 10, true), "2-6");
        assert_eq!(toggle_tool_page_selection("", 3, 5, true), "3-3");
    }

    #[test]
    fn external_link_url_scheme_validation_rejects_unsafe_schemes() {
        assert!(is_safe_external_link_url("https://example.com/doc"));
        assert!(is_safe_external_link_url("http://example.com"));
        assert!(is_safe_external_link_url("mailto:user@example.com"));
        assert!(!is_safe_external_link_url(
            "file:///C:/Windows/System32/cmd.exe"
        ));
        assert!(!is_safe_external_link_url("javascript:alert(1)"));
    }

    #[test]
    fn selection_to_highlight_quads_merges_adjacent_glyphs_on_same_line() {
        let geom = PageTextGeometry {
            page_index: PageIndex::zero(),
            glyphs: vec![
                GlyphRect {
                    ch: 'A',
                    x: 10.0,
                    y: 100.0,
                    width: 8.0,
                    height: 12.0,
                },
                GlyphRect {
                    ch: 'B',
                    x: 18.0,
                    y: 100.0,
                    width: 8.0,
                    height: 12.0,
                },
            ],
            links: Vec::new(),
        };
        let quads = selection_to_highlight_quads(&geom, PageIndex::zero(), 0, 2, 200.0, 200.0);
        assert_eq!(quads.len(), 1);
        assert!((quads[0].x_norm - 0.05).abs() < 1e-4);
        assert!((quads[0].w_norm - 0.08).abs() < 1e-4);
    }

    #[test]
    fn erase_strokes_near_removes_only_matching_page_stroke_within_radius() {
        let mut strokes = vec![
            InkStroke {
                page: PageIndex::zero(),
                points: vec![(0.2, 0.2), (0.3, 0.3)],
                color: InkColor::Black,
                width_pts: 4.0,
            },
            InkStroke {
                page: PageIndex::from_raw(1),
                points: vec![(0.2, 0.2)],
                color: InkColor::Red,
                width_pts: 4.0,
            },
        ];
        assert!(erase_strokes_near(
            &mut strokes,
            PageIndex::zero(),
            0.21,
            0.21,
            0.035
        ));
        assert_eq!(strokes.len(), 1);
        assert_eq!(strokes[0].page, PageIndex::from_raw(1));
    }

    #[test]
    fn consume_ui_password_wraps_in_secret_and_rejects_oversized_inputs() {
        let mut valid = consume_ui_password(SharedString::from("unlock-123"))
            .expect("valid password within byte limit");
        assert_eq!(valid.expose(), "unlock-123");
        valid.clear();
        assert!(valid.bytes_for_test().is_empty());

        let oversized = SharedString::from("x".repeat(MAX_PASSWORD_BYTES + 1));
        assert!(consume_ui_password(oversized).is_err());
    }

    #[test]
    fn deferred_printer_sink_defers_spooler_initialization_until_worker_begin() {
        use super::{DeferredPrinterSink, PrintError, PrintJobId, PrintPage, PrinterSink};
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        struct DummySink(PrintJobId);
        impl PrinterSink for DummySink {
            fn job_id(&self) -> PrintJobId {
                self.0
            }
            fn target_dpi(&self) -> u16 {
                300
            }
            fn begin(&mut self, _title: &str) -> Result<(), PrintError> {
                Ok(())
            }
            fn write_page(&mut self, _page: PrintPage<'_>) -> Result<(), PrintError> {
                Ok(())
            }
            fn finish(self: Box<Self>) -> Result<(), PrintError> {
                Ok(())
            }
        }

        let job_id = PrintJobId::new(42).expect("valid job id");
        let initialized = Arc::new(AtomicBool::new(false));
        let initialized_for_factory = initialized.clone();

        let mut sink: Box<dyn PrinterSink> =
            Box::new(DeferredPrinterSink::new(job_id, 300, move || {
                initialized_for_factory.store(true, Ordering::SeqCst);
                Ok(Box::new(DummySink(job_id)))
            }));

        // Constructing the sink on the UI thread must NOT run the spooler factory.
        assert!(!initialized.load(Ordering::SeqCst));
        assert_eq!(sink.job_id(), job_id);
        assert_eq!(sink.target_dpi(), 300);

        // Calling begin() on the PrintWorker thread initializes the inner sink.
        assert!(sink.begin("doc.pdf").is_ok());
        assert!(initialized.load(Ordering::SeqCst));
        assert!(sink.finish().is_ok());
    }

    #[test]
    fn wave2_drawing_segment_eraser_removes_strokes_along_drag_path() {
        use crate::presentation::state::ERASER_RADII;
        use barepdf_core::{erase_ink_strokes_along_segment, InkColor, InkStroke, PageIndex};

        let page = PageIndex::zero();
        let mut strokes = vec![
            InkStroke {
                page,
                points: vec![(0.5, 0.1), (0.5, 0.9)],
                color: InkColor::Black,
                width_pts: 2.0,
            },
            InkStroke {
                page,
                points: vec![(0.1, 0.1), (0.2, 0.2)],
                color: InkColor::Red,
                width_pts: 2.0,
            },
        ];

        let radius = ERASER_RADII[1]; // default index 1: 0.028
        let modified = erase_ink_strokes_along_segment(
            &mut strokes,
            page,
            (0.4, 0.5),
            (0.6, 0.5),
            radius,
            radius,
        );

        assert!(modified);
        // The vertical stroke was cut into 2 pieces, horizontal unaffected (total 3 strokes)
        assert_eq!(strokes.len(), 3);
    }

    #[test]
    fn wave2_smooth_ink_points_subdivides_strokes() {
        use barepdf_core::smooth_ink_points;

        let points = vec![(0.0, 0.0), (0.5, 0.5), (1.0, 0.0)];
        let smoothed = smooth_ink_points(&points);
        assert!(smoothed.len() > points.len());
        assert_eq!(smoothed.first().copied(), Some((0.0, 0.0)));
        assert_eq!(smoothed.last().copied(), Some((1.0, 0.0)));
    }
}
