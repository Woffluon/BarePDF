use barepdf_core::{PageCount, PageIndex, PageRangeSelection, PdfError, Rotation};
use barepdf_pdf::conversion::{
    convert_pdf, CancellationToken, ConversionDpi, ConversionFormat, ConversionReport,
    ConversionRequest, JobPassword,
};
use barepdf_pdf::{PdfBackend, PdfOperationInput, PdfOperations, PdfiumEngine};
use barepdf_platform_windows::WindowsImageEncoder;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const COMMAND_CAPACITY: usize = 1;
const EVENT_CAPACITY: usize = 1;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolJobKey {
    id: u64,
    generation: u64,
    source_token: u64,
}

impl ToolJobKey {
    #[must_use]
    pub(crate) const fn new(id: u64, generation: u64, source_token: u64) -> Self {
        Self {
            id,
            generation,
            source_token,
        }
    }

    #[must_use]
    pub(crate) const fn is_current(self, generation: u64, source_token: u64) -> bool {
        self.generation == generation && self.source_token == source_token
    }
}

pub(crate) struct SecretPassword {
    bytes: Vec<u8>,
}

impl SecretPassword {
    fn new(password: String) -> Self {
        Self {
            bytes: password.into_bytes(),
        }
    }

    fn expose(&self) -> &str {
        std::str::from_utf8(&self.bytes).unwrap_or_default()
    }

    fn clear(&mut self) {
        self.bytes.fill(0);
    }

    #[cfg(test)]
    fn bytes_for_test(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for SecretPassword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretPassword([REDACTED])")
    }
}

impl Drop for SecretPassword {
    fn drop(&mut self) {
        self.clear();
    }
}

#[derive(Debug)]
pub(crate) enum ToolOperation {
    Merge {
        inputs: Vec<PathBuf>,
        output: PathBuf,
    },
    Extract {
        source: PathBuf,
        range: String,
        output: PathBuf,
    },
    SplitAll {
        source: PathBuf,
        output_parent: PathBuf,
        base_name: String,
    },
    Delete {
        source: PathBuf,
        range: String,
        output: PathBuf,
    },
    Rotate {
        source: PathBuf,
        range: String,
        rotation: Rotation,
        output: PathBuf,
    },
    Convert {
        source: PathBuf,
        output_parent: PathBuf,
        range: String,
        format: ConversionFormat,
        dpi: ConversionDpi,
    },
    #[cfg(test)]
    Test,
}

#[derive(Debug)]
pub(crate) struct ToolRequest {
    pub(crate) key: ToolJobKey,
    operation: ToolOperation,
    cancellation: CancellationToken,
    passwords: Vec<(PathBuf, SecretPassword)>,
}

impl ToolRequest {
    #[must_use]
    pub(crate) fn new(key: ToolJobKey, operation: ToolOperation) -> Self {
        Self {
            key,
            operation,
            cancellation: CancellationToken::new(),
            passwords: Vec::new(),
        }
    }

    #[must_use]
    pub(crate) fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    fn password_for(&self, source: &Path) -> Option<&str> {
        self.passwords
            .iter()
            .find_map(|(path, password)| (path == source).then(|| password.expose()))
    }

    fn replace_password(&mut self, source: PathBuf, password: String) {
        if let Some((_, existing)) = self.passwords.iter_mut().find(|(path, _)| path == &source) {
            existing.clear();
            *existing = SecretPassword::new(password);
        } else {
            self.passwords.push((source, SecretPassword::new(password)));
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

#[derive(Debug)]
pub(crate) enum ToolOutcome {
    Pdf {
        output: PathBuf,
    },
    Split {
        output_directory: PathBuf,
        file_count: usize,
    },
    Conversion(ConversionReport),
    #[cfg(test)]
    Test,
}

#[derive(Debug)]
pub(crate) enum ToolEvent {
    Completed {
        key: ToolJobKey,
        outcome: ToolOutcome,
    },
    PasswordRequired {
        key: ToolJobKey,
        source: PathBuf,
        wrong_password: bool,
    },
    Cancelled {
        key: ToolJobKey,
    },
    Failed {
        key: ToolJobKey,
        message: String,
    },
}

impl ToolEvent {
    #[must_use]
    pub(crate) const fn key(&self) -> ToolJobKey {
        match self {
            Self::Completed { key, .. }
            | Self::PasswordRequired { key, .. }
            | Self::Cancelled { key }
            | Self::Failed { key, .. } => *key,
        }
    }

    #[must_use]
    pub(crate) const fn is_terminal(&self) -> bool {
        !matches!(self, Self::PasswordRequired { .. })
    }
}

enum ToolCommand {
    Start(ToolRequest),
    ProvidePassword {
        key: ToolJobKey,
        source: PathBuf,
        password: String,
    },
    Cancel(ToolJobKey),
    Shutdown,
}

enum ExecutionResult {
    Completed(ToolOutcome),
    PasswordRequired {
        source: PathBuf,
        wrong_password: bool,
    },
    Cancelled,
    Failed(String),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ToolWorkerError {
    #[error("could not start PDF tool worker: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("PDF tool worker is busy")]
    Busy,
    #[error("PDF tool worker is unavailable")]
    Disconnected,
    #[error("PDF tool worker did not stop within 250 ms")]
    ShutdownTimeout,
    #[error("PDF tool worker panicked")]
    Panicked,
}

type ToolExecutor = Box<dyn FnMut(&mut ToolRequest) -> ExecutionResult + Send + 'static>;

pub(crate) struct ToolWorker {
    command_sender: Option<SyncSender<ToolCommand>>,
    event_receiver: Receiver<ToolEvent>,
    active: Arc<AtomicBool>,
    current_cancellation: Arc<Mutex<Option<CancellationToken>>>,
    shutdown: Arc<AtomicBool>,
    done_receiver: Receiver<()>,
    handle: Option<JoinHandle<()>>,
}

impl ToolWorker {
    pub(crate) fn spawn() -> Result<Self, ToolWorkerError> {
        Self::spawn_with_executor(execute_request)
    }

    fn spawn_with_executor(
        executor: impl FnMut(&mut ToolRequest) -> ExecutionResult + Send + 'static,
    ) -> Result<Self, ToolWorkerError> {
        let (command_sender, command_receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let (event_sender, event_receiver) = mpsc::sync_channel(EVENT_CAPACITY);
        let (done_sender, done_receiver) = mpsc::sync_channel(1);
        let active = Arc::new(AtomicBool::new(false));
        let current_cancellation = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_active = active.clone();
        let worker_cancellation = current_cancellation.clone();
        let worker_shutdown = shutdown.clone();
        let handle = std::thread::Builder::new()
            .name("barepdf-tools".into())
            .spawn(move || {
                worker_loop(
                    &command_receiver,
                    &event_sender,
                    Box::new(executor),
                    &worker_active,
                    &worker_cancellation,
                    &worker_shutdown,
                );
                let _ = done_sender.try_send(());
            })
            .map_err(ToolWorkerError::Spawn)?;
        Ok(Self {
            command_sender: Some(command_sender),
            event_receiver,
            active,
            current_cancellation,
            shutdown,
            done_receiver,
            handle: Some(handle),
        })
    }

    pub(crate) fn submit(
        &self,
        request: ToolRequest,
    ) -> Result<CancellationToken, ToolWorkerError> {
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ToolWorkerError::Busy);
        }
        let cancellation = request.cancellation();
        let sender = self
            .command_sender
            .as_ref()
            .ok_or(ToolWorkerError::Disconnected);
        let result = sender.and_then(|sender| {
            sender
                .try_send(ToolCommand::Start(request))
                .map_err(|error| match error {
                    TrySendError::Full(_) => ToolWorkerError::Busy,
                    TrySendError::Disconnected(_) => ToolWorkerError::Disconnected,
                })
        });
        if result.is_err() {
            self.active.store(false, Ordering::Release);
        }
        result.map(|()| cancellation)
    }

    pub(crate) fn provide_password(
        &self,
        key: ToolJobKey,
        source: PathBuf,
        password: String,
    ) -> Result<(), ToolWorkerError> {
        let sender = self
            .command_sender
            .as_ref()
            .ok_or(ToolWorkerError::Disconnected)?;
        sender
            .try_send(ToolCommand::ProvidePassword {
                key,
                source,
                password,
            })
            .map_err(|error| match error {
                TrySendError::Full(ToolCommand::ProvidePassword { password, .. }) => {
                    clear_rejected_password(password);
                    ToolWorkerError::Busy
                }
                TrySendError::Disconnected(ToolCommand::ProvidePassword { password, .. }) => {
                    clear_rejected_password(password);
                    ToolWorkerError::Disconnected
                }
                TrySendError::Full(_) => ToolWorkerError::Busy,
                TrySendError::Disconnected(_) => ToolWorkerError::Disconnected,
            })
    }

    pub(crate) fn cancel(&self, key: ToolJobKey, cancellation: &CancellationToken) {
        cancellation.cancel();
        if let Some(sender) = self.command_sender.as_ref() {
            let _ = sender.try_send(ToolCommand::Cancel(key));
        }
    }

    pub(crate) fn try_recv_event(&self) -> Option<ToolEvent> {
        self.event_receiver.try_recv().ok()
    }

    fn shutdown(&mut self) -> Result<(), ToolWorkerError> {
        let Some(_) = self.handle.as_ref() else {
            return Ok(());
        };
        self.shutdown.store(true, Ordering::Release);
        if let Ok(guard) = self.current_cancellation.lock() {
            if let Some(cancellation) = guard.as_ref() {
                cancellation.cancel();
            }
        }
        if let Some(sender) = self.command_sender.take() {
            let _ = sender.try_send(ToolCommand::Shutdown);
        }
        match self.done_receiver.recv_timeout(SHUTDOWN_TIMEOUT) {
            Ok(()) => self
                .handle
                .take()
                .ok_or(ToolWorkerError::Disconnected)?
                .join()
                .map_err(|_| ToolWorkerError::Panicked),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ToolWorkerError::ShutdownTimeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => self
                .handle
                .take()
                .ok_or(ToolWorkerError::Disconnected)?
                .join()
                .map_err(|_| ToolWorkerError::Panicked),
        }
    }

    #[cfg(test)]
    fn submit_test_job(&self, key: ToolJobKey) -> Result<CancellationToken, ToolWorkerError> {
        self.submit(ToolRequest::new(key, ToolOperation::Test))
    }

    #[cfg(test)]
    fn recv_event_timeout(&self, timeout: Duration) -> Option<ToolEvent> {
        self.event_receiver.recv_timeout(timeout).ok()
    }

    #[cfg(test)]
    fn spawn_password_test_worker(source: PathBuf) -> Result<Self, ToolWorkerError> {
        Self::spawn_with_executor(move |request| match request.password_for(&source) {
            None => ExecutionResult::PasswordRequired {
                source: source.clone(),
                wrong_password: false,
            },
            Some("good") => ExecutionResult::Completed(ToolOutcome::Test),
            Some(_) => ExecutionResult::PasswordRequired {
                source: source.clone(),
                wrong_password: true,
            },
        })
    }
}

impl Drop for ToolWorker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn worker_loop(
    commands: &Receiver<ToolCommand>,
    events: &SyncSender<ToolEvent>,
    mut executor: ToolExecutor,
    active: &AtomicBool,
    current_cancellation: &Mutex<Option<CancellationToken>>,
    shutdown: &AtomicBool,
) {
    let mut pending: Option<(ToolRequest, PathBuf, bool)> = None;
    while !shutdown.load(Ordering::Acquire) {
        let Ok(command) = commands.recv() else {
            break;
        };
        match command {
            ToolCommand::Start(request) => run_and_emit(
                request,
                events,
                &mut executor,
                active,
                current_cancellation,
                &mut pending,
            ),
            ToolCommand::ProvidePassword {
                key,
                source,
                password,
            } => {
                let Some((mut request, awaited_source, wrong_password)) = pending.take() else {
                    clear_rejected_password(password);
                    continue;
                };
                if request.key != key || awaited_source != source {
                    clear_rejected_password(password);
                    let event = ToolEvent::PasswordRequired {
                        key: request.key,
                        source: awaited_source.clone(),
                        wrong_password,
                    };
                    pending = Some((request, awaited_source, wrong_password));
                    if events.send(event).is_err() {
                        break;
                    }
                    continue;
                }
                request.replace_password(source, password);
                run_and_emit(
                    request,
                    events,
                    &mut executor,
                    active,
                    current_cancellation,
                    &mut pending,
                );
            }
            ToolCommand::Cancel(key) => {
                if pending
                    .as_ref()
                    .is_some_and(|(request, _, _)| request.key == key)
                {
                    pending = None;
                    active.store(false, Ordering::Release);
                    clear_current_cancellation(current_cancellation);
                    if events.send(ToolEvent::Cancelled { key }).is_err() {
                        break;
                    }
                }
            }
            ToolCommand::Shutdown => break,
        }
    }
    pending = None;
    drop(pending);
    active.store(false, Ordering::Release);
    clear_current_cancellation(current_cancellation);
}

fn run_and_emit(
    mut request: ToolRequest,
    events: &SyncSender<ToolEvent>,
    executor: &mut ToolExecutor,
    active: &AtomicBool,
    current_cancellation: &Mutex<Option<CancellationToken>>,
    pending: &mut Option<(ToolRequest, PathBuf, bool)>,
) {
    if let Ok(mut guard) = current_cancellation.lock() {
        *guard = Some(request.cancellation());
    }
    let key = request.key;
    let execution = executor(&mut request);
    let event = match execution {
        ExecutionResult::Completed(outcome) => ToolEvent::Completed { key, outcome },
        ExecutionResult::PasswordRequired {
            source,
            wrong_password,
        } => {
            let event = ToolEvent::PasswordRequired {
                key,
                source: source.clone(),
                wrong_password,
            };
            *pending = Some((request, source, wrong_password));
            if events.send(event).is_err() {
                *pending = None;
            }
            return;
        }
        ExecutionResult::Cancelled => ToolEvent::Cancelled { key },
        ExecutionResult::Failed(message) => ToolEvent::Failed { key, message },
    };
    active.store(false, Ordering::Release);
    clear_current_cancellation(current_cancellation);
    let _ = events.send(event);
}

fn clear_current_cancellation(current: &Mutex<Option<CancellationToken>>) {
    if let Ok(mut guard) = current.lock() {
        *guard = None;
    }
}

fn clear_rejected_password(password: String) {
    let mut bytes = password.into_bytes();
    bytes.fill(0);
}

fn execute_request(request: &mut ToolRequest) -> ExecutionResult {
    if request.is_cancelled() {
        return ExecutionResult::Cancelled;
    }
    match execute_request_inner(request) {
        Ok(outcome) if request.is_cancelled() => {
            drop(outcome);
            ExecutionResult::Cancelled
        }
        Ok(outcome) => ExecutionResult::Completed(outcome),
        Err(ToolFailure::PasswordRequired {
            source,
            wrong_password,
        }) => ExecutionResult::PasswordRequired {
            source,
            wrong_password,
        },
        Err(ToolFailure::Cancelled) => ExecutionResult::Cancelled,
        Err(ToolFailure::Failed(message)) => ExecutionResult::Failed(message),
    }
}

enum ToolFailure {
    PasswordRequired {
        source: PathBuf,
        wrong_password: bool,
    },
    Cancelled,
    Failed(String),
}

fn execute_request_inner(request: &ToolRequest) -> Result<ToolOutcome, ToolFailure> {
    match &request.operation {
        ToolOperation::Merge { inputs, output } => {
            let backend = new_backend()?;
            for source in inputs {
                preflight(&backend, source, request.password_for(source))?;
                check_cancel(request)?;
            }
            let staged = tempfile::NamedTempFile::new_in(parent_directory(output))
                .map_err(|error| ToolFailure::Failed(error.to_string()))?;
            let entries = inputs
                .iter()
                .map(|source| PdfOperationInput::new(source, request.password_for(source)))
                .collect::<Vec<_>>();
            PdfOperations::merge_files_with_passwords(&entries, staged.path())
                .map_err(|error| operation_error(inputs.first().map(PathBuf::as_path), error))?;
            check_cancel(request)?;
            persist_file(staged, output)?;
            Ok(ToolOutcome::Pdf {
                output: output.clone(),
            })
        }
        ToolOperation::Extract {
            source,
            range,
            output,
        } => {
            let backend = new_backend()?;
            let page_count = preflight(&backend, source, request.password_for(source))?;
            let pages = PageRangeSelection::parse(range, page_count)
                .map_err(|error| ToolFailure::Failed(error.to_string()))?;
            let staged = tempfile::NamedTempFile::new_in(parent_directory(output))
                .map_err(|error| ToolFailure::Failed(error.to_string()))?;
            PdfOperations::extract_pages_with_password(
                source,
                &pages,
                staged.path(),
                request.password_for(source),
            )
            .map_err(|error| operation_error(Some(source), error))?;
            check_cancel(request)?;
            persist_file(staged, output)?;
            Ok(ToolOutcome::Pdf {
                output: output.clone(),
            })
        }
        ToolOperation::SplitAll {
            source,
            output_parent,
            base_name,
        } => {
            let backend = new_backend()?;
            preflight(&backend, source, request.password_for(source))?;
            let staging = tempfile::Builder::new()
                .prefix(".barepdf-split-")
                .tempdir_in(output_parent)
                .map_err(|error| ToolFailure::Failed(error.to_string()))?;
            let paths = PdfOperations::split_into_single_pages_with_password(
                source,
                staging.path(),
                base_name,
                request.password_for(source),
            )
            .map_err(|error| operation_error(Some(source), error))?;
            check_cancel(request)?;
            let output_directory = unique_output_directory(output_parent, base_name)?;
            std::fs::rename(staging.path(), &output_directory)
                .map_err(|error| ToolFailure::Failed(error.to_string()))?;
            Ok(ToolOutcome::Split {
                output_directory,
                file_count: paths.len(),
            })
        }
        ToolOperation::Delete {
            source,
            range,
            output,
        } => {
            let backend = new_backend()?;
            let page_count = preflight(&backend, source, request.password_for(source))?;
            let pages = PageRangeSelection::parse(range, page_count)
                .map_err(|error| ToolFailure::Failed(error.to_string()))?;
            let staged = tempfile::NamedTempFile::new_in(parent_directory(output))
                .map_err(|error| ToolFailure::Failed(error.to_string()))?;
            PdfOperations::delete_pages_with_password(
                source,
                &pages,
                staged.path(),
                request.password_for(source),
            )
            .map_err(|error| operation_error(Some(source), error))?;
            check_cancel(request)?;
            persist_file(staged, output)?;
            Ok(ToolOutcome::Pdf {
                output: output.clone(),
            })
        }
        ToolOperation::Rotate {
            source,
            range,
            rotation,
            output,
        } => {
            let backend = new_backend()?;
            let page_count = preflight(&backend, source, request.password_for(source))?;
            let pages = parse_pages_or_all(range, page_count)?;
            let rotations = pages
                .into_iter()
                .map(|page| (page, *rotation))
                .collect::<Vec<_>>();
            let staged = tempfile::NamedTempFile::new_in(parent_directory(output))
                .map_err(|error| ToolFailure::Failed(error.to_string()))?;
            PdfOperations::rotate_pages_with_password(
                source,
                &rotations,
                staged.path(),
                request.password_for(source),
            )
            .map_err(|error| operation_error(Some(source), error))?;
            check_cancel(request)?;
            persist_file(staged, output)?;
            Ok(ToolOutcome::Pdf {
                output: output.clone(),
            })
        }
        ToolOperation::Convert {
            source,
            output_parent,
            range,
            format,
            dpi,
        } => {
            let backend = new_backend()?;
            let page_count = preflight(&backend, source, request.password_for(source))?;
            let pages = parse_pages_or_all(range, page_count)?;
            let mut conversion =
                ConversionRequest::new(source.clone(), output_parent.clone(), pages, *format)
                    .with_dpi(*dpi)
                    .with_cancellation(request.cancellation());
            if let Some(password) = request.password_for(source) {
                conversion = conversion.with_password(JobPassword::new(password.to_owned()));
            }
            convert_pdf(&backend, Some(&WindowsImageEncoder), conversion)
                .map(ToolOutcome::Conversion)
                .map_err(|error| match error {
                    barepdf_pdf::conversion::ConversionError::Pdf(pdf_error) => {
                        operation_error(Some(source), pdf_error)
                    }
                    barepdf_pdf::conversion::ConversionError::Cancelled => ToolFailure::Cancelled,
                    other => ToolFailure::Failed(other.to_string()),
                })
        }
        #[cfg(test)]
        ToolOperation::Test => Ok(ToolOutcome::Test),
    }
}

fn new_backend() -> Result<PdfiumEngine, ToolFailure> {
    PdfiumEngine::new().map_err(|error| ToolFailure::Failed(error.to_string()))
}

fn preflight(
    backend: &dyn PdfBackend,
    source: &Path,
    password: Option<&str>,
) -> Result<PageCount, ToolFailure> {
    backend
        .open_path(source, password)
        .and_then(|document| document.page_count())
        .map_err(|error| operation_error(Some(source), error))
}

fn operation_error(source: Option<&Path>, error: PdfError) -> ToolFailure {
    match error {
        PdfError::PasswordRequired => ToolFailure::PasswordRequired {
            source: source.map(Path::to_path_buf).unwrap_or_default(),
            wrong_password: false,
        },
        PdfError::IncorrectPassword => ToolFailure::PasswordRequired {
            source: source.map(Path::to_path_buf).unwrap_or_default(),
            wrong_password: true,
        },
        other => ToolFailure::Failed(other.to_string()),
    }
}

fn check_cancel(request: &ToolRequest) -> Result<(), ToolFailure> {
    if request.is_cancelled() {
        Err(ToolFailure::Cancelled)
    } else {
        Ok(())
    }
}

fn parse_pages_or_all(range: &str, page_count: PageCount) -> Result<Vec<PageIndex>, ToolFailure> {
    if range.trim().is_empty() || matches!(range.trim(), "all" | "*") {
        Ok((0..page_count.get()).map(PageIndex::from_raw).collect())
    } else {
        PageRangeSelection::parse(range, page_count)
            .map_err(|error| ToolFailure::Failed(error.to_string()))
    }
}

fn parent_directory(path: &Path) -> &Path {
    path.parent().unwrap_or_else(|| Path::new("."))
}

fn persist_file(staged: tempfile::NamedTempFile, output: &Path) -> Result<(), ToolFailure> {
    staged
        .persist_noclobber(output)
        .map(|_| ())
        .map_err(|error| ToolFailure::Failed(error.error.to_string()))
}

fn unique_output_directory(parent: &Path, base_name: &str) -> Result<PathBuf, ToolFailure> {
    for suffix in 1..=10_000_u32 {
        let name = if suffix == 1 {
            format!("{base_name}_pages")
        } else {
            format!("{base_name}_pages_{suffix}")
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(ToolFailure::Failed(
        "could not allocate a unique split output directory".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        persist_file, ExecutionResult, SecretPassword, ToolEvent, ToolJobKey, ToolWorker,
        ToolWorkerError,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn persist_file_never_overwrites_an_existing_destination() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let destination = directory.path().join("output.pdf");
        fs::write(&destination, b"original").expect("existing destination");
        let staged = tempfile::NamedTempFile::new_in(directory.path()).expect("staged file");
        fs::write(staged.path(), b"replacement").expect("staged contents");

        assert!(persist_file(staged, &destination).is_err());
        assert_eq!(
            fs::read(&destination).expect("destination remains"),
            b"original"
        );
    }

    #[test]
    fn worker_allows_only_one_active_or_queued_job() {
        let release = Arc::new(AtomicBool::new(false));
        let worker_release = release.clone();
        let worker = ToolWorker::spawn_with_executor(move |request| {
            while !worker_release.load(Ordering::Acquire) && !request.is_cancelled() {
                std::thread::yield_now();
            }
            ExecutionResult::Cancelled
        })
        .expect("test worker starts");
        let first = ToolJobKey::new(1, 7, 11);
        let second = ToolJobKey::new(2, 7, 11);

        let cancellation = worker.submit_test_job(first).expect("first job queues");
        assert!(matches!(
            worker.submit_test_job(second),
            Err(ToolWorkerError::Busy)
        ));

        cancellation.cancel();
        release.store(true, Ordering::Release);
        assert!(matches!(
            worker.recv_event_timeout(Duration::from_secs(1)),
            Some(ToolEvent::Cancelled { key }) if key == first
        ));
    }

    #[test]
    fn password_is_redacted_and_zeroized_when_cleared() {
        let mut password = SecretPassword::new("not-for-logs".to_owned());

        assert_eq!(format!("{password:?}"), "SecretPassword([REDACTED])");
        password.clear();
        assert!(password.bytes_for_test().iter().all(|byte| *byte == 0));
    }

    #[test]
    fn job_key_rejects_stale_generation_and_source_tokens() {
        let key = ToolJobKey::new(3, 19, 23);

        assert!(key.is_current(19, 23));
        assert!(!key.is_current(20, 23));
        assert!(!key.is_current(19, 24));
    }

    #[test]
    fn password_retry_stays_on_the_same_job_and_source() {
        let worker = ToolWorker::spawn_password_test_worker(PathBuf::from("protected.pdf"))
            .expect("test worker starts");
        let key = ToolJobKey::new(4, 29, 31);
        worker.submit_test_job(key).expect("job queues");
        assert!(matches!(
            worker.recv_event_timeout(Duration::from_secs(1)),
            Some(ToolEvent::PasswordRequired { key: event_key, ref source, wrong_password: false })
                if event_key == key && source == &PathBuf::from("protected.pdf")
        ));

        worker
            .provide_password(key, PathBuf::from("other.pdf"), "wrong-target".to_owned())
            .expect("response command queues");
        assert!(matches!(
            worker.recv_event_timeout(Duration::from_secs(1)),
            Some(ToolEvent::PasswordRequired { key: event_key, ref source, wrong_password: false })
                if event_key == key && source == &PathBuf::from("protected.pdf")
        ));

        worker
            .provide_password(key, PathBuf::from("protected.pdf"), "bad".to_owned())
            .expect("password command queues");
        assert!(matches!(
            worker.recv_event_timeout(Duration::from_secs(1)),
            Some(ToolEvent::PasswordRequired { key: event_key, ref source, wrong_password: true })
                if event_key == key && source == &PathBuf::from("protected.pdf")
        ));
    }
}
