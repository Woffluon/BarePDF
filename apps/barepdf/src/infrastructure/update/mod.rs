mod download;
mod manifest;
mod transport;

use barepdf_platform_windows::launch_installer;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

pub(crate) use manifest::VerifiedUpdate;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTO_CHECK_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(250);

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
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum UpdateShutdownError {
    #[error("Update worker did not stop before the shutdown deadline")]
    TimedOut,
    #[error("Update worker panicked during shutdown")]
    Panicked,
}

struct CompletionSignal(Option<Sender<()>>);

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
            let _ = sender.send(());
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
        match self.completed.recv_timeout(SHUTDOWN_JOIN_TIMEOUT) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {}
            Err(RecvTimeoutError::Timeout) => return Err(UpdateShutdownError::TimedOut),
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
    }
}

pub(crate) fn start_worker() -> (UpdateWorker, Receiver<UpdateEvent>) {
    let (command_sender, command_receiver) = mpsc::channel();
    let (event_sender, event_receiver) = mpsc::channel();
    let (completed_sender, completed_receiver) = mpsc::channel();
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
        },
        event_receiver,
    )
}

fn run_worker(
    commands: &Receiver<UpdateCommand>,
    events: &Sender<UpdateEvent>,
    cancelled: &AtomicBool,
) {
    let agent = transport::new_agent();
    while let Ok(command) = commands.recv() {
        let event = match command {
            UpdateCommand::Check(check_cancelled) => {
                match manifest::check_for_update(&agent, &check_cancelled) {
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
        if cancelled.load(Ordering::Acquire) || events.send(event).is_err() {
            break;
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
        let (completed_sender, completed) = mpsc::channel();
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
        let (completed_sender, completed) = mpsc::channel();
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
        let (event_sender, event_receiver) = mpsc::channel();
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
}
