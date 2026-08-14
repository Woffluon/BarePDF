use crate::infrastructure::{PrintEvent, PrintRequest, PrintWorker, PrintWorkerError};
use barepdf_platform::printing::{Copies, PrintJobId, PrintRange, PrinterSink};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub(crate) enum PrintControllerError {
    #[error("another print job is already active")]
    Busy,
    #[error("print job ID space is exhausted")]
    IdExhausted,
    #[error("print job is no longer active")]
    NotActive,
    #[error(transparent)]
    Worker(#[from] PrintWorkerError),
}

struct ActivePrint {
    id: PrintJobId,
    cancel: Arc<AtomicBool>,
}

pub(crate) struct PrintController {
    worker: PrintWorker,
    active: Option<ActivePrint>,
    next_id: u64,
}

impl PrintController {
    pub(crate) fn spawn() -> Result<Self, PrintWorkerError> {
        Ok(Self {
            worker: PrintWorker::spawn()?,
            active: None,
            next_id: 0,
        })
    }

    pub(crate) fn reserve_job(&mut self) -> Result<PrintJobId, PrintControllerError> {
        if self.active.is_some() {
            return Err(PrintControllerError::Busy);
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(PrintControllerError::IdExhausted)?;
        let id = PrintJobId::new(self.next_id).ok_or(PrintControllerError::IdExhausted)?;
        self.active = Some(ActivePrint {
            id,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        Ok(id)
    }

    pub(crate) fn release_reservation(&mut self, id: PrintJobId) {
        if self.active.as_ref().is_some_and(|active| active.id == id) {
            self.active = None;
        }
    }

    pub(crate) fn submit(
        &mut self,
        id: PrintJobId,
        path: PathBuf,
        title: String,
        range: PrintRange,
        copies: Copies,
        sink: Box<dyn PrinterSink>,
    ) -> Result<(), PrintControllerError> {
        let cancel = self
            .active
            .as_ref()
            .filter(|active| active.id == id)
            .map(|active| active.cancel.clone())
            .ok_or(PrintControllerError::NotActive)?;
        let result = self.worker.submit(PrintRequest {
            id,
            path,
            title,
            range,
            copies,
            sink: Some(sink),
            cancel,
        });
        if result.is_err() {
            self.active = None;
        }
        result.map_err(Into::into)
    }

    pub(crate) fn cancel(&self) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.cancel.store(true, Ordering::Release);
            true
        })
    }

    #[must_use]
    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn try_recv_event(&mut self) -> Option<PrintEvent> {
        loop {
            let event = self.worker.try_recv_event()?;
            let is_active = self
                .active
                .as_ref()
                .is_some_and(|active| active.id == event.job_id());
            if !is_active {
                continue;
            }
            if event.is_terminal() {
                self.active = None;
            }
            return Some(event);
        }
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), PrintWorkerError> {
        let _ = self.cancel();
        self.active = None;
        self.worker.shutdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::PrintWorker;

    #[test]
    fn concurrent_reservation_is_rejected_and_release_is_idempotent() {
        let worker = PrintWorker::spawn_fake_idle().expect("test worker should start");
        let mut controller = PrintController {
            worker,
            active: None,
            next_id: 0,
        };
        let first = controller.reserve_job().expect("first job should reserve");

        assert!(matches!(
            controller.reserve_job(),
            Err(PrintControllerError::Busy)
        ));
        controller.release_reservation(first);
        controller.release_reservation(first);
        assert!(controller.reserve_job().is_ok());
        assert!(controller.shutdown().is_ok());
        assert!(controller.shutdown().is_ok());
    }
}
