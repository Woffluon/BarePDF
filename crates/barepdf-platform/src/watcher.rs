use std::path::PathBuf;
use std::time::Duration;

/// Default debounce window to absorb multi-pass file writes and ensure write completion.
pub const DEFAULT_DEBOUNCE_DURATION: Duration = Duration::from_millis(400);

/// An event representing a change to a watched document file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeEvent {
    /// The watched file was modified, overwritten, or recreated.
    Modified(PathBuf),
}

impl FileChangeEvent {
    /// Returns the path of the file that changed.
    #[must_use]
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Modified(path) => path,
        }
    }
}

/// Abstract interface for a file watcher monitoring document file changes.
pub trait DocumentFileWatcher: Send + Sync {
    /// Returns the currently watched file path, or `None` if stopped or not watching.
    fn watched_path(&self) -> Option<PathBuf>;

    /// Stops watching the file.
    fn stop(&mut self);
}

/// Alias for [`DocumentFileWatcher`].
pub trait FileWatcher: DocumentFileWatcher {}
impl<T: DocumentFileWatcher + ?Sized> FileWatcher for T {}

/// Alias for [`DocumentFileWatcher`].
pub trait FileChangeWatcher: DocumentFileWatcher {}
impl<T: DocumentFileWatcher + ?Sized> FileChangeWatcher for T {}
