use crate::cache::{CacheKey, SharedBitmapCache};
use barepdf_core::{DocumentId, MemoryBudget, PageIndex, PdfError, RequestId, Rotation};
use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    high_cmd_sender: Sender<RenderCommand>,
    low_cmd_sender: Sender<RenderCommand>,
    event_receiver: Receiver<RenderEvent>,
    current_generation: Arc<AtomicU64>,
    pending_renders: Arc<Mutex<HashSet<RenderRequestKey>>>,
    cache: SharedBitmapCache,
}

impl RenderScheduler {
    pub fn spawn<B: PdfBackend + 'static>(backend: B, budget: MemoryBudget) -> Self {
        // Control and visible work must never block the UI thread.
        let (high_tx, high_rx) = unbounded::<RenderCommand>();
        let (low_tx, low_rx) = bounded::<RenderCommand>(128);
        let (event_tx, event_rx) = bounded::<RenderEvent>(128);
        let current_gen = Arc::new(AtomicU64::new(1));
        let pending_renders = Arc::new(Mutex::new(HashSet::new()));
        let cache = SharedBitmapCache::new(budget);

        let cache_clone = cache.clone();
        let gen_clone = current_gen.clone();
        let pending_clone = pending_renders.clone();

        thread::spawn(move || {
            let mut active_doc: Option<(DocumentId, Box<dyn PdfDocument>)> = None;

            loop {
                let cmd = match high_rx.try_recv() {
                    Ok(command) => command,
                    Err(_) => match crossbeam_channel::select! {
                        recv(high_rx) -> message => message,
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
                            let count = doc.page_count().get();
                            let first_dims = doc
                                .page_dimensions(PageIndex::zero())
                                .unwrap_or((612.0, 792.0));
                            active_doc = Some((document_id, doc));
                            cache_clone.clear();
                            let _ = event_tx.send(RenderEvent::DocumentOpened {
                                document_id,
                                page_count: count,
                                first_page_dimensions: first_dims,
                            });
                        }
                        Err(error) => {
                            let _ = event_tx.send(RenderEvent::Error {
                                request_id: None,
                                document_id,
                                generation: None,
                                error,
                            });
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

                        if let Some(bitmap) = cache_clone.get(&cache_key) {
                            let _ = event_tx.send(RenderEvent::PageRendered {
                                request_id: job.request_id,
                                generation: job.generation,
                                document_id: job.document_id,
                                page_index: job.page_index,
                                kind: job.kind,
                                bitmap,
                            });
                            finish();
                            continue;
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
                                        let bitmap = cache_clone.insert(cache_key, raw_bitmap);
                                        let _ = event_tx.send(RenderEvent::PageRendered {
                                            request_id: job.request_id,
                                            generation: job.generation,
                                            document_id: job.document_id,
                                            page_index: job.page_index,
                                            kind: job.kind,
                                            bitmap,
                                        });
                                    }
                                    Err(error) => {
                                        let _ = event_tx.send(RenderEvent::Error {
                                            request_id: Some(job.request_id),
                                            document_id: job.document_id,
                                            generation: Some(job.generation),
                                            error,
                                        });
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
                                let text = doc.extract_text(page_index).unwrap_or_default();
                                let spans = doc.extract_text_spans(page_index).unwrap_or_default();
                                let _ = event_tx.send(RenderEvent::TextExtracted {
                                    document_id,
                                    generation,
                                    page_index,
                                    text,
                                    spans,
                                });
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
                                if let Ok(geometry) = doc.get_page_text_geometry(page_index) {
                                    let _ = event_tx.send(RenderEvent::TextGeometryFetched {
                                        document_id,
                                        generation,
                                        page_index,
                                        geometry,
                                    });
                                }
                            }
                        }
                    }
                    RenderCommand::FetchOutline { document_id } => {
                        if let Some((doc_id, ref doc)) = active_doc {
                            if doc_id == document_id {
                                if let Ok(outline) = doc.get_outline() {
                                    let _ = event_tx.send(RenderEvent::OutlineFetched {
                                        document_id,
                                        outline,
                                    });
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
                                let end = start.saturating_add(count).min(doc.page_count().get());
                                let dimensions = (start..end)
                                    .filter_map(|index| {
                                        doc.page_dimensions(PageIndex::from_raw(index)).ok()
                                    })
                                    .collect();
                                let _ = event_tx.send(RenderEvent::PageDimensionsFetched {
                                    document_id,
                                    start,
                                    dimensions,
                                });
                            }
                        }
                    }
                    RenderCommand::CloseDocument(doc_id) => {
                        if active_doc.as_ref().is_some_and(|(id, _)| *id == doc_id) {
                            active_doc = None;
                            cache_clone.clear();
                        }
                    }
                }
            }
        });

        Self {
            high_cmd_sender: high_tx,
            low_cmd_sender: low_tx,
            event_receiver: event_rx,
            current_generation: current_gen,
            pending_renders,
            cache,
        }
    }

    pub fn bump_generation(&self) -> u64 {
        let generation = self.current_generation.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut pending) = self.pending_renders.lock() {
            pending.retain(|key| key.generation == generation);
        }
        generation
    }

    pub fn current_generation(&self) -> u64 {
        self.current_generation.load(Ordering::Acquire)
    }

    /// Queues work without blocking the caller. Returns false for duplicate work or a full
    /// background queue so the UI can remain responsive under load.
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

        let high_priority = matches!(
            &cmd,
            RenderCommand::OpenDocument { .. }
                | RenderCommand::CloseDocument(_)
                | RenderCommand::RenderPage(RenderJob {
                    priority: Priority::Visible,
                    ..
                })
        );

        let result = if high_priority {
            self.high_cmd_sender.send(cmd).map_err(|_| ())
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

    pub fn try_recv_event(&self) -> Option<RenderEvent> {
        self.event_receiver.try_recv().ok()
    }

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
        fn page_count(&self) -> PageCount {
            PageCount::new(4).expect("non-zero")
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
                Some(RenderEvent::DocumentOpened { document_id, .. }) if document_id == id
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
        scheduler.bump_generation();
        assert!(scheduler.send_command(RenderCommand::RenderPage(job(
            stale_generation,
            RenderKind::Page,
        ))));
        std::thread::sleep(Duration::from_millis(30));
        assert!(!matches!(
            scheduler.try_recv_event(),
            Some(RenderEvent::PageRendered { .. })
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
            Some(RenderEvent::PageRendered { .. })
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
