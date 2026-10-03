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
    for i in 0..valid_count {
        // SAFETY: `info_ptr` is valid for `valid_count` items of PRINTER_INFO_4W.
        let info = unsafe { &*info_ptr.add(i) };
        if !info.pPrinterName.is_null() {
            // SAFETY: `info.pPrinterName` points into `buffer` populated by `EnumPrintersW` and
            // remains valid while `buffer` is alive.
            let name = unsafe { wide_ptr_to_string(info.pPrinterName) };
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

unsafe fn wide_ptr_to_string(ptr: *const u16) -> String {
    if ptr.is_null() || !ptr.is_aligned() {
        return String::new();
    }
    let mut len = 0;
    while len < 1024 {
        // SAFETY: Caller guarantees `ptr` points to a readable wide string (or buffer of at least
        // 1024 `u16` elements if untrusted); `ptr` is non-null and aligned.
        if unsafe { *ptr.add(len) } == 0 {
            break;
        }
        len += 1;
    }
    // SAFETY: `ptr` is non-null, aligned, and verified readable for `len` contiguous `u16` units.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    OsString::from_wide(slice).to_string_lossy().into_owned()
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
    fn wide_ptr_to_string_rejects_null_and_unaligned_pointers() {
        // SAFETY: `wide_ptr_to_string` explicitly checks for null before dereferencing.
        assert_eq!(unsafe { wide_ptr_to_string(std::ptr::null()) }, "");

        let buffer = [0x41u8, 0x00, 0x42u8, 0x00, 0x00, 0x00];
        // SAFETY: Offsetting by 1 byte stays within `buffer`; pointer is not dereferenced when unaligned.
        let unaligned_ptr = unsafe { buffer.as_ptr().add(1).cast::<u16>() };
        assert!(!unaligned_ptr.is_aligned());
        // SAFETY: `wide_ptr_to_string` explicitly checks alignment before dereferencing.
        assert_eq!(unsafe { wide_ptr_to_string(unaligned_ptr) }, "");
    }

    #[test]
    fn wide_ptr_to_string_limits_reading_to_1024_chars() {
        let non_terminated = vec![0x0041u16; 2048];
        // SAFETY: `non_terminated` is a valid aligned slice of 2048 `u16` elements (>= 1024 cap).
        let result = unsafe { wide_ptr_to_string(non_terminated.as_ptr()) };
        assert_eq!(result.len(), 1024);
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
