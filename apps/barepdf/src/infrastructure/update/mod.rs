mod download;
mod manifest;
mod transport;

#[cfg(target_os = "linux")]
use barepdf_platform_linux::launch_package_installer as launch_installer;
#[cfg(not(target_os = "linux"))]
use barepdf_platform_windows::launch_installer;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

pub(crate) use manifest::VerifiedUpdate;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
#[allow(dead_code)]
pub const AUTO_CHECK_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

const EVENT_CAPACITY: usize = 16;
const COMPLETION_CAPACITY: usize = 1;
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(250);
const SHUTDOWN_RETRY_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Debug, thiserror::Error)]
pub(crate) enum UpdateFailure {
    #[error("Update operation was cancelled")]
    Cancelled,
    #[error("{0}")]
    Rejected(&'static str),
    #[error("{operation}: {source}")]
    Transport {
        operation: &'static str,
        #[source]
        source: ureq::Error,
    },
    #[error("{operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("{operation}: {source}")]
    Manifest {
        operation: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("{operation}: {source}")]
    Version {
        operation: &'static str,
        #[source]
        source: semver::Error,
    },
    #[error("{operation}: {source}")]
    Platform {
        operation: &'static str,
        #[source]
        source: barepdf_platform::PlatformError,
    },
}

impl UpdateFailure {
    pub(crate) fn is_network_error(&self) -> bool {
        matches!(self, Self::Transport { .. } | Self::Io { .. })
    }
}

#[derive(Debug)]
pub(crate) enum UpdateCommand {
    Check(Arc<AtomicBool>),
    Download(VerifiedUpdate),
    Install {
        path: PathBuf,
        update: VerifiedUpdate,
    },
    Shutdown,
}

#[derive(Debug)]
pub(crate) enum UpdateEvent {
    UpToDate,
    Available(VerifiedUpdate),
    Downloaded {
        path: PathBuf,
        update: VerifiedUpdate,
    },
    InstallerStarted,
    Error(UpdateFailure),
}

pub(crate) struct UpdateWorker {
    commands: Sender<UpdateCommand>,
    cancelled: Arc<AtomicBool>,
    check_canceller: UpdateCheckCanceller,
    completed: Receiver<()>,
    worker: Option<JoinHandle<()>>,
    shutdown_timed_out: bool,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum UpdateShutdownError {
    #[error("Update worker did not stop before the shutdown deadline")]
    TimedOut,
    #[error("Update worker panicked during shutdown")]
    Panicked,
}

struct CompletionSignal(Option<SyncSender<()>>);

#[derive(Clone, Default)]
pub(crate) struct UpdateCheckCanceller {
    active: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

impl UpdateCheckCanceller {
    pub(crate) fn begin_check(&self) -> Arc<AtomicBool> {
        let cancellation = Arc::new(AtomicBool::new(false));
        *lock_recovering_poison(&self.active) = Some(cancellation.clone());
        cancellation
    }

    pub(crate) fn cancel_pending_check(&self) {
        if let Some(cancellation) = lock_recovering_poison(&self.active).as_ref() {
            cancellation.store(true, Ordering::Release);
        }
    }
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl Drop for CompletionSignal {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.try_send(());
        }
    }
}

impl UpdateWorker {
    pub(crate) fn command_sender(&self) -> Sender<UpdateCommand> {
        self.commands.clone()
    }

    pub(crate) fn check_canceller(&self) -> UpdateCheckCanceller {
        self.check_canceller.clone()
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), UpdateShutdownError> {
        self.cancelled.store(true, Ordering::Release);
        self.check_canceller.cancel_pending_check();
        let _ = self.commands.send(UpdateCommand::Shutdown);
        if self.worker.is_none() {
            return Ok(());
        }
        let timeout = if self.shutdown_timed_out {
            SHUTDOWN_RETRY_TIMEOUT
        } else {
            SHUTDOWN_JOIN_TIMEOUT
        };
        match self.completed.recv_timeout(timeout) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {}
            Err(RecvTimeoutError::Timeout) => {
                self.shutdown_timed_out = true;
                return Err(UpdateShutdownError::TimedOut);
            }
        }
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker.join().map_err(|_| UpdateShutdownError::Panicked)
    }
}

impl Drop for UpdateWorker {
    fn drop(&mut self) {
        let _ = self.shutdown();
        if let Some(worker) = self.worker.take() {
            self.cancelled.store(true, Ordering::Release);
            self.check_canceller.cancel_pending_check();
            let _ = self.completed.recv_timeout(SHUTDOWN_RETRY_TIMEOUT);
            let _ = worker.join();
        }
    }
}

pub(crate) fn start_worker() -> (UpdateWorker, Receiver<UpdateEvent>) {
    let (command_sender, command_receiver) = mpsc::channel();
    let (event_sender, event_receiver) = mpsc::sync_channel(EVENT_CAPACITY);
    let (completed_sender, completed_receiver) = mpsc::sync_channel(COMPLETION_CAPACITY);
    let cancelled = Arc::new(AtomicBool::new(false));
    let check_canceller = UpdateCheckCanceller::default();
    let worker_cancelled = cancelled.clone();
    let worker = std::thread::spawn(move || {
        let _completion = CompletionSignal(Some(completed_sender));
        run_worker(&command_receiver, &event_sender, &worker_cancelled);
    });
    (
        UpdateWorker {
            commands: command_sender,
            cancelled,
            check_canceller,
            completed: completed_receiver,
            worker: Some(worker),
            shutdown_timed_out: false,
        },
        event_receiver,
    )
}

pub(crate) const CHECK_BACKOFF_SCHEDULE: [Duration; 3] = [
    Duration::from_millis(500),
    Duration::from_millis(1000),
    Duration::from_millis(2000),
];

fn sleep_with_cancellation(
    duration: Duration,
    worker_cancelled: &AtomicBool,
    check_cancelled: &AtomicBool,
) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        if worker_cancelled.load(Ordering::Acquire) || check_cancelled.load(Ordering::Acquire) {
            return false;
        }
        let remaining = duration.saturating_sub(start.elapsed());
        let step = remaining.min(Duration::from_millis(25));
        std::thread::sleep(step);
    }
    !worker_cancelled.load(Ordering::Acquire) && !check_cancelled.load(Ordering::Acquire)
}

fn check_with_retry<F>(
    mut check: F,
    check_cancelled: &AtomicBool,
    worker_cancelled: &AtomicBool,
    backoff_schedule: &[Duration],
) -> Result<Option<VerifiedUpdate>, UpdateFailure>
where
    F: FnMut() -> Result<Option<VerifiedUpdate>, UpdateFailure>,
{
    let mut attempt = 0;
    loop {
        if worker_cancelled.load(Ordering::Acquire) || check_cancelled.load(Ordering::Acquire) {
            return Err(UpdateFailure::Cancelled);
        }
        match check() {
            Ok(result) => return Ok(result),
            Err(error) => {
                if worker_cancelled.load(Ordering::Acquire)
                    || check_cancelled.load(Ordering::Acquire)
                {
                    return Err(UpdateFailure::Cancelled);
                }
                if error.is_network_error() && attempt < backoff_schedule.len() {
                    crate::diagnostics::warn_redacted(
                        crate::diagnostics::DiagnosticEvent::Update,
                        &error,
                    );
                    let backoff = backoff_schedule[attempt];
                    attempt += 1;
                    if !sleep_with_cancellation(backoff, worker_cancelled, check_cancelled) {
                        return Err(UpdateFailure::Cancelled);
                    }
                } else {
                    return Err(error);
                }
            }
        }
    }
}

fn run_worker(
    commands: &Receiver<UpdateCommand>,
    events: &SyncSender<UpdateEvent>,
    cancelled: &AtomicBool,
) {
    let agent = transport::new_agent();
    while let Ok(command) = commands.recv() {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        if matches!(command, UpdateCommand::Shutdown) {
            break;
        }
        let event = match command {
            UpdateCommand::Check(check_cancelled) => {
                match check_with_retry(
                    || manifest::check_for_update(&agent, &check_cancelled),
                    &check_cancelled,
                    cancelled,
                    &CHECK_BACKOFF_SCHEDULE,
                ) {
                    Ok(Some(update)) => UpdateEvent::Available(update),
                    Ok(None) => UpdateEvent::UpToDate,
                    Err(error) => UpdateEvent::Error(error),
                }
            }
            UpdateCommand::Download(update) => {
                match download::download_update(&agent, &update, cancelled) {
                    Ok(path) => UpdateEvent::Downloaded { path, update },
                    Err(error) => UpdateEvent::Error(error),
                }
            }
            UpdateCommand::Install { path, update } => {
                match download::verify_download(&path, &update).and_then(|()| {
                    transport::check_cancelled(cancelled)?;
                    launch_installer(&path).map_err(|source| UpdateFailure::Platform {
                        operation: "Could not launch verified update",
                        source,
                    })
                }) {
                    Ok(()) => UpdateEvent::InstallerStarted,
                    Err(error) => UpdateEvent::Error(error),
                }
            }
            UpdateCommand::Shutdown => break,
        };
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        if send_event_with_backpressure(events, event, cancelled).is_err() {
            break;
        }
    }
}

fn send_event_with_backpressure(
    events: &SyncSender<UpdateEvent>,
    mut event: UpdateEvent,
    cancelled: &AtomicBool,
) -> Result<(), ()> {
    if cancelled.load(Ordering::Acquire) {
        let _ = events.try_send(event);
        return Ok(());
    }
    match events.try_send(event) {
        Ok(()) => Ok(()),
        Err(TrySendError::Disconnected(_)) => Err(()),
        Err(TrySendError::Full(returned)) => {
            event = returned;
            loop {
                if cancelled.load(Ordering::Acquire) {
                    let _ = events.try_send(event);
                    return Ok(());
                }
                match events.try_send(event) {
                    Ok(()) => return Ok(()),
                    Err(TrySendError::Disconnected(_)) => return Err(()),
                    Err(TrySendError::Full(returned)) => {
                        event = returned;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn updater_failures_keep_their_source_chain() {
        let error = UpdateFailure::Platform {
            operation: "Could not inspect update",
            source: barepdf_platform::PlatformError::InvalidData {
                operation: "Could not read Windows file version metadata",
                reason: "metadata is truncated",
            },
        };

        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn worker_shutdown_is_idempotent() {
        let (mut worker, events) = start_worker();
        drop(events);

        assert!(worker.shutdown().is_ok());
        assert!(worker.shutdown().is_ok());
    }

    #[test]
    fn shutdown_cancels_an_active_check_before_joining() {
        let (mut worker, events) = start_worker();
        drop(events);
        let cancellation = worker.check_canceller().begin_check();

        assert!(worker.shutdown().is_ok());
        assert!(matches!(
            transport::check_cancelled(&cancellation),
            Err(UpdateFailure::Cancelled)
        ));
    }

    #[test]
    fn worker_shutdown_retains_active_work_and_joins_on_retry() {
        let (commands, _command_receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (completed_sender, completed) = mpsc::sync_channel(COMPLETION_CAPACITY);
        let (release_sender, release_receiver) = mpsc::channel();
        let worker_thread = std::thread::spawn(move || {
            let _completion = CompletionSignal(Some(completed_sender));
            let _ = release_receiver.recv();
        });
        let mut worker = UpdateWorker {
            commands,
            cancelled,
            check_canceller: UpdateCheckCanceller::default(),
            completed,
            worker: Some(worker_thread),
            shutdown_timed_out: false,
        };

        let started = Instant::now();
        assert!(matches!(
            worker.shutdown(),
            Err(UpdateShutdownError::TimedOut)
        ));
        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(worker.worker.is_some());
        assert!(release_sender.send(()).is_ok());
        assert!(worker.shutdown().is_ok());
        assert!(worker.worker.is_none());
    }

    #[test]
    fn worker_panic_after_timeout_is_reported_on_retry() {
        let (commands, _command_receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (completed_sender, completed) = mpsc::sync_channel(COMPLETION_CAPACITY);
        let (release_sender, release_receiver) = mpsc::channel();
        let worker_thread = std::thread::spawn(move || {
            let _completion = CompletionSignal(Some(completed_sender));
            let _ = release_receiver.recv();
            panic!("update worker test panic");
        });
        let mut worker = UpdateWorker {
            commands,
            cancelled,
            check_canceller: UpdateCheckCanceller::default(),
            completed,
            worker: Some(worker_thread),
            shutdown_timed_out: false,
        };

        assert!(matches!(
            worker.shutdown(),
            Err(UpdateShutdownError::TimedOut)
        ));
        assert!(release_sender.send(()).is_ok());
        assert!(matches!(
            worker.shutdown(),
            Err(UpdateShutdownError::Panicked)
        ));
        assert!(worker.shutdown().is_ok());
    }

    #[test]
    fn cancelling_a_check_stops_queued_work_before_a_request() {
        let canceller = UpdateCheckCanceller::default();
        let cancellation = canceller.begin_check();
        canceller.cancel_pending_check();

        assert!(matches!(
            transport::check_cancelled(&cancellation),
            Err(UpdateFailure::Cancelled)
        ));

        let next_check = canceller.begin_check();
        assert!(transport::check_cancelled(&next_check).is_ok());
    }

    #[test]
    fn queued_cancelled_check_is_reported_without_starting_a_request() {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::sync_channel(EVENT_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let check_cancelled = Arc::new(AtomicBool::new(true));
        assert!(command_sender
            .send(UpdateCommand::Check(check_cancelled))
            .is_ok());
        assert!(command_sender.send(UpdateCommand::Shutdown).is_ok());

        let worker = std::thread::spawn(move || {
            run_worker(&command_receiver, &event_sender, &cancelled);
        });
        assert!(matches!(
            event_receiver.recv_timeout(Duration::from_secs(1)),
            Ok(UpdateEvent::Error(UpdateFailure::Cancelled))
        ));
        assert!(worker.join().is_ok());
    }

    #[test]
    fn worker_drop_without_explicit_shutdown_joins_thread_cleanly() {
        let (worker, _events) = start_worker();
        // Dropping worker must coordinate shutdown and join thread without leaving it orphaned
        drop(worker);
    }

    #[test]
    fn bounded_channel_backpressure_does_not_deadlock_on_cancel() {
        let (event_sender, _event_receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));

        // Fill channel to capacity
        assert!(event_sender.try_send(UpdateEvent::UpToDate).is_ok());

        // Worker attempts to send next event in thread while channel is full
        let sender_cancelled = cancelled.clone();
        let handle = std::thread::spawn(move || {
            send_event_with_backpressure(&event_sender, UpdateEvent::UpToDate, &sender_cancelled)
        });

        // Cancel while sender is waiting on full channel
        std::thread::sleep(Duration::from_millis(20));
        cancelled.store(true, Ordering::Release);

        // Sender must unblock due to cancellation without deadlocking
        assert!(handle.join().is_ok());
    }

    #[test]
    fn default_backoff_schedule_has_three_exponential_intervals() {
        assert_eq!(CHECK_BACKOFF_SCHEDULE.len(), 3);
        assert!(CHECK_BACKOFF_SCHEDULE[0] < CHECK_BACKOFF_SCHEDULE[1]);
        assert!(CHECK_BACKOFF_SCHEDULE[1] < CHECK_BACKOFF_SCHEDULE[2]);
    }

    #[test]
    fn check_with_retry_succeeds_without_retries_on_first_try() {
        let check_cancelled = AtomicBool::new(false);
        let worker_cancelled = AtomicBool::new(false);
        let mut attempts = 0;
        let schedule = [
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(3),
        ];

        let result = check_with_retry(
            || {
                attempts += 1;
                Ok(Some(VerifiedUpdate::for_test(
                    "2.0.0",
                    "a".repeat(64),
                    1024,
                )))
            },
            &check_cancelled,
            &worker_cancelled,
            &schedule,
        );

        assert_eq!(attempts, 1);
        assert!(matches!(result, Ok(Some(update)) if update.version() == "2.0.0"));
    }

    #[test]
    fn check_with_retry_retries_on_transient_network_error_and_succeeds() {
        let check_cancelled = AtomicBool::new(false);
        let worker_cancelled = AtomicBool::new(false);
        let mut attempts = 0;
        let schedule = [
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(3),
        ];

        let result = check_with_retry(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(UpdateFailure::Io {
                        operation: "test transient error",
                        source: std::io::Error::other("connection reset"),
                    })
                } else {
                    Ok(None)
                }
            },
            &check_cancelled,
            &worker_cancelled,
            &schedule,
        );

        assert_eq!(attempts, 3);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn check_with_retry_exhausts_retries_on_persistent_network_error() {
        let check_cancelled = AtomicBool::new(false);
        let worker_cancelled = AtomicBool::new(false);
        let mut attempts = 0;
        let schedule = [
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(3),
        ];

        let result = check_with_retry(
            || {
                attempts += 1;
                Err(UpdateFailure::Io {
                    operation: "test persistent error",
                    source: std::io::Error::other("host unreachable"),
                })
            },
            &check_cancelled,
            &worker_cancelled,
            &schedule,
        );

        // 1 initial attempt + 3 retries = 4 attempts total
        assert_eq!(attempts, 4);
        assert!(matches!(result, Err(UpdateFailure::Io { .. })));
    }

    #[test]
    fn check_with_retry_does_not_retry_on_non_network_error() {
        let check_cancelled = AtomicBool::new(false);
        let worker_cancelled = AtomicBool::new(false);
        let mut attempts = 0;
        let schedule = [
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(3),
        ];

        let result = check_with_retry(
            || {
                attempts += 1;
                Err(UpdateFailure::Rejected("invalid signature"))
            },
            &check_cancelled,
            &worker_cancelled,
            &schedule,
        );

        assert_eq!(attempts, 1);
        assert!(matches!(result, Err(UpdateFailure::Rejected(_))));
    }

    #[test]
    fn check_with_retry_aborts_promptly_when_cancelled_during_backoff() {
        let check_cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::new(AtomicBool::new(false));
        let schedule = [Duration::from_millis(500)];

        let canceller = check_cancelled.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            canceller.store(true, Ordering::Release);
        });

        let start = Instant::now();
        let result = check_with_retry(
            || {
                Err(UpdateFailure::Io {
                    operation: "test abort error",
                    source: std::io::Error::other("offline"),
                })
            },
            &check_cancelled,
            &worker_cancelled,
            &schedule,
        );

        assert!(matches!(result, Err(UpdateFailure::Cancelled)));
        assert!(start.elapsed() < Duration::from_millis(250));
    }
}
