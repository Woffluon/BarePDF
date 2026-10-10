use barepdf_platform::watcher::{
    DocumentFileWatcher as DocumentFileWatcherTrait, FileChangeEvent, DEFAULT_DEBOUNCE_DURATION,
};
use barepdf_platform::PlatformError;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
    FILE_NOTIFY_CHANGE_CREATION, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_NOTIFY_CHANGE_SIZE,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE,
};

const WAIT_OBJECT_0: u32 = 0;

#[derive(Clone, Copy)]
struct SyncHandle(HANDLE);

// SAFETY: Windows kernel event handles are thread-safe and can be signaled or closed from any thread.
unsafe impl Send for SyncHandle {}
// SAFETY: Access to Windows kernel handle operations is thread-safe.
unsafe impl Sync for SyncHandle {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSnapshot {
    exists: bool,
    modified: Option<std::time::SystemTime>,
    len: u64,
}

impl FileSnapshot {
    fn capture(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(metadata) => Self {
                exists: true,
                modified: metadata.modified().ok(),
                len: metadata.len(),
            },
            Err(_) => Self {
                exists: false,
                modified: None,
                len: 0,
            },
        }
    }

    fn has_changed_from(&self, previous: &Self) -> bool {
        if !self.exists {
            return false;
        }
        if !previous.exists {
            return true;
        }
        self.modified != previous.modified || self.len != previous.len
    }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

/// Windows file watcher monitoring a target document file for external modifications.
///
/// Uses Win32 directory change notifications with kernel event waiting (0% CPU at idle)
/// and configurable write-completion debouncing.
pub struct WindowsDocumentFileWatcher {
    watched_path: Arc<Mutex<Option<PathBuf>>>,
    debounce: Duration,
    stop_event: Mutex<Option<SyncHandle>>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    notify_callback: Arc<dyn Fn(FileChangeEvent) + Send + Sync + 'static>,
}

impl WindowsDocumentFileWatcher {
    /// Starts watching `path` with the default debounce duration ([`DEFAULT_DEBOUNCE_DURATION`]).
    /// Returns the watcher instance and a receiver delivering [`FileChangeEvent`]s.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError`] if the parent directory does not exist or Windows handle
    /// creation fails.
    pub fn new(path: impl AsRef<Path>) -> Result<(Self, Receiver<FileChangeEvent>), PlatformError> {
        Self::with_debounce(path, DEFAULT_DEBOUNCE_DURATION)
    }

    /// Starts watching `path` with a custom debounce duration.
    /// Returns the watcher instance and a receiver delivering [`FileChangeEvent`]s.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError`] if the parent directory does not exist or Windows handle
    /// creation fails.
    pub fn with_debounce(
        path: impl AsRef<Path>,
        debounce: Duration,
    ) -> Result<(Self, Receiver<FileChangeEvent>), PlatformError> {
        let (tx, rx) = mpsc::channel();
        let callback = Arc::new(move |event| {
            let _ = tx.send(event);
        });
        let watcher = Self::with_callback_internal(path.as_ref(), debounce, callback)?;
        Ok((watcher, rx))
    }

    /// Starts watching `path` with a callback invoked on external modification.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError`] if the parent directory does not exist or Windows handle
    /// creation fails.
    pub fn with_callback<F>(
        path: impl AsRef<Path>,
        debounce: Duration,
        callback: F,
    ) -> Result<Self, PlatformError>
    where
        F: Fn(FileChangeEvent) + Send + Sync + 'static,
    {
        Self::with_callback_internal(path.as_ref(), debounce, Arc::new(callback))
    }

    fn with_callback_internal(
        path: &Path,
        debounce: Duration,
        callback: Arc<dyn Fn(FileChangeEvent) + Send + Sync + 'static>,
    ) -> Result<Self, PlatformError> {
        let watcher = Self {
            watched_path: Arc::new(Mutex::new(None)),
            debounce,
            stop_event: Mutex::new(None),
            thread_handle: Mutex::new(None),
            notify_callback: callback,
        };

        watcher.start_watching(path)?;
        Ok(watcher)
    }

    fn start_watching(&self, target_path: &Path) -> Result<(), PlatformError> {
        let target_path = if target_path.is_absolute() {
            target_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(target_path))
                .unwrap_or_else(|_| target_path.to_path_buf())
        };

        let parent_dir = target_path.parent().ok_or(PlatformError::InvalidData {
            operation: "Watch document",
            reason: "Target path has no valid parent directory",
        })?;

        if !parent_dir.is_dir() {
            return Err(PlatformError::InvalidData {
                operation: "Watch document",
                reason: "Parent directory does not exist or is not a directory",
            });
        }

        let dir_wide = wide_null(parent_dir.as_os_str());
        let notify_filter = FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_LAST_WRITE
            | FILE_NOTIFY_CHANGE_SIZE
            | FILE_NOTIFY_CHANGE_CREATION;

        // SAFETY: `dir_wide` is a valid null-terminated UTF-16 string buffer.
        let change_handle = unsafe {
            FindFirstChangeNotificationW(
                dir_wide.as_ptr(),
                0, // watch only immediate directory, not recursive subtree
                notify_filter,
            )
        };

        if change_handle == INVALID_HANDLE_VALUE || change_handle.is_null() {
            // SAFETY: GetLastError has no preconditions and is called immediately upon failure.
            let code = unsafe { GetLastError() };
            return Err(PlatformError::Windows {
                operation: "Could not create directory change notification handle",
                code,
            });
        }

        // SAFETY: CreateEventW creates an anonymous manual-reset event initially nonsignaled.
        let stop_event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if stop_event.is_null() {
            // SAFETY: change_handle is closed immediately on error.
            unsafe {
                FindCloseChangeNotification(change_handle);
            }
            // SAFETY: GetLastError called immediately upon CreateEventW failure.
            let code = unsafe { GetLastError() };
            return Err(PlatformError::Windows {
                operation: "Could not create watcher stop event",
                code,
            });
        }

        let debounce = self.debounce;
        let thread_target = target_path.clone();
        let thread_stop = SyncHandle(stop_event);
        let thread_change = SyncHandle(change_handle);
        let thread_callback = Arc::clone(&self.notify_callback);

        let handle = std::thread::Builder::new()
            .name("barepdf-file-watcher".into())
            .spawn(move || {
                run_watcher_loop(
                    thread_target,
                    thread_stop,
                    thread_change,
                    debounce,
                    thread_callback,
                );
            })
            .map_err(|source| {
                // SAFETY: stop_event and change_handle are cleaned up if thread spawning fails.
                unsafe {
                    CloseHandle(stop_event);
                    FindCloseChangeNotification(change_handle);
                }
                PlatformError::Io {
                    operation: "Could not spawn file watcher thread",
                    source,
                }
            })?;

        if let Ok(mut wp) = self.watched_path.lock() {
            *wp = Some(target_path);
        }
        if let Ok(mut se) = self.stop_event.lock() {
            *se = Some(SyncHandle(stop_event));
        }
        if let Ok(mut th) = self.thread_handle.lock() {
            *th = Some(handle);
        }

        Ok(())
    }

    /// Changes the target file being watched.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError`] if the new target cannot be watched.
    pub fn watch(&self, path: impl AsRef<Path>) -> Result<(), PlatformError> {
        self.stop();
        self.start_watching(path.as_ref())
    }

    /// Stops watching the current file.
    pub fn stop(&self) {
        if let Ok(mut wp) = self.watched_path.lock() {
            *wp = None;
        }

        let stop_handle = self
            .stop_event
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());
        let thread = self
            .thread_handle
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());

        if let Some(event) = stop_handle {
            // SAFETY: event.0 is a valid Win32 event handle.
            unsafe {
                SetEvent(event.0);
            }
            if let Some(th) = thread {
                let _ = th.join();
            }
            // SAFETY: Worker thread has terminated and will not access event.0 again.
            unsafe {
                CloseHandle(event.0);
            }
        }
    }
}

fn run_watcher_loop(
    target_path: PathBuf,
    stop_event: SyncHandle,
    change_handle: SyncHandle,
    debounce_duration: Duration,
    callback: Arc<dyn Fn(FileChangeEvent) + Send + Sync + 'static>,
) {
    let stop_event = stop_event.0;
    let change_handle = change_handle.0;
    let handles = [stop_event, change_handle];
    let mut last_snapshot = FileSnapshot::capture(&target_path);

    loop {
        // SAFETY: `handles` contains 2 valid kernel handles. INFINITE wait sleeps with 0% CPU.
        let wait_res = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };

        if wait_res == WAIT_OBJECT_0 {
            // stop_event signaled
            break;
        }
        if wait_res != WAIT_OBJECT_0 + 1 {
            // Error, handle closed, or WAIT_FAILED
            break;
        }

        // SAFETY: change_handle is a valid notification handle being reset for subsequent events.
        let next_ok = unsafe { FindNextChangeNotification(change_handle) };
        if next_ok == 0 {
            break;
        }

        // Debounce phase: absorb multi-step writes (such as LaTeX/Typst compiler passes)
        let start = Instant::now();
        let max_debounce_deadline = start + Duration::from_millis(2500);
        let mut quiet_deadline = Instant::now() + debounce_duration;
        let mut stop_signaled = false;

        while Instant::now() < quiet_deadline && Instant::now() < max_debounce_deadline {
            let remaining = quiet_deadline.saturating_duration_since(Instant::now());
            let wait_ms = (remaining.as_millis() as u32).max(1);

            // SAFETY: `handles` contains 2 valid kernel handles.
            let debounce_res = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, wait_ms) };

            if debounce_res == WAIT_OBJECT_0 {
                stop_signaled = true;
                break;
            } else if debounce_res == WAIT_OBJECT_0 + 1 {
                // Additional change during quiet window; reset quiet window
                // SAFETY: change_handle is a valid notification handle.
                let _ = unsafe { FindNextChangeNotification(change_handle) };
                quiet_deadline = Instant::now() + debounce_duration;
            } else if debounce_res == WAIT_TIMEOUT {
                // Quiet window elapsed without further changes
                break;
            } else {
                // WAIT_FAILED or error
                break;
            }
        }

        if stop_signaled {
            break;
        }

        // Write completion check: if file exists, verify it can be opened for reading
        let mut current_snapshot = FileSnapshot::capture(&target_path);
        if current_snapshot.exists {
            for _ in 0..6 {
                if std::fs::File::open(&target_path).is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
                current_snapshot = FileSnapshot::capture(&target_path);
            }
        }

        if current_snapshot.has_changed_from(&last_snapshot) {
            last_snapshot = current_snapshot;
            callback(FileChangeEvent::Modified(target_path.clone()));
        }
    }

    // SAFETY: change_handle is owned solely by this background thread and is closed on exit.
    unsafe {
        FindCloseChangeNotification(change_handle);
    }
}

impl Drop for WindowsDocumentFileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

impl DocumentFileWatcherTrait for WindowsDocumentFileWatcher {
    fn watched_path(&self) -> Option<PathBuf> {
        self.watched_path
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    fn stop(&mut self) {
        WindowsDocumentFileWatcher::stop(self);
    }
}

pub use WindowsDocumentFileWatcher as DocumentFileWatcher;
pub use WindowsDocumentFileWatcher as FileChangeWatcher;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn watcher_creation_succeeds_and_watched_path_matches() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("document.pdf");
        fs::write(&file_path, b"%PDF-1.4 initial content").expect("write file");

        let (watcher, rx) = WindowsDocumentFileWatcher::new(&file_path).expect("create watcher");
        assert_eq!(watcher.watched_path(), Some(file_path.clone()));
        drop(watcher);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn watcher_fails_on_nonexistent_parent_dir() {
        let nonexistent = Path::new("Z:\\nonexistent_dir_12345\\sub\\doc.pdf");
        let result = WindowsDocumentFileWatcher::new(nonexistent);
        assert!(result.is_err());
    }

    #[test]
    fn watcher_detects_in_place_file_modification() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("paper.pdf");
        fs::write(&file_path, b"%PDF-1.4 version 1").expect("write initial");

        let debounce = Duration::from_millis(80);
        let (watcher, rx) =
            WindowsDocumentFileWatcher::with_debounce(&file_path, debounce).expect("watcher");

        // Give background watcher thread a brief moment to enter kernel wait
        std::thread::sleep(Duration::from_millis(50));

        // Perform in-place write modification
        {
            let mut f = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&file_path)
                .expect("open for write");
            f.write_all(b"%PDF-1.4 version 2 modified content")
                .expect("write updated");
            f.flush().expect("flush");
        }

        let event = rx
            .recv_timeout(Duration::from_millis(2000))
            .expect("should receive modification event");
        assert_eq!(event.path(), &file_path);

        drop(watcher);
    }

    #[test]
    fn watcher_detects_atomic_file_replacement() {
        // Typical LaTeX/Typst workflow: compiler generates temp file and renames over target PDF
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("thesis.pdf");
        fs::write(&file_path, b"%PDF-1.4 old thesis").expect("write initial");

        let debounce = Duration::from_millis(80);
        let (watcher, rx) =
            WindowsDocumentFileWatcher::with_debounce(&file_path, debounce).expect("watcher");

        std::thread::sleep(Duration::from_millis(50));

        let temp_build = dir.path().join("thesis.pdf.tmp");
        fs::write(&temp_build, b"%PDF-1.4 newly compiled thesis by typst").expect("compile temp");
        fs::rename(&temp_build, &file_path).expect("atomic rename");

        let event = rx
            .recv_timeout(Duration::from_millis(2000))
            .expect("should receive replacement event");
        assert_eq!(event.path(), &file_path);

        drop(watcher);
    }

    #[test]
    fn watcher_debounces_rapid_writes_into_single_notification() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("notes.pdf");
        fs::write(&file_path, b"%PDF-1.4 start").expect("write initial");

        let debounce = Duration::from_millis(150);
        let (watcher, rx) =
            WindowsDocumentFileWatcher::with_debounce(&file_path, debounce).expect("watcher");

        std::thread::sleep(Duration::from_millis(50));

        // Simulate multi-pass compiler writing 4 rapid bursts within debounce window
        for i in 1..=4 {
            fs::write(&file_path, format!("%PDF-1.4 pass {i}")).expect("pass write");
            std::thread::sleep(Duration::from_millis(20));
        }

        // Wait for debounce window to settle and yield notification
        let event = rx
            .recv_timeout(Duration::from_millis(2000))
            .expect("should receive debounced event");
        assert_eq!(event.path(), &file_path);

        // Verify no second notification was queued
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            rx.try_recv().is_err(),
            "rapid writes must coalesce into a single notification"
        );

        drop(watcher);
    }

    #[test]
    fn watcher_ignores_unrelated_files_in_same_directory() {
        let dir = tempdir().expect("tempdir");
        let target_pdf = dir.path().join("report.pdf");
        let aux_file = dir.path().join("report.aux");
        fs::write(&target_pdf, b"%PDF-1.4 report").expect("write target");
        fs::write(&aux_file, b"auxiliary compiler state").expect("write aux");

        let debounce = Duration::from_millis(80);
        let (watcher, rx) =
            WindowsDocumentFileWatcher::with_debounce(&target_pdf, debounce).expect("watcher");

        std::thread::sleep(Duration::from_millis(50));

        // Write to unrelated file in the same directory
        fs::write(&aux_file, b"updated auxiliary state").expect("update aux");

        // Give time for any spurious notification
        std::thread::sleep(Duration::from_millis(250));

        assert!(
            rx.try_recv().is_err(),
            "changes to unrelated files in the same directory must not trigger target watcher"
        );

        drop(watcher);
    }

    #[test]
    fn watcher_can_switch_watched_file() {
        let dir = tempdir().expect("tempdir");
        let file_a = dir.path().join("doc_a.pdf");
        let file_b = dir.path().join("doc_b.pdf");
        fs::write(&file_a, b"%PDF-1.4 doc a").expect("write a");
        fs::write(&file_b, b"%PDF-1.4 doc b").expect("write b");

        let debounce = Duration::from_millis(80);
        let (watcher, rx) =
            WindowsDocumentFileWatcher::with_debounce(&file_a, debounce).expect("watcher");

        std::thread::sleep(Duration::from_millis(50));

        // Switch to watching file_b
        watcher.watch(&file_b).expect("switch target");
        assert_eq!(watcher.watched_path(), Some(file_b.clone()));

        std::thread::sleep(Duration::from_millis(50));

        // Modify file_a (should be ignored now)
        fs::write(&file_a, b"%PDF-1.4 doc a updated").expect("modify a");
        std::thread::sleep(Duration::from_millis(150));
        assert!(rx.try_recv().is_err());

        // Modify file_b (should trigger notification)
        fs::write(&file_b, b"%PDF-1.4 doc b updated").expect("modify b");
        let event = rx
            .recv_timeout(Duration::from_millis(2000))
            .expect("should receive event for new target");
        assert_eq!(event.path(), &file_b);

        drop(watcher);
    }

    #[test]
    fn watcher_stop_cleanly_terminates_worker() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("stop_test.pdf");
        fs::write(&file_path, b"%PDF-1.4 initial").expect("write");

        let debounce = Duration::from_millis(80);
        let (watcher, rx) =
            WindowsDocumentFileWatcher::with_debounce(&file_path, debounce).expect("watcher");

        std::thread::sleep(Duration::from_millis(50));

        // Stop the watcher
        watcher.stop();
        assert_eq!(watcher.watched_path(), None);

        // Modifying after stop does not trigger notification
        fs::write(&file_path, b"%PDF-1.4 modified after stop").expect("write after stop");
        std::thread::sleep(Duration::from_millis(200));

        assert!(rx.try_recv().is_err());
    }
}
