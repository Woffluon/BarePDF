use crate::cache::{CacheKey, SharedBitmapCache};
use barepdf_core::{DocumentId, MemoryBudget, PageIndex, PdfError, RequestId, Rotation};
use barepdf_pdf::{PdfBackend, PdfDocument, RawBitmap, TextSpan};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Visible = 0,
    Prefetch = 1,
    Thumbnail = 2,
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
        page_index: PageIndex,
    },
    FetchTextGeometry {
        document_id: DocumentId,
        page_index: PageIndex,
    },
    CloseDocument(DocumentId),
}

pub enum RenderEvent {
    DocumentOpened {
        document_id: DocumentId,
        page_count: u32,
        first_page_dimensions: (f32, f32),
        all_page_dimensions: Vec<(f32, f32)>,
    },
    PageRendered {
        request_id: RequestId,
        generation: u64,
        document_id: DocumentId,
        page_index: PageIndex,
        bitmap: Arc<RawBitmap>,
    },
    TextExtracted {
        document_id: DocumentId,
        page_index: PageIndex,
        text: String,
        spans: Vec<TextSpan>,
    },
    TextGeometryFetched {
        document_id: DocumentId,
        page_index: PageIndex,
        geometry: barepdf_core::PageTextGeometry,
    },
    Error {
        request_id: Option<RequestId>,
        error: PdfError,
    },
}

pub struct RenderScheduler {
    cmd_sender: Sender<RenderCommand>,
    event_receiver: Receiver<RenderEvent>,
    current_generation: Arc<AtomicU64>,
    cache: SharedBitmapCache,
}

impl RenderScheduler {
    pub fn spawn<B: PdfBackend + 'static>(backend: B, budget: MemoryBudget) -> Self {
        let (cmd_tx, cmd_rx) = bounded::<RenderCommand>(128);
        let (event_tx, event_rx) = bounded::<RenderEvent>(128);
        let current_gen = Arc::new(AtomicU64::new(1));
        let cache = SharedBitmapCache::new(budget);

        let cache_clone = cache.clone();
        let gen_clone = current_gen.clone();

        thread::spawn(move || {
            let mut active_doc: Option<(DocumentId, Box<dyn PdfDocument>)> = None;

            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    RenderCommand::OpenDocument {
                        document_id,
                        path,
                        password,
                    } => match backend.open_path(&path, password.as_deref()) {
                        Ok(doc) => {
                            let count = doc.page_count().get();
                            let all_dims = doc.all_page_dimensions().unwrap_or_default();
                            let first_dims = all_dims.first().copied().unwrap_or((612.0, 792.0));
                            active_doc = Some((document_id, doc));
                            let _ = event_tx.send(RenderEvent::DocumentOpened {
                                document_id,
                                page_count: count,
                                first_page_dimensions: first_dims,
                                all_page_dimensions: all_dims,
                            });
                        }
                        Err(err) => {
                            let _ = event_tx.send(RenderEvent::Error {
                                request_id: None,
                                error: err,
                            });
                        }
                    },
                    RenderCommand::RenderPage(job) => {
                        // Check if generation stale
                        if job.generation < gen_clone.load(Ordering::Relaxed) {
                            continue;
                        }

                        let cache_key = CacheKey {
                            document_id: job.document_id,
                            page_index: job.page_index,
                            target_width: job.target_width,
                            target_height: job.target_height,
                            rotation: job.rotation,
                        };

                        if let Some(cached) = cache_clone.get(&cache_key) {
                            let _ = event_tx.send(RenderEvent::PageRendered {
                                request_id: job.request_id,
                                generation: job.generation,
                                document_id: job.document_id,
                                page_index: job.page_index,
                                bitmap: cached,
                            });
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
                                        let arc_bitmap = cache_clone.insert(cache_key, raw_bitmap);
                                        let _ = event_tx.send(RenderEvent::PageRendered {
                                            request_id: job.request_id,
                                            generation: job.generation,
                                            document_id: job.document_id,
                                            page_index: job.page_index,
                                            bitmap: arc_bitmap,
                                        });
                                    }
                                    Err(err) => {
                                        let _ = event_tx.send(RenderEvent::Error {
                                            request_id: Some(job.request_id),
                                            error: err,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    RenderCommand::ExtractText {
                        document_id,
                        page_index,
                    } => {
                        if let Some((doc_id, ref doc)) = active_doc {
                            if doc_id == document_id {
                                let text = doc.extract_text(page_index).unwrap_or_default();
                                let spans = doc.extract_text_spans(page_index).unwrap_or_default();
                                let _ = event_tx.send(RenderEvent::TextExtracted {
                                    document_id,
                                    page_index,
                                    text,
                                    spans,
                                });
                            }
                        }
                    }
                    RenderCommand::FetchTextGeometry {
                        document_id,
                        page_index,
                    } => {
                        if let Some((doc_id, ref doc)) = active_doc {
                            if doc_id == document_id {
                                if let Ok(geom) = doc.get_page_text_geometry(page_index) {
                                    let _ = event_tx.send(RenderEvent::TextGeometryFetched {
                                        document_id,
                                        page_index,
                                        geometry: geom,
                                    });
                                }
                            }
                        }
                    }
                    RenderCommand::CloseDocument(doc_id) => {
                        if let Some((id, _)) = active_doc {
                            if id == doc_id {
                                active_doc = None;
                                cache_clone.clear();
                            }
                        }
                    }
                }
            }
        });

        Self {
            cmd_sender: cmd_tx,
            event_receiver: event_rx,
            current_generation: current_gen,
            cache,
        }
    }

    pub fn bump_generation(&self) -> u64 {
        self.current_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn current_generation(&self) -> u64 {
        self.current_generation.load(Ordering::SeqCst)
    }

    pub fn send_command(&self, cmd: RenderCommand) {
        let _ = self.cmd_sender.send(cmd);
    }

    pub fn try_recv_event(&self) -> Option<RenderEvent> {
        self.event_receiver.try_recv().ok()
    }

    pub fn cache(&self) -> &SharedBitmapCache {
        &self.cache
    }
}
