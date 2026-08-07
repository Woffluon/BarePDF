use std::ffi::c_void;
use std::ptr::null_mut;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::{
    CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC,
};

/// Fits target dimensions within `cx * cx` box while preserving aspect ratio.
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
pub fn create_32bit_dib_section(width: u32, height: u32, rgba_pixels: &[u8]) -> Option<HBITMAP> {
    if width == 0 || height == 0 {
        return None;
    }

    let expected_len = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;

    if rgba_pixels.len() < expected_len {
        return None;
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // Top-down orientation
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            biSizeImage: expected_len as u32,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [Default::default()],
    };

    let mut bits_ptr: *mut c_void = null_mut();

    // SAFETY: CreateDIBSection takes BITMAPINFO pointer and returns handle and buffer pointer.
    let hbitmap = unsafe {
        CreateDIBSection(
            HDC::default(),
            &bmi as *const BITMAPINFO,
            DIB_RGB_COLORS,
            &mut bits_ptr as *mut *mut c_void,
            HANDLE::default(),
            0,
        )
    }
    .ok()?;

    if bits_ptr.is_null() {
        // SAFETY: Delete handle on failure.
        unsafe {
            let _ = DeleteObject(hbitmap);
        }
        return None;
    }

    // SAFETY: Copy RGBA to BGRA in DIB section memory buffer.
    unsafe {
        let dest = std::slice::from_raw_parts_mut(bits_ptr as *mut u8, expected_len);
        for i in 0..(width * height) as usize {
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

    Some(hbitmap)
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
