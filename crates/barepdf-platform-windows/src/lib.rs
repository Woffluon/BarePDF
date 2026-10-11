#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod dialogs;
#[cfg(windows)]
mod drop_target;
#[cfg(windows)]
mod executable;
#[cfg(windows)]
mod ffi;
#[cfg(windows)]
mod image_encoder;
#[cfg(windows)]
mod printers;
#[cfg(windows)]
mod printing;
#[cfg(windows)]
mod shell;
#[cfg(windows)]
mod watcher;
#[cfg(windows)]
mod window_chrome;

#[cfg(windows)]
pub use clipboard::WindowsClipboard;
#[cfg(windows)]
pub use dialogs::{ask_yes_no, show_fatal_error, WindowsFileDialogs};
#[cfg(windows)]
pub use drop_target::install_file_drop;
#[cfg(windows)]
pub use executable::{executable_file_version, is_installed_build, launch_installer};
#[cfg(windows)]
pub use image_encoder::{decode_image_rgba, WindowsImageEncoder};
#[cfg(windows)]
pub use printers::{enumerate_installed_printers, InstalledPrinter};
#[cfg(windows)]
pub use printing::{PrintOrientation, WindowsPrinterDialog, WindowsPrinterSink};
#[cfg(windows)]
pub use shell::open_url;
#[cfg(windows)]
pub use watcher::{DocumentFileWatcher, FileChangeWatcher, WindowsDocumentFileWatcher};
#[cfg(windows)]
pub use window_chrome::{
    apply_frameless_style, begin_native_drag, from_resize_edge, hit_test_code, is_window_maximized,
    send_window_command, WindowCommand, WindowHit,
};

#[cfg(windows)]
#[must_use]
pub fn reduce_visual_effects() -> bool {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SPI_GETHIGHCONTRAST,
    };

    let mut high_contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        dwFlags: 0,
        lpszDefaultScheme: std::ptr::null_mut(),
    };
    let mut client_area_animations = 0;
    let high_contrast_active = unsafe {
        // SAFETY: Windows writes a HIGHCONTRASTW structure to the valid mutable pointer.
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            high_contrast.cbSize,
            (&mut high_contrast as *mut HIGHCONTRASTW).cast(),
            0,
        ) != 0
    }
    .then_some(high_contrast.dwFlags & HCF_HIGHCONTRASTON != 0);
    let animations_enabled = unsafe {
        // SAFETY: Windows writes a BOOL to the valid mutable pointer.
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut client_area_animations as *mut i32).cast(),
            0,
        ) != 0
    }
    .then_some(client_area_animations != 0);

    should_reduce_visual_effects(high_contrast_active, animations_enabled)
}

#[cfg(windows)]
fn should_reduce_visual_effects(
    high_contrast_active: Option<bool>,
    animations_enabled: Option<bool>,
) -> bool {
    high_contrast_active != Some(false) || animations_enabled != Some(true)
}

#[cfg(not(windows))]
mod non_windows;
#[cfg(not(windows))]
pub use non_windows::*;

#[cfg(test)]
mod tests {
    use super::should_reduce_visual_effects;

    #[test]
    fn visual_effects_are_reduced_when_accessibility_data_requires_or_cannot_confirm_them() {
        assert!(should_reduce_visual_effects(Some(true), Some(true)));
        assert!(should_reduce_visual_effects(Some(false), Some(false)));
        assert!(should_reduce_visual_effects(None, Some(true)));
        assert!(should_reduce_visual_effects(Some(false), None));
        assert!(!should_reduce_visual_effects(Some(false), Some(true)));
    }
}
