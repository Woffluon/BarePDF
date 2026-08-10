use std::ffi::c_void;
use std::ptr::null_mut;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::{
    CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC,
    RGBQUAD,
};

struct OwnedBitmap(HBITMAP);

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        // SAFETY: This type owns the HBITMAP returned by CreateDIBSection until into_raw().
        unsafe {
            let _ = DeleteObject(self.0);
        }
    }
}

impl OwnedBitmap {
    fn into_raw(self) -> HBITMAP {
        let bitmap = self.0;
        std::mem::forget(self);
        bitmap
    }
}

/// Fits target dimensions within `cx * cx` box while preserving aspect ratio.
#[must_use]
#[allow(clippy::cast_precision_loss)] // Thumbnail dimensions are bounded by the Shell request.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Positive finite values are clamped to 1..=cx.
pub fn calculate_thumbnail_dimensions(page_width: f32, page_height: f32, cx: u32) -> (u32, u32) {
    if page_width <= 0.0 || page_height <= 0.0 || cx == 0 {
        return (cx.max(1), cx.max(1));
    }

    let max_dim = cx as f32;
    let aspect_ratio = page_width / page_height;

    let (w, h) = if page_width >= page_height {
        let target_w = max_dim;
        let target_h = (max_dim / aspect_ratio).round();
        (target_w, target_h)
    } else {
        let target_h = max_dim;
        let target_w = (max_dim * aspect_ratio).round();
        (target_w, target_h)
    };

    let final_w = (w as u32).clamp(1, cx);
    let final_h = (h as u32).clamp(1, cx);
    (final_w, final_h)
}

/// Converts RGBA buffer to BGRA and creates a 32-bit top-down Win32 DIB Section HBITMAP.
#[must_use]
pub fn create_32bit_dib_section(width: u32, height: u32, rgba_pixels: &[u8]) -> Option<HBITMAP> {
    if width == 0 || height == 0 {
        return None;
    }

    let width_i32 = i32::try_from(width).ok()?;
    let height_i32 = i32::try_from(height).ok()?;
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let expected_len = pixel_count.checked_mul(4)?;
    if expected_len > isize::MAX as usize {
        return None;
    }
    let image_size = u32::try_from(expected_len).ok()?;

    if rgba_pixels.len() != expected_len {
        return None;
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).ok()?,
            biWidth: width_i32,
            biHeight: height_i32.checked_neg()?, // Top-down orientation
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            biSizeImage: image_size,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD::default()],
    };

    let mut bits_ptr: *mut c_void = null_mut();

    // SAFETY: CreateDIBSection takes BITMAPINFO pointer and returns handle and buffer pointer.
    let hbitmap = OwnedBitmap(
        unsafe {
            CreateDIBSection(
                HDC::default(),
                &raw const bmi,
                DIB_RGB_COLORS,
                &raw mut bits_ptr,
                HANDLE::default(),
                0,
            )
        }
        .ok()?,
    );

    if bits_ptr.is_null() {
        return None;
    }

    // SAFETY: Copy RGBA to BGRA in DIB section memory buffer.
    unsafe {
        let dest = std::slice::from_raw_parts_mut(bits_ptr.cast::<u8>(), expected_len);
        for i in 0..pixel_count {
            let src_idx = i * 4;
            let r = rgba_pixels[src_idx];
            let g = rgba_pixels[src_idx + 1];
            let b = rgba_pixels[src_idx + 2];
            let a = rgba_pixels[src_idx + 3];

            dest[src_idx] = b;
            dest[src_idx + 1] = g;
            dest[src_idx + 2] = r;
            dest[src_idx + 3] = a;
        }
    }

    Some(hbitmap.into_raw())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_thumbnail_dimensions_portrait() {
        let (w, h) = calculate_thumbnail_dimensions(612.0, 792.0, 256);
        assert_eq!(h, 256);
        assert_eq!(w, 198);
    }

    #[test]
    fn test_calculate_thumbnail_dimensions_landscape() {
        let (w, h) = calculate_thumbnail_dimensions(792.0, 612.0, 256);
        assert_eq!(w, 256);
        assert_eq!(h, 198);
    }

    #[test]
    fn test_calculate_thumbnail_dimensions_square() {
        let (w, h) = calculate_thumbnail_dimensions(500.0, 500.0, 256);
        assert_eq!(w, 256);
        assert_eq!(h, 256);
    }
}
