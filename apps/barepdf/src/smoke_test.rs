use crate::app_error::AppError;
use barepdf_core::{PageIndex, PdfError, Rotation};
use barepdf_pdf::{PdfBackend, PdfiumEngine};
use std::ffi::OsStr;

const SMOKE_RENDER_WIDTH: u32 = 64;
const SMOKE_RENDER_HEIGHT: u32 = 64;

/// Minimal valid single-page PDF 1.4 document with exact byte xref offsets.
const MINIMAL_SMOKE_PDF: &[u8] = b"%PDF-1.4\n\
1 0 obj\n\
<< /Type /Catalog /Pages 2 0 R >>\n\
endobj\n\
2 0 obj\n\
<< /Type /Pages /Kids [3 0 R] /Count 1 >>\n\
endobj\n\
3 0 obj\n\
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << >> >>\n\
endobj\n\
xref\n\
0 4\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000058 00000 n \n\
0000000115 00000 n \n\
trailer\n\
<< /Size 4 /Root 1 0 R >>\n\
startxref\n\
201\n\
%%EOF\n";

pub(crate) fn has_smoke_test_flag<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == OsStr::new("--smoke-test"))
}

pub(crate) fn run() -> Result<(), AppError> {
    let engine = PdfiumEngine::new()?;
    run_with_backend(&engine)
}

fn run_with_backend(backend: &dyn PdfBackend) -> Result<(), AppError> {
    let document = backend.open_bytes(MINIMAL_SMOKE_PDF.to_vec(), None)?;
    let page_count = document.page_count()?.get();
    if page_count != 1 {
        return Err(PdfError::InvalidPdfReason(format!(
            "smoke test expected 1 page, got {page_count}"
        ))
        .into());
    }

    let bitmap = document.render_page(
        PageIndex::zero(),
        SMOKE_RENDER_WIDTH,
        SMOKE_RENDER_HEIGHT,
        Rotation::Degrees0,
    )?;
    if bitmap.width() == 0 || bitmap.height() == 0 || bitmap.pixels().is_empty() {
        return Err(PdfError::RenderingFailed {
            page_index: 0,
            reason: "smoke test rendered an empty bitmap".to_owned(),
        }
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{has_smoke_test_flag, run, run_with_backend};
    use crate::app_error::AppError;
    use barepdf_core::{PageCount, PageIndex, PageTextGeometry, PdfError, Rotation};
    use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
    use std::path::Path;

    struct StubBackend {
        pages: u32,
    }

    struct StubDocument {
        pages: u32,
    }

    impl PdfBackend for StubBackend {
        fn open_path(
            &self,
            path: &Path,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Err(PdfError::FileNotFound(path.display().to_string()))
        }

        fn open_bytes(
            &self,
            _bytes: Vec<u8>,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Ok(Box::new(StubDocument { pages: self.pages }))
        }
    }

    impl PdfDocument for StubDocument {
        fn page_count(&self) -> Result<PageCount, PdfError> {
            PageCount::new(self.pages)
                .ok_or_else(|| PdfError::InvalidPdfReason("invalid test page count".to_owned()))
        }

        fn page_dimensions(&self, _page_index: PageIndex) -> Result<(f32, f32), PdfError> {
            Ok((64.0, 64.0))
        }

        fn render_page(
            &self,
            page_index: PageIndex,
            target_width: u32,
            target_height: u32,
            _rotation: Rotation,
        ) -> Result<RawBitmap, PdfError> {
            let pixel_len = usize::try_from(target_width)
                .ok()
                .and_then(|w| usize::try_from(target_height).ok()?.checked_mul(w))
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| PdfError::RenderingFailed {
                    page_index: page_index.get(),
                    reason: "bitmap size overflow".to_owned(),
                })?;
            RawBitmap::new(target_width, target_height, vec![0; pixel_len]).map_err(|error| {
                PdfError::RenderingFailed {
                    page_index: page_index.get(),
                    reason: error.to_string(),
                }
            })
        }

        fn extract_text(&self, _page_index: PageIndex) -> Result<String, PdfError> {
            Ok(String::new())
        }

        fn extract_text_spans(&self, _page_index: PageIndex) -> Result<Vec<TextSpan>, PdfError> {
            Ok(Vec::new())
        }

        fn get_page_text_geometry(
            &self,
            page_index: PageIndex,
        ) -> Result<PageTextGeometry, PdfError> {
            Ok(PageTextGeometry {
                page_index,
                glyphs: Vec::new(),
                links: Vec::new(),
            })
        }

        fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn has_smoke_test_flag_matches_exact_cli_argument() {
        assert!(!has_smoke_test_flag(["barepdf"].into_iter().skip(1)));
        assert!(!has_smoke_test_flag(["--smoke-test"].into_iter().skip(1)));
        assert!(!has_smoke_test_flag(
            ["barepdf", "--log"].into_iter().skip(1)
        ));
        assert!(!has_smoke_test_flag(
            ["barepdf", "--smoke-test=true"].into_iter().skip(1)
        ));
        assert!(has_smoke_test_flag(
            ["barepdf", "--smoke-test"].into_iter().skip(1)
        ));
        assert!(has_smoke_test_flag(
            ["barepdf", "--log", "--smoke-test"].into_iter().skip(1)
        ));
    }

    #[test]
    fn run_with_backend_accepts_single_page_and_rejects_wrong_page_count() -> Result<(), AppError> {
        run_with_backend(&StubBackend { pages: 1 })?;

        let err = run_with_backend(&StubBackend { pages: 2 });
        assert!(matches!(
            err,
            Err(AppError::Pdf(PdfError::InvalidPdfReason(_)))
        ));
        Ok(())
    }

    #[test]
    fn run_executes_pdfium_smoke_test_successfully() -> Result<(), AppError> {
        run()
    }
}
