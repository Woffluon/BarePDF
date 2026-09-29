use crate::ffi::{self, DialogPrinter, PrinterJob};
use barepdf_core::{PageCount, PageIndex};
use barepdf_platform::printing::{
    Copies, PrintError, PrintJobId, PrintPage, PrintRange, PrintSelection, PrinterDialog,
    PrinterSink,
};
use windows_sys::Win32::Foundation::HWND;

pub struct WindowsPrinterDialog {
    owner: HWND,
    target_dpi: u16,
}

pub use barepdf_platform::printing::PrintOrientation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrintDialogOptions {
    range: PrintRange,
    orientation: PrintOrientation,
}

impl PrintDialogOptions {
    pub(crate) const fn new(range: PrintRange, orientation: PrintOrientation) -> Self {
        Self { range, orientation }
    }

    pub(crate) const fn range(self) -> PrintRange {
        self.range
    }

    pub(crate) const fn orientation(self) -> PrintOrientation {
        self.orientation
    }
}

impl WindowsPrinterDialog {
    pub const DEFAULT_DPI: u16 = 300;
    pub const MAX_DPI: u16 = 600;
    const MIN_DPI: u16 = 1;

    #[must_use]
    pub const fn new(owner: HWND) -> Self {
        Self {
            owner,
            target_dpi: Self::DEFAULT_DPI,
        }
    }

    /// Overrides raster target DPI while retaining driver-owned paper and orientation settings.
    ///
    /// # Errors
    ///
    /// Returns [`PrintError::InvalidDpi`] unless `dpi` is in `1..=600`.
    pub fn with_target_dpi(mut self, dpi: u16) -> Result<Self, PrintError> {
        if !(Self::MIN_DPI..=Self::MAX_DPI).contains(&dpi) {
            return Err(PrintError::InvalidDpi(dpi));
        }
        self.target_dpi = dpi;
        Ok(self)
    }

    /// Shows the native dialog with the in-app preview choices selected initially.
    /// The native dialog remains authoritative and may change either choice.
    ///
    /// # Errors
    ///
    /// Returns an error when the native dialog fails or returns invalid settings.
    pub fn select_with_defaults(
        &mut self,
        job_id: PrintJobId,
        page_count: PageCount,
        range: PrintRange,
        orientation_index: i32,
    ) -> Result<Option<PrintSelection<WindowsPrinterSink>>, PrintError> {
        let options =
            PrintDialogOptions::new(range, PrintOrientation::from_index(orientation_index));
        let Some(selection) = ffi::show_print_dialog(self.owner, page_count.get(), options)? else {
            return Ok(None);
        };
        selection_from_dialog(selection, job_id, page_count, self.target_dpi).map(Some)
    }
}

impl PrinterDialog for WindowsPrinterDialog {
    type Sink = WindowsPrinterSink;

    fn select(
        &mut self,
        job_id: PrintJobId,
        page_count: PageCount,
    ) -> Result<Option<PrintSelection<Self::Sink>>, PrintError> {
        self.select_with_defaults(job_id, page_count, PrintRange::all(page_count), 0)
    }
}

fn selection_from_dialog(
    selection: DialogPrinter,
    job_id: PrintJobId,
    page_count: PageCount,
    target_dpi: u16,
) -> Result<PrintSelection<WindowsPrinterSink>, PrintError> {
    let range = if selection.page_numbers {
        let first = u32::from(selection.from_page)
            .checked_sub(1)
            .map(PageIndex::from_raw)
            .ok_or(PrintError::InvalidRange)?;
        let last = u32::from(selection.to_page)
            .checked_sub(1)
            .map(PageIndex::from_raw)
            .ok_or(PrintError::InvalidRange)?;
        PrintRange::new(first, last, page_count)?
    } else {
        PrintRange::all(page_count)
    };
    Ok(PrintSelection {
        sink: WindowsPrinterSink {
            job_id,
            target_dpi,
            device: Some(selection.device),
            job: None,
        },
        range,
        copies: Copies::new(selection.copies)?,
    })
}

pub struct WindowsPrinterSink {
    job_id: PrintJobId,
    target_dpi: u16,
    device: Option<ffi::PrinterDevice>,
    job: Option<PrinterJob>,
}

impl WindowsPrinterSink {
    pub const DEFAULT_DPI: u16 = 300;

    /// Creates a printer sink targeting a specific printer directly, without showing any Windows dialog.
    ///
    /// # Errors
    ///
    /// Returns an error if the printer device context cannot be created.
    pub fn direct(
        job_id: PrintJobId,
        target_dpi: u16,
        printer_name: &str,
        orientation: PrintOrientation,
        copies: u16,
    ) -> Result<Self, PrintError> {
        let device = ffi::create_direct_printer_device(printer_name, orientation, copies)?;
        Ok(Self {
            job_id,
            target_dpi,
            device: Some(device),
            job: None,
        })
    }
}

impl PrinterSink for WindowsPrinterSink {
    fn job_id(&self) -> PrintJobId {
        self.job_id
    }

    fn target_dpi(&self) -> u16 {
        self.target_dpi
    }

    fn begin(&mut self, title: &str) -> Result<(), PrintError> {
        if self.job.is_some() {
            return Err(PrintError::InvalidState);
        }
        let device = self.device.take().ok_or(PrintError::InvalidState)?;
        self.job = Some(device.start_document(title)?);
        Ok(())
    }

    fn write_page(&mut self, page: PrintPage<'_>) -> Result<(), PrintError> {
        self.job
            .as_mut()
            .ok_or(PrintError::InvalidState)?
            .write_page(page)
    }

    fn finish(mut self: Box<Self>) -> Result<(), PrintError> {
        self.job.take().ok_or(PrintError::InvalidState)?.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{PrintDialogOptions, PrintOrientation, WindowsPrinterDialog};
    use barepdf_core::{PageCount, PageIndex};
    use barepdf_platform::printing::{PrintError, PrintRange};

    #[test]
    fn target_dpi_defaults_to_three_hundred_and_is_capped() {
        let dialog = WindowsPrinterDialog::new(std::ptr::null_mut());
        assert_eq!(dialog.target_dpi, 300);
        assert!(WindowsPrinterDialog::new(std::ptr::null_mut())
            .with_target_dpi(600)
            .is_ok());
        assert!(matches!(
            WindowsPrinterDialog::new(std::ptr::null_mut()).with_target_dpi(0),
            Err(PrintError::InvalidDpi(0))
        ));
        assert!(matches!(
            WindowsPrinterDialog::new(std::ptr::null_mut()).with_target_dpi(601),
            Err(PrintError::InvalidDpi(601))
        ));
    }

    #[test]
    fn preview_options_preserve_initial_range_and_orientation() {
        let page_count = PageCount::new(8).expect("test page count");
        let range = PrintRange::new(PageIndex::from_raw(1), PageIndex::from_raw(5), page_count)
            .expect("test range");

        let options = PrintDialogOptions::new(range, PrintOrientation::Landscape);

        assert_eq!(options.range(), range);
        assert_eq!(options.orientation(), PrintOrientation::Landscape);
    }

    #[test]
    fn direct_sink_rejects_empty_printer_name() {
        use super::WindowsPrinterSink;
        use barepdf_platform::printing::PrintJobId;
        let job_id = PrintJobId::new(1).unwrap();
        let result = WindowsPrinterSink::direct(job_id, 300, "", PrintOrientation::Portrait, 1);
        assert!(result.is_err());
    }

    #[test]
    fn direct_sink_creates_device_for_installed_printer_if_any() {
        use super::WindowsPrinterSink;
        use crate::printers::enumerate_installed_printers;
        use barepdf_platform::printing::{PrintJobId, PrinterSink};
        let printers = enumerate_installed_printers();
        if let Some(printer) = printers.first() {
            let job_id = PrintJobId::new(1).unwrap();
            let sink = WindowsPrinterSink::direct(
                job_id,
                300,
                &printer.name,
                PrintOrientation::Landscape,
                1,
            );
            assert!(sink.is_ok());
            let sink = sink.unwrap();
            assert_eq!(sink.job_id(), job_id);
            assert_eq!(sink.target_dpi(), 300);
        }
    }
}
