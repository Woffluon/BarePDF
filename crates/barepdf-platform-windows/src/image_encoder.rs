use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use barepdf_pdf::conversion::{EncodedImageFormat, ImageEncodeError, ImageEncoder};
use barepdf_pdf::RawBitmap;
use windows::core::{PCWSTR, PWSTR, VARIANT};
use windows::Win32::Foundation::{GENERIC_WRITE, RPC_E_CHANGED_MODE};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng,
    GUID_WICPixelFormat24bppBGR, GUID_WICPixelFormat32bppBGRA, IWICBitmapEncoder,
    IWICBitmapFrameEncode, IWICImagingFactory, WICBitmapEncoderNoCache,
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
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(ImageEncodeError::from_io)?;
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
}

impl ComApartment {
    fn initialize() -> Result<Self, ImageEncodeError> {
        let result = unsafe {
            // SAFETY: Null reserved pointer is required. The matching uninitialize call is owned by
            // this guard only when COM reports that this call initialized the thread apartment.
            CoInitializeEx(None, COINIT_MULTITHREADED)
        };
        if result.is_ok() {
            Ok(Self { uninitialize: true })
        } else if result == RPC_E_CHANGED_MODE {
            Ok(Self {
                uninitialize: false,
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
