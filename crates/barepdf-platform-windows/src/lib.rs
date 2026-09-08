mod clipboard;
mod dialogs;
mod drop_target;
mod executable;
mod ffi;
mod image_encoder;
mod printing;
mod shell;

pub use clipboard::WindowsClipboard;
pub use dialogs::{ask_yes_no, show_fatal_error, WindowsFileDialogs};
pub use drop_target::install_file_drop;
pub use executable::{executable_file_version, is_installed_build, launch_installer};
pub use image_encoder::WindowsImageEncoder;
pub use printing::{WindowsPrinterDialog, WindowsPrinterSink};
pub use shell::open_url;

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

fn should_reduce_visual_effects(
    high_contrast_active: Option<bool>,
    animations_enabled: Option<bool>,
) -> bool {
    high_contrast_active != Some(false) || animations_enabled != Some(true)
}

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
