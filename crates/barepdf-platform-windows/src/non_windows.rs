use barepdf_core::{EncodedImageFormat, ImageEncodeError, ImageEncoder, PageCount, RawBitmap};
use barepdf_platform::printing::{
    PrintDuplex, PrintError, PrintJobId, PrintPage, PrintRange, PrintSelection, PrinterDialog,
    PrinterSink,
};
use barepdf_platform::watcher::DocumentFileWatcher as DocumentFileWatcherTrait;
use barepdf_platform::PlatformError;
use raw_window_handle::WindowHandle;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

pub use barepdf_platform::printing::{InstalledPrinter, PrintOrientation};
pub use barepdf_platform::watcher::{DocumentFileWatcher, FileChangeEvent, FileChangeWatcher};
pub use barepdf_platform_linux::{
    ask_yes_no, open_url, show_fatal_error, LinuxClipboard as WindowsClipboard,
    LinuxFileDialogs as WindowsFileDialogs,
};

#[must_use]
pub fn enumerate_installed_printers() -> Vec<InstalledPrinter> {
    Vec::new()
}

pub struct WindowsPrinterSink {
    job_id: PrintJobId,
    target_dpi: u16,
}

impl WindowsPrinterSink {
    pub const DEFAULT_DPI: u16 = 300;

    pub fn direct(
        job_id: PrintJobId,
        target_dpi: u16,
        _printer_name: &str,
        _orientation: PrintOrientation,
        _duplex: PrintDuplex,
        _copies: u16,
    ) -> Result<Self, PrintError> {
        Ok(Self { job_id, target_dpi })
    }
}

impl PrinterSink for WindowsPrinterSink {
    fn job_id(&self) -> PrintJobId {
        self.job_id
    }

    fn target_dpi(&self) -> u16 {
        self.target_dpi
    }

    fn begin(&mut self, _title: &str) -> Result<(), PrintError> {
        Err(PrintError::InvalidState)
    }

    fn write_page(&mut self, _page: PrintPage<'_>) -> Result<(), PrintError> {
        Err(PrintError::InvalidState)
    }

    fn finish(self: Box<Self>) -> Result<(), PrintError> {
        Ok(())
    }
}

pub struct WindowsPrinterDialog;

impl WindowsPrinterDialog {
    #[must_use]
    pub fn new(_owner: WindowHandle<'_>, _target_dpi: u16) -> Self {
        Self
    }

    pub fn select_with_defaults(
        &mut self,
        _job_id: PrintJobId,
        _page_count: PageCount,
        _range: PrintRange,
        _orientation_index: i32,
    ) -> Result<Option<PrintSelection<WindowsPrinterSink>>, PrintError> {
        Ok(None)
    }
}

impl PrinterDialog for WindowsPrinterDialog {
    type Sink = WindowsPrinterSink;

    fn select(
        &mut self,
        _job_id: PrintJobId,
        _page_count: PageCount,
    ) -> Result<Option<PrintSelection<Self::Sink>>, PrintError> {
        Ok(None)
    }
}

pub struct WindowsDocumentFileWatcher {
    path: PathBuf,
}

impl WindowsDocumentFileWatcher {
    pub fn with_callback<F>(
        path: &Path,
        _debounce: std::time::Duration,
        _callback: F,
    ) -> Result<Self, PlatformError>
    where
        F: FnMut(FileChangeEvent) + Send + 'static,
    {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    pub fn stop(&self) {}
}

impl DocumentFileWatcherTrait for WindowsDocumentFileWatcher {
    fn watched_path(&self) -> Option<PathBuf> {
        Some(self.path.clone())
    }

    fn stop(&mut self) {}
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsImageEncoder;

impl ImageEncoder for WindowsImageEncoder {
    fn encode_rgba(
        &self,
        _output: &Path,
        _bitmap: &RawBitmap,
        _format: EncodedImageFormat,
        _dpi: u16,
    ) -> Result<(), ImageEncodeError> {
        Err(ImageEncodeError::new(
            "Image encoding is not supported on this platform",
        ))
    }
}

pub fn decode_image_rgba(_path: &Path) -> Result<RawBitmap, ImageEncodeError> {
    Err(ImageEncodeError::new(
        "Image decoding is not supported on this platform",
    ))
}

#[must_use]
pub fn install_file_drop(_window: WindowHandle<'_>) -> Option<Receiver<Vec<PathBuf>>> {
    None
}

pub fn executable_file_version(_path: &Path) -> Result<[u16; 4], PlatformError> {
    Err(PlatformError::Unavailable {
        operation: "executable_file_version",
    })
}

#[must_use]
pub fn is_installed_build() -> bool {
    std::env::current_exe()
        .map(|path| path.starts_with("/usr"))
        .unwrap_or(false)
}

pub fn launch_installer(path: &Path) -> Result<(), PlatformError> {
    #[cfg(target_os = "linux")]
    {
        barepdf_platform_linux::launch_package_installer(path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Err(PlatformError::Unavailable {
            operation: "launch_installer",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowHit {
    Caption,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl WindowHit {
    #[must_use]
    pub const fn hit_test_code(self) -> usize {
        0
    }

    #[must_use]
    pub const fn from_resize_edge(edge: i32) -> Option<Self> {
        from_resize_edge(edge)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCommand {
    Minimize,
    Maximize,
    ToggleMaximize,
    Close,
}

#[must_use]
pub fn apply_frameless_style(_window: WindowHandle<'_>) -> bool {
    false
}

#[must_use]
pub fn begin_native_drag(_window: WindowHandle<'_>, _hit: WindowHit) -> bool {
    false
}

#[must_use]
pub const fn from_resize_edge(edge: i32) -> Option<WindowHit> {
    match edge {
        0 => Some(WindowHit::Left),
        1 => Some(WindowHit::Right),
        2 => Some(WindowHit::Top),
        3 => Some(WindowHit::Bottom),
        4 => Some(WindowHit::TopLeft),
        5 => Some(WindowHit::TopRight),
        6 => Some(WindowHit::BottomLeft),
        7 => Some(WindowHit::BottomRight),
        _ => None,
    }
}

#[must_use]
pub const fn hit_test_code(_hit: WindowHit) -> usize {
    0
}

#[must_use]
pub fn is_window_maximized(_window: WindowHandle<'_>) -> bool {
    false
}

#[must_use]
pub fn send_window_command(_window: WindowHandle<'_>, _command: WindowCommand) -> bool {
    false
}

#[must_use]
pub fn reduce_visual_effects() -> bool {
    false
}
