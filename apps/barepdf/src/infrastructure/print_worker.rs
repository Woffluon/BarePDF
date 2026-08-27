use barepdf_core::{PdfError, Rotation};
use barepdf_pdf::{PdfBackend, PdfiumEngine};
use barepdf_platform::printing::{
    Copies, PrintError, PrintJobId, PrintPage, PrintRange, PrinterSink,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const COMMAND_CAPACITY: usize = 1;
const EVENT_CAPACITY: usize = 1;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);
const SHUTDOWN_RETRY_TIMEOUT: Duration = Duration::from_secs(1);
const POINTS_PER_INCH: f64 = 72.0;
const MAX_TARGET_DPI: u16 = 600;
const MAX_PRINT_BITMAP_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) struct PrintRequest {
    pub(crate) id: PrintJobId,
    pub(crate) path: PathBuf,
    pub(crate) title: String,
    pub(crate) range: PrintRange,
    pub(crate) copies: Copies,
    pub(crate) sink: Option<Box<dyn PrinterSink>>,
    pub(crate) cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PrintEvent {
    Progress {
        job_id: PrintJobId,
        completed: u32,
        total: u32,
    },
    Finished {
        job_id: PrintJobId,
    },
    Cancelled {
        job_id: PrintJobId,
    },
    Failed {
        job_id: PrintJobId,
        message: String,
    },
}

impl PrintEvent {
    #[must_use]
    pub(crate) const fn job_id(&self) -> PrintJobId {
        match self {
            Self::Progress { job_id, .. }
            | Self::Finished { job_id }
            | Self::Cancelled { job_id }
            | Self::Failed { job_id, .. } => *job_id,
        }
    }

    #[must_use]
    pub(crate) const fn is_terminal(&self) -> bool {
        !matches!(self, Self::Progress { .. })
    }
}

enum PrintCommand {
    Start(PrintRequest),
    Shutdown,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PrintWorkerError {
    #[error("could not start print worker: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("print worker command queue is busy")]
    Busy,
    #[error("print worker is unavailable")]
    Disconnected,
    #[error("print worker did not stop within 250 ms")]
    ShutdownTimeout,
    #[error("print worker panicked")]
    Panicked,
}

type BackendFactory = Box<dyn FnOnce() -> Result<Box<dyn PdfBackend>, String> + Send + 'static>;

pub(crate) struct PrintWorker {
    command_sender: Option<SyncSender<PrintCommand>>,
    progress_receiver: Receiver<PrintEvent>,
    terminal_receiver: Receiver<PrintEvent>,
    shutdown: Arc<AtomicBool>,
    done_receiver: Receiver<()>,
    handle: Option<JoinHandle<()>>,
    shutdown_timed_out: bool,
}

impl PrintWorker {
    pub(crate) fn spawn() -> Result<Self, PrintWorkerError> {
        Self::spawn_with_factory(Box::new(|| {
            PdfiumEngine::new()
                .map(|engine| Box::new(engine) as Box<dyn PdfBackend>)
                .map_err(|error| error.to_string())
        }))
    }

    fn spawn_with_factory(factory: BackendFactory) -> Result<Self, PrintWorkerError> {
        let (command_sender, command_receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let (progress_sender, progress_receiver) = mpsc::sync_channel(EVENT_CAPACITY);
        let (terminal_sender, terminal_receiver) = mpsc::sync_channel(EVENT_CAPACITY);
        let (done_sender, done_receiver) = mpsc::sync_channel(1);
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = shutdown.clone();
        let handle = std::thread::Builder::new()
            .name("barepdf-print".into())
            .spawn(move || {
                worker_loop(
                    factory,
                    &command_receiver,
                    &progress_sender,
                    &terminal_sender,
                    &worker_shutdown,
                );
                let _ = done_sender.try_send(());
            })
            .map_err(PrintWorkerError::Spawn)?;
        Ok(Self {
            command_sender: Some(command_sender),
            progress_receiver,
            terminal_receiver,
            shutdown,
            done_receiver,
            handle: Some(handle),
            shutdown_timed_out: false,
        })
    }

    pub(crate) fn submit(&self, request: PrintRequest) -> Result<(), PrintWorkerError> {
        let sender = self
            .command_sender
            .as_ref()
            .ok_or(PrintWorkerError::Disconnected)?;
        sender
            .try_send(PrintCommand::Start(request))
            .map_err(|error| match error {
                TrySendError::Full(_) => PrintWorkerError::Busy,
                TrySendError::Disconnected(_) => PrintWorkerError::Disconnected,
            })
    }

    pub(crate) fn try_recv_event(&self) -> Option<PrintEvent> {
        match self.terminal_receiver.try_recv() {
            Ok(event) => return Some(event),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
        }
        self.progress_receiver.try_recv().ok()
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), PrintWorkerError> {
        let Some(_) = self.handle.as_ref() else {
            return Ok(());
        };
        self.shutdown.store(true, Ordering::Release);
        if let Some(sender) = self.command_sender.take() {
            let _ = sender.try_send(PrintCommand::Shutdown);
        }
        let wait_timeout = if self.shutdown_timed_out {
            SHUTDOWN_RETRY_TIMEOUT
        } else {
            SHUTDOWN_TIMEOUT
        };
        match self.done_receiver.recv_timeout(wait_timeout) {
            Ok(()) => self
                .handle
                .take()
                .ok_or(PrintWorkerError::Disconnected)?
                .join()
                .map_err(|_| PrintWorkerError::Panicked),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.shutdown_timed_out = true;
                Err(PrintWorkerError::ShutdownTimeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let Some(handle) = self.handle.take() else {
                    return Ok(());
                };
                handle.join().map_err(|_| PrintWorkerError::Panicked)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn spawn_fake_idle() -> Result<Self, PrintWorkerError> {
        Self::spawn_with_factory(Box::new(|| Err("unused fake backend".into())))
    }
}

impl Drop for PrintWorker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn worker_loop(
    factory: BackendFactory,
    commands: &Receiver<PrintCommand>,
    progress: &SyncSender<PrintEvent>,
    terminal: &SyncSender<PrintEvent>,
    shutdown: &AtomicBool,
) {
    let mut factory = Some(factory);
    let mut backend: Option<Box<dyn PdfBackend>> = None;
    while !shutdown.load(Ordering::Acquire) {
        let Ok(command) = commands.recv() else {
            break;
        };
        match command {
            PrintCommand::Start(request) => {
                let event = match backend.as_ref() {
                    Some(backend) => run_job(backend.as_ref(), request, progress, shutdown),
                    None => match factory.take() {
                        Some(factory) => match factory() {
                            Ok(created_backend) => {
                                backend = Some(created_backend);
                                match backend.as_ref() {
                                    Some(backend) => {
                                        run_job(backend.as_ref(), request, progress, shutdown)
                                    }
                                    None => PrintEvent::Failed {
                                        job_id: request.id,
                                        message: "print backend is unavailable".into(),
                                    },
                                }
                            }
                            Err(message) => PrintEvent::Failed {
                                job_id: request.id,
                                message,
                            },
                        },
                        None => PrintEvent::Failed {
                            job_id: request.id,
                            message: "print backend is unavailable".into(),
                        },
                    },
                };
                let _ = terminal.send(event);
            }
            PrintCommand::Shutdown => break,
        }
    }
}

enum JobFailure {
    Cancelled,
    Failed(String),
}

fn run_job(
    backend: &dyn PdfBackend,
    mut request: PrintRequest,
    progress: &SyncSender<PrintEvent>,
    shutdown: &AtomicBool,
) -> PrintEvent {
    let job_id = request.id;
    match execute_job(backend, &mut request, progress, shutdown) {
        Ok(()) => PrintEvent::Finished { job_id },
        Err(JobFailure::Cancelled) => PrintEvent::Cancelled { job_id },
        Err(JobFailure::Failed(message)) => PrintEvent::Failed { job_id, message },
    }
}

fn execute_job(
    backend: &dyn PdfBackend,
    request: &mut PrintRequest,
    progress: &SyncSender<PrintEvent>,
    shutdown: &AtomicBool,
) -> Result<(), JobFailure> {
    check_cancel(request, shutdown)?;
    let document = backend
        .open_path(&request.path, None)
        .map_err(|error| pdf_failure(&error))?;
    let page_count = document.page_count().map_err(|error| pdf_failure(&error))?;
    PrintRange::new(request.range.first(), request.range.last(), page_count)
        .map_err(|error| print_failure(&error))?;
    let pages_per_copy = request
        .range
        .last()
        .get()
        .checked_sub(request.range.first().get())
        .and_then(|distance| distance.checked_add(1))
        .ok_or_else(|| JobFailure::Failed("print page count overflow".into()))?;
    let total = pages_per_copy
        .checked_mul(u32::from(request.copies.get()))
        .ok_or_else(|| JobFailure::Failed("print page count overflow".into()))?;
    let mut sink = request
        .sink
        .take()
        .ok_or_else(|| JobFailure::Failed("print sink is unavailable".into()))?;
    let dpi = sink.target_dpi();
    if dpi == 0 || dpi > MAX_TARGET_DPI {
        return Err(print_failure(&PrintError::InvalidDpi(dpi)));
    }
    sink.begin(&request.title)
        .map_err(|error| print_failure(&error))?;

    let mut completed = 0_u32;
    for _ in 0..request.copies.get() {
        for page_index in request.range.pages() {
            check_cancel(request, shutdown)?;
            let (width_points, height_points) = document
                .page_dimensions(page_index)
                .map_err(|error| pdf_failure(&error))?;
            let (target_width, target_height) =
                raster_dimensions(width_points, height_points, dpi)?;
            let bitmap = document
                .render_page(page_index, target_width, target_height, Rotation::Degrees0)
                .map_err(|error| pdf_failure(&error))?;
            let (width, height, mut pixels) = bitmap.into_parts();
            rgba_to_bgra(&mut pixels);
            let page =
                PrintPage::new(width, height, &pixels).map_err(|error| print_failure(&error))?;
            sink.write_page(page)
                .map_err(|error| print_failure(&error))?;
            drop(pixels);
            completed = completed.saturating_add(1);
            let _ = progress.try_send(PrintEvent::Progress {
                job_id: request.id,
                completed,
                total,
            });
            check_cancel(request, shutdown)?;
        }
    }
    sink.finish().map_err(|error| print_failure(&error))
}

fn check_cancel(request: &PrintRequest, shutdown: &AtomicBool) -> Result<(), JobFailure> {
    if request.cancel.load(Ordering::Acquire) || shutdown.load(Ordering::Acquire) {
        Err(JobFailure::Cancelled)
    } else {
        Ok(())
    }
}

fn raster_dimensions(width: f32, height: f32, dpi: u16) -> Result<(u32, u32), JobFailure> {
    fn pixels(points: f32, dpi: u16) -> Option<u32> {
        let value = f64::from(points) * f64::from(dpi) / POINTS_PER_INCH;
        if !value.is_finite() || value <= 0.0 || value > f64::from(u32::MAX) {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let pixels = value.ceil() as u32;
        Some(pixels)
    }

    let width = pixels(width, dpi)
        .ok_or_else(|| JobFailure::Failed("invalid PDF page width for printing".into()))?;
    let height = pixels(height, dpi)
        .ok_or_else(|| JobFailure::Failed("invalid PDF page height for printing".into()))?;
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| JobFailure::Failed("print bitmap size overflow".into()))?;
    if bytes > MAX_PRINT_BITMAP_BYTES {
        return Err(JobFailure::Failed(
            "print bitmap exceeds the 256 MiB page limit".into(),
        ));
    }
    Ok((width, height))
}

fn rgba_to_bgra(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

fn pdf_failure(error: &PdfError) -> JobFailure {
    JobFailure::Failed(error.to_string())
}

fn print_failure(error: &PrintError) -> JobFailure {
    JobFailure::Failed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use barepdf_core::{PageCount, PageIndex, PageTextGeometry, Rotation};
    use barepdf_pdf::{OutlineNode, PdfDocument, RawBitmap, TextSpan};
    use std::path::Path;
    use std::sync::Mutex;
    use std::time::Instant;

    #[derive(Default)]
    struct FakeState {
        rendered: Vec<u32>,
        spooled: Vec<u8>,
        render_pointers: Vec<usize>,
        spool_pointers: Vec<usize>,
        began: usize,
        finished: usize,
    }

    struct FakeBackend {
        state: Arc<Mutex<FakeState>>,
        page_count: PageCount,
    }

    impl PdfBackend for FakeBackend {
        fn open_path(
            &self,
            _path: &Path,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Ok(Box::new(FakeDocument {
                state: self.state.clone(),
                page_count: self.page_count,
            }))
        }

        fn open_bytes(
            &self,
            _bytes: Vec<u8>,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Err(PdfError::InvalidPdfReason("unused fake path".into()))
        }
    }

    struct FakeDocument {
        state: Arc<Mutex<FakeState>>,
        page_count: PageCount,
    }

    impl PdfDocument for FakeDocument {
        fn page_count(&self) -> Result<PageCount, PdfError> {
            Ok(self.page_count)
        }

        fn page_dimensions(&self, _page_index: PageIndex) -> Result<(f32, f32), PdfError> {
            Ok((0.24, 0.24))
        }

        fn render_page(
            &self,
            page_index: PageIndex,
            _target_width: u32,
            _target_height: u32,
            _rotation: Rotation,
        ) -> Result<RawBitmap, PdfError> {
            let page = u8::try_from(page_index.get()).map_err(|_| PdfError::RenderingFailed {
                page_index: page_index.get(),
                reason: "fake page index exceeds u8".into(),
            })?;
            let pixels = vec![page, 2, 3, 255];
            let pointer = pixels.as_ptr() as usize;
            let mut state = self.state.lock().expect("fake state lock");
            state.rendered.push(page_index.get());
            state.render_pointers.push(pointer);
            drop(state);
            RawBitmap::new(1, 1, pixels)
                .map_err(|error| PdfError::InvalidPdfReason(error.to_string()))
        }

        fn extract_text(&self, _page_index: PageIndex) -> Result<String, PdfError> {
            Ok(String::new())
        }

        fn extract_text_spans(&self, _page_index: PageIndex) -> Result<Vec<TextSpan>, PdfError> {
            Ok(Vec::new())
        }

        fn get_page_text_geometry(
            &self,
            page_index: PageIndex,
        ) -> Result<PageTextGeometry, PdfError> {
            Ok(PageTextGeometry {
                page_index,
                glyphs: Vec::new(),
            })
        }

        fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError> {
            Ok(Vec::new())
        }
    }

    struct FakeSink {
        id: PrintJobId,
        dpi: u16,
        state: Arc<Mutex<FakeState>>,
        fail_begin: bool,
        cancel_after_write: Option<Arc<AtomicBool>>,
    }

    impl PrinterSink for FakeSink {
        fn job_id(&self) -> PrintJobId {
            self.id
        }

        fn target_dpi(&self) -> u16 {
            self.dpi
        }

        fn begin(&mut self, _title: &str) -> Result<(), PrintError> {
            if self.fail_begin {
                return Err(PrintError::Platform {
                    operation: "fake begin",
                    code: 5,
                });
            }
            self.state.lock().expect("fake state lock").began += 1;
            Ok(())
        }

        fn write_page(&mut self, page: PrintPage<'_>) -> Result<(), PrintError> {
            let mut state = self.state.lock().expect("fake state lock");
            state.spooled.push(page.bgra()[2]);
            state.spool_pointers.push(page.bgra().as_ptr() as usize);
            drop(state);
            if let Some(cancel) = self.cancel_after_write.as_ref() {
                cancel.store(true, Ordering::Release);
            }
            Ok(())
        }

        fn finish(self: Box<Self>) -> Result<(), PrintError> {
            self.state.lock().expect("fake state lock").finished += 1;
            Ok(())
        }
    }

    fn id() -> PrintJobId {
        PrintJobId::new(1).expect("test print job ID")
    }

    fn count(value: u32) -> PageCount {
        PageCount::new(value).expect("test page count")
    }

    fn request(
        state: Arc<Mutex<FakeState>>,
        range: PrintRange,
        copies: Copies,
        cancel: Arc<AtomicBool>,
    ) -> PrintRequest {
        PrintRequest {
            id: id(),
            path: PathBuf::from("fake.pdf"),
            title: "fake".into(),
            range,
            copies,
            sink: Some(Box::new(FakeSink {
                id: id(),
                dpi: 300,
                state,
                fail_begin: false,
                cancel_after_write: None,
            })),
            cancel,
        }
    }

    #[test]
    fn range_copies_progress_and_single_allocation_flow_are_preserved() {
        let state = Arc::new(Mutex::new(FakeState::default()));
        let backend = FakeBackend {
            state: state.clone(),
            page_count: count(3),
        };
        let range = PrintRange::new(PageIndex::from_raw(1), PageIndex::from_raw(2), count(3))
            .expect("valid range");
        let mut request = request(
            state.clone(),
            range,
            Copies::new(2).expect("valid copies"),
            Arc::new(AtomicBool::new(false)),
        );
        let (progress_sender, progress_receiver) = mpsc::sync_channel(8);

        assert!(execute_job(
            &backend,
            &mut request,
            &progress_sender,
            &AtomicBool::new(false)
        )
        .is_ok());
        let state = state.lock().expect("fake state lock");
        assert_eq!(state.rendered, vec![1, 2, 1, 2]);
        assert_eq!(state.spooled, vec![1, 2, 1, 2]);
        assert_eq!(state.render_pointers, state.spool_pointers);
        assert_eq!((state.began, state.finished), (1, 1));
        drop(state);
        let progress = progress_receiver.try_iter().collect::<Vec<_>>();
        assert_eq!(progress.len(), 4);
        assert!(matches!(
            progress.last(),
            Some(PrintEvent::Progress {
                completed: 4,
                total: 4,
                ..
            })
        ));
    }

    #[test]
    fn invalid_dpi_and_sink_start_failure_do_not_render() {
        let state = Arc::new(Mutex::new(FakeState::default()));
        let backend = FakeBackend {
            state: state.clone(),
            page_count: count(1),
        };
        let range = PrintRange::all(count(1));
        let cancel = Arc::new(AtomicBool::new(false));
        let (progress_sender, _progress_receiver) = mpsc::sync_channel(1);
        let mut invalid = request(state.clone(), range, Copies::default(), cancel.clone());
        invalid.sink = Some(Box::new(FakeSink {
            id: id(),
            dpi: 601,
            state: state.clone(),
            fail_begin: false,
            cancel_after_write: None,
        }));
        assert!(matches!(
            execute_job(
                &backend,
                &mut invalid,
                &progress_sender,
                &AtomicBool::new(false)
            ),
            Err(JobFailure::Failed(_))
        ));

        let mut start_failure = request(state.clone(), range, Copies::default(), cancel);
        start_failure.sink = Some(Box::new(FakeSink {
            id: id(),
            dpi: 300,
            state: state.clone(),
            fail_begin: true,
            cancel_after_write: None,
        }));
        assert!(matches!(
            execute_job(
                &backend,
                &mut start_failure,
                &progress_sender,
                &AtomicBool::new(false)
            ),
            Err(JobFailure::Failed(_))
        ));
        assert!(state.lock().expect("fake state lock").rendered.is_empty());
    }

    #[test]
    fn cancellation_on_a_five_hundred_page_document_stops_at_page_boundary() {
        let state = Arc::new(Mutex::new(FakeState::default()));
        let backend = FakeBackend {
            state: state.clone(),
            page_count: count(500),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let mut request = request(
            state.clone(),
            PrintRange::all(count(500)),
            Copies::default(),
            cancel.clone(),
        );
        request.sink = Some(Box::new(FakeSink {
            id: id(),
            dpi: 300,
            state: state.clone(),
            fail_begin: false,
            cancel_after_write: Some(cancel),
        }));
        let (progress_sender, _progress_receiver) = mpsc::sync_channel(2);

        assert!(matches!(
            execute_job(
                &backend,
                &mut request,
                &progress_sender,
                &AtomicBool::new(false)
            ),
            Err(JobFailure::Cancelled)
        ));
        let state = state.lock().expect("fake state lock");
        assert_eq!(state.rendered, vec![0]);
        assert_eq!(state.spooled, vec![0]);
        assert_eq!(state.finished, 0);
    }

    struct BlockingSink {
        id: PrintJobId,
        entered: Option<SyncSender<()>>,
    }

    impl PrinterSink for BlockingSink {
        fn job_id(&self) -> PrintJobId {
            self.id
        }

        fn target_dpi(&self) -> u16 {
            300
        }

        fn begin(&mut self, _title: &str) -> Result<(), PrintError> {
            Ok(())
        }

        fn write_page(&mut self, _page: PrintPage<'_>) -> Result<(), PrintError> {
            if let Some(entered) = self.entered.take() {
                let _ = entered.try_send(());
            }
            std::thread::sleep(Duration::from_millis(500));
            Ok(())
        }

        fn finish(self: Box<Self>) -> Result<(), PrintError> {
            Ok(())
        }
    }

    #[test]
    fn active_native_call_has_bounded_shutdown_wait() {
        let state = Arc::new(Mutex::new(FakeState::default()));
        let backend_state = state.clone();
        let mut worker = PrintWorker::spawn_with_factory(Box::new(move || {
            Ok(Box::new(FakeBackend {
                state: backend_state,
                page_count: count(1),
            }))
        }))
        .expect("test worker should start");
        let (entered_sender, entered_receiver) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let request = PrintRequest {
            id: id(),
            path: PathBuf::from("fake.pdf"),
            title: "fake".into(),
            range: PrintRange::all(count(1)),
            copies: Copies::default(),
            sink: Some(Box::new(BlockingSink {
                id: id(),
                entered: Some(entered_sender),
            })),
            cancel,
        };
        worker.submit(request).expect("job should queue");
        entered_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("fake spool call should start");

        let started = Instant::now();
        assert!(matches!(
            worker.shutdown(),
            Err(PrintWorkerError::ShutdownTimeout)
        ));
        assert!(started.elapsed() < Duration::from_millis(400));
        assert!(worker.shutdown().is_ok());
    }

    #[test]
    fn backend_is_not_created_until_a_print_job_is_submitted() {
        let factory_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls = factory_calls.clone();
        let mut worker = PrintWorker::spawn_with_factory(Box::new(move || {
            calls.fetch_add(1, Ordering::AcqRel);
            Err("unused fake backend".into())
        }))
        .expect("test worker should start");

        assert_eq!(factory_calls.load(Ordering::Acquire), 0);
        assert!(worker.shutdown().is_ok());
        assert_eq!(factory_calls.load(Ordering::Acquire), 0);
    }
}
