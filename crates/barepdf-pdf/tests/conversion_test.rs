use std::path::{Path, PathBuf};
use std::sync::Mutex;

use barepdf_core::{PageCount, PageIndex, PageTextGeometry, PdfError, Rotation};
use barepdf_pdf::conversion::{
    convert_pdf, CancellationToken, ConversionDpi, ConversionError, ConversionFormat,
    ConversionRequest, ConversionWarning, EncodedImageFormat, ImageEncodeError, ImageEncoder,
    JobPassword,
};
use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
use tempfile::tempdir;

#[derive(Clone)]
struct FakeBackend {
    texts: Vec<String>,
    dimensions: Vec<(f32, f32)>,
    required_password: Option<String>,
}

impl FakeBackend {
    fn new(texts: &[&str]) -> Self {
        Self {
            texts: texts.iter().map(|text| (*text).to_owned()).collect(),
            dimensions: vec![(72.0, 144.0); texts.len()],
            required_password: None,
        }
    }

    fn encrypted(texts: &[&str], password: &str) -> Self {
        Self {
            required_password: Some(password.to_owned()),
            ..Self::new(texts)
        }
    }
}

impl PdfBackend for FakeBackend {
    fn open_path(
        &self,
        _path: &Path,
        password: Option<&str>,
    ) -> Result<Box<dyn PdfDocument>, PdfError> {
        if let Some(required) = &self.required_password {
            match password {
                None => return Err(PdfError::PasswordRequired),
                Some(candidate) if candidate != required => {
                    return Err(PdfError::IncorrectPassword);
                }
                Some(_) => {}
            }
        }
        Ok(Box::new(FakeDocument {
            texts: self.texts.clone(),
            dimensions: self.dimensions.clone(),
        }))
    }

    fn open_bytes(
        &self,
        _bytes: Vec<u8>,
        _password: Option<&str>,
    ) -> Result<Box<dyn PdfDocument>, PdfError> {
        Err(PdfError::InvalidPdfReason("unused fake path".into()))
    }
}

struct FakeDocument {
    texts: Vec<String>,
    dimensions: Vec<(f32, f32)>,
}

impl PdfDocument for FakeDocument {
    fn page_count(&self) -> Result<PageCount, PdfError> {
        PageCount::new(self.texts.len() as u32)
            .ok_or_else(|| PdfError::InvalidPdfReason("empty fake document".into()))
    }

    fn page_dimensions(&self, page_index: PageIndex) -> Result<(f32, f32), PdfError> {
        self.dimensions
            .get(page_index.get() as usize)
            .copied()
            .ok_or_else(|| PdfError::InvalidPdfReason("fake page out of range".into()))
    }

    fn render_page(
        &self,
        page_index: PageIndex,
        target_width: u32,
        target_height: u32,
        _rotation: Rotation,
    ) -> Result<RawBitmap, PdfError> {
        let byte_len = usize::try_from(target_width)
            .ok()
            .and_then(|width| usize::try_from(target_height).ok()?.checked_mul(width))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(PdfError::OutOfMemoryBudget(usize::MAX))?;
        let mut pixels = vec![0; byte_len];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[page_index.get() as u8, 2, 3, 255]);
        }
        RawBitmap::new(target_width, target_height, pixels)
            .map_err(|error| PdfError::InvalidPdfReason(error.to_string()))
    }

    fn extract_text(&self, page_index: PageIndex) -> Result<String, PdfError> {
        self.texts
            .get(page_index.get() as usize)
            .cloned()
            .ok_or_else(|| PdfError::InvalidPdfReason("fake page out of range".into()))
    }

    fn extract_text_spans(&self, _page_index: PageIndex) -> Result<Vec<TextSpan>, PdfError> {
        Ok(Vec::new())
    }

    fn get_page_text_geometry(&self, page_index: PageIndex) -> Result<PageTextGeometry, PdfError> {
        Ok(PageTextGeometry {
            page_index,
            glyphs: Vec::new(),
        })
    }

    fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncodeCall {
    file_name: String,
    width: u32,
    height: u32,
    format: EncodedImageFormat,
    dpi: u16,
}

#[derive(Default)]
struct RecordingEncoder {
    calls: Mutex<Vec<EncodeCall>>,
    fail_after_write: bool,
    cancel_after_write: Option<CancellationToken>,
}

impl ImageEncoder for RecordingEncoder {
    fn encode_rgba(
        &self,
        output: &Path,
        bitmap: &RawBitmap,
        format: EncodedImageFormat,
        dpi: u16,
    ) -> Result<(), ImageEncodeError> {
        std::fs::write(output, b"encoded").map_err(ImageEncodeError::from_io)?;
        self.calls.lock().unwrap().push(EncodeCall {
            file_name: output
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            width: bitmap.width(),
            height: bitmap.height(),
            format,
            dpi,
        });
        if let Some(cancellation) = &self.cancel_after_write {
            cancellation.cancel();
        }
        if self.fail_after_write {
            return Err(ImageEncodeError::new("intentional encoder failure"));
        }
        Ok(())
    }
}

fn idx(raw: u32) -> PageIndex {
    PageIndex::from_raw(raw)
}

fn request(
    source: PathBuf,
    output_parent: PathBuf,
    pages: Vec<PageIndex>,
    format: ConversionFormat,
) -> ConversionRequest {
    ConversionRequest::new(source, output_parent, pages, format)
}

fn create_source(directory: &Path) -> PathBuf {
    let source = directory.join("Quarterly Report.pdf");
    std::fs::write(&source, b"fake pdf").unwrap();
    source
}

fn visible_directories(parent: &Path) -> Vec<PathBuf> {
    let mut directories = std::fs::read_dir(parent)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    directories
}

#[test]
fn text_conversion_uses_selected_embedded_text_and_job_password() {
    let directory = tempdir().unwrap();
    let source = create_source(directory.path());
    let backend = FakeBackend::encrypted(&["first", "second", "third"], "correct horse");
    let request = request(
        source,
        directory.path().to_owned(),
        vec![idx(2), idx(0)],
        ConversionFormat::Text,
    )
    .with_password(JobPassword::new("correct horse".to_owned()));

    let report = convert_pdf(&backend, None, request).unwrap();

    assert_eq!(report.files.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&report.files[0]).unwrap(),
        "third\n\nfirst\n"
    );
    assert!(report.warnings.is_empty());
    assert_eq!(
        visible_directories(directory.path()),
        vec![report.output_directory]
    );
}

#[test]
fn markdown_marks_pages_separates_them_and_warns_for_blank_pages() {
    let directory = tempdir().unwrap();
    let source = create_source(directory.path());
    let backend = FakeBackend::new(&["alpha", " \r\n", "gamma"]);
    let request = request(
        source,
        directory.path().to_owned(),
        vec![idx(0), idx(1), idx(2)],
        ConversionFormat::Markdown,
    );

    let report = convert_pdf(&backend, None, request).unwrap();

    let markdown = std::fs::read_to_string(&report.files[0]).unwrap();
    assert_eq!(
        markdown,
        "<!-- Page 1 -->\n\nalpha\n\n---\n\n<!-- Page 2 -->\n\n\n\n---\n\n<!-- Page 3 -->\n\ngamma\n"
    );
    assert_eq!(
        report.warnings,
        vec![ConversionWarning::PagesWithoutEmbeddedText(vec![idx(1)])]
    );
}

#[test]
fn all_blank_selected_pages_fail_without_publishing_empty_output() {
    let directory = tempdir().unwrap();
    let source = create_source(directory.path());
    let backend = FakeBackend::new(&[" ", "\n"]);
    let request = request(
        source,
        directory.path().to_owned(),
        vec![idx(0), idx(1)],
        ConversionFormat::Text,
    );

    let error = convert_pdf(&backend, None, request).unwrap_err();

    assert!(matches!(error, ConversionError::OcrNotSupported { .. }));
    assert!(visible_directories(directory.path()).is_empty());
}

#[test]
fn wrong_password_leaves_no_output_or_staging_directory() {
    let directory = tempdir().unwrap();
    let source = create_source(directory.path());
    let backend = FakeBackend::encrypted(&["secret text"], "right");
    let request = request(
        source,
        directory.path().to_owned(),
        vec![idx(0)],
        ConversionFormat::Text,
    )
    .with_password(JobPassword::new("wrong".to_owned()));

    let error = convert_pdf(&backend, None, request).unwrap_err();

    assert!(matches!(
        error,
        ConversionError::Pdf(PdfError::IncorrectPassword)
    ));
    assert!(visible_directories(directory.path()).is_empty());
}

#[test]
fn image_conversion_uses_selected_dpi_lossless_png_and_jpeg_quality_ninety() {
    let directory = tempdir().unwrap();
    let source = create_source(directory.path());
    let backend = FakeBackend::new(&["one", "two"]);
    let encoder = RecordingEncoder::default();
    let png_request = request(
        source.clone(),
        directory.path().to_owned(),
        vec![idx(1)],
        ConversionFormat::Png,
    )
    .with_dpi(ConversionDpi::Dpi150);

    convert_pdf(&backend, Some(&encoder), png_request).unwrap();

    let jpeg_request = request(
        source,
        directory.path().to_owned(),
        vec![idx(0)],
        ConversionFormat::Jpeg,
    )
    .with_dpi(ConversionDpi::Dpi300);
    convert_pdf(&backend, Some(&encoder), jpeg_request).unwrap();

    assert_eq!(
        *encoder.calls.lock().unwrap(),
        vec![
            EncodeCall {
                file_name: "page-0002.png".into(),
                width: 150,
                height: 300,
                format: EncodedImageFormat::Png,
                dpi: 150,
            },
            EncodeCall {
                file_name: "page-0001.jpg".into(),
                width: 300,
                height: 600,
                format: EncodedImageFormat::Jpeg { quality: 90 },
                dpi: 300,
            },
        ]
    );
}

#[test]
fn repeated_conversion_chooses_a_new_folder_and_does_not_overwrite() {
    let directory = tempdir().unwrap();
    let source = create_source(directory.path());
    let backend = FakeBackend::new(&["one"]);

    let first = convert_pdf(
        &backend,
        None,
        request(
            source.clone(),
            directory.path().to_owned(),
            vec![idx(0)],
            ConversionFormat::Text,
        ),
    )
    .unwrap();
    std::fs::write(&first.files[0], "user-owned").unwrap();
    let second = convert_pdf(
        &backend,
        None,
        request(
            source,
            directory.path().to_owned(),
            vec![idx(0)],
            ConversionFormat::Text,
        ),
    )
    .unwrap();

    assert_ne!(first.output_directory, second.output_directory);
    assert_eq!(
        std::fs::read_to_string(first.files[0].clone()).unwrap(),
        "user-owned"
    );
    assert_eq!(
        std::fs::read_to_string(second.files[0].clone()).unwrap(),
        "one\n"
    );
}

#[test]
fn cancellation_after_an_encoded_page_removes_all_partial_files() {
    let directory = tempdir().unwrap();
    let source = create_source(directory.path());
    let backend = FakeBackend::new(&["one", "two"]);
    let cancellation = CancellationToken::new();
    let encoder = RecordingEncoder {
        cancel_after_write: Some(cancellation.clone()),
        ..RecordingEncoder::default()
    };
    let request = request(
        source,
        directory.path().to_owned(),
        vec![idx(0), idx(1)],
        ConversionFormat::Png,
    )
    .with_cancellation(cancellation);

    let error = convert_pdf(&backend, Some(&encoder), request).unwrap_err();

    assert!(matches!(error, ConversionError::Cancelled));
    assert!(visible_directories(directory.path()).is_empty());
}

#[test]
fn encoder_failure_after_writing_removes_all_partial_files() {
    let directory = tempdir().unwrap();
    let source = create_source(directory.path());
    let backend = FakeBackend::new(&["one"]);
    let encoder = RecordingEncoder {
        fail_after_write: true,
        ..RecordingEncoder::default()
    };
    let request = request(
        source,
        directory.path().to_owned(),
        vec![idx(0)],
        ConversionFormat::Png,
    );

    let error = convert_pdf(&backend, Some(&encoder), request).unwrap_err();

    assert!(matches!(error, ConversionError::ImageEncoding { .. }));
    assert!(visible_directories(directory.path()).is_empty());
}

#[test]
fn password_debug_output_is_redacted() {
    let password = JobPassword::new("never-log-me".to_owned());

    let debug = format!("{password:?}");

    assert_eq!(debug, "JobPassword([REDACTED])");
    assert!(!debug.contains("never-log-me"));
}
