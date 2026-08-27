use barepdf_pdf::conversion::{EncodedImageFormat, ImageEncoder};
use barepdf_pdf::RawBitmap;
use barepdf_platform_windows::WindowsImageEncoder;
use tempfile::tempdir;

fn sample_bitmap() -> RawBitmap {
    RawBitmap::new(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 255, 255,
        ],
    )
    .unwrap()
}

#[test]
fn wic_encodes_lossless_png_and_quality_ninety_jpeg() {
    let directory = tempdir().unwrap();
    let png = directory.path().join("page.png");
    let jpeg = directory.path().join("page.jpg");
    let encoder = WindowsImageEncoder;
    let bitmap = sample_bitmap();

    encoder
        .encode_rgba(&png, &bitmap, EncodedImageFormat::Png, 150)
        .unwrap();
    encoder
        .encode_rgba(
            &jpeg,
            &bitmap,
            EncodedImageFormat::Jpeg { quality: 90 },
            300,
        )
        .unwrap();

    let png_bytes = std::fs::read(png).unwrap();
    let jpeg_bytes = std::fs::read(jpeg).unwrap();
    assert_eq!(&png_bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&jpeg_bytes[..2], &[0xff, 0xd8]);
    assert_eq!(&jpeg_bytes[jpeg_bytes.len() - 2..], &[0xff, 0xd9]);
}

#[test]
fn wic_rejects_invalid_jpeg_quality_without_creating_a_file() {
    let directory = tempdir().unwrap();
    let output = directory.path().join("invalid.jpg");
    let encoder = WindowsImageEncoder;

    let error = encoder
        .encode_rgba(
            &output,
            &sample_bitmap(),
            EncodedImageFormat::Jpeg { quality: 0 },
            300,
        )
        .unwrap_err();

    assert!(error.to_string().contains("quality"));
    assert!(!output.exists());
}
