use std::path::Path;
use std::time::Duration;

use barepdf_core::types::{GlyphRect, PageCount, PageIndex, PageTextGeometry, Rotation};
use barepdf_core::{DocumentId, MemoryBudget, PdfError};
use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
use barepdf_render::protocol::{PageRange, RenderCommand, RenderEvent, SearchMatchRecord};
use barepdf_render::RenderScheduler;

struct MockSearchBackend;
struct MockSearchDocument;

impl PdfBackend for MockSearchBackend {
    fn open_path(
        &self,
        _path: &Path,
        _password: Option<&str>,
    ) -> Result<Box<dyn PdfDocument>, PdfError> {
        Ok(Box::new(MockSearchDocument))
    }

    fn open_bytes(
        &self,
        _bytes: Vec<u8>,
        _password: Option<&str>,
    ) -> Result<Box<dyn PdfDocument>, PdfError> {
        Ok(Box::new(MockSearchDocument))
    }
}

impl PdfDocument for MockSearchDocument {
    fn page_count(&self) -> Result<PageCount, PdfError> {
        Ok(PageCount::new(3).expect("non-zero"))
    }

    fn page_dimensions(&self, _page_index: PageIndex) -> Result<(f32, f32), PdfError> {
        Ok((100.0, 100.0))
    }

    fn render_page(
        &self,
        _page_index: PageIndex,
        _target_width: u32,
        _target_height: u32,
        _rotation: Rotation,
    ) -> Result<RawBitmap, PdfError> {
        RawBitmap::new(1, 1, vec![0; 4]).map_err(|_| PdfError::RenderingFailed {
            page_index: 0,
            reason: "invalid bitmap".into(),
        })
    }

    fn extract_text(&self, _page_index: PageIndex) -> Result<String, PdfError> {
        Ok(String::new())
    }

    fn extract_text_spans(&self, _page_index: PageIndex) -> Result<Vec<TextSpan>, PdfError> {
        Ok(Vec::new())
    }

    fn get_page_text_geometry(&self, page_index: PageIndex) -> Result<PageTextGeometry, PdfError> {
        let text = match page_index.get() {
            0 => "Hello World",
            1 => "Rust Systems Programming",
            _ => "World of PDF",
        };
        let glyphs = text
            .chars()
            .enumerate()
            .map(|(i, ch)| GlyphRect {
                x: i as f32 * 10.0,
                y: 20.0,
                width: 8.0,
                height: 12.0,
                ch,
            })
            .collect();
        Ok(PageTextGeometry {
            page_index,
            glyphs,
            links: Vec::new(),
        })
    }

    fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError> {
        Ok(Vec::new())
    }

    fn annotations(
        &self,
        _page: PageIndex,
    ) -> Result<Vec<barepdf_pdf::AnnotationRecord>, PdfError> {
        Ok(Vec::new())
    }
}

#[test]
fn search_document_flow_across_entire_document() {
    let scheduler = RenderScheduler::spawn(MockSearchBackend, MemoryBudget::new(1024 * 1024));
    let doc_id = DocumentId::new(42);

    assert!(scheduler.send_command(RenderCommand::OpenDocument {
        document_id: doc_id,
        path: "mock.pdf".into(),
        password: None,
    }));

    let mut opened = false;
    for _ in 0..100 {
        if let Ok(Some(RenderEvent::DocumentOpened { document_id, .. })) =
            scheduler.try_recv_event()
        {
            if document_id == doc_id {
                opened = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(opened, "document should be opened");

    // Search for "World" across all pages
    assert!(scheduler.send_command(RenderCommand::SearchDocument {
        document_id: doc_id,
        query: "World".to_string(),
        range: None,
    }));

    let mut results: Option<Vec<SearchMatchRecord>> = None;
    for _ in 0..100 {
        if let Ok(Some(RenderEvent::SearchResults {
            document_id,
            query,
            matches,
        })) = scheduler.try_recv_event()
        {
            if document_id == doc_id && query == "World" {
                results = Some(matches);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let matches = results.expect("should receive search results");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].page_index.get(), 0);
    assert_eq!(matches[0].match_index_in_doc, 0);
    assert_eq!(matches[0].glyph_boxes.len(), 5);
    assert_eq!(matches[1].page_index.get(), 2);
    assert_eq!(matches[1].match_index_in_doc, 1);
    assert_eq!(matches[1].glyph_boxes.len(), 5);
}

#[test]
fn search_document_flow_with_page_range() {
    let scheduler = RenderScheduler::spawn(MockSearchBackend, MemoryBudget::new(1024 * 1024));
    let doc_id = DocumentId::new(43);

    assert!(scheduler.send_command(RenderCommand::OpenDocument {
        document_id: doc_id,
        path: "mock.pdf".into(),
        password: None,
    }));

    for _ in 0..100 {
        if let Ok(Some(RenderEvent::DocumentOpened { document_id, .. })) =
            scheduler.try_recv_event()
        {
            if document_id == doc_id {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    // Search only page 0
    assert!(scheduler.send_command(RenderCommand::SearchDocument {
        document_id: doc_id,
        query: "World".to_string(),
        range: Some(PageRange::new(0, 1)),
    }));

    let mut results: Option<Vec<SearchMatchRecord>> = None;
    for _ in 0..100 {
        if let Ok(Some(RenderEvent::SearchResults {
            document_id,
            query,
            matches,
        })) = scheduler.try_recv_event()
        {
            if document_id == doc_id && query == "World" {
                results = Some(matches);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let matches = results.expect("should receive search results");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].page_index.get(), 0);
}

#[test]
fn search_document_flow_empty_query_returns_empty_results() {
    let scheduler = RenderScheduler::spawn(MockSearchBackend, MemoryBudget::new(1024 * 1024));
    let doc_id = DocumentId::new(44);

    assert!(scheduler.send_command(RenderCommand::OpenDocument {
        document_id: doc_id,
        path: "mock.pdf".into(),
        password: None,
    }));

    for _ in 0..100 {
        if let Ok(Some(RenderEvent::DocumentOpened { document_id, .. })) =
            scheduler.try_recv_event()
        {
            if document_id == doc_id {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(scheduler.send_command(RenderCommand::SearchDocument {
        document_id: doc_id,
        query: "".to_string(),
        range: None,
    }));

    let mut results: Option<Vec<SearchMatchRecord>> = None;
    for _ in 0..100 {
        if let Ok(Some(RenderEvent::SearchResults {
            document_id,
            query,
            matches,
        })) = scheduler.try_recv_event()
        {
            if document_id == doc_id && query.is_empty() {
                results = Some(matches);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let matches = results.expect("should receive empty search results");
    assert!(matches.is_empty());
}

#[test]
fn search_document_flow_no_matches_found() {
    let scheduler = RenderScheduler::spawn(MockSearchBackend, MemoryBudget::new(1024 * 1024));
    let doc_id = DocumentId::new(45);

    assert!(scheduler.send_command(RenderCommand::OpenDocument {
        document_id: doc_id,
        path: "mock.pdf".into(),
        password: None,
    }));

    for _ in 0..100 {
        if let Ok(Some(RenderEvent::DocumentOpened { document_id, .. })) =
            scheduler.try_recv_event()
        {
            if document_id == doc_id {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(scheduler.send_command(RenderCommand::SearchDocument {
        document_id: doc_id,
        query: "NonExistentTerm".to_string(),
        range: None,
    }));

    let mut results: Option<Vec<SearchMatchRecord>> = None;
    for _ in 0..100 {
        if let Ok(Some(RenderEvent::SearchResults {
            document_id,
            query,
            matches,
        })) = scheduler.try_recv_event()
        {
            if document_id == doc_id && query == "NonExistentTerm" {
                results = Some(matches);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let matches = results.expect("should receive search results");
    assert!(matches.is_empty());
}

#[test]
fn page_range_conversion_and_api() {
    let range = PageRange::new(5, 15);
    assert_eq!(range.start, 5);
    assert_eq!(range.end, 15);

    let std_range: std::ops::Range<u32> = range.into();
    assert_eq!(std_range, 5..15);

    let from_std: PageRange = (10..20).into();
    assert_eq!(from_std.start, 10);
    assert_eq!(from_std.end, 20);
}
