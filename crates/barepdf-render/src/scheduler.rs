use crate::cache::{CacheKey, SharedBitmapCache};
use barepdf_core::{DocumentId, MemoryBudget, PageIndex, PdfError, RequestId, Rotation};
use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, TrySendError};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Visible = 0,
    Prefetch = 1,
    Thumbnail = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderKind {
    Page,
    Thumbnail,
}

#[derive(Debug, Clone)]
pub struct RenderJob {
    pub request_id: RequestId,
    pub generation: u64,
    pub document_id: DocumentId,
    pub page_index: PageIndex,
    pub target_width: u32,
    pub target_height: u32,
    pub rotation: Rotation,
    pub priority: Priority,
    pub kind: RenderKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RenderRequestKey {
    generation: u64,
    document_id: DocumentId,
    page_index: PageIndex,
    target_width: u32,
    target_height: u32,
    rotation: Rotation,
    kind: RenderKind,
}

impl From<&RenderJob> for RenderRequestKey {
    fn from(job: &RenderJob) -> Self {
        Self {
            generation: job.generation,
            document_id: job.document_id,
            page_index: job.page_index,
            target_width: job.target_width,
            target_height: job.target_height,
            rotation: job.rotation,
            kind: job.kind,
        }
    }
}

pub enum RenderCommand {
    OpenDocument {
        document_id: DocumentId,
        path: PathBuf,
        password: Option<String>,
    },
    RenderPage(RenderJob),
    ExtractText {
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
    },
    FetchTextGeometry {
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
    },
    FetchOutline {
        document_id: DocumentId,
    },
    FetchPageDimensions {
        document_id: DocumentId,
        start: u32,
        count: u32,
    },
    CloseDocument(DocumentId),
}

pub enum RenderEvent {
    DocumentOpened {
        document_id: DocumentId,
        page_count: u32,
        first_page_dimensions: (f32, f32),
    },
    PageRendered {
        request_id: RequestId,
        generation: u64,
        document_id: DocumentId,
        page_index: PageIndex,
        kind: RenderKind,
        bitmap: Arc<RawBitmap>,
    },
    TextExtracted {
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
        text: String,
        spans: Vec<TextSpan>,
    },
    TextGeometryFetched {
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
        geometry: barepdf_core::PageTextGeometry,
    },
    OutlineFetched {
        document_id: DocumentId,
        outline: Vec<OutlineNode>,
    },
    PageDimensionsFetched {
        document_id: DocumentId,
        start: u32,
        dimensions: Vec<(f32, f32)>,
    },
    Error {
        request_id: Option<RequestId>,
        document_id: DocumentId,
        generation: Option<u64>,
        error: PdfError,
    },
}

pub struct RenderScheduler {
    control_cmd_sender: Sender<RenderCommand>,
    visible_cmd_sender: Sender<RenderCommand>,
    low_cmd_sender: Sender<RenderCommand>,
    critical_event_receiver: Receiver<RenderEvent>,
    event_receiver: Receiver<RenderEvent>,
    current_generation: Arc<AtomicU64>,
    pending_renders: Arc<Mutex<HashSet<RenderRequestKey>>>,
    cache: SharedBitmapCache,
}

fn emit_data(event_sender: &Sender<RenderEvent>, event: RenderEvent) -> bool {
    !matches!(
        event_sender.try_send(event),
        Err(TrySendError::Disconnected(_))
    )
}

fn emit_critical(event_sender: &Sender<RenderEvent>, event: RenderEvent) -> bool {
    event_sender.send(event).is_ok()
}

impl RenderScheduler {
    #[allow(clippy::too_many_lines)] // Worker command handling stays co-located with its channels and state.
    pub fn spawn<B: PdfBackend + 'static>(backend: B, budget: MemoryBudget) -> Self {
        // Control and visible work must never block the UI thread.
        let (control_tx, control_rx) = bounded::<RenderCommand>(8);
        let (visible_tx, visible_rx) = bounded::<RenderCommand>(32);
        let (low_tx, low_rx) = bounded::<RenderCommand>(128);
        let (critical_event_tx, critical_event_rx) = bounded::<RenderEvent>(32);
        let (event_tx, event_rx) = bounded::<RenderEvent>(128);
        let current_gen = Arc::new(AtomicU64::new(1));
        let pending_renders = Arc::new(Mutex::new(HashSet::new()));
        let cache = SharedBitmapCache::new(budget);

        let cache_clone = cache.clone();
        let gen_clone = current_gen.clone();
        let pending_clone = pending_renders.clone();

        thread::spawn(move || {
            let mut active_doc: Option<(DocumentId, Box<dyn PdfDocument>)> = None;

            'worker: loop {
                let cmd = match control_rx.try_recv() {
                    Ok(command) => command,
                    Err(_) => match crossbeam_channel::select! {
                        recv(control_rx) -> message => message,
                        recv(visible_rx) -> message => message,
                        recv(low_rx) -> message => message,
                    } {
                        Ok(command) => command,
                        Err(_) => break,
                    },
                };

                match cmd {
                    RenderCommand::OpenDocument {
                        document_id,
                        path,
                        password,
                    } => match backend.open_path(&path, password.as_deref()) {
                        Ok(doc) => {
                            match doc.page_count().and_then(|count| {
                                doc.page_dimensions(PageIndex::zero())
                                    .map(|dimensions| (count, dimensions))
                            }) {
                                Ok((count, first_page_dimensions)) => {
                                    active_doc = Some((document_id, doc));
                                    match cache_clone.clear() {
                                        Ok(()) => {
                                            if !emit_critical(
                                                &critical_event_tx,
                                                RenderEvent::DocumentOpened {
                                                    document_id,
                                                    page_count: count.get(),
                                                    first_page_dimensions,
                                                },
                                            ) {
                                                break 'worker;
                                            }
                                        }
                                        Err(error) => {
                                            if !emit_critical(
                                                &critical_event_tx,
                                                RenderEvent::Error {
                                                    request_id: None,
                                                    document_id,
                                                    generation: None,
                                                    error,
                                                },
                                            ) {
                                                break 'worker;
                                            }
                                        }
                                    }
                                }
                                Err(error) => {
                                    if !emit_critical(
                                        &critical_event_tx,
                                        RenderEvent::Error {
                                            request_id: None,
                                            document_id,
                                            generation: None,
                                            error,
                                        },
                                    ) {
                                        break 'worker;
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            if !emit_critical(
                                &critical_event_tx,
                                RenderEvent::Error {
                                    request_id: None,
                                    document_id,
                                    generation: None,
                                    error,
                                },
                            ) {
                                break 'worker;
                            }
                        }
                    },
                    RenderCommand::RenderPage(job) => {
                        let pending_key = RenderRequestKey::from(&job);
                        let finish = || {
                            if let Ok(mut pending) = pending_clone.lock() {
                                pending.remove(&pending_key);
                            }
                        };

                        if job.generation != gen_clone.load(Ordering::Acquire) {
                            finish();
                            continue;
                        }

                        let cache_key = CacheKey {
                            document_id: job.document_id,
                            page_index: job.page_index,
                            target_width: job.target_width,
                            target_height: job.target_height,
                            rotation: job.rotation,
                        };

                        match cache_clone.get(&cache_key) {
                            Ok(Some(bitmap)) => {
                                let emitted = emit_data(
                                    &event_tx,
                                    RenderEvent::PageRendered {
                                        request_id: job.request_id,
                                        generation: job.generation,
                                        document_id: job.document_id,
                                        page_index: job.page_index,
                                        kind: job.kind,
                                        bitmap,
                                    },
                                );
                                finish();
                                if !emitted {
                                    break 'worker;
                                }
                                continue;
                            }
                            Ok(None) => {}
                            Err(error) => {
                                let emitted = emit_critical(
                                    &critical_event_tx,
                                    RenderEvent::Error {
                                        request_id: Some(job.request_id),
                                        document_id: job.document_id,
                                        generation: Some(job.generation),
                                        error,
                                    },
                                );
                                finish();
                                if !emitted {
                                    break 'worker;
                                }
                                continue;
                            }
                        }

                        if let Some((doc_id, ref doc)) = active_doc {
                            if doc_id == job.document_id {
                                match doc.render_page(
                                    job.page_index,
                                    job.target_width,
                                    job.target_height,
                                    job.rotation,
                                ) {
                                    Ok(raw_bitmap) => {
                                        match cache_clone.insert(cache_key, raw_bitmap) {
                                            Ok(bitmap) => {
                                                if !emit_data(
                                                    &event_tx,
                                                    RenderEvent::PageRendered {
                                                        request_id: job.request_id,
                                                        generation: job.generation,
                                                        document_id: job.document_id,
                                                        page_index: job.page_index,
                                                        kind: job.kind,
                                                        bitmap,
                                                    },
                                                ) {
                                                    finish();
                                                    break 'worker;
                                                }
                                            }
                                            Err(error) => {
                                                if !emit_critical(
                                                    &critical_event_tx,
                                                    RenderEvent::Error {
                                                        request_id: Some(job.request_id),
                                                        document_id: job.document_id,
                                                        generation: Some(job.generation),
                                                        error,
                                                    },
                                                ) {
                                                    finish();
                                                    break 'worker;
                                                }
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        if !emit_critical(
                                            &critical_event_tx,
                                            RenderEvent::Error {
                                                request_id: Some(job.request_id),
                                                document_id: job.document_id,
                                                generation: Some(job.generation),
                                                error,
                                            },
                                        ) {
                                            finish();
                                            break 'worker;
                                        }
                                    }
                                }
                            }
                        }
                        finish();
                    }
                    RenderCommand::ExtractText {
                        document_id,
                        generation,
                        page_index,
                    } => {
                        if generation != gen_clone.load(Ordering::Acquire) {
                            continue;
                        }
                        if let Some((doc_id, ref doc)) = active_doc {
                            if doc_id == document_id {
                                match doc.extract_text(page_index).and_then(|text| {
                                    doc.extract_text_spans(page_index)
                                        .map(|spans| (text, spans))
                                }) {
                                    Ok((text, spans)) => {
                                        if !emit_data(
                                            &event_tx,
                                            RenderEvent::TextExtracted {
                                                document_id,
                                                generation,
                                                page_index,
                                                text,
                                                spans,
                                            },
                                        ) {
                                            break 'worker;
                                        }
                                    }
                                    Err(error) => {
                                        if !emit_critical(
                                            &critical_event_tx,
                                            RenderEvent::Error {
                                                request_id: None,
                                                document_id,
                                                generation: Some(generation),
                                                error,
                                            },
                                        ) {
                                            break 'worker;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RenderCommand::FetchTextGeometry {
                        document_id,
                        generation,
                        page_index,
                    } => {
                        if generation != gen_clone.load(Ordering::Acquire) {
                            continue;
                        }
                        if let Some((doc_id, ref doc)) = active_doc {
                            if doc_id == document_id {
                                match doc.get_page_text_geometry(page_index) {
                                    Ok(geometry) => {
                                        if !emit_data(
                                            &event_tx,
                                            RenderEvent::TextGeometryFetched {
                                                document_id,
                                                generation,
                                                page_index,
                                                geometry,
                                            },
                                        ) {
                                            break 'worker;
                                        }
                                    }
                                    Err(error) => {
                                        if !emit_critical(
                                            &critical_event_tx,
                                            RenderEvent::Error {
                                                request_id: None,
                                                document_id,
                                                generation: Some(generation),
                                                error,
                                            },
                                        ) {
                                            break 'worker;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RenderCommand::FetchOutline { document_id } => {
                        if let Some((doc_id, ref doc)) = active_doc {
                            if doc_id == document_id {
                                match doc.get_outline() {
                                    Ok(outline) => {
                                        if !emit_data(
                                            &event_tx,
                                            RenderEvent::OutlineFetched {
                                                document_id,
                                                outline,
                                            },
                                        ) {
                                            break 'worker;
                                        }
                                    }
                                    Err(error) => {
                                        if !emit_critical(
                                            &critical_event_tx,
                                            RenderEvent::Error {
                                                request_id: None,
                                                document_id,
                                                generation: None,
                                                error,
                                            },
                                        ) {
                                            break 'worker;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RenderCommand::FetchPageDimensions {
                        document_id,
                        start,
                        count,
                    } => {
                        if let Some((doc_id, ref doc)) = active_doc {
                            if doc_id == document_id {
                                match doc.page_count() {
                                    Ok(page_count) => {
                                        let end = start.saturating_add(count).min(page_count.get());
                                        let dimensions = (start..end)
                                            .map(|index| {
                                                doc.page_dimensions(PageIndex::from_raw(index))
                                            })
                                            .collect::<Result<Vec<_>, _>>();
                                        match dimensions {
                                            Ok(dimensions) => {
                                                if !emit_data(
                                                    &event_tx,
                                                    RenderEvent::PageDimensionsFetched {
                                                        document_id,
                                                        start,
                                                        dimensions,
                                                    },
                                                ) {
                                                    break 'worker;
                                                }
                                            }
                                            Err(error) => {
                                                if !emit_critical(
                                                    &critical_event_tx,
                                                    RenderEvent::Error {
                                                        request_id: None,
                                                        document_id,
                                                        generation: None,
                                                        error,
                                                    },
                                                ) {
                                                    break 'worker;
                                                }
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        if !emit_critical(
                                            &critical_event_tx,
                                            RenderEvent::Error {
                                                request_id: None,
                                                document_id,
                                                generation: None,
                                                error,
                                            },
                                        ) {
                                            break 'worker;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RenderCommand::CloseDocument(doc_id) => {
                        if active_doc.as_ref().is_some_and(|(id, _)| *id == doc_id) {
                            active_doc = None;
                            if let Err(error) = cache_clone.clear() {
                                if !emit_critical(
                                    &critical_event_tx,
                                    RenderEvent::Error {
                                        request_id: None,
                                        document_id: doc_id,
                                        generation: None,
                                        error,
                                    },
                                ) {
                                    break 'worker;
                                }
                            }
                        }
                    }
                }
            }
        });

        Self {
            control_cmd_sender: control_tx,
            visible_cmd_sender: visible_tx,
            low_cmd_sender: low_tx,
            critical_event_receiver: critical_event_rx,
            event_receiver: event_rx,
            current_generation: current_gen,
            pending_renders,
            cache,
        }
    }

    #[must_use]
    pub fn bump_generation(&self) -> u64 {
        let generation = self.current_generation.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut pending) = self.pending_renders.lock() {
            pending.retain(|key| key.generation == generation);
        }
        generation
    }

    #[must_use]
    pub fn current_generation(&self) -> u64 {
        self.current_generation.load(Ordering::Acquire)
    }

    /// Queues work without blocking the caller. Returns false for duplicate work or a full
    /// background queue so the UI can remain responsive under load.
    #[must_use]
    pub fn send_command(&self, cmd: RenderCommand) -> bool {
        let pending_key = match &cmd {
            RenderCommand::RenderPage(job) => {
                let key = RenderRequestKey::from(job);
                let Ok(mut pending) = self.pending_renders.lock() else {
                    return false;
                };
                if !pending.insert(key.clone()) {
                    return false;
                }
                Some(key)
            }
            _ => None,
        };

        let control_command = matches!(
            &cmd,
            RenderCommand::OpenDocument { .. } | RenderCommand::CloseDocument(_)
        );
        let visible_render = matches!(
            &cmd,
            RenderCommand::RenderPage(RenderJob {
                priority: Priority::Visible,
                ..
            })
        );

        let result = if control_command {
            self.control_cmd_sender
                .try_send(cmd)
                .map_err(|error| match error {
                    TrySendError::Full(_) | TrySendError::Disconnected(_) => (),
                })
        } else if visible_render {
            self.visible_cmd_sender
                .try_send(cmd)
                .map_err(|error| match error {
                    TrySendError::Full(_) | TrySendError::Disconnected(_) => (),
                })
        } else {
            self.low_cmd_sender
                .try_send(cmd)
                .map_err(|error| match error {
                    TrySendError::Full(_) | TrySendError::Disconnected(_) => (),
                })
        };

        if result.is_err() {
            if let (Some(key), Ok(mut pending)) = (pending_key, self.pending_renders.lock()) {
                pending.remove(&key);
            }
            return false;
        }
        true
    }

    /// Returns `Ok(None)` while worker remains live without an event, and
    /// `PdfError::WorkerTerminated` after worker event senders are disconnected.
    ///
    /// # Errors
    ///
    /// Returns `PdfError::WorkerTerminated` when the worker has stopped and all queued events
    /// were already received.
    pub fn try_recv_event(&self) -> Result<Option<RenderEvent>, PdfError> {
        match self.critical_event_receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => match self.event_receiver.try_recv() {
                Ok(event) => Ok(Some(event)),
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => Err(PdfError::WorkerTerminated),
            },
            Err(TryRecvError::Disconnected) => match self.event_receiver.try_recv() {
                Ok(event) => Ok(Some(event)),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                    Err(PdfError::WorkerTerminated)
                }
            },
        }
    }

    #[must_use]
    pub fn cache(&self) -> &SharedBitmapCache {
        &self.cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use barepdf_core::{PageCount, PageTextGeometry};
    use std::path::Path;
    use std::time::{Duration, Instant};

    struct MockBackend {
        render_delay: Duration,
    }

    struct MockDocument {
        render_delay: Duration,
    }

    impl PdfBackend for MockBackend {
        fn open_path(
            &self,
            _path: &Path,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Ok(Box::new(MockDocument {
                render_delay: self.render_delay,
            }))
        }

        fn open_bytes(
            &self,
            _bytes: Vec<u8>,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            self.open_path(Path::new("mock.pdf"), None)
        }
    }

    impl PdfDocument for MockDocument {
        fn page_count(&self) -> Result<PageCount, PdfError> {
            Ok(PageCount::new(4).expect("non-zero"))
        }

        fn page_dimensions(&self, _page_index: PageIndex) -> Result<(f32, f32), PdfError> {
            Ok((600.0, 800.0))
        }

        fn render_page(
            &self,
            _page_index: PageIndex,
            _target_width: u32,
            _target_height: u32,
            _rotation: Rotation,
        ) -> Result<RawBitmap, PdfError> {
            std::thread::sleep(self.render_delay);
            Ok(RawBitmap {
                width: 1,
                height: 1,
                pixels: vec![0; 4],
            })
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

    fn job(generation: u64, kind: RenderKind) -> RenderJob {
        RenderJob {
            request_id: RequestId::new(1),
            generation,
            document_id: DocumentId::new(7),
            page_index: PageIndex::from_raw(2),
            target_width: 800,
            target_height: 1000,
            rotation: Rotation::Degrees0,
            priority: Priority::Visible,
            kind,
        }
    }

    fn scheduler(render_delay: Duration) -> RenderScheduler {
        RenderScheduler::spawn(MockBackend { render_delay }, MemoryBudget::new(1024 * 1024))
    }

    fn open_document(scheduler: &RenderScheduler, id: DocumentId) {
        assert!(scheduler.send_command(RenderCommand::OpenDocument {
            document_id: id,
            path: PathBuf::from("mock.pdf"),
            password: None,
        }));
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if matches!(
                scheduler.try_recv_event(),
                Ok(Some(RenderEvent::DocumentOpened { document_id, .. })) if document_id == id
            ) {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("document did not open");
    }

    #[test]
    fn render_request_key_includes_generation_and_kind() {
        assert_ne!(
            RenderRequestKey::from(&job(1, RenderKind::Page)),
            RenderRequestKey::from(&job(2, RenderKind::Page))
        );
        assert_ne!(
            RenderRequestKey::from(&job(1, RenderKind::Page)),
            RenderRequestKey::from(&job(1, RenderKind::Thumbnail))
        );
    }

    #[test]
    fn disconnected_event_channels_report_worker_termination() {
        let (control_sender, _control_receiver) = bounded(1);
        let (visible_sender, _visible_receiver) = bounded(1);
        let (low_sender, _low_receiver) = bounded(1);
        let (critical_sender, critical_receiver) = bounded(1);
        let (event_sender, event_receiver) = bounded(1);
        drop(critical_sender);
        drop(event_sender);

        let scheduler = RenderScheduler {
            control_cmd_sender: control_sender,
            visible_cmd_sender: visible_sender,
            low_cmd_sender: low_sender,
            critical_event_receiver: critical_receiver,
            event_receiver,
            current_generation: Arc::new(AtomicU64::new(1)),
            pending_renders: Arc::new(Mutex::new(HashSet::new())),
            cache: SharedBitmapCache::new(MemoryBudget::new(1024)),
        };

        assert!(matches!(
            scheduler.try_recv_event(),
            Err(PdfError::WorkerTerminated)
        ));
    }

    #[test]
    fn duplicate_render_is_only_queued_once() {
        let scheduler = scheduler(Duration::from_millis(80));
        open_document(&scheduler, DocumentId::new(7));
        let request = job(scheduler.current_generation(), RenderKind::Page);
        assert!(scheduler.send_command(RenderCommand::RenderPage(request.clone())));
        assert!(!scheduler.send_command(RenderCommand::RenderPage(request)));
    }

    #[test]
    fn stale_generation_is_rejected_for_visible_work() {
        let scheduler = scheduler(Duration::ZERO);
        open_document(&scheduler, DocumentId::new(7));
        let stale_generation = scheduler.current_generation();
        let _ = scheduler.bump_generation();
        assert!(scheduler.send_command(RenderCommand::RenderPage(job(
            stale_generation,
            RenderKind::Page,
        ))));
        std::thread::sleep(Duration::from_millis(30));
        assert!(!matches!(
            scheduler.try_recv_event(),
            Ok(Some(RenderEvent::PageRendered { .. }))
        ));
    }

    #[test]
    fn render_for_replaced_document_is_ignored() {
        let scheduler = scheduler(Duration::ZERO);
        open_document(&scheduler, DocumentId::new(7));
        open_document(&scheduler, DocumentId::new(8));
        let mut request = job(scheduler.current_generation(), RenderKind::Page);
        request.document_id = DocumentId::new(7);
        assert!(scheduler.send_command(RenderCommand::RenderPage(request)));
        std::thread::sleep(Duration::from_millis(30));
        assert!(!matches!(
            scheduler.try_recv_event(),
            Ok(Some(RenderEvent::PageRendered { .. }))
        ));
    }

    #[test]
    fn full_background_queue_does_not_block_and_high_priority_stays_available() {
        let scheduler = scheduler(Duration::from_millis(250));
        open_document(&scheduler, DocumentId::new(7));
        assert!(scheduler.send_command(RenderCommand::RenderPage(job(
            scheduler.current_generation(),
            RenderKind::Page,
        ))));
        std::thread::sleep(Duration::from_millis(10));

        let start = Instant::now();
        let mut rejected = 0;
        for index in 0..512 {
            if !scheduler.send_command(RenderCommand::FetchTextGeometry {
                document_id: DocumentId::new(7),
                generation: scheduler.current_generation(),
                page_index: PageIndex::from_raw(index % 4),
            }) {
                rejected += 1;
            }
        }
        assert!(rejected > 0);
        assert!(start.elapsed() < Duration::from_millis(100));
        assert!(scheduler.send_command(RenderCommand::CloseDocument(DocumentId::new(7))));
    }
}
