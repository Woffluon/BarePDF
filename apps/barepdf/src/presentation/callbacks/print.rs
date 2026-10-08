use crate::application::{PrintController, PrintControllerError};
use crate::infrastructure::PrintEvent;
use barepdf_core::{PageCount, PageIndex, Rotation};
use barepdf_platform::printing::{
    Copies, InstalledPrinter, PrintDuplex, PrintError, PrintJobId, PrintOrientation, PrintPage,
    PrintRange, PrinterSink,
};
use barepdf_platform_windows::{enumerate_installed_printers, WindowsPrinterSink};
use barepdf_render::{Priority, RenderCommand, RenderJob, RenderKind, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, Image, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

use super::super::state::{
    next_print_preview_request_id, AppState, BackgroundUiEvent, PRINT_PREVIEW_MAX_EDGE,
};
use super::super::ui::{send_render_command, show_banner};
use super::window_language;

pub(super) fn parse_print_preview_range(
    input: &str,
    page_count: PageCount,
) -> Option<(PageIndex, PageIndex)> {
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

pub(super) fn print_preview_dimensions(dimensions: (f32, f32), rotation: Rotation) -> (u32, u32) {
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

pub(super) fn request_print_preview_render(
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

pub(crate) fn requeue_print_preview_for_generation(
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

pub(super) fn close_print_preview(app: &mut AppState, window: &AppWindow) {
    app.print_preview.close();
    window.set_print_preview_open(false);
    window.set_print_preview_has_image(false);
    window.set_print_preview_image(Image::default());
}

pub(super) fn populate_print_preview_printers(window: &AppWindow, installed: &[InstalledPrinter]) {
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

pub(super) struct DeferredPrinterSink<F> {
    job_id: PrintJobId,
    target_dpi: u16,
    factory: Option<F>,
    inner: Option<Box<dyn PrinterSink>>,
}

impl<F> DeferredPrinterSink<F>
where
    F: FnOnce() -> Result<Box<dyn PrinterSink>, PrintError> + Send,
{
    pub(super) fn new(job_id: PrintJobId, target_dpi: u16, factory: F) -> Self {
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

pub(super) fn connect_print_callbacks(
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

pub(crate) fn handle_print_event(event: PrintEvent, window: &AppWindow) {
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
