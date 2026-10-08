pub use barepdf_platform::printing::InstalledPrinter;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows_sys::Win32::Graphics::Printing::{
    EnumPrintersW, GetDefaultPrinterW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL,
    PRINTER_INFO_4W,
};

#[must_use]
pub fn enumerate_installed_printers() -> Vec<InstalledPrinter> {
    let default_name = get_default_printer_name();
    let mut printers = Vec::new();

    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let mut bytes_needed = 0u32;
    let mut count = 0u32;

    // First call to query required buffer size.
    // SAFETY: Querying size with null buffer is documented Win32 behavior.
    unsafe {
        EnumPrintersW(
            flags,
            std::ptr::null_mut(),
            4,
            std::ptr::null_mut(),
            0,
            &raw mut bytes_needed,
            &raw mut count,
        );
    }

    if bytes_needed == 0 {
        return printers;
    }

    let word_len = (bytes_needed as usize).div_ceil(std::mem::size_of::<usize>());
    let mut buffer = vec![0usize; word_len];
    let buffer_bytes = buffer.len() * std::mem::size_of::<usize>();

    // SAFETY: Buffer is appropriately sized according to `bytes_needed` and aligned to `usize`.
    let success = unsafe {
        EnumPrintersW(
            flags,
            std::ptr::null_mut(),
            4,
            buffer.as_mut_ptr().cast::<u8>(),
            bytes_needed,
            &raw mut bytes_needed,
            &raw mut count,
        )
    };

    if success == 0 || count == 0 {
        return printers;
    }

    let valid_count = (count as usize).min(buffer_bytes / std::mem::size_of::<PRINTER_INFO_4W>());
    let info_ptr = buffer.as_ptr().cast::<PRINTER_INFO_4W>();
    let buffer_start = buffer.as_ptr() as usize;
    let buffer_end = buffer_start.saturating_add(buffer_bytes);
    for i in 0..valid_count {
        // SAFETY: `info_ptr` is valid for `valid_count` items of PRINTER_INFO_4W.
        let info = unsafe { &*info_ptr.add(i) };
        if !info.pPrinterName.is_null() {
            // SAFETY: `wide_ptr_to_string` verifies that `info.pPrinterName` and its
            // null-terminated UTF-16 sequence lie within `[buffer_start, buffer_end)`.
            let name = unsafe { wide_ptr_to_string(info.pPrinterName, buffer_start, buffer_end) };
            if !name.is_empty() {
                let is_default = default_name.as_deref() == Some(&name);
                printers.push(InstalledPrinter { name, is_default });
            }
        }
    }

    printers
}

fn get_default_printer_name() -> Option<String> {
    let mut size = 0u32;
    // SAFETY: First call to determine buffer size (in characters).
    unsafe {
        GetDefaultPrinterW(std::ptr::null_mut(), &raw mut size);
    }
    if size == 0 {
        return None;
    }

    let mut buffer = vec![0u16; size as usize];
    // SAFETY: Buffer has `size` characters as requested.
    let success = unsafe { GetDefaultPrinterW(buffer.as_mut_ptr(), &raw mut size) };
    if success == 0 || size == 0 {
        return None;
    }

    let len = usize::try_from(size.saturating_sub(1)).ok()?;
    let slice = buffer.get(..len)?;
    Some(OsString::from_wide(slice).to_string_lossy().into_owned())
}

unsafe fn wide_ptr_to_string(ptr: *const u16, buffer_start: usize, buffer_end: usize) -> String {
    if ptr.is_null() || !ptr.is_aligned() {
        return String::new();
    }
    let addr = ptr as usize;
    if addr < buffer_start || addr >= buffer_end {
        return String::new();
    }
    let max_len = ((buffer_end - addr) / std::mem::size_of::<u16>()).min(1024);
    let mut len = 0;
    while len < max_len {
        // SAFETY: `ptr` is non-null, aligned, and `len < max_len` guarantees `ptr.add(len)`
        // stays within `[buffer_start, buffer_end)`.
        if unsafe { *ptr.add(len) } == 0 {
            // SAFETY: `ptr` is non-null, aligned, and verified readable for `len` contiguous `u16` units.
            let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
            return OsString::from_wide(slice).to_string_lossy().into_owned();
        }
        len += 1;
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerate_printers_does_not_panic() {
        let printers = enumerate_installed_printers();
        // Even on machines with no printer, it should return an empty or valid list without crashing.
        for p in &printers {
            assert!(!p.name.is_empty());
        }
    }

    #[test]
    fn installed_printer_properties_and_defaults_are_consistent() {
        let printers = enumerate_installed_printers();
        let default_count = printers.iter().filter(|p| p.is_default).count();
        // There can be at most one default printer in Windows.
        assert!(default_count <= 1);

        for p in &printers {
            assert!(!p.name.trim().is_empty());
            assert!(!p.name.contains('\0'));
        }

        let test_printer = InstalledPrinter {
            name: "Test Printer".to_string(),
            is_default: true,
        };
        assert_eq!(test_printer.clone(), test_printer);
        assert_eq!(test_printer.name, "Test Printer");
        assert!(test_printer.is_default);
    }

    #[test]
    fn wide_ptr_to_string_reads_valid_utf16_within_buffer_bounds() {
        let mut buffer: Vec<u16> = vec![0xFFFF, 0xFFFF];
        buffer.extend("BarePDF Printer".encode_utf16());
        buffer.push(0);
        buffer.extend([0xAAAA, 0xBBBB]);

        let buffer_start = buffer.as_ptr() as usize;
        let buffer_end = buffer_start + buffer.len() * std::mem::size_of::<u16>();
        // SAFETY: Offsetting by 2 elements stays within `buffer`.
        let name_ptr = unsafe { buffer.as_ptr().add(2) };
        // SAFETY: `name_ptr` points inside `[buffer_start, buffer_end)` to a null-terminated UTF-16 string.
        let result = unsafe { wide_ptr_to_string(name_ptr, buffer_start, buffer_end) };
        assert_eq!(result, "BarePDF Printer");
    }

    #[test]
    fn wide_ptr_to_string_rejects_null_and_unaligned_pointers() {
        let buffer = [0x41u8, 0x00, 0x42u8, 0x00, 0x00, 0x00];
        let buffer_start = buffer.as_ptr() as usize;
        let buffer_end = buffer_start + buffer.len();

        // SAFETY: `wide_ptr_to_string` explicitly checks for null before dereferencing.
        let null_res = unsafe { wide_ptr_to_string(std::ptr::null(), buffer_start, buffer_end) };
        assert_eq!(null_res, "");

        // SAFETY: Offsetting by 1 byte stays within `buffer`; pointer is not dereferenced when unaligned.
        let unaligned_ptr = unsafe { buffer.as_ptr().add(1).cast::<u16>() };
        assert!(!unaligned_ptr.is_aligned());
        // SAFETY: `wide_ptr_to_string` explicitly checks alignment before dereferencing.
        let unaligned_res = unsafe { wide_ptr_to_string(unaligned_ptr, buffer_start, buffer_end) };
        assert_eq!(unaligned_res, "");
    }

    #[test]
    fn wide_ptr_to_string_rejects_out_of_bounds_and_unterminated_pointers() {
        let buffer: Vec<u16> = "Printer\0".encode_utf16().collect();
        let buffer_start = buffer.as_ptr() as usize;
        let buffer_end = buffer_start + buffer.len() * std::mem::size_of::<u16>();

        // Pointer before buffer_start
        // SAFETY: `wide_ptr_to_string` rejects pointers below `buffer_start` before dereferencing.
        let before_start = unsafe {
            wide_ptr_to_string(
                buffer.as_ptr(),
                buffer_start + std::mem::size_of::<u16>(),
                buffer_end,
            )
        };
        assert_eq!(before_start, "");

        // Pointer at buffer_end (one past end)
        // SAFETY: `buffer.as_ptr().add(buffer.len())` is a valid one-past-end pointer and is not dereferenced.
        let one_past_end = unsafe { buffer.as_ptr().add(buffer.len()) };
        // SAFETY: `wide_ptr_to_string` rejects pointers `>= buffer_end` before dereferencing.
        let past_end_res = unsafe { wide_ptr_to_string(one_past_end, buffer_start, buffer_end) };
        assert_eq!(past_end_res, "");

        // Trailing 1-byte slice (insufficient for a single u16)
        // SAFETY: `wide_ptr_to_string` checks remaining byte length before dereferencing.
        let short_res =
            unsafe { wide_ptr_to_string(buffer.as_ptr(), buffer_start, buffer_start + 1) };
        assert_eq!(short_res, "");

        // Non-terminated buffer within bounds
        let non_terminated = [0x0041u16; 16];
        let nt_start = non_terminated.as_ptr() as usize;
        let nt_end = nt_start + non_terminated.len() * std::mem::size_of::<u16>();
        // SAFETY: `non_terminated` is valid for `[nt_start, nt_end)`; missing null terminator is rejected safely.
        let nt_res = unsafe { wide_ptr_to_string(non_terminated.as_ptr(), nt_start, nt_end) };
        assert_eq!(nt_res, "");

        // Non-terminated buffer exceeding 1024-char cap
        let oversized = vec![0x0041u16; 2048];
        let over_start = oversized.as_ptr() as usize;
        let over_end = over_start + oversized.len() * std::mem::size_of::<u16>();
        // SAFETY: `oversized` is valid for `[over_start, over_end)`; missing null terminator within 1024 chars is rejected.
        let over_res = unsafe { wide_ptr_to_string(oversized.as_ptr(), over_start, over_end) };
        assert_eq!(over_res, "");
    }

    #[test]
    fn buffer_allocation_and_valid_count_guard() {
        let bytes_needed = 100u32;
        let word_len = (bytes_needed as usize).div_ceil(std::mem::size_of::<usize>());
        let buffer = vec![0usize; word_len];
        assert_eq!(buffer.as_ptr() as usize % std::mem::align_of::<usize>(), 0);
        assert!(std::mem::align_of::<PRINTER_INFO_4W>() <= std::mem::align_of::<usize>());

        let buffer_bytes = buffer.len() * std::mem::size_of::<usize>();
        let reported_excessive_count = 10_000u32;
        let valid_count = (reported_excessive_count as usize)
            .min(buffer_bytes / std::mem::size_of::<PRINTER_INFO_4W>());
        assert!(valid_count <= buffer_bytes / std::mem::size_of::<PRINTER_INFO_4W>());
    }
}
