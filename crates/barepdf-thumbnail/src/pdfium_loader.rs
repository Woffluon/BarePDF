use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

/// Resolves path to pdfium.dll in the same directory as this DLL module.
pub fn get_installed_pdfium_path(hinstance: HMODULE) -> Option<PathBuf> {
    let mut buf = vec![0u16; 1024];
    // SAFETY: GetModuleFileNameW accepts buffer length in u32 and returns copied length.
    let len = unsafe { GetModuleFileNameW(hinstance, &mut buf) } as usize;
    if len == 0 || len >= buf.len() {
        return None;
    }
    let module_path = String::from_utf16_lossy(&buf[..len]);
    let path = Path::new(&module_path);
    let parent = path.parent()?;
    let pdfium_path = parent.join("pdfium.dll");
    if pdfium_path.is_file() {
        Some(pdfium_path)
    } else {
        None
    }
}

/// Binds PDFium library deterministically without search path vulnerabilities.
pub fn init_pdfium(hinstance: HMODULE) -> Result<Pdfium, String> {
    let bindings = if let Some(pdfium_path) = get_installed_pdfium_path(hinstance) {
        Pdfium::bind_to_library(&pdfium_path)
            .map_err(|e| format!("Failed to bind PDFium at {}: {e}", pdfium_path.display()))?
    } else {
        Pdfium::bind_to_system_library()
            .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(".")))
            .map_err(|e| format!("Failed to bind fallback PDFium: {e}"))?
    };

    Ok(Pdfium::new(bindings))
}
