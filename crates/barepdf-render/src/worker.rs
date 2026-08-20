use crate::cache::{BitmapCache, CacheKey};
use crate::error::RenderError;
use crate::observability::RenderObservability;
use crate::protocol::{RenderCommand, RenderEvent, RenderJob, RenderRequestKey};
use crate::queue::receive_command;
use barepdf_core::{DocumentId, MemoryBudget, PageIndex, PdfError, RequestId};
use barepdf_pdf::{PdfBackend, PdfDocument, RawBitmap};
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
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
    active_doc: Option<(DocumentId, Box<dyn PdfDocument>)>,
    cache: BitmapCache,
    current_generation: Arc<AtomicU64>,
    pending_renders: Arc<Mutex<HashSet<RenderRequestKey>>>,
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
            active_doc: None,
            cache: BitmapCache::new(budget),
            current_generation,
            pending_renders,
            shutdown_receiver,
            critical_event_sender,
            event_sender,
            observability: RenderObservability::default(),
        }
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
            } => self.open_document(document_id, &path, password.as_deref()),
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
                self.active_doc = Some((document_id, document));
                self.cache.clear();
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
        if let Ok(mut pending) = self.pending_renders.lock() {
            pending.remove(&pending_key);
        }
        emitted
    }

    fn render_page_inner(&mut self, job: &RenderJob) -> bool {
        if job.generation != self.current_generation.load(Ordering::Acquire) {
            self.observability.stale_work("generation");
            return true;
        }
        let cache_key = CacheKey {
            document_id: job.document_id,
            page_index: job.page_index,
            target_width: job.target_width,
            target_height: job.target_height,
            rotation: job.rotation,
        };
        if let Some(bitmap) = self.cache.get(&cache_key) {
            self.observability.cache_hit();
            return self.emit_rendered(job, bitmap);
        }
        self.observability.cache_miss();
        let Some((document_id, document)) = self.active_doc.as_ref() else {
            self.observability.stale_work("no_active_document");
            return true;
        };
        if *document_id != job.document_id {
            self.observability.stale_work("document_replaced");
            return true;
        }
        match document.render_page(
            job.page_index,
            job.target_width,
            job.target_height,
            job.rotation,
        ) {
            Ok(bitmap) => {
                if self.shutdown_requested() {
                    return false;
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
        emit_lossy_event(
            &self.shutdown_receiver,
            &self.event_sender,
            &self.observability,
            RenderEvent::PageRendered {
                request_id: job.request_id,
                generation: job.generation,
                document_id: job.document_id,
                page_index: job.page_index,
                kind: job.kind,
                bitmap,
            },
        )
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
        &self,
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
    ) -> bool {
        if generation != self.current_generation.load(Ordering::Acquire) {
            self.observability.stale_work("generation");
            return true;
        }
        let Some((active_document_id, document)) = self.active_doc.as_ref() else {
            self.observability.stale_work("no_active_document");
            return true;
        };
        if *active_document_id != document_id {
            self.observability.stale_work("document_replaced");
            return true;
        }
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
        &self,
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
    ) -> bool {
        if generation != self.current_generation.load(Ordering::Acquire) {
            self.observability.stale_work("generation");
            return true;
        }
        let Some((active_document_id, document)) = self.active_doc.as_ref() else {
            self.observability.stale_work("no_active_document");
            return true;
        };
        if *active_document_id != document_id {
            self.observability.stale_work("document_replaced");
            return true;
        }
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

    fn fetch_outline(&self, document_id: DocumentId) -> bool {
        let Some((active_document_id, document)) = self.active_doc.as_ref() else {
            self.observability.stale_work("no_active_document");
            return true;
        };
        if *active_document_id != document_id {
            self.observability.stale_work("document_replaced");
            return true;
        }
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

    fn fetch_page_dimensions(&self, document_id: DocumentId, start: u32, count: u32) -> bool {
        let Some((active_document_id, document)) = self.active_doc.as_ref() else {
            self.observability.stale_work("no_active_document");
            return true;
        };
        if *active_document_id != document_id {
            self.observability.stale_work("document_replaced");
            return true;
        }
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
        if self
            .active_doc
            .as_ref()
            .is_some_and(|(id, _)| *id == document_id)
        {
            self.active_doc = None;
            self.cache.clear();
        }
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
