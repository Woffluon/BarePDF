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
}

impl PrinterDialog for WindowsPrinterDialog {
    type Sink = WindowsPrinterSink;

    fn select(
        &mut self,
        job_id: PrintJobId,
        page_count: PageCount,
    ) -> Result<Option<PrintSelection<Self::Sink>>, PrintError> {
        let Some(selection) = ffi::show_print_dialog(self.owner, page_count.get())? else {
            return Ok(None);
        };
        selection_from_dialog(selection, job_id, page_count, self.target_dpi).map(Some)
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
    use super::WindowsPrinterDialog;
    use barepdf_platform::printing::PrintError;

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
}
