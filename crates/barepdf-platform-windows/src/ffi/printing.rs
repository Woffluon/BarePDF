use crate::printing::{PrintDialogOptions, PrintOrientation};
use barepdf_platform::printing::{PrintError, PrintPage};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Foundation::{GetLastError, GlobalFree, HGLOBAL, HWND};
use windows_sys::Win32::Graphics::Gdi::{
    DeleteDC, GetDeviceCaps, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DEVMODEW,
    DIB_RGB_COLORS, DMORIENT_LANDSCAPE, DMORIENT_PORTRAIT, DM_ORIENTATION, GDI_ERROR, HDC, HORZRES,
    RGBQUAD, SRCCOPY, VERTRES,
};
use windows_sys::Win32::Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, PrintDlgW, PD_ALLPAGES, PD_NOSELECTION, PD_PAGENUMS, PD_RETURNDC,
    PD_RETURNDEFAULT, PRINTDLGW,
};

pub(crate) struct DialogPrinter {
    pub(crate) device: PrinterDevice,
    pub(crate) from_page: u16,
    pub(crate) to_page: u16,
    pub(crate) copies: u16,
    pub(crate) page_numbers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DialogInitialValues {
    from_page: u16,
    to_page: u16,
    page_numbers: bool,
    orientation: Option<PrintOrientation>,
}

fn dialog_initial_values(page_count: u32, options: PrintDialogOptions) -> DialogInitialValues {
    let maximum = u16::try_from(page_count).unwrap_or(u16::MAX).max(1);
    let from_page = u16::try_from(options.range().first().get().saturating_add(1))
        .unwrap_or(maximum)
        .clamp(1, maximum);
    let to_page = u16::try_from(options.range().last().get().saturating_add(1))
        .unwrap_or(maximum)
        .clamp(from_page, maximum);
    let orientation = match options.orientation() {
        PrintOrientation::Auto => None,
        orientation => Some(orientation),
    };
    DialogInitialValues {
        from_page,
        to_page,
        page_numbers: from_page != 1 || to_page != maximum,
        orientation,
    }
}

struct DialogAllocations {
    dev_mode: HGLOBAL,
    dev_names: HGLOBAL,
}

impl Drop for DialogAllocations {
    fn drop(&mut self) {
        if !self.dev_mode.is_null() {
            // SAFETY: PrintDlgW returned this movable global-memory handle to the caller. No lock
            // or borrowed pointer remains, and this owner frees it exactly once.
            let _ = unsafe { GlobalFree(self.dev_mode) };
        }
        if !self.dev_names.is_null() {
            // SAFETY: Same ownership invariant as `dev_mode`; both handles are independent.
            let _ = unsafe { GlobalFree(self.dev_names) };
        }
    }
}

pub(crate) fn show_print_dialog(
    owner: HWND,
    page_count: u32,
    options: PrintDialogOptions,
) -> Result<Option<DialogPrinter>, PrintError> {
    // SAFETY: PRINTDLGW is a plain C aggregate for which Windows specifies zero initialization
    // before required fields are assigned below. Every pointer field stays null.
    let mut dialog = unsafe { std::mem::zeroed::<PRINTDLGW>() };
    dialog.lStructSize =
        u32::try_from(std::mem::size_of::<PRINTDLGW>()).map_err(|_| PrintError::Platform {
            operation: "PrintDlgW size",
            code: 0,
        })?;
    dialog.hwndOwner = owner;
    let initial = dialog_initial_values(page_count, options);
    dialog.Flags = PD_NOSELECTION | PD_RETURNDC;
    if initial.page_numbers {
        dialog.Flags |= PD_PAGENUMS;
    } else {
        dialog.Flags |= PD_ALLPAGES;
    }
    dialog.nMinPage = 1;
    dialog.nMaxPage = u16::try_from(page_count).unwrap_or(u16::MAX);
    dialog.nFromPage = initial.from_page;
    dialog.nToPage = initial.to_page;
    dialog.nCopies = 1;

    if let Some(orientation) = initial.orientation {
        let mut defaults = dialog;
        defaults.Flags = PD_RETURNDEFAULT;
        defaults.hwndOwner = std::ptr::null_mut();
        // SAFETY: The structure is initialized and both required input handles are null. This call
        // does not show UI and returns movable global-memory handles for the default printer.
        if unsafe { PrintDlgW(&raw mut defaults) } != 0 {
            dialog.hDevMode = defaults.hDevMode;
            dialog.hDevNames = defaults.hDevNames;
            apply_orientation(dialog.hDevMode, orientation);
        } else {
            // A missing default printer is not treated as cancellation here; the visible dialog may
            // still let the user choose a printer. A real common-dialog error still fails closed.
            let code = unsafe { CommDlgExtendedError() };
            if code != 0 {
                return Err(PrintError::Dialog(code));
            }
        }
    }

    // SAFETY: `dialog` has the documented size and initialized scalar fields; optional handles and
    // callback/template pointers are null. The owner HWND may be null or a live UI-owned window.
    let accepted = unsafe { PrintDlgW(&raw mut dialog) };
    let allocations = DialogAllocations {
        dev_mode: dialog.hDevMode,
        dev_names: dialog.hDevNames,
    };
    if accepted == 0 {
        // SAFETY: Called immediately after failed PrintDlgW on the same thread, before another
        // common-dialog API can overwrite its thread-local extended error.
        let code = unsafe { CommDlgExtendedError() };
        return if code == 0 {
            Ok(None)
        } else {
            Err(PrintError::Dialog(code))
        };
    }
    if dialog.hDC.is_null() {
        return Err(PrintError::Platform {
            operation: "PrintDlgW printer DC",
            code: 0,
        });
    }
    let result = DialogPrinter {
        device: PrinterDevice(dialog.hDC),
        from_page: dialog.nFromPage,
        to_page: dialog.nToPage,
        copies: dialog.nCopies,
        page_numbers: dialog.Flags & PD_PAGENUMS != 0,
    };
    drop(allocations);
    Ok(Some(result))
}

fn apply_orientation(dev_mode: HGLOBAL, orientation: PrintOrientation) {
    if dev_mode.is_null() {
        return;
    }
    let orientation = match orientation {
        PrintOrientation::Portrait => i16::try_from(DMORIENT_PORTRAIT).unwrap_or(1),
        PrintOrientation::Landscape => i16::try_from(DMORIENT_LANDSCAPE).unwrap_or(2),
        PrintOrientation::Auto => return,
    };
    // SAFETY: PrintDlgW returned a movable global-memory handle containing DEVMODEW. The pointer is
    // used only while locked and no other thread can access this private dialog setup structure.
    let pointer = unsafe { GlobalLock(dev_mode) }.cast::<DEVMODEW>();
    if pointer.is_null() {
        return;
    }
    unsafe {
        (*pointer).dmFields |= DM_ORIENTATION;
        (*pointer).Anonymous1.Anonymous1.dmOrientation = orientation;
        let _ = GlobalUnlock(dev_mode);
    }
}

pub(crate) struct PrinterDevice(HDC);

// SAFETY: This wrapper owns a printer memory DC, never exposes its HDC, and allows only serialized
// `&mut self` access. GDI printer DCs may be transferred between threads when no concurrent call
// uses the handle; the creator thread retains no alias after PrintDlgW returns it.
unsafe impl Send for PrinterDevice {}

impl PrinterDevice {
    pub(crate) fn start_document(self, title: &str) -> Result<PrinterJob, PrintError> {
        let title = wide_null(OsStr::new(title));
        let info = DOCINFOW {
            cbSize: i32::try_from(std::mem::size_of::<DOCINFOW>()).map_err(|_| {
                PrintError::Platform {
                    operation: "StartDocW size",
                    code: 0,
                }
            })?,
            lpszDocName: title.as_ptr(),
            lpszOutput: std::ptr::null(),
            lpszDatatype: std::ptr::null(),
            fwType: 0,
        };
        // SAFETY: This owner exclusively holds a live printer HDC. `info` and its NUL-terminated
        // title remain live for the synchronous call; all optional pointers are null.
        if unsafe { StartDocW(self.0, &raw const info) } <= 0 {
            return Err(last_error("StartDocW"));
        }
        Ok(PrinterJob {
            device: self,
            active: true,
        })
    }
}

impl Drop for PrinterDevice {
    fn drop(&mut self) {
        // SAFETY: This wrapper exclusively owns the non-null printer HDC returned by PrintDlgW and
        // calls DeleteDC exactly once after any active print document has ended or been aborted.
        let _ = unsafe { DeleteDC(self.0) };
    }
}

pub(crate) struct PrinterJob {
    device: PrinterDevice,
    active: bool,
}

impl PrinterJob {
    pub(crate) fn write_page(&mut self, page: PrintPage<'_>) -> Result<(), PrintError> {
        if !self.active {
            return Err(PrintError::InvalidState);
        }
        let result = self.write_page_inner(page);
        if result.is_err() {
            self.abort();
        }
        result
    }

    fn write_page_inner(&mut self, page: PrintPage<'_>) -> Result<(), PrintError> {
        let width = i32::try_from(page.width()).map_err(|_| PrintError::InvalidPage)?;
        let height = i32::try_from(page.height()).map_err(|_| PrintError::InvalidPage)?;
        let image_size = u32::try_from(page.bgra().len()).map_err(|_| PrintError::InvalidPage)?;

        // SAFETY: The job exclusively owns a live printer HDC between successful StartDocW and
        // EndDoc/AbortDoc. No Rust pointer crosses this call.
        if unsafe { StartPage(self.device.0) } <= 0 {
            return Err(last_error("StartPage"));
        }

        let (x, y, output_width, output_height) = fitted_page(self.device.0, width, height)?;
        let bitmap = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>())
                    .map_err(|_| PrintError::InvalidPage)?,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: image_size,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };
        // SAFETY: The HDC is live and exclusively borrowed. `page.bgra()` points to exactly the
        // validated width*height*4 bytes for the synchronous call. `bitmap` describes that top-down
        // 32-bit BI_RGB buffer; source and destination dimensions are positive and in range.
        let copied = unsafe {
            StretchDIBits(
                self.device.0,
                x,
                y,
                output_width,
                output_height,
                0,
                0,
                width,
                height,
                page.bgra().as_ptr().cast(),
                &raw const bitmap,
                DIB_RGB_COLORS,
                SRCCOPY,
            )
        };
        if copied == 0 || copied == GDI_ERROR {
            return Err(last_error("StretchDIBits"));
        }
        // SAFETY: StartPage succeeded for this exclusive active printer job; all raster input has
        // been consumed synchronously and no borrowed pointer remains.
        if unsafe { EndPage(self.device.0) } <= 0 {
            return Err(last_error("EndPage"));
        }
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<(), PrintError> {
        if !self.active {
            return Err(PrintError::InvalidState);
        }
        // SAFETY: This job exclusively owns an HDC with a successfully started document and no
        // page call in progress. EndDoc consumes the spool document state synchronously.
        if unsafe { EndDoc(self.device.0) } <= 0 {
            return Err(last_error("EndDoc"));
        }
        self.active = false;
        Ok(())
    }

    fn abort(&mut self) {
        if self.active {
            // SAFETY: This job exclusively owns the live HDC and StartDocW succeeded. Any failed or
            // cancelled page is terminated before PrinterDevice subsequently deletes the DC.
            let _ = unsafe { AbortDoc(self.device.0) };
            self.active = false;
        }
    }
}

impl Drop for PrinterJob {
    fn drop(&mut self) {
        self.abort();
    }
}

fn fitted_page(
    hdc: HDC,
    source_width: i32,
    source_height: i32,
) -> Result<(i32, i32, i32, i32), PrintError> {
    // SAFETY: Caller exclusively owns this live printer HDC. GetDeviceCaps only reads driver
    // metadata and writes through no pointers.
    let horizontal_resolution = i32::try_from(HORZRES).map_err(|_| PrintError::InvalidPage)?;
    let vertical_resolution = i32::try_from(VERTRES).map_err(|_| PrintError::InvalidPage)?;
    let destination_width = unsafe { GetDeviceCaps(hdc, horizontal_resolution) };
    // SAFETY: Same HDC and read-only device-capability query as above.
    let destination_height = unsafe { GetDeviceCaps(hdc, vertical_resolution) };
    if destination_width <= 0 || destination_height <= 0 {
        return Err(last_error("GetDeviceCaps"));
    }
    let (width, height) = fit_dimensions(
        source_width,
        source_height,
        destination_width,
        destination_height,
    )?;
    Ok((
        (destination_width - width) / 2,
        (destination_height - height) / 2,
        width,
        height,
    ))
}

fn fit_dimensions(
    source_width: i32,
    source_height: i32,
    destination_width: i32,
    destination_height: i32,
) -> Result<(i32, i32), PrintError> {
    let source_width_64 = i64::from(source_width);
    let source_height_64 = i64::from(source_height);
    let destination_width_64 = i64::from(destination_width);
    let destination_height_64 = i64::from(destination_height);
    let (width, height) =
        if destination_width_64 * source_height_64 <= destination_height_64 * source_width_64 {
            (
                destination_width_64,
                source_height_64 * destination_width_64 / source_width_64,
            )
        } else {
            (
                source_width_64 * destination_height_64 / source_height_64,
                destination_height_64,
            )
        };
    let width = i32::try_from(width).map_err(|_| PrintError::InvalidPage)?;
    let height = i32::try_from(height).map_err(|_| PrintError::InvalidPage)?;
    Ok((width, height))
}

fn last_error(operation: &'static str) -> PrintError {
    // SAFETY: GetLastError has no pointer or lifetime requirements and reads calling-thread state.
    let code = unsafe { GetLastError() };
    PrintError::Platform { operation, code }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::{dialog_initial_values, fit_dimensions};
    use crate::printing::{PrintDialogOptions, PrintOrientation};
    use barepdf_core::{PageCount, PageIndex};
    use barepdf_platform::printing::PrintRange;

    #[test]
    fn page_fit_preserves_aspect_ratio_in_both_driver_orientations() {
        assert_eq!(fit_dimensions(600, 800, 2400, 3000), Ok((2250, 3000)));
        assert_eq!(fit_dimensions(800, 600, 3000, 2400), Ok((3000, 2250)));
    }

    #[test]
    fn dialog_initial_values_select_preview_range_and_landscape() {
        let page_count = PageCount::new(10).expect("test page count");
        let range = PrintRange::new(PageIndex::from_raw(1), PageIndex::from_raw(6), page_count)
            .expect("test range");
        let values = dialog_initial_values(
            page_count.get(),
            PrintDialogOptions::new(range, PrintOrientation::Landscape),
        );

        assert_eq!((values.from_page, values.to_page), (2, 7));
        assert!(values.page_numbers);
        assert_eq!(values.orientation, Some(PrintOrientation::Landscape));
    }

    #[test]
    fn dialog_initial_values_keep_all_pages_and_driver_orientation_by_default() {
        let page_count = PageCount::new(10).expect("test page count");
        let values = dialog_initial_values(
            page_count.get(),
            PrintDialogOptions::new(PrintRange::all(page_count), PrintOrientation::Auto),
        );

        assert_eq!((values.from_page, values.to_page), (1, 10));
        assert!(!values.page_numbers);
        assert_eq!(values.orientation, None);
    }
}
