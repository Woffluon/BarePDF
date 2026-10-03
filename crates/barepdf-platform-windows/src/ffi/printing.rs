use crate::printing::{PrintDialogOptions, PrintOrientation};
use barepdf_platform::printing::{PrintDuplex, PrintError, PrintPage};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Foundation::{GetLastError, GlobalFree, HANDLE, HGLOBAL, HWND};
use windows_sys::Win32::Graphics::Gdi::{
    CreateDCW, DeleteDC, GetDeviceCaps, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DEVMODEW, DIB_RGB_COLORS, DMORIENT_LANDSCAPE, DMORIENT_PORTRAIT, DM_ORIENTATION, GDI_ERROR,
    HDC, HORZRES, RGBQUAD, SRCCOPY, VERTRES,
};
use windows_sys::Win32::Graphics::Printing::{ClosePrinter, DocumentPropertiesW, OpenPrinterW};
use windows_sys::Win32::Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, PrintDlgW, PD_ALLPAGES, PD_NOSELECTION, PD_PAGENUMS, PD_RETURNDC,
    PD_RETURNDEFAULT, PRINTDLGW,
};

const DM_COPIES: u32 = 0x0000_0100;
const DM_DUPLEX: u32 = 0x0000_1000;
const DMDUP_SIMPLEX: i16 = 1;
const DMDUP_VERTICAL: i16 = 2;
const DMDUP_HORIZONTAL: i16 = 3;
const DM_IN_BUFFER: u32 = 8;
const DM_OUT_BUFFER: u32 = 2;

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
    duplex: Option<PrintDuplex>,
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
    let duplex = match options.duplex() {
        PrintDuplex::OneSided => None,
        duplex => Some(duplex),
    };
    DialogInitialValues {
        from_page,
        to_page,
        page_numbers: from_page != 1 || to_page != maximum,
        orientation,
        duplex,
    }
}

struct SafeGlobalHandle(HGLOBAL);

impl SafeGlobalHandle {
    const fn new(handle: HGLOBAL) -> Self {
        Self(handle)
    }

    fn get(&self) -> HGLOBAL {
        self.0
    }

    fn take(&mut self) -> HGLOBAL {
        let handle = self.0;
        self.0 = std::ptr::null_mut();
        handle
    }

    fn free(&mut self) {
        if !self.0.is_null() {
            // SAFETY: PrintDlgW/GlobalAlloc returned this movable global-memory handle.
            // Resetting pointer to null guarantees at-most-once deallocation.
            let _ = unsafe { GlobalFree(self.0) };
            self.0 = std::ptr::null_mut();
        }
    }
}

impl Drop for SafeGlobalHandle {
    fn drop(&mut self) {
        self.free();
    }
}

struct DialogAllocations {
    dev_mode: SafeGlobalHandle,
    dev_names: SafeGlobalHandle,
}

impl DialogAllocations {
    const fn new() -> Self {
        Self {
            dev_mode: SafeGlobalHandle::new(std::ptr::null_mut()),
            dev_names: SafeGlobalHandle::new(std::ptr::null_mut()),
        }
    }

    fn set(&mut self, dev_mode: HGLOBAL, dev_names: HGLOBAL) {
        // Prevent double free if both point to the same non-null handle
        if !dev_names.is_null() && dev_names == dev_mode {
            self.set_distinct(dev_mode, std::ptr::null_mut());
        } else {
            self.set_distinct(dev_mode, dev_names);
        }
    }

    fn set_distinct(&mut self, dev_mode: HGLOBAL, dev_names: HGLOBAL) {
        if self.dev_mode.get() != dev_mode {
            self.dev_mode.free();
            self.dev_mode = SafeGlobalHandle::new(dev_mode);
        }
        if self.dev_names.get() != dev_names {
            self.dev_names.free();
            self.dev_names = SafeGlobalHandle::new(dev_names);
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
    // SAFETY: `IsWindow` safely inspects a handle value without dereferencing it in Rust.
    let is_valid_window = !owner.is_null()
        && unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(owner) } != 0;
    let owner = if is_valid_window {
        owner
    } else {
        std::ptr::null_mut()
    };
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

    let mut allocations = DialogAllocations::new();

    if initial.orientation.is_some() || initial.duplex.is_some() {
        let mut defaults = dialog;
        defaults.Flags = PD_RETURNDEFAULT;
        defaults.hwndOwner = std::ptr::null_mut();
        // SAFETY: The structure is initialized and both required input handles are null. This call
        // does not show UI and returns movable global-memory handles for the default printer.
        if unsafe { PrintDlgW(&raw mut defaults) } != 0 {
            allocations.set(defaults.hDevMode, defaults.hDevNames);
            dialog.hDevMode = allocations.dev_mode.get();
            dialog.hDevNames = allocations.dev_names.get();
            if let Some(orientation) = initial.orientation {
                apply_orientation(dialog.hDevMode, orientation);
            }
            if let Some(duplex) = initial.duplex {
                apply_duplex(dialog.hDevMode, duplex);
            }
        } else {
            // A missing default printer is not treated as cancellation here; the visible dialog may
            // still let the user choose a printer. A real common-dialog error still fails closed.
            // SAFETY: Called immediately after a failed `PrintDlgW` on the same thread.
            let code = unsafe { CommDlgExtendedError() };
            if code != 0 {
                return Err(PrintError::Dialog(code));
            }
        }
    }

    // Pass any default handles into dialog. Detach from allocations so PrintDlgW has exclusive
    // control to reallocate/free if the user picks a different printer.
    dialog.hDevMode = allocations.dev_mode.take();
    dialog.hDevNames = allocations.dev_names.take();

    // SAFETY: `dialog` has the documented size and initialized scalar fields; optional handles and
    // callback/template pointers are null. The owner HWND may be null or a live UI-owned window.
    let accepted = unsafe { PrintDlgW(&raw mut dialog) };
    allocations.set(dialog.hDevMode, dialog.hDevNames);

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
    let result = dialog_printer_from_dialog(&dialog);
    drop(allocations);
    Ok(Some(result))
}

fn dialog_printer_from_dialog(dialog: &PRINTDLGW) -> DialogPrinter {
    DialogPrinter {
        device: PrinterDevice(dialog.hDC),
        from_page: dialog.nFromPage,
        to_page: dialog.nToPage,
        copies: dialog.nCopies,
        page_numbers: dialog.Flags & PD_PAGENUMS != 0,
    }
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
    // SAFETY: `pointer` is non-null, locked from `PrintDlgW`'s `DEVMODEW` allocation, and unlocked
    // immediately after updating the orientation fields.
    unsafe {
        (*pointer).dmFields |= DM_ORIENTATION;
        (*pointer).Anonymous1.Anonymous1.dmOrientation = orientation;
        let _ = GlobalUnlock(dev_mode);
    }
}

fn apply_duplex(dev_mode: HGLOBAL, duplex: PrintDuplex) {
    if dev_mode.is_null() {
        return;
    }
    let duplex_val = match duplex {
        PrintDuplex::OneSided => DMDUP_SIMPLEX,
        PrintDuplex::TwoSidedLongEdge => DMDUP_VERTICAL,
        PrintDuplex::TwoSidedShortEdge => DMDUP_HORIZONTAL,
    };
    // SAFETY: PrintDlgW returned a movable global-memory handle containing DEVMODEW. The pointer is
    // used only while locked and no other thread can access this private dialog setup structure.
    let pointer = unsafe { GlobalLock(dev_mode) }.cast::<DEVMODEW>();
    if pointer.is_null() {
        return;
    }
    // SAFETY: `pointer` is non-null, locked from `PrintDlgW`'s `DEVMODEW` allocation, and unlocked
    // immediately after updating the duplex fields.
    unsafe {
        (*pointer).dmFields |= DM_DUPLEX;
        (*pointer).dmDuplex = duplex_val;
        let _ = GlobalUnlock(dev_mode);
    }
}

#[inline]
pub(crate) fn is_valid_devmode_size(size: i32) -> bool {
    size > 0 && (size as usize) >= std::mem::size_of::<DEVMODEW>()
}

pub(crate) fn create_direct_printer_device(
    printer_name: &str,
    orientation: PrintOrientation,
    duplex: PrintDuplex,
    copies: u16,
) -> Result<PrinterDevice, PrintError> {
    if printer_name.trim().is_empty() {
        return Err(PrintError::Platform {
            operation: "create_direct_printer_device empty name",
            code: 0,
        });
    }

    let wide_name = wide_null(OsStr::new(printer_name));
    let mut hprinter: HANDLE = std::ptr::null_mut();

    // SAFETY: OpenPrinterW initializes hprinter handle on success.
    let opened = unsafe {
        OpenPrinterW(
            wide_name.as_ptr() as *mut u16,
            &raw mut hprinter,
            std::ptr::null(),
        )
    };

    let devmode_buf = if opened != 0 && !hprinter.is_null() {
        struct PrinterGuard(HANDLE);
        impl Drop for PrinterGuard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    // SAFETY: ClosePrinter is called once on a valid open printer handle.
                    unsafe { ClosePrinter(self.0) };
                }
            }
        }
        let _guard = PrinterGuard(hprinter);

        // Query required DEVMODE buffer size
        // SAFETY: Passing NULL for output DEVMODE returns required buffer size in bytes.
        let devmode_size = unsafe {
            DocumentPropertiesW(
                std::ptr::null_mut(),
                hprinter,
                wide_name.as_ptr() as *mut u16,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };

        if is_valid_devmode_size(devmode_size) {
            let u64_len = (devmode_size as usize).div_ceil(std::mem::size_of::<u64>());
            let mut in_buf = vec![0u64; u64_len];

            // Retrieve current devmode
            // SAFETY: Buffer is allocated with `devmode_size` bytes and 8-byte aligned.
            let get_res = unsafe {
                DocumentPropertiesW(
                    std::ptr::null_mut(),
                    hprinter,
                    wide_name.as_ptr() as *mut u16,
                    in_buf.as_mut_ptr().cast(),
                    std::ptr::null_mut(),
                    DM_OUT_BUFFER,
                )
            };

            if get_res >= 0 {
                let devmode = in_buf.as_mut_ptr().cast::<DEVMODEW>();
                if copies > 0 {
                    // SAFETY: Pointer is within allocated devmode buffer.
                    unsafe {
                        (*devmode).dmFields |= DM_COPIES;
                        (*devmode).Anonymous1.Anonymous1.dmCopies = copies as i16;
                    }
                }
                match orientation {
                    PrintOrientation::Portrait => {
                        // SAFETY: Pointer is within allocated devmode buffer.
                        unsafe {
                            (*devmode).dmFields |= DM_ORIENTATION;
                            (*devmode).Anonymous1.Anonymous1.dmOrientation =
                                DMORIENT_PORTRAIT as i16;
                        }
                    }
                    PrintOrientation::Landscape => {
                        // SAFETY: Pointer is within allocated devmode buffer.
                        unsafe {
                            (*devmode).dmFields |= DM_ORIENTATION;
                            (*devmode).Anonymous1.Anonymous1.dmOrientation =
                                DMORIENT_LANDSCAPE as i16;
                        }
                    }
                    PrintOrientation::Auto => {}
                }
                let duplex_val = match duplex {
                    PrintDuplex::OneSided => DMDUP_SIMPLEX,
                    PrintDuplex::TwoSidedLongEdge => DMDUP_VERTICAL,
                    PrintDuplex::TwoSidedShortEdge => DMDUP_HORIZONTAL,
                };
                // SAFETY: Pointer is within allocated devmode buffer.
                unsafe {
                    (*devmode).dmFields |= DM_DUPLEX;
                    (*devmode).dmDuplex = duplex_val;
                }

                // Merge and validate with driver (without displaying any dialog)
                let mut out_buf = vec![0u64; u64_len];
                // SAFETY: in_buf and out_buf are distinct non-overlapping 8-byte aligned buffers.
                let merge_res = unsafe {
                    DocumentPropertiesW(
                        std::ptr::null_mut(),
                        hprinter,
                        wide_name.as_ptr() as *mut u16,
                        out_buf.as_mut_ptr().cast(),
                        in_buf.as_ptr().cast(),
                        DM_IN_BUFFER | DM_OUT_BUFFER,
                    )
                };
                if merge_res >= 0 {
                    Some(out_buf)
                } else {
                    Some(in_buf)
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let pdm = devmode_buf
        .as_ref()
        .map_or(std::ptr::null(), |b| b.as_ptr().cast());

    // SAFETY: CreateDCW creates a device context for the named printer.
    let mut hdc = unsafe { CreateDCW(std::ptr::null(), wide_name.as_ptr(), std::ptr::null(), pdm) };

    if hdc.is_null() {
        let winspool = wide_null(OsStr::new("WINSPOOL"));
        // SAFETY: Fallback with explicit WINSPOOL driver name.
        hdc = unsafe { CreateDCW(winspool.as_ptr(), wide_name.as_ptr(), std::ptr::null(), pdm) };
    }

    if hdc.is_null() {
        return Err(last_error("CreateDCW"));
    }

    Ok(PrinterDevice(hdc))
}

pub(crate) struct PrinterDevice(HDC);

// SAFETY: `PrinterDevice` owns a Win32 GDI printer device context (`HDC`) created via `PrintDlgW`
// (`PD_RETURNDC`) or `CreateDCW` and never exposes the raw `HDC` outside this module.
// GDI `HDC`s are not generally free-threaded: concurrent GDI operations on the same `HDC` from
// multiple threads are undefined behavior, and while an `HDC` is actively selected into or executing
// a GDI call on one thread it cannot be used on another. However, Win32 GDI allows a printer `HDC`
// to be transferred across thread boundaries provided:
// 1. The creating thread performs no further GDI operations on the `HDC` after creation and retains
//    no alias to it.
// 2. Ownership transfer is exclusive (`Send`, not `Sync`), and all subsequent `StartDocW`,
//    `StartPage`, `StretchDIBits`, `EndPage`, `EndDoc`, `AbortDoc`, and `DeleteDC` calls are
//    strictly serialized through unique ownership (`self` or `&mut self`).
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
        // SAFETY: This wrapper exclusively owns the non-null printer HDC returned by PrintDlgW or
        // CreateDCW and calls DeleteDC exactly once after any active print document has ended or
        // been aborted.
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
            let err = last_error("StartPage");
            self.abort();
            return Err(err);
        }

        let (x, y, output_width, output_height) = match fitted_page(self.device.0, width, height) {
            Ok(dims) => dims,
            Err(err) => {
                self.abort();
                return Err(err);
            }
        };
        let bitmap = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).map_err(|_| {
                    self.abort();
                    PrintError::InvalidPage
                })?,
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
        if copied <= 0 || copied == GDI_ERROR {
            let err = last_error("StretchDIBits");
            self.abort();
            return Err(err);
        }
        // SAFETY: StartPage and StretchDIBits succeeded for this exclusive active printer job; all
        // raster input has been consumed synchronously and no borrowed pointer remains.
        if unsafe { EndPage(self.device.0) } <= 0 {
            let err = last_error("EndPage");
            self.abort();
            return Err(err);
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
            let err = last_error("EndDoc");
            self.abort();
            return Err(err);
        }
        self.active = false;
        Ok(())
    }

    fn abort(&mut self) {
        if self.active {
            // SAFETY: This job exclusively owns the live HDC and StartDocW succeeded. Any failed or
            // cancelled page is terminated via AbortDoc (without calling EndPage on a broken page)
            // before PrinterDevice subsequently deletes the DC.
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
    let horizontal_resolution = i32::try_from(HORZRES).map_err(|_| PrintError::InvalidPage)?;
    let vertical_resolution = i32::try_from(VERTRES).map_err(|_| PrintError::InvalidPage)?;
    // SAFETY: Caller exclusively owns this live printer HDC. GetDeviceCaps only reads driver
    // metadata and writes through no pointers.
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
    use crate::printing::{PrintDialogOptions, PrintDuplex, PrintOrientation};
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
            PrintDialogOptions::new(
                range,
                PrintOrientation::Landscape,
                PrintDuplex::TwoSidedLongEdge,
            ),
        );

        assert_eq!((values.from_page, values.to_page), (2, 7));
        assert!(values.page_numbers);
        assert_eq!(values.orientation, Some(PrintOrientation::Landscape));
        assert_eq!(values.duplex, Some(PrintDuplex::TwoSidedLongEdge));
    }

    #[test]
    fn dialog_initial_values_keep_all_pages_and_driver_orientation_by_default() {
        let page_count = PageCount::new(10).expect("test page count");
        let values = dialog_initial_values(
            page_count.get(),
            PrintDialogOptions::new(
                PrintRange::all(page_count),
                PrintOrientation::Auto,
                PrintDuplex::OneSided,
            ),
        );

        assert_eq!((values.from_page, values.to_page), (1, 10));
        assert!(!values.page_numbers);
        assert_eq!(values.orientation, None);
        assert_eq!(values.duplex, None);
    }

    #[test]
    fn dialog_initial_values_select_short_edge_duplex() {
        let page_count = PageCount::new(5).expect("test page count");
        let values = dialog_initial_values(
            page_count.get(),
            PrintDialogOptions::new(
                PrintRange::all(page_count),
                PrintOrientation::Portrait,
                PrintDuplex::TwoSidedShortEdge,
            ),
        );

        assert_eq!(values.orientation, Some(PrintOrientation::Portrait));
        assert_eq!(values.duplex, Some(PrintDuplex::TwoSidedShortEdge));
    }

    #[test]
    fn devmode_size_validation_enforces_minimum_struct_size() {
        use super::is_valid_devmode_size;
        use windows_sys::Win32::Graphics::Gdi::DEVMODEW;

        assert!(!is_valid_devmode_size(-1));
        assert!(!is_valid_devmode_size(0));
        assert!(!is_valid_devmode_size(
            (std::mem::size_of::<DEVMODEW>() - 1) as i32
        ));
        assert!(is_valid_devmode_size(std::mem::size_of::<DEVMODEW>() as i32));
        assert!(is_valid_devmode_size(
            (std::mem::size_of::<DEVMODEW>() + 128) as i32
        ));
    }

    #[test]
    fn devmode_buffers_are_eight_byte_aligned_and_distinct() {
        use windows_sys::Win32::Graphics::Gdi::DEVMODEW;

        let devmode_size = (std::mem::size_of::<DEVMODEW>() + 64) as i32;
        let u64_len = (devmode_size as usize).div_ceil(std::mem::size_of::<u64>());
        let in_buf = vec![0u64; u64_len];
        let out_buf = vec![0u64; u64_len];

        assert_eq!(in_buf.as_ptr() as usize % 8, 0);
        assert_eq!(out_buf.as_ptr() as usize % 8, 0);
        assert!(std::mem::align_of::<DEVMODEW>() <= 8);

        // Buffers must be completely non-overlapping
        let in_start = in_buf.as_ptr() as usize;
        let in_end = in_start + in_buf.len() * 8;
        let out_start = out_buf.as_ptr() as usize;
        let out_end = out_start + out_buf.len() * 8;

        assert!(in_end <= out_start || out_end <= in_start);
    }

    #[test]
    fn dialog_allocations_handles_null_and_avoids_double_free() {
        use super::{DialogAllocations, SafeGlobalHandle};
        use windows_sys::Win32::System::Memory::{GlobalAlloc, GMEM_FIXED};

        // Null handle drop is safe
        let handle = SafeGlobalHandle::new(std::ptr::null_mut());
        drop(handle);

        // Real memory allocation through SafeGlobalHandle
        // SAFETY: `GlobalAlloc` with `GMEM_FIXED` allocates 32 bytes from the process heap.
        let mem = unsafe { GlobalAlloc(GMEM_FIXED, 32) };
        assert!(!mem.is_null());
        let handle = SafeGlobalHandle::new(mem);
        drop(handle);

        // DialogAllocations with identical handles avoids double free
        // SAFETY: `GlobalAlloc` with `GMEM_FIXED` allocates 32 bytes from the process heap.
        let mem2 = unsafe { GlobalAlloc(GMEM_FIXED, 32) };
        assert!(!mem2.is_null());
        let mut allocs = DialogAllocations::new();
        allocs.set(mem2, mem2); // Both point to the same memory
        drop(allocs); // Must not double-free
    }
}
