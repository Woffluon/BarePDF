use crate::bitmap::{calculate_thumbnail_dimensions, create_32bit_dib_section};
use crate::pdfium_loader::{init_pdfium, PdfiumLoadError};
use crate::{add_active_object, remove_active_object};
use pdfium_render::prelude::*;
use std::error::Error as StdError;
use std::fmt;
use std::sync::{Arc, Mutex};
use windows::core::{implement, Error, Result, HRESULT};
use windows::Win32::Foundation::{
    E_FAIL, E_INVALIDARG, E_OUTOFMEMORY, E_POINTER, E_UNEXPECTED, HMODULE,
};
use windows::Win32::Graphics::Gdi::HBITMAP;
use windows::Win32::System::Com::IStream;
use windows::Win32::UI::Shell::PropertiesSystem::{
    IInitializeWithStream, IInitializeWithStream_Impl,
};
use windows::Win32::UI::Shell::{
    IThumbnailProvider, IThumbnailProvider_Impl, WTSAT_ARGB, WTSAT_UNKNOWN, WTS_ALPHATYPE,
};

/// Max allowed PDF size for stream buffering in thumbnail handler (100 MiB).
const MAX_THUMBNAIL_PDF_BYTES: usize = 100 * 1024 * 1024;
const STREAM_CHUNK_BYTES: usize = 64 * 1024;
const MAX_THUMBNAIL_DIMENSION: u32 = 1024;

type ThumbnailResult<T> = std::result::Result<T, ThumbnailError>;

#[derive(Debug)]
enum ThumbnailError {
    InvalidArgument,
    StreamRead(HRESULT),
    InvalidStreamReadLength,
    StreamTooLarge,
    Allocation,
    EmptyStream,
    ProviderStatePoisoned,
    NotInitialized,
    PdfiumInitialization(PdfiumLoadError),
    DocumentLoad(PdfiumError),
    EmptyDocument,
    FirstPage(PdfiumError),
    InvalidPageDimensions,
    InvalidRenderDimensions,
    Render(PdfiumError),
    InvalidBitmapDimensions,
    BitmapCreation,
}

impl fmt::Display for ThumbnailError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let category = match self {
            Self::InvalidArgument => "invalid thumbnail argument",
            Self::StreamRead(_) => "thumbnail stream read failed",
            Self::InvalidStreamReadLength => "thumbnail stream reported an invalid read length",
            Self::StreamTooLarge => "thumbnail stream exceeds its byte limit",
            Self::Allocation => "thumbnail allocation failed",
            Self::EmptyStream => "thumbnail stream is empty",
            Self::ProviderStatePoisoned => "thumbnail provider state is poisoned",
            Self::NotInitialized => "thumbnail provider is not initialized",
            Self::PdfiumInitialization(_) => "PDFium initialization failed",
            Self::DocumentLoad(_) => "PDF document load failed",
            Self::EmptyDocument => "PDF document has no pages",
            Self::FirstPage(_) => "PDF first page access failed",
            Self::InvalidPageDimensions => "PDF page dimensions are invalid",
            Self::InvalidRenderDimensions => "thumbnail render dimensions are invalid",
            Self::Render(_) => "PDF thumbnail render failed",
            Self::InvalidBitmapDimensions => "rendered bitmap dimensions are invalid",
            Self::BitmapCreation => "thumbnail bitmap creation failed",
        };
        formatter.write_str(category)
    }
}

impl StdError for ThumbnailError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::PdfiumInitialization(source) => Some(source),
            Self::DocumentLoad(source) | Self::FirstPage(source) | Self::Render(source) => {
                Some(source)
            }
            _ => None,
        }
    }
}

impl ThumbnailError {
    fn into_com_error(self) -> Error {
        match self {
            Self::InvalidArgument => Error::from(E_INVALIDARG),
            Self::StreamRead(hresult) => Error::from(hresult),
            Self::InvalidStreamReadLength
            | Self::ProviderStatePoisoned
            | Self::InvalidRenderDimensions => Error::from(E_UNEXPECTED),
            Self::StreamTooLarge | Self::Allocation => Error::from(E_OUTOFMEMORY),
            Self::EmptyStream
            | Self::NotInitialized
            | Self::PdfiumInitialization(_)
            | Self::DocumentLoad(_)
            | Self::EmptyDocument
            | Self::FirstPage(_)
            | Self::InvalidPageDimensions
            | Self::Render(_)
            | Self::InvalidBitmapDimensions
            | Self::BitmapCreation => Error::from(E_FAIL),
        }
    }
}

/// Windows Shell Thumbnail Provider implementation for `BarePDF`.
#[implement(IInitializeWithStream, IThumbnailProvider)]
pub struct BarePdfThumbnailProvider {
    hinstance: HMODULE,
    pdf_bytes: Mutex<Option<Arc<[u8]>>>,
}

impl BarePdfThumbnailProvider {
    #[must_use]
    pub fn new(hinstance: HMODULE) -> Self {
        add_active_object();
        Self {
            hinstance,
            pdf_bytes: Mutex::new(None),
        }
    }

    fn initialize(&self, stream: Option<&IStream>) -> ThumbnailResult<()> {
        let stream = stream.ok_or(ThumbnailError::InvalidArgument)?;
        let pdf_bytes = read_stream(stream);
        let mut stored_bytes = self
            .pdf_bytes
            .lock()
            .map_err(|_| ThumbnailError::ProviderStatePoisoned)?;

        match pdf_bytes {
            Ok(pdf_bytes) => {
                *stored_bytes = Some(pdf_bytes);
                Ok(())
            }
            Err(error) => {
                *stored_bytes = None;
                Err(error)
            }
        }
    }

    fn get_thumbnail(&self, cx: u32) -> ThumbnailResult<HBITMAP> {
        if cx == 0 {
            return Err(ThumbnailError::InvalidArgument);
        }

        let pdf_data = {
            let stored_bytes = self
                .pdf_bytes
                .lock()
                .map_err(|_| ThumbnailError::ProviderStatePoisoned)?;
            Arc::clone(
                stored_bytes
                    .as_ref()
                    .ok_or(ThumbnailError::NotInitialized)?,
            )
        };

        let pdfium = init_pdfium(self.hinstance).map_err(ThumbnailError::PdfiumInitialization)?;
        let document = pdfium
            .load_pdf_from_byte_slice(pdf_data.as_ref(), None)
            .map_err(ThumbnailError::DocumentLoad)?;
        let pages = document.pages();

        if pages.is_empty() {
            return Err(ThumbnailError::EmptyDocument);
        }

        let first_page = pages.get(0).map_err(ThumbnailError::FirstPage)?;
        let page_width = first_page.width().value;
        let page_height = first_page.height().value;

        if !page_width.is_finite()
            || !page_height.is_finite()
            || page_width <= 0.0
            || page_height <= 0.0
        {
            return Err(ThumbnailError::InvalidPageDimensions);
        }

        let requested_dimension = cx.min(MAX_THUMBNAIL_DIMENSION);
        let (target_width, target_height) =
            calculate_thumbnail_dimensions(page_width, page_height, requested_dimension);
        let target_width =
            i32::try_from(target_width).map_err(|_| ThumbnailError::InvalidRenderDimensions)?;
        let target_height =
            i32::try_from(target_height).map_err(|_| ThumbnailError::InvalidRenderDimensions)?;

        let render_config = PdfRenderConfig::new()
            .set_target_width(target_width)
            .set_target_height(target_height)
            .limit_render_image_cache_size(true);
        let bitmap = first_page
            .render_with_config(&render_config)
            .map_err(ThumbnailError::Render)?;
        let width =
            u32::try_from(bitmap.width()).map_err(|_| ThumbnailError::InvalidBitmapDimensions)?;
        let height =
            u32::try_from(bitmap.height()).map_err(|_| ThumbnailError::InvalidBitmapDimensions)?;
        let rgba = bitmap.as_rgba_bytes();

        create_32bit_dib_section(width, height, &rgba).ok_or(ThumbnailError::BitmapCreation)
    }
}

impl Drop for BarePdfThumbnailProvider {
    fn drop(&mut self) {
        remove_active_object();
    }
}

fn read_stream(stream: &IStream) -> ThumbnailResult<Arc<[u8]>> {
    let mut buffer = Vec::new();
    let mut chunk = Vec::new();
    chunk
        .try_reserve_exact(STREAM_CHUNK_BYTES)
        .map_err(|_| ThumbnailError::Allocation)?;
    chunk.resize(STREAM_CHUNK_BYTES, 0);

    loop {
        let mut read_bytes = 0_u32;
        // SAFETY: COM supplies a valid `IStream`; `chunk` is writable for exactly its length.
        let hresult = unsafe {
            stream.Read(
                chunk.as_mut_ptr().cast(),
                u32::try_from(chunk.len()).map_err(|_| ThumbnailError::InvalidStreamReadLength)?,
                Some(&raw mut read_bytes),
            )
        };

        if hresult.is_err() {
            return Err(ThumbnailError::StreamRead(hresult));
        }

        let read_length =
            usize::try_from(read_bytes).map_err(|_| ThumbnailError::InvalidStreamReadLength)?;
        if read_length == 0 {
            break;
        }
        if read_length > chunk.len() {
            return Err(ThumbnailError::InvalidStreamReadLength);
        }

        let new_length = buffer
            .len()
            .checked_add(read_length)
            .ok_or(ThumbnailError::StreamTooLarge)?;
        if new_length > MAX_THUMBNAIL_PDF_BYTES {
            return Err(ThumbnailError::StreamTooLarge);
        }

        buffer
            .try_reserve(read_length)
            .map_err(|_| ThumbnailError::Allocation)?;
        buffer.extend_from_slice(&chunk[..read_length]);
    }

    (!buffer.is_empty())
        .then_some(Arc::from(buffer))
        .ok_or(ThumbnailError::EmptyStream)
}

impl IInitializeWithStream_Impl for BarePdfThumbnailProvider_Impl {
    fn Initialize(&self, pstream: Option<&IStream>, _grfmode: u32) -> Result<()> {
        std::panic::catch_unwind(|| {
            self.initialize(pstream)
                .map_err(ThumbnailError::into_com_error)
        })
        .unwrap_or_else(|_| Err(Error::from(E_UNEXPECTED)))
    }
}

impl IThumbnailProvider_Impl for BarePdfThumbnailProvider_Impl {
    /// # Safety
    /// COM guarantees that non-null output pointers are writable for their pointed-to types for
    /// this call. Arbitrary non-null pointers cannot be validated by Rust or Windows.
    #[allow(
        clippy::not_unsafe_ptr_arg_deref,
        reason = "IThumbnailProvider's generated COM ABI exposes required raw out-pointers"
    )]
    fn GetThumbnail(
        &self,
        cx: u32,
        phbmp: *mut HBITMAP,
        pdwalpha: *mut WTS_ALPHATYPE,
    ) -> Result<()> {
        if phbmp.is_null() || pdwalpha.is_null() {
            return Err(Error::from(E_POINTER));
        }

        // SAFETY: COM caller contract documented above guarantees both non-null outputs are valid.
        unsafe {
            *phbmp = HBITMAP::default();
            *pdwalpha = WTSAT_UNKNOWN;
        }

        std::panic::catch_unwind(|| {
            let hbitmap = self
                .get_thumbnail(cx)
                .map_err(ThumbnailError::into_com_error)?;
            // SAFETY: COM caller contract documented above guarantees both outputs remain valid.
            unsafe {
                *phbmp = hbitmap;
                *pdwalpha = WTSAT_ARGB;
            }
            Ok(())
        })
        .unwrap_or_else(|_| Err(Error::from(E_UNEXPECTED)))
    }
}
