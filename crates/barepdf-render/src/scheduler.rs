use crate::observability::RenderObservability;
use crate::protocol::RenderRequestKey;
use crate::worker::RenderWorker;
use crate::RenderError;
use barepdf_core::MemoryBudget;
use barepdf_pdf::PdfBackend;
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, TrySendError};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub use crate::protocol::{Priority, RenderCommand, RenderEvent, RenderJob, RenderKind};

const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(250);

pub struct RenderScheduler {
    worker: Mutex<Option<JoinHandle<()>>>,
    shutdown_sender: Sender<()>,
    done_receiver: Receiver<()>,
    control_cmd_sender: Sender<RenderCommand>,
    visible_cmd_sender: Sender<RenderCommand>,
    low_cmd_sender: Sender<RenderCommand>,
    critical_event_receiver: Receiver<RenderEvent>,
    event_receiver: Receiver<RenderEvent>,
    current_generation: Arc<AtomicU64>,
    pending_renders: Arc<Mutex<HashSet<RenderRequestKey>>>,
    observability: RenderObservability,
}

impl RenderScheduler {
    pub fn spawn<B: PdfBackend + 'static>(backend: B, budget: MemoryBudget) -> Self {
        // Control and visible work must never block the UI thread.
        let (control_tx, control_rx) = bounded::<RenderCommand>(8);
        let (visible_tx, visible_rx) = bounded::<RenderCommand>(32);
        let (low_tx, low_rx) = bounded::<RenderCommand>(128);
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (done_tx, done_rx) = bounded::<()>(1);
        let (critical_event_tx, critical_event_rx) = bounded::<RenderEvent>(32);
        let (event_tx, event_rx) = bounded::<RenderEvent>(128);
        let current_generation = Arc::new(AtomicU64::new(1));
        let pending_renders = Arc::new(Mutex::new(HashSet::new()));
        let observability = RenderObservability::default();
        let worker = thread::spawn({
            let current_generation = current_generation.clone();
            let pending_renders = pending_renders.clone();
            move || {
                RenderWorker::new(
                    backend,
                    budget,
                    current_generation,
                    pending_renders,
                    shutdown_rx,
                    critical_event_tx,
                    event_tx,
                )
                .run(&control_rx, &visible_rx, &low_rx);
                let _ = done_tx.try_send(());
            }
        });

        Self {
            worker: Mutex::new(Some(worker)),
            shutdown_sender: shutdown_tx,
            done_receiver: done_rx,
            control_cmd_sender: control_tx,
            visible_cmd_sender: visible_tx,
            low_cmd_sender: low_tx,
            critical_event_receiver: critical_event_rx,
            event_receiver: event_rx,
            current_generation,
            pending_renders,
            observability,
        }
    }

    /// Stops the worker and waits for it to release its active PDF document.
    ///
    /// # Errors
    ///
    /// Returns `RenderError::WorkerTerminated` when the worker panics or does not stop before the
    /// bounded shutdown deadline.
    pub fn shutdown(&self) -> Result<(), RenderError> {
        let _ = self.shutdown_sender.try_send(());
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| RenderError::WorkerTerminated)?;
        if worker.is_none() {
            return Ok(());
        }
        match self.done_receiver.recv_timeout(SHUTDOWN_JOIN_TIMEOUT) {
            Ok(()) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                let Some(handle) = worker.take() else {
                    return Ok(());
                };
                drop(worker);
                handle.join().map_err(|_| RenderError::WorkerTerminated)
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => Err(RenderError::WorkerTerminated),
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
            RenderCommand::OpenDocument { .. }
                | RenderCommand::CloseDocument(_)
                | RenderCommand::Shutdown
        );
        let visible_render = matches!(
            &cmd,
            RenderCommand::RenderPage(RenderJob {
                priority: Priority::Visible,
                ..
            })
        );

        let (queue, result) = if control_command {
            ("control", self.control_cmd_sender.try_send(cmd))
        } else if visible_render {
            ("visible", self.visible_cmd_sender.try_send(cmd))
        } else {
            ("low", self.low_cmd_sender.try_send(cmd))
        };

        match result {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.observability.queue_full(queue);
                if let (Some(key), Ok(mut pending)) = (pending_key, self.pending_renders.lock()) {
                    pending.remove(&key);
                }
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.observability.queue_disconnected(queue);
                if let (Some(key), Ok(mut pending)) = (pending_key, self.pending_renders.lock()) {
                    pending.remove(&key);
                }
                false
            }
        }
    }

    /// Returns `Ok(None)` while worker remains live without an event, and
    /// `RenderError::WorkerStopped` after worker event senders are disconnected.
    ///
    /// # Errors
    ///
    /// Returns `RenderError::WorkerStopped` when the worker has stopped and all queued events
    /// were already received.
    pub fn try_recv_event(&self) -> Result<Option<RenderEvent>, RenderError> {
        let critical_status = match self.critical_event_receiver.try_recv() {
            Ok(event) => return Ok(Some(event)),
            Err(status) => status,
        };
        match self.event_receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Disconnected)
                if matches!(critical_status, TryRecvError::Disconnected) =>
            {
                Err(RenderError::WorkerStopped)
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => Ok(None),
        }
    }
}

impl Drop for RenderScheduler {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::emit_critical;
    use barepdf_core::{
        DocumentId, PageCount, PageIndex, PageTextGeometry, PdfError, RequestId, Rotation,
    };
    use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    struct MockBackend {
        render_delay: Duration,
        render_started: Option<Sender<()>>,
    }

    struct MockDocument {
        render_delay: Duration,
        render_started: Option<Sender<()>>,
    }

    impl PdfBackend for MockBackend {
        fn open_path(
            &self,
            _path: &Path,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Ok(Box::new(MockDocument {
                render_delay: self.render_delay,
                render_started: self.render_started.clone(),
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
            if let Some(started) = &self.render_started {
                let _ = started.try_send(());
            }
            std::thread::sleep(self.render_delay);
            RawBitmap::new(1, 1, vec![0; 4]).map_err(|_| PdfError::RenderingFailed {
                page_index: 0,
                reason: "mock bitmap layout is invalid".into(),
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
        RenderScheduler::spawn(
            MockBackend {
                render_delay,
                render_started: None,
            },
            MemoryBudget::new(1024 * 1024),
        )
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
    fn disconnected_event_channels_report_worker_termination() {
        let (shutdown_sender, _shutdown_receiver) = bounded(1);
        let (_done_sender, done_receiver) = bounded(1);
        let (control_sender, _control_receiver) = bounded(1);
        let (visible_sender, _visible_receiver) = bounded(1);
        let (low_sender, _low_receiver) = bounded(1);
        let (critical_sender, critical_receiver) = bounded(1);
        let (event_sender, event_receiver) = bounded(1);
        drop(critical_sender);

        let scheduler = RenderScheduler {
            worker: Mutex::new(None),
            shutdown_sender,
            done_receiver,
            control_cmd_sender: control_sender,
            visible_cmd_sender: visible_sender,
            low_cmd_sender: low_sender,
            critical_event_receiver: critical_receiver,
            event_receiver,
            current_generation: Arc::new(AtomicU64::new(1)),
            pending_renders: Arc::new(Mutex::new(HashSet::new())),
            observability: RenderObservability::default(),
        };

        assert!(matches!(scheduler.try_recv_event(), Ok(None)));
        drop(event_sender);
        assert!(matches!(
            scheduler.try_recv_event(),
            Err(RenderError::WorkerStopped)
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
    fn metadata_events_use_the_critical_channel() {
        let scheduler = scheduler(Duration::ZERO);
        let document_id = DocumentId::new(7);
        open_document(&scheduler, document_id);
        assert!(scheduler.send_command(RenderCommand::RenderPage(job(
            scheduler.current_generation(),
            RenderKind::Page,
        ))));
        let deadline = Instant::now() + Duration::from_secs(1);
        while scheduler.event_receiver.is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!scheduler.event_receiver.is_empty());

        assert!(scheduler.send_command(RenderCommand::FetchOutline { document_id }));
        while scheduler.critical_event_receiver.is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(matches!(
            scheduler.try_recv_event(),
            Ok(Some(RenderEvent::OutlineFetched { document_id: event_id, .. })) if event_id == document_id
        ));

        assert!(scheduler.send_command(RenderCommand::FetchPageDimensions {
            document_id,
            start: 0,
            count: 1,
        }));
        while scheduler.critical_event_receiver.is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(matches!(
            scheduler.try_recv_event(),
            Ok(Some(RenderEvent::PageDimensionsFetched { document_id: event_id, .. })) if event_id == document_id
        ));
    }

    #[test]
    fn shutdown_joins_after_worker_sends_done_signal_and_rejects_new_commands() {
        let scheduler = scheduler(Duration::ZERO);
        assert!(scheduler.shutdown().is_ok());
        assert!(scheduler.shutdown().is_ok());
        assert!(!scheduler.send_command(RenderCommand::OpenDocument {
            document_id: DocumentId::new(7),
            path: PathBuf::from("mock.pdf"),
            password: None,
        }));
    }

    #[test]
    fn shutdown_unblocks_worker_when_critical_event_channel_is_full() {
        let (shutdown_sender, shutdown_receiver) = bounded(1);
        let (done_sender, done_receiver) = bounded(1);
        let (control_sender, _control_receiver) = bounded(1);
        let (visible_sender, _visible_receiver) = bounded(1);
        let (low_sender, _low_receiver) = bounded(1);
        let (critical_sender, critical_receiver) = bounded(1);
        let (_event_sender, event_receiver) = bounded(1);
        critical_sender
            .try_send(RenderEvent::OutlineFetched {
                document_id: DocumentId::new(7),
                outline: Vec::new(),
            })
            .expect("critical event channel has one free slot");
        let worker = thread::spawn(move || {
            emit_critical(
                &shutdown_receiver,
                &critical_sender,
                RenderEvent::OutlineFetched {
                    document_id: DocumentId::new(7),
                    outline: Vec::new(),
                },
            );
            let _ = done_sender.try_send(());
        });
        let scheduler = RenderScheduler {
            worker: Mutex::new(Some(worker)),
            shutdown_sender,
            done_receiver,
            control_cmd_sender: control_sender,
            visible_cmd_sender: visible_sender,
            low_cmd_sender: low_sender,
            critical_event_receiver: critical_receiver,
            event_receiver,
            current_generation: Arc::new(AtomicU64::new(1)),
            pending_renders: Arc::new(Mutex::new(HashSet::new())),
            observability: RenderObservability::default(),
        };

        let start = Instant::now();
        assert!(scheduler.shutdown().is_ok());
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn worker_panic_is_returned_as_structured_shutdown_error() {
        let (shutdown_sender, _shutdown_receiver) = bounded(1);
        let (done_sender, done_receiver) = bounded(1);
        let (control_sender, _control_receiver) = bounded(1);
        let (visible_sender, _visible_receiver) = bounded(1);
        let (low_sender, _low_receiver) = bounded(1);
        let (_critical_sender, critical_event_receiver) = bounded(1);
        let (_event_sender, event_receiver) = bounded(1);
        drop(done_sender);
        let scheduler = RenderScheduler {
            worker: Mutex::new(Some(thread::spawn(|| panic!("worker test panic")))),
            shutdown_sender,
            done_receiver,
            control_cmd_sender: control_sender,
            visible_cmd_sender: visible_sender,
            low_cmd_sender: low_sender,
            critical_event_receiver,
            event_receiver,
            current_generation: Arc::new(AtomicU64::new(1)),
            pending_renders: Arc::new(Mutex::new(HashSet::new())),
            observability: RenderObservability::default(),
        };

        assert!(matches!(
            scheduler.shutdown(),
            Err(RenderError::WorkerTerminated)
        ));
        assert!(scheduler.shutdown().is_ok());
    }

    #[test]
    fn shutdown_retains_blocking_render_and_joins_it_on_retry() {
        let (started_sender, started_receiver) = bounded(1);
        let scheduler = RenderScheduler::spawn(
            MockBackend {
                render_delay: Duration::from_millis(600),
                render_started: Some(started_sender),
            },
            MemoryBudget::new(1024 * 1024),
        );
        open_document(&scheduler, DocumentId::new(7));
        assert!(scheduler.send_command(RenderCommand::RenderPage(job(
            scheduler.current_generation(),
            RenderKind::Page,
        ))));
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("mock render starts before shutdown");

        let started = Instant::now();
        assert!(matches!(
            scheduler.shutdown(),
            Err(RenderError::WorkerTerminated)
        ));
        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(scheduler.worker.lock().is_ok_and(|worker| worker.is_some()));

        let deadline = Instant::now() + Duration::from_secs(1);
        let mut worker_stopped = false;
        while Instant::now() < deadline {
            match scheduler.try_recv_event() {
                Ok(Some(RenderEvent::PageRendered { .. })) => {
                    panic!("cancelled render emitted a bitmap")
                }
                Err(RenderError::WorkerStopped) => {
                    worker_stopped = true;
                    break;
                }
                Ok(Some(_) | None) => thread::sleep(Duration::from_millis(5)),
                Err(error) => panic!("unexpected render event error: {error}"),
            }
        }
        assert!(
            worker_stopped,
            "render worker did not stop after cancellation"
        );
        assert!(scheduler.shutdown().is_ok());
        assert!(scheduler.worker.lock().is_ok_and(|worker| worker.is_none()));
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
