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

    let mut buffer = vec![0u8; bytes_needed as usize];

    // SAFETY: Buffer is appropriately sized according to `bytes_needed`.
    let success = unsafe {
        EnumPrintersW(
            flags,
            std::ptr::null_mut(),
            4,
            buffer.as_mut_ptr(),
            bytes_needed,
            &raw mut bytes_needed,
            &raw mut count,
        )
    };

    if success == 0 || count == 0 {
        return printers;
    }

    let info_ptr = buffer.as_ptr().cast::<PRINTER_INFO_4W>();
    for i in 0..count {
        // SAFETY: `info_ptr` is valid for `count` items of PRINTER_INFO_4W.
        let info = unsafe { &*info_ptr.add(i as usize) };
        if !info.pPrinterName.is_null() {
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
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
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
}
