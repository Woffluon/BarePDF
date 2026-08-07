use crate::bitmap::{calculate_thumbnail_dimensions, create_32bit_dib_section};
use crate::pdfium_loader::init_pdfium;
use pdfium_render::prelude::*;
use std::cell::RefCell;
use std::sync::Mutex;
use windows::core::{implement, Error, Result};
use windows::Win32::Foundation::{E_FAIL, E_INVALIDARG, E_POINTER, E_UNEXPECTED, HMODULE};
use windows::Win32::Graphics::Gdi::HBITMAP;
use windows::Win32::System::Com::IStream;
use windows::Win32::UI::Shell::PropertiesSystem::*;
use windows::Win32::UI::Shell::*;

/// Max allowed PDF size for stream buffering in thumbnail handler (100 MB).
const MAX_THUMBNAIL_PDF_BYTES: usize = 100 * 1024 * 1024;

/// Windows Shell Thumbnail Provider implementation for BarePDF.
#[implement(IInitializeWithStream, IThumbnailProvider)]
pub struct BarePdfThumbnailProvider {
    hinstance: HMODULE,
    pdf_bytes: Mutex<RefCell<Option<Vec<u8>>>>,
}

impl BarePdfThumbnailProvider {
    pub fn new(hinstance: HMODULE) -> Self {
        Self {
            hinstance,
            pdf_bytes: Mutex::new(RefCell::new(None)),
        }
    }
}

impl IInitializeWithStream_Impl for BarePdfThumbnailProvider_Impl {
    fn Initialize(&self, pstream: Option<&IStream>, _grfmode: u32) -> Result<()> {
        let res = std::panic::catch_unwind(|| {
            let stream = match pstream {
                Some(s) => s,
                None => return Err(Error::from(E_INVALIDARG)),
            };

            let mut buffer = Vec::new();
            let mut chunk = vec![0u8; 64 * 1024];

            loop {
                let mut read_bytes = 0u32;
                // SAFETY: Direct call to IStream::Read with stack buffer.
                let hr = unsafe {
                    stream.Read(
                        chunk.as_mut_ptr() as *mut _,
                        chunk.len() as u32,
                        Some(&mut read_bytes),
                    )
                };

                if hr.is_err() || read_bytes == 0 {
                    break;
                }

                buffer.extend_from_slice(&chunk[..read_bytes as usize]);
                if buffer.len() > MAX_THUMBNAIL_PDF_BYTES {
                    return Err(Error::from(E_FAIL));
                }
            }

            if buffer.is_empty() {
                return Err(Error::from(E_FAIL));
            }

            if let Ok(guard) = self.pdf_bytes.lock() {
                *guard.borrow_mut() = Some(buffer);
                Ok(())
            } else {
                Err(Error::from(E_UNEXPECTED))
            }
        });

        res.unwrap_or_else(|_| Err(Error::from(E_UNEXPECTED)))
    }
}

impl IThumbnailProvider_Impl for BarePdfThumbnailProvider_Impl {
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn GetThumbnail(
        &self,
        cx: u32,
        phbmp: *mut HBITMAP,
        pdwalpha: *mut WTS_ALPHATYPE,
    ) -> Result<()> {
        let res = std::panic::catch_unwind(|| {
            if phbmp.is_null() || pdwalpha.is_null() || cx == 0 {
                return Err(Error::from(E_POINTER));
            }

            let pdf_data = {
                let guard = self
                    .pdf_bytes
                    .lock()
                    .map_err(|_| Error::from(E_UNEXPECTED))?;
                let cell = guard.borrow();
                match cell.as_ref() {
                    Some(data) => data.clone(),
                    None => return Err(Error::from(E_FAIL)),
                }
            };

            let pdfium = init_pdfium(self.hinstance).map_err(|_| Error::from(E_FAIL))?;

            let doc = pdfium
                .load_pdf_from_byte_slice(&pdf_data, None)
                .map_err(|_| Error::from(E_FAIL))?;

            let pages = doc.pages();
            if pages.is_empty() {
                return Err(Error::from(E_FAIL));
            }

            let first_page = pages.get(0).map_err(|_| Error::from(E_FAIL))?;

            let page_w = first_page.width().value;
            let page_h = first_page.height().value;

            let (target_w, target_h) = calculate_thumbnail_dimensions(page_w, page_h, cx);

            let render_config = PdfRenderConfig::new()
                .set_target_width(target_w as i32)
                .set_target_height(target_h as i32);

            let bitmap = first_page
                .render_with_config(&render_config)
                .map_err(|_| Error::from(E_FAIL))?;

            let image = bitmap.as_image().map_err(|_| Error::from(E_FAIL))?;

            let rgba = image.to_rgba8();
            let (real_w, real_h) = rgba.dimensions();

            let hbitmap = create_32bit_dib_section(real_w, real_h, rgba.as_raw())
                .ok_or_else(|| Error::from(E_FAIL))?;

            // SAFETY: Dereferencing valid non-null out pointers verified above.
            unsafe {
                *phbmp = hbitmap;
                *pdwalpha = WTSAT_ARGB;
            }

            Ok(())
        });

        res.unwrap_or_else(|_| Err(Error::from(E_UNEXPECTED)))
    }
}
