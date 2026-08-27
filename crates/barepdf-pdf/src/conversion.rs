use std::collections::HashSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use barepdf_core::{validate_page_selection, MemoryBudget, PageIndex, PdfError, Rotation};

use crate::backend::{PdfBackend, RawBitmap};
use crate::text;

const POINTS_PER_INCH: f64 = 72.0;
const JPEG_QUALITY: u8 = 90;
const MAX_UNIQUE_ATTEMPTS: u32 = 10_000;

static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionFormat {
    Text,
    Markdown,
    Png,
    Jpeg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConversionDpi {
    Dpi150,
    #[default]
    Dpi300,
}

impl ConversionDpi {
    #[must_use]
    pub const fn get(self) -> u16 {
        match self {
            Self::Dpi150 => 150,
            Self::Dpi300 => 300,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedImageFormat {
    Png,
    Jpeg { quality: u8 },
}

#[derive(Debug)]
pub struct ImageEncodeError {
    reason: String,
}

impl ImageEncodeError {
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn from_io(source: io::Error) -> Self {
        Self::new(source.to_string())
    }
}

impl fmt::Display for ImageEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for ImageEncodeError {}

pub trait ImageEncoder: Send + Sync {
    /// Encodes one tightly packed RGBA bitmap to a newly staged output file.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested format is unsupported or the staged file cannot be
    /// encoded completely.
    fn encode_rgba(
        &self,
        output: &Path,
        bitmap: &RawBitmap,
        format: EncodedImageFormat,
        dpi: u16,
    ) -> Result<(), ImageEncodeError>;
}

pub struct JobPassword {
    bytes: Vec<u8>,
}

impl JobPassword {
    #[must_use]
    pub fn new(password: String) -> Self {
        Self {
            bytes: password.into_bytes(),
        }
    }

    fn expose(&self) -> &str {
        std::str::from_utf8(&self.bytes)
            .expect("JobPassword is constructed from a valid UTF-8 String")
    }
}

impl fmt::Debug for JobPassword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JobPassword([REDACTED])")
    }
}

impl Drop for JobPassword {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

#[derive(Debug)]
pub struct ConversionRequest {
    source: PathBuf,
    output_parent: PathBuf,
    pages: Vec<PageIndex>,
    format: ConversionFormat,
    dpi: ConversionDpi,
    password: Option<JobPassword>,
    cancellation: CancellationToken,
}

impl ConversionRequest {
    #[must_use]
    pub fn new(
        source: PathBuf,
        output_parent: PathBuf,
        pages: Vec<PageIndex>,
        format: ConversionFormat,
    ) -> Self {
        Self {
            source,
            output_parent,
            pages,
            format,
            dpi: ConversionDpi::default(),
            password: None,
            cancellation: CancellationToken::default(),
        }
    }

    #[must_use]
    pub fn with_dpi(mut self, dpi: ConversionDpi) -> Self {
        self.dpi = dpi;
        self
    }

    #[must_use]
    pub fn with_password(mut self, password: JobPassword) -> Self {
        self.password = Some(password);
        self
    }

    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversionWarning {
    PagesWithoutEmbeddedText(Vec<PageIndex>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionReport {
    pub output_directory: PathBuf,
    pub files: Vec<PathBuf>,
    pub warnings: Vec<ConversionWarning>,
}

#[derive(Debug)]
pub enum ConversionError {
    Pdf(PdfError),
    InvalidPageSelection(String),
    DuplicatePage(PageIndex),
    OcrNotSupported {
        pages: Vec<PageIndex>,
    },
    ImageEncoderUnavailable,
    ImageEncoding {
        page_index: PageIndex,
        source: ImageEncodeError,
    },
    Cancelled,
    Io {
        operation: &'static str,
        source: io::Error,
    },
}

impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pdf(error) => write!(formatter, "{error}"),
            Self::InvalidPageSelection(reason) => {
                write!(formatter, "invalid conversion page selection: {reason}")
            }
            Self::DuplicatePage(page) => {
                write!(
                    formatter,
                    "conversion page {page} was selected more than once"
                )
            }
            Self::OcrNotSupported { .. } => formatter.write_str(
                "selected pages contain no embedded text; OCR conversion is not supported",
            ),
            Self::ImageEncoderUnavailable => {
                formatter.write_str("image conversion is unavailable on this platform")
            }
            Self::ImageEncoding { page_index, source } => {
                write!(formatter, "could not encode page {page_index}: {source}")
            }
            Self::Cancelled => formatter.write_str("conversion cancelled"),
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
        }
    }
}

impl std::error::Error for ConversionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pdf(error) => Some(error),
            Self::ImageEncoding { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            Self::InvalidPageSelection(_)
            | Self::DuplicatePage(_)
            | Self::OcrNotSupported { .. }
            | Self::ImageEncoderUnavailable
            | Self::Cancelled => None,
        }
    }
}

impl From<PdfError> for ConversionError {
    fn from(error: PdfError) -> Self {
        Self::Pdf(error)
    }
}

/// Converts selected pages without publishing partial output.
///
/// This function performs PDF, raster, and file-system work synchronously. Callers must run it on
/// a worker thread rather than the UI thread. Text formats do not require an image encoder.
///
/// # Errors
///
/// Returns an error for invalid page selections, PDF/password failures, missing embedded text,
/// cancellation, image encoding failures, or file-system failures. All staged output is removed on
/// every error path.
pub fn convert_pdf(
    backend: &dyn PdfBackend,
    image_encoder: Option<&dyn ImageEncoder>,
    mut request: ConversionRequest,
) -> Result<ConversionReport, ConversionError> {
    check_cancel(&request.cancellation)?;
    if !request.source.is_file() {
        return Err(PdfError::FileNotFound(request.source.display().to_string()).into());
    }
    if !request.output_parent.is_dir() {
        return Err(io_failure(
            "conversion output parent is not a directory",
            io::Error::new(io::ErrorKind::NotFound, "output directory does not exist"),
        ));
    }

    let document_result = backend.open_path(
        &request.source,
        request.password.as_ref().map(JobPassword::expose),
    );
    request.password = None;
    let document = document_result?;
    check_cancel(&request.cancellation)?;

    let page_count = document.page_count()?;
    validate_page_selection(&request.pages, page_count)
        .map_err(|error| ConversionError::InvalidPageSelection(error.to_string()))?;
    validate_no_duplicates(&request.pages)?;

    let output_stem = safe_output_stem(&request.source);
    let staging = StagingDirectory::create(&request.output_parent, &output_stem)?;
    let (file_names, warnings) = match request.format {
        ConversionFormat::Text | ConversionFormat::Markdown => {
            convert_text(&*document, &request, staging.path())?
        }
        ConversionFormat::Png | ConversionFormat::Jpeg => convert_images(
            &*document,
            image_encoder.ok_or(ConversionError::ImageEncoderUnavailable)?,
            &request,
            staging.path(),
        )?,
    };
    check_cancel(&request.cancellation)?;
    let output_directory = staging.publish()?;
    let files = file_names
        .into_iter()
        .map(|name| output_directory.join(name))
        .collect();
    Ok(ConversionReport {
        output_directory,
        files,
        warnings,
    })
}

fn validate_no_duplicates(pages: &[PageIndex]) -> Result<(), ConversionError> {
    let mut seen = HashSet::with_capacity(pages.len());
    for page in pages {
        if !seen.insert(page.get()) {
            return Err(ConversionError::DuplicatePage(*page));
        }
    }
    Ok(())
}

fn convert_text(
    document: &dyn crate::backend::PdfDocument,
    request: &ConversionRequest,
    staging: &Path,
) -> Result<(Vec<PathBuf>, Vec<ConversionWarning>), ConversionError> {
    let mut pages = Vec::with_capacity(request.pages.len());
    let mut blank_pages = Vec::new();
    for page_index in &request.pages {
        check_cancel(&request.cancellation)?;
        let embedded_text = document.extract_text(*page_index)?;
        if embedded_text.trim().is_empty() {
            blank_pages.push(*page_index);
        }
        pages.push((*page_index, embedded_text));
    }
    if blank_pages.len() == pages.len() {
        return Err(ConversionError::OcrNotSupported { pages: blank_pages });
    }

    let (file_name, contents) = match request.format {
        ConversionFormat::Text => (PathBuf::from("converted.txt"), text::plain_text(&pages)),
        ConversionFormat::Markdown => (PathBuf::from("converted.md"), text::markdown(&pages)),
        ConversionFormat::Png | ConversionFormat::Jpeg => unreachable!("text formats only"),
    };
    write_new_file(&staging.join(&file_name), contents.as_bytes())?;
    let warnings = if blank_pages.is_empty() {
        Vec::new()
    } else {
        vec![ConversionWarning::PagesWithoutEmbeddedText(blank_pages)]
    };
    Ok((vec![file_name], warnings))
}

fn convert_images(
    document: &dyn crate::backend::PdfDocument,
    image_encoder: &dyn ImageEncoder,
    request: &ConversionRequest,
    staging: &Path,
) -> Result<(Vec<PathBuf>, Vec<ConversionWarning>), ConversionError> {
    let (extension, encoded_format) = match request.format {
        ConversionFormat::Png => ("png", EncodedImageFormat::Png),
        ConversionFormat::Jpeg => (
            "jpg",
            EncodedImageFormat::Jpeg {
                quality: JPEG_QUALITY,
            },
        ),
        ConversionFormat::Text | ConversionFormat::Markdown => unreachable!("image formats only"),
    };
    let dpi = request.dpi.get();
    let mut file_names = Vec::with_capacity(request.pages.len());
    for page_index in &request.pages {
        check_cancel(&request.cancellation)?;
        let (width_points, height_points) = document.page_dimensions(*page_index)?;
        let (width, height) = raster_dimensions(width_points, height_points, dpi)?;
        let bitmap = document.render_page(*page_index, width, height, Rotation::Degrees0)?;
        check_cancel(&request.cancellation)?;
        let display_page = page_index.get().checked_add(1).unwrap_or(page_index.get());
        let file_name = PathBuf::from(format!("page-{display_page:04}.{extension}"));
        image_encoder
            .encode_rgba(&staging.join(&file_name), &bitmap, encoded_format, dpi)
            .map_err(|source| ConversionError::ImageEncoding {
                page_index: *page_index,
                source,
            })?;
        check_cancel(&request.cancellation)?;
        file_names.push(file_name);
    }
    Ok((file_names, Vec::new()))
}

fn raster_dimensions(
    width_points: f32,
    height_points: f32,
    dpi: u16,
) -> Result<(u32, u32), ConversionError> {
    fn pixels(points: f32, dpi: u16) -> Option<u32> {
        let value = f64::from(points) * f64::from(dpi) / POINTS_PER_INCH;
        if !value.is_finite() || value <= 0.0 || value > f64::from(u32::MAX) {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(value.ceil() as u32)
    }

    let width = pixels(width_points, dpi).ok_or_else(|| {
        ConversionError::Pdf(PdfError::InvalidPdfReason(
            "invalid PDF page width for conversion".into(),
        ))
    })?;
    let height = pixels(height_points, dpi).ok_or_else(|| {
        ConversionError::Pdf(PdfError::InvalidPdfReason(
            "invalid PDF page height for conversion".into(),
        ))
    })?;
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| ConversionError::Pdf(PdfError::OutOfMemoryBudget(usize::MAX)))?;
    let budget = MemoryBudget::DEFAULT_BYTES as u64;
    if bytes > budget {
        return Err(ConversionError::Pdf(PdfError::OutOfMemoryBudget(
            usize::try_from(bytes).unwrap_or(usize::MAX),
        )));
    }
    Ok((width, height))
}

fn check_cancel(cancellation: &CancellationToken) -> Result<(), ConversionError> {
    if cancellation.is_cancelled() {
        Err(ConversionError::Cancelled)
    } else {
        Ok(())
    }
}

fn safe_output_stem(source: &Path) -> String {
    let mut stem = source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(80)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    while stem.ends_with(['.', ' ', '_']) {
        stem.pop();
    }
    if stem.is_empty() {
        "document".into()
    } else {
        stem
    }
}

fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), ConversionError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| io_failure("could not create staged conversion file", source))?;
    file.write_all(contents)
        .map_err(|source| io_failure("could not write staged conversion file", source))?;
    file.sync_all()
        .map_err(|source| io_failure("could not flush staged conversion file", source))
}

fn io_failure(operation: &'static str, source: io::Error) -> ConversionError {
    ConversionError::Io { operation, source }
}

struct StagingDirectory {
    path: Option<PathBuf>,
    output_parent: PathBuf,
    output_stem: String,
}

impl StagingDirectory {
    fn create(output_parent: &Path, output_stem: &str) -> Result<Self, ConversionError> {
        for _ in 0..MAX_UNIQUE_ATTEMPTS {
            let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let name = format!(
                ".{output_stem}-converted.staging-{}-{sequence}",
                std::process::id()
            );
            let path = output_parent.join(name);
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path: Some(path),
                        output_parent: output_parent.to_owned(),
                        output_stem: output_stem.to_owned(),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(io_failure(
                        "could not create conversion staging directory",
                        source,
                    ));
                }
            }
        }
        Err(io_failure(
            "could not create conversion staging directory",
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "unique name attempts exhausted",
            ),
        ))
    }

    fn path(&self) -> &Path {
        self.path
            .as_deref()
            .expect("staging path exists until publication")
    }

    fn publish(mut self) -> Result<PathBuf, ConversionError> {
        for suffix in 1..=MAX_UNIQUE_ATTEMPTS {
            let name = if suffix == 1 {
                format!("{}-converted", self.output_stem)
            } else {
                format!("{}-converted-{suffix}", self.output_stem)
            };
            let destination = self.output_parent.join(name);
            if destination.exists() {
                continue;
            }
            let staging = self
                .path
                .as_ref()
                .expect("staging path exists until publication");
            match fs::rename(staging, &destination) {
                Ok(()) => {
                    self.path = None;
                    return Ok(destination);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(io_failure(
                        "could not publish converted output atomically",
                        source,
                    ));
                }
            }
        }
        Err(io_failure(
            "could not publish converted output atomically",
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "unique name attempts exhausted",
            ),
        ))
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{raster_dimensions, safe_output_stem, JobPassword};
    use barepdf_core::{MemoryBudget, PdfError};
    use std::path::Path;

    #[test]
    fn raster_dimensions_enforce_the_existing_memory_budget() {
        assert_eq!(raster_dimensions(72.0, 144.0, 150).unwrap(), (150, 300));
        let error = raster_dimensions(10_000.0, 10_000.0, 300).unwrap_err();
        assert!(matches!(
            error,
            super::ConversionError::Pdf(PdfError::OutOfMemoryBudget(bytes))
                if bytes > MemoryBudget::DEFAULT_BYTES
        ));
    }

    #[test]
    fn output_stem_is_bounded_and_does_not_preserve_path_punctuation() {
        let stem = safe_output_stem(Path::new("quarter:report?.pdf"));
        assert_eq!(stem, "quarter_report");
    }

    #[test]
    fn clearing_password_overwrites_its_utf8_buffer() {
        let mut password = JobPassword::new("sensitive".to_owned());
        password.bytes.fill(0);
        assert!(password.bytes.iter().all(|byte| *byte == 0));
    }
}
