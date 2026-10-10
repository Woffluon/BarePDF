#![forbid(unsafe_code)]

mod error;
pub mod printing;
pub mod watcher;

pub use error::PlatformError;
pub use watcher::{
    DocumentFileWatcher, FileChangeEvent, FileChangeWatcher, FileWatcher, DEFAULT_DEBOUNCE_DURATION,
};
