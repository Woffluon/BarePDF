use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::marker::PhantomData;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use barepdf_pdf::conversion::{EncodedImageFormat, ImageEncodeError, ImageEncoder};
use barepdf_pdf::RawBitmap;
use windows::core::{PCWSTR, PWSTR, VARIANT};
use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, RPC_E_CHANGED_MODE};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng,
    GUID_WICPixelFormat24bppBGR, GUID_WICPixelFormat32bppBGRA, GUID_WICPixelFormat32bppRGBA,
    IWICBitmapEncoder, IWICBitmapFrameEncode, IWICImagingFactory, IWICPalette,
    WICBitmapDitherTypeNone, WICBitmapEncoderNoCache, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::StructuredStorage::{IPropertyBag2, PROPBAG2};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VT_R4;

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsImageEncoder;

impl ImageEncoder for WindowsImageEncoder {
    fn encode_rgba(
        &self,
        output: &Path,
        bitmap: &RawBitmap,
        format: EncodedImageFormat,
        dpi: u16,
    ) -> Result<(), ImageEncodeError> {
        let format = WicFormat::try_from(format)?;
        if dpi == 0 {
            return Err(ImageEncodeError::new("image DPI must be greater than zero"));
        }

        let mut reservation = OutputReservation::create(output)?;
        encode_with_wic(output, bitmap, format, dpi)?;
        OpenOptions::new()
            .write(true)
            .open(output)
            .and_then(|file| file.sync_all())
            .map_err(ImageEncodeError::from_io)?;
        reservation.commit();
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum WicFormat {
    Png,
    Jpeg { quality: u8 },
}

impl TryFrom<EncodedImageFormat> for WicFormat {
    type Error = ImageEncodeError;

    fn try_from(format: EncodedImageFormat) -> Result<Self, Self::Error> {
        match format {
            EncodedImageFormat::Png => Ok(Self::Png),
            EncodedImageFormat::Jpeg { quality } if (1..=100).contains(&quality) => {
                Ok(Self::Jpeg { quality })
            }
            EncodedImageFormat::Jpeg { .. } => Err(ImageEncodeError::new(
                "JPEG quality must be between 1 and 100",
            )),
        }
    }
}

fn encode_with_wic(
    output: &Path,
    bitmap: &RawBitmap,
    format: WicFormat,
    dpi: u16,
) -> Result<(), ImageEncodeError> {
    let _apartment = ComApartment::initialize()?;
    let factory: IWICImagingFactory = unsafe {
        // SAFETY: COM is initialized for this thread. The CLSID and requested interface are the
        // documented in-process Windows Imaging Component factory pair, with no outer aggregate.
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
    }
    .map_err(|error| wic_error("could not create the WIC imaging factory", error))?;

    let stream = unsafe {
        // SAFETY: `factory` is a live WIC factory owned by this thread.
        factory.CreateStream()
    }
    .map_err(|error| wic_error("could not create the WIC output stream", error))?;
    let path = wide_null(output.as_os_str());
    unsafe {
        // SAFETY: `path` is a live NUL-terminated UTF-16 allocation for this call. The output was
        // reserved with create-new semantics and remains private to this conversion job.
        stream.InitializeFromFilename(PCWSTR(path.as_ptr()), GENERIC_WRITE.0)
    }
    .map_err(|error| wic_error("could not open the WIC output file", error))?;

    let container = match format {
        WicFormat::Png => &GUID_ContainerFormatPng,
        WicFormat::Jpeg { .. } => &GUID_ContainerFormatJpeg,
    };
    let encoder = unsafe {
        // SAFETY: `factory` is live; both GUID pointers remain valid for the duration of the call.
        factory.CreateEncoder(container, std::ptr::null())
    }
    .map_err(|error| wic_error("could not create the requested WIC encoder", error))?;
    unsafe {
        // SAFETY: `encoder` and `stream` are live COM interfaces owned by this thread.
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)
    }
    .map_err(|error| wic_error("could not initialize the WIC encoder", error))?;

    let (frame, options) = create_frame(&encoder)?;
    if let WicFormat::Jpeg { quality } = format {
        set_jpeg_quality(&options, quality)?;
    }
    unsafe {
        // SAFETY: `options` was returned for this exact frame and remains live through the call.
        frame.Initialize(&options)
    }
    .map_err(|error| wic_error("could not initialize the WIC frame", error))?;
    unsafe {
        // SAFETY: Bitmap dimensions were validated by RawBitmap construction and are passed
        // unchanged to the frame created above.
        frame.SetSize(bitmap.width(), bitmap.height())
    }
    .map_err(|error| wic_error("could not set the WIC frame size", error))?;
    unsafe {
        // SAFETY: The finite integer DPI value is converted losslessly to f64 for WIC metadata.
        frame.SetResolution(f64::from(dpi), f64::from(dpi))
    }
    .map_err(|error| wic_error("could not set the WIC frame resolution", error))?;

    write_bitmap(&frame, bitmap, format)?;
    unsafe {
        // SAFETY: All frame metadata and pixel rows have been supplied successfully.
        frame.Commit()
    }
    .map_err(|error| wic_error("could not commit the WIC frame", error))?;
    unsafe {
        // SAFETY: The encoder owns exactly the committed frame and live output stream above.
        encoder.Commit()
    }
    .map_err(|error| wic_error("could not commit the WIC image", error))
}

fn create_frame(
    encoder: &IWICBitmapEncoder,
) -> Result<(IWICBitmapFrameEncode, IPropertyBag2), ImageEncodeError> {
    let mut frame = None;
    let mut options = None;
    unsafe {
        // SAFETY: Both out-parameters point to initialized Option storage and `encoder` is live.
        encoder.CreateNewFrame(&mut frame, &mut options)
    }
    .map_err(|error| wic_error("could not create the WIC image frame", error))?;
    let frame = frame.ok_or_else(|| ImageEncodeError::new("WIC returned no image frame"))?;
    let options = options.ok_or_else(|| ImageEncodeError::new("WIC returned no frame options"))?;
    Ok((frame, options))
}

fn set_jpeg_quality(options: &IPropertyBag2, quality: u8) -> Result<(), ImageEncodeError> {
    let mut name = "ImageQuality\0".encode_utf16().collect::<Vec<_>>();
    let property = PROPBAG2 {
        vt: VT_R4,
        pstrName: PWSTR(name.as_mut_ptr()),
        ..PROPBAG2::default()
    };
    let value = VARIANT::from(f32::from(quality) / 100.0);
    unsafe {
        // SAFETY: The property name and VARIANT remain live for the synchronous write. WIC owns
        // neither pointer after the call returns.
        options.Write(1, &property, &value)
    }
    .map_err(|error| wic_error("could not set WIC JPEG quality", error))
}

fn write_bitmap(
    frame: &IWICBitmapFrameEncode,
    bitmap: &RawBitmap,
    format: WicFormat,
) -> Result<(), ImageEncodeError> {
    match format {
        WicFormat::Png => write_png_rows(frame, bitmap),
        WicFormat::Jpeg { .. } => write_jpeg_rows(frame, bitmap),
    }
}

fn write_png_rows(
    frame: &IWICBitmapFrameEncode,
    bitmap: &RawBitmap,
) -> Result<(), ImageEncodeError> {
    let mut pixel_format = GUID_WICPixelFormat32bppBGRA;
    unsafe {
        // SAFETY: WIC may update the live GUID to its negotiated format.
        frame.SetPixelFormat(&raw mut pixel_format)
    }
    .map_err(|error| wic_error("could not set the WIC PNG pixel format", error))?;
    if pixel_format != GUID_WICPixelFormat32bppBGRA {
        return Err(ImageEncodeError::new(
            "the WIC PNG encoder does not support BGRA pixels",
        ));
    }

    let stride = bitmap
        .width()
        .checked_mul(4)
        .ok_or_else(|| ImageEncodeError::new("PNG row stride overflow"))?;
    let stride_len = usize::try_from(stride)
        .map_err(|_| ImageEncodeError::new("PNG output row is too large"))?;
    let mut output_row = vec![0_u8; stride_len];
    for source_row in bitmap.pixels().chunks_exact(stride_len) {
        for (rgba, bgra) in source_row
            .chunks_exact(4)
            .zip(output_row.chunks_exact_mut(4))
        {
            bgra.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
        }
        unsafe {
            // SAFETY: `output_row` contains exactly one tightly packed BGRA row and WIC consumes
            // it synchronously before the next iteration.
            frame.WritePixels(1, stride, &output_row)
        }
        .map_err(|error| wic_error("could not write WIC PNG pixels", error))?;
    }
    Ok(())
}

fn write_jpeg_rows(
    frame: &IWICBitmapFrameEncode,
    bitmap: &RawBitmap,
) -> Result<(), ImageEncodeError> {
    let mut pixel_format = GUID_WICPixelFormat24bppBGR;
    unsafe {
        // SAFETY: WIC may update the live GUID to its negotiated format.
        frame.SetPixelFormat(&raw mut pixel_format)
    }
    .map_err(|error| wic_error("could not set the WIC JPEG pixel format", error))?;
    if pixel_format != GUID_WICPixelFormat24bppBGR {
        return Err(ImageEncodeError::new(
            "the WIC JPEG encoder does not support BGR pixels",
        ));
    }

    let source_stride = usize::try_from(bitmap.width())
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| ImageEncodeError::new("JPEG source row stride overflow"))?;
    let output_stride = bitmap
        .width()
        .checked_mul(3)
        .ok_or_else(|| ImageEncodeError::new("JPEG output row stride overflow"))?;
    let output_len = usize::try_from(output_stride)
        .map_err(|_| ImageEncodeError::new("JPEG output row is too large"))?;
    let mut output_row = vec![0_u8; output_len];

    for source_row in bitmap.pixels().chunks_exact(source_stride) {
        for (rgba, bgr) in source_row
            .chunks_exact(4)
            .zip(output_row.chunks_exact_mut(3))
        {
            bgr.copy_from_slice(&[rgba[2], rgba[1], rgba[0]]);
        }
        unsafe {
            // SAFETY: `output_row` contains exactly one tightly packed BGR row with the stride
            // negotiated above; WIC consumes it synchronously before the next iteration.
            frame.WritePixels(1, output_stride, &output_row)
        }
        .map_err(|error| wic_error("could not write WIC JPEG pixels", error))?;
    }
    Ok(())
}

fn wic_error(operation: &'static str, error: windows::core::Error) -> ImageEncodeError {
    ImageEncodeError::new(format!("{operation}: {error}"))
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

struct OutputReservation {
    path: Option<PathBuf>,
}

impl OutputReservation {
    fn create(path: &Path) -> Result<Self, ImageEncodeError> {
        // Reserve the target path atomically with `CREATE_NEW` (`create_new(true)`), then
        // immediately drop the `std::fs::File` handle so `IWICStream::InitializeFromFilename` can
        // open the newly created empty file with `GENERIC_WRITE` without a Win32 sharing violation.
        // If `encode_with_wic` fails or panics before `commit()`, `Drop` removes the partial file
        // after `IWICStream` and `IWICBitmapEncoder` have already been dropped.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(ImageEncodeError::from_io)?;
        drop(file);
        Ok(Self {
            path: Some(path.to_owned()),
        })
    }

    fn commit(&mut self) {
        self.path = None;
    }
}

impl Drop for OutputReservation {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

struct ComApartment {
    uninitialize: bool,
    _not_send_or_sync: PhantomData<*mut ()>,
}

impl ComApartment {
    fn initialize() -> Result<Self, ImageEncodeError> {
        let result = unsafe {
            // SAFETY: Null reserved pointer is required. The matching uninitialize call is owned by
            // this guard only when COM reports that this call initialized the thread apartment.
            CoInitializeEx(None, COINIT_MULTITHREADED)
        };
        if result.is_ok() {
            Ok(Self {
                uninitialize: true,
                _not_send_or_sync: PhantomData,
            })
        } else if result == RPC_E_CHANGED_MODE {
            Ok(Self {
                uninitialize: false,
                _not_send_or_sync: PhantomData,
            })
        } else {
            Err(ImageEncodeError::new(format!(
                "could not initialize COM for WIC: {}",
                windows::core::Error::from_hresult(result)
            )))
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.uninitialize {
            unsafe {
                // SAFETY: This guard records one successful CoInitializeEx call on this thread and
                // is dropped on the same stack before the thread can terminate.
                CoUninitialize();
            }
        }
    }
}

/// Decodes a PNG or JPEG image file into a 32-bit RGBA [`RawBitmap`] using Windows WIC.
///
/// # Errors
///
/// Returns [`ImageEncodeError`] when COM/WIC initialization, image decoding, pixel format
/// conversion, or buffer allocation fails.
pub fn decode_image_rgba(path: &Path) -> Result<RawBitmap, ImageEncodeError> {
    let _apartment = ComApartment::initialize()?;
    let factory: IWICImagingFactory = unsafe {
        // SAFETY: COM is initialized for this thread; CLSID and interface match in-process WIC.
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
    }
    .map_err(|error| wic_error("could not create the WIC imaging factory", error))?;

    let wide_path = wide_null(path.as_os_str());
    let decoder = unsafe {
        // SAFETY: `wide_path` is a live NUL-terminated UTF-16 buffer for the duration of the call.
        factory.CreateDecoderFromFilename(
            PCWSTR(wide_path.as_ptr()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )
    }
    .map_err(|error| wic_error("could not decode the selected image file", error))?;

    let frame = unsafe {
        // SAFETY: `decoder` is a live WIC decoder on this thread.
        decoder.GetFrame(0)
    }
    .map_err(|error| wic_error("could not read the first image frame", error))?;

    let converter = unsafe {
        // SAFETY: `factory` is a live WIC factory on this thread.
        factory.CreateFormatConverter()
    }
    .map_err(|error| wic_error("could not create the WIC format converter", error))?;

    unsafe {
        // SAFETY: `frame` and `converter` are live COM interfaces; GUID pointer is static.
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppRGBA,
            WICBitmapDitherTypeNone,
            None::<&IWICPalette>,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
    }
    .map_err(|error| wic_error("could not initialize RGBA format conversion", error))?;

    let mut width = 0_u32;
    let mut height = 0_u32;
    unsafe {
        // SAFETY: Out-pointers reference valid stack u32 variables.
        converter.GetSize(&mut width, &mut height)
    }
    .map_err(|error| wic_error("could not read decoded image dimensions", error))?;

    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return Err(ImageEncodeError::new(
            "decoded image dimensions are invalid or exceed 8192x8192",
        ));
    }

    let stride = width
        .checked_mul(4)
        .ok_or_else(|| ImageEncodeError::new("decoded image row stride overflow"))?;
    let total_bytes = usize::try_from(stride)
        .ok()
        .and_then(|s| usize::try_from(height).ok()?.checked_mul(s))
        .ok_or_else(|| ImageEncodeError::new("decoded image buffer size overflow"))?;

    let mut rgba_pixels = vec![0_u8; total_bytes];
    unsafe {
        // SAFETY: `rgba_pixels` has exact capacity `stride * height` for all rows of the frame.
        converter.CopyPixels(std::ptr::null(), stride, &mut rgba_pixels)
    }
    .map_err(|error| wic_error("could not copy decoded RGBA pixels", error))?;

    RawBitmap::new(width, height, rgba_pixels).map_err(|e| ImageEncodeError::new(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn encode_and_decode_png_roundtrips_rgba_pixels() {
        let dir = tempdir().unwrap();
        let png_path = dir.path().join("sig.png");
        let original = RawBitmap::new(
            2,
            2,
            vec![
                10, 20, 30, 255, 40, 50, 60, 128, 70, 80, 90, 255, 200, 150, 100, 64,
            ],
        )
        .unwrap();

        WindowsImageEncoder
            .encode_rgba(&png_path, &original, EncodedImageFormat::Png, 150)
            .unwrap();

        let decoded = decode_image_rgba(&png_path).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn com_apartment_initializes_and_is_thread_bound() {
        // Verify PhantomData<*mut ()> is zero-sized and ComApartment initializes/drops cleanly
        assert_eq!(
            std::mem::size_of::<ComApartment>(),
            std::mem::size_of::<bool>()
        );
        let apt1 = ComApartment::initialize().expect("first COM apartment init succeeds");
        let apt2 = ComApartment::initialize().expect("nested COM apartment init succeeds");
        drop(apt2);
        drop(apt1);
    }

    #[test]
    fn output_reservation_releases_handle_for_wic_and_cleans_up_on_drop() {
        let dir = tempdir().unwrap();
        let reserved_path = dir.path().join("reserved.png");

        let reservation = OutputReservation::create(&reserved_path).unwrap();
        assert!(reserved_path.exists());
        // Verify a second create_new on the same path fails while reserved
        assert!(OutputReservation::create(&reserved_path).is_err());

        // Dropping uncommitted reservation removes the file
        drop(reservation);
        assert!(!reserved_path.exists());
    }
}
