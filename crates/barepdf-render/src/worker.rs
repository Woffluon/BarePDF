use crate::cache::{BitmapCache, CacheKey};
use crate::error::RenderError;
use crate::memory_budget::{calculate_adaptive_memory_budget, SystemHardwareProfile};
use crate::observability::RenderObservability;
use crate::protocol::{RenderCommand, RenderEvent, RenderJob, RenderKind, RenderRequestKey};
use crate::queue::receive_command;
use barepdf_core::{DocumentId, MemoryBudget, PageIndex, PdfError, RequestId};
use barepdf_pdf::{PdfBackend, PdfDocument, RawBitmap};
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

fn emit_lossy_event(
    shutdown_receiver: &Receiver<()>,
    event_sender: &Sender<RenderEvent>,
    observability: &RenderObservability,
    event: RenderEvent,
) -> bool {
    if !matches!(shutdown_receiver.try_recv(), Err(TryRecvError::Empty)) {
        return false;
    }
    match event_sender.try_send(event) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            observability.event_dropped();
            true
        }
        Err(TrySendError::Disconnected(_)) => false,
    }
}

pub(crate) fn emit_critical(
    shutdown_receiver: &Receiver<()>,
    event_sender: &Sender<RenderEvent>,
    event: RenderEvent,
) -> bool {
    if !matches!(shutdown_receiver.try_recv(), Err(TryRecvError::Empty)) {
        return false;
    }
    crossbeam_channel::select! {
        send(event_sender, event) -> result => result.is_ok(),
        recv(shutdown_receiver) -> _ => false,
    }
}

pub(crate) struct RenderWorker<B> {
    backend: B,
    active_docs: lru::LruCache<DocumentId, Box<dyn PdfDocument>>,
    cache: BitmapCache,
    current_generation: Arc<AtomicU64>,
    pending_renders: Arc<Mutex<HashSet<RenderRequestKey>>>,
    invert_colors: Arc<AtomicBool>,
    shutdown_receiver: Receiver<()>,
    critical_event_sender: Sender<RenderEvent>,
    event_sender: Sender<RenderEvent>,
    observability: RenderObservability,
}

impl<B: PdfBackend> RenderWorker<B> {
    pub(crate) fn new(
        backend: B,
        budget: MemoryBudget,
        current_generation: Arc<AtomicU64>,
        pending_renders: Arc<Mutex<HashSet<RenderRequestKey>>>,
        shutdown_receiver: Receiver<()>,
        critical_event_sender: Sender<RenderEvent>,
        event_sender: Sender<RenderEvent>,
    ) -> Self {
        Self {
            backend,
            active_docs: lru::LruCache::new(
                std::num::NonZeroUsize::new(4).unwrap_or(std::num::NonZeroUsize::MIN),
            ),
            cache: BitmapCache::new(budget),
            current_generation,
            pending_renders,
            invert_colors: Arc::new(AtomicBool::new(false)),
            shutdown_receiver,
            critical_event_sender,
            event_sender,
            observability: RenderObservability::default(),
        }
    }

    #[must_use]
    pub(crate) fn with_invert_colors(mut self, invert_colors: Arc<AtomicBool>) -> Self {
        self.invert_colors = invert_colors;
        self
    }

    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn with_hardware_profile(
        backend: B,
        profile: &SystemHardwareProfile,
        current_generation: Arc<AtomicU64>,
        pending_renders: Arc<Mutex<HashSet<RenderRequestKey>>>,
        shutdown_receiver: Receiver<()>,
        critical_event_sender: Sender<RenderEvent>,
        event_sender: Sender<RenderEvent>,
    ) -> Self {
        let budget = calculate_adaptive_memory_budget(profile);
        Self::new(
            backend,
            budget,
            current_generation,
            pending_renders,
            shutdown_receiver,
            critical_event_sender,
            event_sender,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn set_hardware_profile(&mut self, profile: &SystemHardwareProfile) {
        let budget = calculate_adaptive_memory_budget(profile);
        self.set_budget(budget);
    }

    #[allow(dead_code)]
    pub(crate) fn set_budget(&mut self, budget: MemoryBudget) {
        self.cache.set_budget(budget);
    }

    #[allow(dead_code)]
    #[must_use]
    pub(crate) const fn budget(&self) -> MemoryBudget {
        self.cache.budget()
    }

    pub(crate) fn run(
        mut self,
        control_receiver: &Receiver<RenderCommand>,
        visible_receiver: &Receiver<RenderCommand>,
        low_receiver: &Receiver<RenderCommand>,
    ) {
        let mut visible_budget = 0;
        while let Some(command) = receive_command(
            &self.shutdown_receiver,
            control_receiver,
            visible_receiver,
            low_receiver,
            &mut visible_budget,
        ) {
            if !self.handle_command(command) {
                break;
            }
        }
    }

    fn handle_command(&mut self, command: RenderCommand) -> bool {
        match command {
            RenderCommand::OpenDocument {
                document_id,
                path,
                password,
            } => self.open_document(
                document_id,
                &path,
                password.as_ref().map(barepdf_core::SecretPassword::expose),
            ),
            RenderCommand::RenderPage(job) => self.render_page(&job),
            RenderCommand::ExtractText {
                document_id,
                generation,
                page_index,
            } => self.extract_text(document_id, generation, page_index),
            RenderCommand::FetchTextGeometry {
                document_id,
                generation,
                page_index,
            } => self.fetch_text_geometry(document_id, generation, page_index),
            RenderCommand::FetchOutline { document_id } => self.fetch_outline(document_id),
            RenderCommand::FetchPageDimensions {
                document_id,
                start,
                count,
            } => self.fetch_page_dimensions(document_id, start, count),
            RenderCommand::CloseDocument(document_id) => self.close_document(document_id),
            RenderCommand::Shutdown => false,
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(document_id = document_id.get())
    )]
    fn open_document(
        &mut self,
        document_id: DocumentId,
        path: &Path,
        password: Option<&str>,
    ) -> bool {
        let result = self.backend.open_path(path, password).and_then(|document| {
            let document_info = document.page_count().and_then(|count| {
                document
                    .page_dimensions(PageIndex::zero())
                    .map(|dimensions| (count, dimensions))
            })?;
            Ok((document, document_info))
        });
        match result {
            Ok((document, (count, first_page_dimensions))) => {
                self.active_docs.put(document_id, document);
                emit_critical(
                    &self.shutdown_receiver,
                    &self.critical_event_sender,
                    RenderEvent::DocumentOpened {
                        document_id,
                        page_count: count.get(),
                        first_page_dimensions,
                    },
                )
            }
            Err(error) => self.emit_error(None, document_id, None, error),
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            document_id = job.document_id.get(),
            page = job.page_index.get(),
            generation = job.generation
        )
    )]
    fn render_page(&mut self, job: &RenderJob) -> bool {
        let pending_key = RenderRequestKey::from(job);
        let emitted = self.render_page_inner(job);
        self.pending_renders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&pending_key);
        emitted
    }

    fn render_page_inner(&mut self, job: &RenderJob) -> bool {
        if job.generation != self.current_generation.load(Ordering::Acquire) {
            self.observability.stale_work("generation");
            return true;
        }
        let invert_colors = self.invert_colors.load(Ordering::Acquire);
        let cache_key = CacheKey {
            document_id: job.document_id,
            page_index: job.page_index,
            target_width: job.target_width,
            target_height: job.target_height,
            rotation: job.rotation,
            invert_colors,
        };
        if let Some(bitmap) = self.cache.get(&cache_key) {
            self.observability.cache_hit();
            return self.emit_rendered(job, bitmap);
        }
        self.observability.cache_miss();
        let Some(document) = self.active_docs.get(&job.document_id) else {
            self.observability.stale_work("document_replaced");
            return true;
        };
        match document.render_page(
            job.page_index,
            job.target_width,
            job.target_height,
            job.rotation,
        ) {
            Ok(mut bitmap) => {
                if self.shutdown_requested() {
                    return false;
                }
                if invert_colors {
                    bitmap.invert_rgb();
                }
                let bitmap = self.cache.insert(cache_key, bitmap);
                self.emit_rendered(job, bitmap)
            }
            Err(error) => self.emit_error(
                Some(job.request_id),
                job.document_id,
                Some(job.generation),
                error,
            ),
        }
    }

    fn emit_rendered(&self, job: &RenderJob, bitmap: Arc<RawBitmap>) -> bool {
        let event = RenderEvent::PageRendered {
            request_id: job.request_id,
            generation: job.generation,
            document_id: job.document_id,
            page_index: job.page_index,
            kind: job.kind,
            bitmap,
        };
        match job.kind {
            RenderKind::Page => {
                if !matches!(self.shutdown_receiver.try_recv(), Err(TryRecvError::Empty)) {
                    return false;
                }
                match self.event_sender.try_send(event) {
                    Ok(()) => true,
                    Err(TrySendError::Full(event)) => {
                        emit_critical(&self.shutdown_receiver, &self.critical_event_sender, event)
                    }
                    Err(TrySendError::Disconnected(_)) => false,
                }
            }
            RenderKind::Thumbnail => emit_lossy_event(
                &self.shutdown_receiver,
                &self.event_sender,
                &self.observability,
                event,
            ),
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            document_id = document_id.get(),
            page = page_index.get(),
            generation
        )
    )]
    fn extract_text(
        &mut self,
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
    ) -> bool {
        if generation != self.current_generation.load(Ordering::Acquire) {
            self.observability.stale_work("generation");
            return true;
        }
        let Some(document) = self.active_docs.get(&document_id) else {
            self.observability.stale_work("document_replaced");
            return true;
        };
        match document.extract_text(page_index).and_then(|text| {
            document
                .extract_text_spans(page_index)
                .map(|spans| (text, spans))
        }) {
            Ok((text, spans)) => emit_lossy_event(
                &self.shutdown_receiver,
                &self.event_sender,
                &self.observability,
                RenderEvent::TextExtracted {
                    document_id,
                    generation,
                    page_index,
                    text,
                    spans,
                },
            ),
            Err(error) => self.emit_error(None, document_id, Some(generation), error),
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            document_id = document_id.get(),
            page = page_index.get(),
            generation
        )
    )]
    fn fetch_text_geometry(
        &mut self,
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
    ) -> bool {
        if generation != self.current_generation.load(Ordering::Acquire) {
            self.observability.stale_work("generation");
            return true;
        }
        let Some(document) = self.active_docs.get(&document_id) else {
            self.observability.stale_work("document_replaced");
            return true;
        };
        match document.get_page_text_geometry(page_index) {
            Ok(geometry) => emit_lossy_event(
                &self.shutdown_receiver,
                &self.event_sender,
                &self.observability,
                RenderEvent::TextGeometryFetched {
                    document_id,
                    generation,
                    page_index,
                    geometry,
                },
            ),
            Err(error) => self.emit_error(None, document_id, Some(generation), error),
        }
    }

    fn fetch_outline(&mut self, document_id: DocumentId) -> bool {
        let Some(document) = self.active_docs.get(&document_id) else {
            self.observability.stale_work("document_replaced");
            return true;
        };
        match document.get_outline() {
            Ok(outline) => emit_critical(
                &self.shutdown_receiver,
                &self.critical_event_sender,
                RenderEvent::OutlineFetched {
                    document_id,
                    outline,
                },
            ),
            Err(error) => self.emit_error(None, document_id, None, error),
        }
    }

    fn fetch_page_dimensions(&mut self, document_id: DocumentId, start: u32, count: u32) -> bool {
        let Some(document) = self.active_docs.get(&document_id) else {
            self.observability.stale_work("document_replaced");
            return true;
        };
        let dimensions = document.page_count().and_then(|page_count| {
            let end = start.saturating_add(count).min(page_count.get());
            (start..end)
                .map(|index| document.page_dimensions(PageIndex::from_raw(index)))
                .collect()
        });
        match dimensions {
            Ok(dimensions) => emit_critical(
                &self.shutdown_receiver,
                &self.critical_event_sender,
                RenderEvent::PageDimensionsFetched {
                    document_id,
                    start,
                    dimensions,
                },
            ),
            Err(error) => self.emit_error(None, document_id, None, error),
        }
    }

    fn close_document(&mut self, document_id: DocumentId) -> bool {
        self.active_docs.pop(&document_id);
        self.cache.evict_document(document_id);
        true
    }

    fn shutdown_requested(&self) -> bool {
        !matches!(self.shutdown_receiver.try_recv(), Err(TryRecvError::Empty))
    }

    fn emit_error(
        &self,
        request_id: Option<RequestId>,
        document_id: DocumentId,
        generation: Option<u64>,
        error: PdfError,
    ) -> bool {
        emit_critical(
            &self.shutdown_receiver,
            &self.critical_event_sender,
            RenderEvent::Error {
                request_id,
                document_id,
                generation,
                error: RenderError::from(error),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Priority, RenderKind};
    use barepdf_core::{DocumentId, PageCount, PageIndex, Rotation};
    use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
    use crossbeam_channel::unbounded;
    use std::sync::atomic::AtomicU64;

    struct TestBackend;
    struct TestDocument;

    impl PdfBackend for TestBackend {
        fn open_path(
            &self,
            _path: &Path,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Ok(Box::new(TestDocument))
        }

        fn open_bytes(
            &self,
            _bytes: Vec<u8>,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Ok(Box::new(TestDocument))
        }
    }

    impl PdfDocument for TestDocument {
        fn page_count(&self) -> Result<PageCount, PdfError> {
            Ok(PageCount::new(10).expect("non-zero"))
        }

        fn page_dimensions(&self, _page_index: PageIndex) -> Result<(f32, f32), PdfError> {
            Ok((100.0, 100.0))
        }

        fn render_page(
            &self,
            _page_index: PageIndex,
            target_width: u32,
            target_height: u32,
            _rotation: Rotation,
        ) -> Result<RawBitmap, PdfError> {
            let bytes = usize::try_from(target_width)
                .unwrap_or(0)
                .checked_mul(usize::try_from(target_height).unwrap_or(0))
                .and_then(|pixels| pixels.checked_mul(4))
                .unwrap_or(0);
            RawBitmap::new(target_width, target_height, vec![0; bytes]).map_err(|_| {
                PdfError::RenderingFailed {
                    page_index: 0,
                    reason: "invalid bitmap".into(),
                }
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
            _page_index: PageIndex,
        ) -> Result<barepdf_core::PageTextGeometry, PdfError> {
            Err(PdfError::PlatformError("not supported".into()))
        }

        fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError> {
            Ok(Vec::new())
        }
    }

    fn create_test_worker(budget: MemoryBudget) -> RenderWorker<TestBackend> {
        let (_shutdown_tx, shutdown_rx) = unbounded();
        let (critical_tx, _critical_rx) = unbounded();
        let (event_tx, _event_rx) = unbounded();
        RenderWorker::new(
            TestBackend,
            budget,
            Arc::new(AtomicU64::new(1)),
            Arc::new(Mutex::new(HashSet::new())),
            shutdown_rx,
            critical_tx,
            event_tx,
        )
    }

    fn create_adaptive_worker(profile: &SystemHardwareProfile) -> RenderWorker<TestBackend> {
        let (_shutdown_tx, shutdown_rx) = unbounded();
        let (critical_tx, _critical_rx) = unbounded();
        let (event_tx, _event_rx) = unbounded();
        RenderWorker::with_hardware_profile(
            TestBackend,
            profile,
            Arc::new(AtomicU64::new(1)),
            Arc::new(Mutex::new(HashSet::new())),
            shutdown_rx,
            critical_tx,
            event_tx,
        )
    }

    #[test]
    fn worker_uses_adaptive_budget_from_hardware_profile() {
        let low_ram = SystemHardwareProfile {
            total_ram_mb: 2048,
            primary_screen_dpi: 96.0,
        };
        let worker_low = create_adaptive_worker(&low_ram);
        assert_eq!(worker_low.budget().get(), 256 * 1024 * 1024);

        let mid_ram = SystemHardwareProfile {
            total_ram_mb: 8192,
            primary_screen_dpi: 96.0,
        };
        let worker_mid = create_adaptive_worker(&mid_ram);
        assert_eq!(worker_mid.budget().get(), 384 * 1024 * 1024);

        let high_ram = SystemHardwareProfile {
            total_ram_mb: 32768,
            primary_screen_dpi: 144.0,
        };
        let worker_high = create_adaptive_worker(&high_ram);
        assert_eq!(worker_high.budget().get(), 1024 * 1024 * 1024);
    }

    #[test]
    fn worker_set_hardware_profile_dynamically_adjusts_budget() {
        let mut worker = create_test_worker(MemoryBudget::new(64 * 1024 * 1024));
        assert_eq!(worker.budget().get(), 64 * 1024 * 1024);

        worker.set_hardware_profile(&SystemHardwareProfile {
            total_ram_mb: 16384,
            primary_screen_dpi: 96.0,
        });
        assert_eq!(worker.budget().get(), 1024 * 1024 * 1024);
    }

    #[test]
    fn worker_evicts_least_recently_used_bitmap_when_budget_exceeded() {
        let (_shutdown_tx, shutdown_rx) = unbounded();
        let (critical_tx, _critical_rx) = unbounded();
        let (event_tx, _event_rx) = unbounded();
        let mut worker = RenderWorker::new(
            TestBackend,
            MemoryBudget::new(8),
            Arc::new(AtomicU64::new(1)),
            Arc::new(Mutex::new(HashSet::new())),
            shutdown_rx,
            critical_tx,
            event_tx,
        );
        let doc_id = DocumentId::new(1);
        assert!(worker.open_document(doc_id, Path::new("dummy.pdf"), None));

        // Render page 1 (4 bytes: 1x1 RGBA)
        assert!(worker.render_page(&RenderJob {
            request_id: RequestId::new(1),
            generation: 1,
            document_id: doc_id,
            page_index: PageIndex::from_raw(1),
            target_width: 1,
            target_height: 1,
            rotation: Rotation::Degrees0,
            priority: Priority::Visible,
            kind: RenderKind::Page,
        }));

        // Render page 2 (4 bytes: 1x1 RGBA) -> total 8 bytes
        assert!(worker.render_page(&RenderJob {
            request_id: RequestId::new(2),
            generation: 1,
            document_id: doc_id,
            page_index: PageIndex::from_raw(2),
            target_width: 1,
            target_height: 1,
            rotation: Rotation::Degrees0,
            priority: Priority::Visible,
            kind: RenderKind::Page,
        }));

        // Render page 3 (4 bytes: 1x1 RGBA) -> evicts page 1 (budget is 8 bytes)
        assert!(worker.render_page(&RenderJob {
            request_id: RequestId::new(3),
            generation: 1,
            document_id: doc_id,
            page_index: PageIndex::from_raw(3),
            target_width: 1,
            target_height: 1,
            rotation: Rotation::Degrees0,
            priority: Priority::Visible,
            kind: RenderKind::Page,
        }));

        assert_eq!(worker.cache.current_bytes(), 8);
        assert!(worker
            .cache
            .get(&CacheKey {
                document_id: doc_id,
                page_index: PageIndex::from_raw(1),
                target_width: 1,
                target_height: 1,
                rotation: Rotation::Degrees0,
                invert_colors: false,
            })
            .is_none());
        assert!(worker
            .cache
            .get(&CacheKey {
                document_id: doc_id,
                page_index: PageIndex::from_raw(2),
                target_width: 1,
                target_height: 1,
                rotation: Rotation::Degrees0,
                invert_colors: false,
            })
            .is_some());
        assert!(worker
            .cache
            .get(&CacheKey {
                document_id: doc_id,
                page_index: PageIndex::from_raw(3),
                target_width: 1,
                target_height: 1,
                rotation: Rotation::Degrees0,
                invert_colors: false,
            })
            .is_some());
    }

    #[test]
    fn page_rendered_event_is_delivered_losslessly_when_event_channel_is_full() {
        let (_shutdown_tx, shutdown_rx) = crossbeam_channel::bounded(1);
        let (critical_tx, critical_rx) = crossbeam_channel::bounded(4);
        let (event_tx, event_rx) = crossbeam_channel::bounded(1);
        let mut worker = RenderWorker::new(
            TestBackend,
            MemoryBudget::new(1024 * 1024),
            Arc::new(AtomicU64::new(1)),
            Arc::new(Mutex::new(HashSet::new())),
            shutdown_rx,
            critical_tx,
            event_tx.clone(),
        );
        let doc_id = DocumentId::new(1);
        worker.open_document(doc_id, Path::new("dummy.pdf"), None);
        // Drain DocumentOpened from critical channel
        assert!(matches!(
            critical_rx.try_recv(),
            Ok(RenderEvent::DocumentOpened { .. })
        ));

        // Saturate the lossy event channel
        event_tx
            .try_send(RenderEvent::TextExtracted {
                document_id: doc_id,
                generation: 1,
                page_index: PageIndex::zero(),
                text: "saturated".into(),
                spans: Vec::new(),
            })
            .expect("saturate event_tx");

        // Render a visible page while event_tx is full
        assert!(worker.render_page(&RenderJob {
            request_id: RequestId::new(42),
            generation: 1,
            document_id: doc_id,
            page_index: PageIndex::zero(),
            target_width: 2,
            target_height: 2,
            rotation: Rotation::Degrees0,
            priority: Priority::Visible,
            kind: RenderKind::Page,
        }));

        // Drain the pre-existing event from event_rx
        let _ = event_rx.try_recv();

        // The PageRendered event must not be lost; it must arrive via critical_rx or event_rx
        let delivered = critical_rx.try_recv().or_else(|_| event_rx.try_recv());
        assert!(
            matches!(
                delivered,
                Ok(RenderEvent::PageRendered {
                    request_id,
                    document_id: id,
                    kind: RenderKind::Page,
                    ..
                }) if request_id == RequestId::new(42) && id == doc_id
            ),
            "expected PageRendered to be delivered losslessly"
        );
    }

    #[test]
    fn worker_inverts_pixels_in_background_when_invert_colors_is_enabled() {
        let (_shutdown_tx, shutdown_rx) = unbounded();
        let (critical_tx, _critical_rx) = unbounded();
        let (event_tx, event_rx) = unbounded();
        let invert_flag = Arc::new(AtomicBool::new(false));
        let mut worker = RenderWorker::new(
            TestBackend,
            MemoryBudget::new(1024 * 1024),
            Arc::new(AtomicU64::new(1)),
            Arc::new(Mutex::new(HashSet::new())),
            shutdown_rx,
            critical_tx,
            event_tx,
        )
        .with_invert_colors(Arc::clone(&invert_flag));

        let doc_id = DocumentId::new(1);
        assert!(worker.open_document(doc_id, Path::new("dummy.pdf"), None));

        // 1. Render with invert_colors = false -> pixels are [0, 0, 0, 0]
        assert!(worker.render_page(&RenderJob {
            request_id: RequestId::new(1),
            generation: 1,
            document_id: doc_id,
            page_index: PageIndex::zero(),
            target_width: 1,
            target_height: 1,
            rotation: Rotation::Degrees0,
            priority: Priority::Visible,
            kind: RenderKind::Page,
        }));
        match event_rx.try_recv() {
            Ok(RenderEvent::PageRendered { bitmap, .. }) => {
                assert_eq!(bitmap.pixels(), &[0, 0, 0, 0]);
            }
            other => panic!("expected normal PageRendered event, got {other:?}"),
        }

        // 2. Enable invert_colors = true -> RGB channels become 255, alpha stays 0
        invert_flag.store(true, Ordering::Release);
        assert!(worker.render_page(&RenderJob {
            request_id: RequestId::new(2),
            generation: 1,
            document_id: doc_id,
            page_index: PageIndex::zero(),
            target_width: 1,
            target_height: 1,
            rotation: Rotation::Degrees0,
            priority: Priority::Visible,
            kind: RenderKind::Page,
        }));
        match event_rx.try_recv() {
            Ok(RenderEvent::PageRendered { bitmap, .. }) => {
                assert_eq!(bitmap.pixels(), &[255, 255, 255, 0]);
            }
            other => panic!("expected inverted PageRendered event, got {other:?}"),
        }
    }
}
