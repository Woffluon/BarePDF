use super::{
    consume_ui_password,
    drawing::{erase_strokes_near, selection_to_highlight_quads},
    navigation::{is_safe_external_link_url, parse_zoom_percent},
    print::{parse_print_preview_range, print_preview_dimensions, DeferredPrinterSink},
    tools::{selected_tool_pages, toggle_tool_page_selection, tool_drop_paths},
};
use crate::presentation::state::{PrintPreviewState, PRINT_PREVIEW_REQUEST_MASK};
use barepdf_core::{
    DocumentId, GlyphRect, InkColor, InkStroke, PageCount, PageIndex, PageTextGeometry, RequestId,
    Rotation, MAX_PASSWORD_BYTES,
};
use barepdf_platform::printing::{PrintError, PrintJobId, PrintPage, PrinterSink};
use slint::SharedString;
use std::path::PathBuf;

fn page_count(value: u32) -> PageCount {
    PageCount::new(value).expect("test page count")
}

#[test]
fn print_preview_initializes_duplex_to_single_sided() {
    let document = DocumentId::new(7);
    let mut preview = PrintPreviewState::default();
    preview.open(document, 11, page_count(4), PageIndex::zero());
    assert_eq!(preview.duplex, 0);
}

#[test]
fn print_preview_accepts_only_the_latest_matching_render_once() {
    let document = DocumentId::new(7);
    let mut preview = PrintPreviewState::default();
    preview.open(document, 11, page_count(4), PageIndex::zero());
    let first = RequestId::new(PRINT_PREVIEW_REQUEST_MASK | 1);
    let latest = RequestId::new(PRINT_PREVIEW_REQUEST_MASK | 2);
    preview.expect_render(first, PageIndex::zero());
    preview.set_page(1);
    preview.expect_render(latest, PageIndex::from_raw(1));

    assert!(!preview.accept_render(first, document, 11, PageIndex::zero()));
    assert!(!preview.accept_render(latest, DocumentId::new(8), 11, PageIndex::from_raw(1)));
    assert!(!preview.accept_render(latest, document, 12, PageIndex::from_raw(1)));
    assert!(preview.accept_render(latest, document, 11, PageIndex::from_raw(1)));
    assert!(!preview.accept_render(latest, document, 11, PageIndex::from_raw(1)));
}

#[test]
fn closing_print_preview_rejects_an_in_flight_render() {
    let document = DocumentId::new(7);
    let mut preview = PrintPreviewState::default();
    preview.open(document, 11, page_count(2), PageIndex::zero());
    let request = RequestId::new(PRINT_PREVIEW_REQUEST_MASK | 3);
    preview.expect_render(request, PageIndex::zero());

    preview.close();

    assert!(!preview.accept_render(request, document, 11, PageIndex::zero()));
}

#[test]
fn print_preview_range_is_contiguous_and_bounded() {
    assert_eq!(
        parse_print_preview_range("", page_count(5)),
        Some((PageIndex::zero(), PageIndex::from_raw(4)))
    );
    assert_eq!(
        parse_print_preview_range("2-4", page_count(5)),
        Some((PageIndex::from_raw(1), PageIndex::from_raw(3)))
    );
    assert_eq!(
        parse_print_preview_range("3", page_count(5)),
        Some((PageIndex::from_raw(2), PageIndex::from_raw(2)))
    );
    assert_eq!(
        parse_print_preview_range("1-2, 3-4", page_count(5)),
        Some((PageIndex::zero(), PageIndex::from_raw(3)))
    );
    assert_eq!(parse_print_preview_range("4-2", page_count(5)), None);
    assert_eq!(parse_print_preview_range("1,3", page_count(5)), None);
    assert_eq!(parse_print_preview_range("1-3,5", page_count(5)), None);
    assert_eq!(parse_print_preview_range("6", page_count(5)), None);
}

#[test]
fn print_preview_bitmap_fits_the_existing_thumbnail_budget() {
    let (width, height) = print_preview_dimensions((2_000.0, 1_000.0), Rotation::Degrees90);
    let bytes = u64::from(width) * u64::from(height) * 4;

    assert_eq!((width, height), (480, 960));
    assert!(bytes <= super::super::ui::THUMB_IMAGE_BUDGET as u64);
}

#[test]
fn parses_and_clamps_integer_zoom_percentages() {
    for (input, expected) in [
        ("125", Some(125)),
        (" 125% ", Some(125)),
        ("125 %", Some(125)),
        ("+125", Some(125)),
        ("0", Some(25)),
        ("-25", Some(25)),
        ("250", Some(250)),
        ("800", Some(800)),
        ("999", Some(800)),
    ] {
        assert_eq!(parse_zoom_percent(input), expected, "{input}");
    }
}

#[test]
fn rejects_non_integer_or_malformed_zoom_percentages() {
    for input in ["", "%", "125.0", "125%%", "abc", "+ 125"] {
        assert_eq!(parse_zoom_percent(input), None, "{input}");
    }
}

#[test]
fn tool_drop_uses_only_pdf_sources_without_opening_them() {
    assert_eq!(
        tool_drop_paths("C:/docs/one.pdf\nC:/docs/two.PDF\nC:/docs/note.txt"),
        vec![
            PathBuf::from("C:/docs/one.pdf"),
            PathBuf::from("C:/docs/two.PDF")
        ]
    );
}

#[test]
fn ctrl_selection_expands_existing_ranges_before_toggling_a_page() {
    assert_eq!(selected_tool_pages("1-3, 5", 5), vec![1, 2, 3, 5]);
    assert!(selected_tool_pages("6", 5).is_empty());
}

#[test]
fn toggle_tool_page_selection_toggles_pages_without_ctrl() {
    assert_eq!(toggle_tool_page_selection("1, 3", 2, 5, false), "1, 2, 3");
    assert_eq!(toggle_tool_page_selection("", 1, 5, false), "1");
    assert_eq!(toggle_tool_page_selection("1-3", 5, 5, false), "1, 2, 3, 5");

    assert_eq!(toggle_tool_page_selection("1-3, 5", 2, 5, false), "1, 3, 5");
    assert_eq!(toggle_tool_page_selection("2", 2, 5, false), "");
}

#[test]
fn toggle_tool_page_selection_handles_shift_range_selection() {
    assert_eq!(toggle_tool_page_selection("2", 5, 5, true), "2-5");
    assert_eq!(toggle_tool_page_selection("5", 2, 5, true), "2-5");
    assert_eq!(toggle_tool_page_selection("2-4", 6, 10, true), "2-6");
    assert_eq!(toggle_tool_page_selection("", 3, 5, true), "3-3");
}

#[test]
fn external_link_url_scheme_validation_rejects_unsafe_schemes() {
    assert!(is_safe_external_link_url("https://example.com/doc"));
    assert!(is_safe_external_link_url("http://example.com"));
    assert!(is_safe_external_link_url("mailto:user@example.com"));
    assert!(!is_safe_external_link_url(
        "file:///C:/Windows/System32/cmd.exe"
    ));
    assert!(!is_safe_external_link_url("javascript:alert(1)"));
}

#[test]
fn selection_to_highlight_quads_merges_adjacent_glyphs_on_same_line() {
    let geom = PageTextGeometry {
        page_index: PageIndex::zero(),
        glyphs: vec![
            GlyphRect {
                ch: 'A',
                x: 10.0,
                y: 100.0,
                width: 8.0,
                height: 12.0,
            },
            GlyphRect {
                ch: 'B',
                x: 18.0,
                y: 100.0,
                width: 8.0,
                height: 12.0,
            },
        ],
        links: Vec::new(),
    };
    let quads = selection_to_highlight_quads(&geom, PageIndex::zero(), 0, 2, 200.0, 200.0);
    assert_eq!(quads.len(), 1);
    assert!((quads[0].x_norm - 0.05).abs() < 1e-4);
    assert!((quads[0].w_norm - 0.08).abs() < 1e-4);
}

#[test]
fn erase_strokes_near_removes_only_matching_page_stroke_within_radius() {
    let mut strokes = vec![
        InkStroke {
            page: PageIndex::zero(),
            points: vec![(0.2, 0.2), (0.3, 0.3)],
            color: InkColor::Black,
            width_pts: 4.0,
        },
        InkStroke {
            page: PageIndex::from_raw(1),
            points: vec![(0.2, 0.2)],
            color: InkColor::Red,
            width_pts: 4.0,
        },
    ];
    assert!(erase_strokes_near(
        &mut strokes,
        PageIndex::zero(),
        0.21,
        0.21,
        0.035
    ));
    assert_eq!(strokes.len(), 1);
    assert_eq!(strokes[0].page, PageIndex::from_raw(1));
}

#[test]
fn consume_ui_password_wraps_in_secret_and_rejects_oversized_inputs() {
    let mut valid = consume_ui_password(SharedString::from("unlock-123"))
        .expect("valid password within byte limit");
    assert_eq!(valid.expose(), "unlock-123");
    valid.clear();
    assert!(valid.bytes_for_test().is_empty());

    let oversized = SharedString::from("x".repeat(MAX_PASSWORD_BYTES + 1));
    assert!(consume_ui_password(oversized).is_err());
}

#[test]
fn deferred_printer_sink_defers_spooler_initialization_until_worker_begin() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    struct DummySink(PrintJobId);
    impl PrinterSink for DummySink {
        fn job_id(&self) -> PrintJobId {
            self.0
        }
        fn target_dpi(&self) -> u16 {
            300
        }
        fn begin(&mut self, _title: &str) -> Result<(), PrintError> {
            Ok(())
        }
        fn write_page(&mut self, _page: PrintPage<'_>) -> Result<(), PrintError> {
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<(), PrintError> {
            Ok(())
        }
    }

    let job_id = PrintJobId::new(42).expect("valid job id");
    let initialized = Arc::new(AtomicBool::new(false));
    let initialized_for_factory = initialized.clone();

    let mut sink: Box<dyn PrinterSink> =
        Box::new(DeferredPrinterSink::new(job_id, 300, move || {
            initialized_for_factory.store(true, Ordering::SeqCst);
            Ok(Box::new(DummySink(job_id)))
        }));

    assert!(!initialized.load(Ordering::SeqCst));
    assert_eq!(sink.job_id(), job_id);
    assert_eq!(sink.target_dpi(), 300);

    assert!(sink.begin("doc.pdf").is_ok());
    assert!(initialized.load(Ordering::SeqCst));
    assert!(sink.finish().is_ok());
}

#[test]
fn wave2_drawing_segment_eraser_removes_strokes_along_drag_path() {
    use crate::presentation::state::ERASER_RADII;
    use barepdf_core::{erase_ink_strokes_along_segment, InkColor, InkStroke, PageIndex};

    let page = PageIndex::zero();
    let mut strokes = vec![
        InkStroke {
            page,
            points: vec![(0.5, 0.1), (0.5, 0.9)],
            color: InkColor::Black,
            width_pts: 2.0,
        },
        InkStroke {
            page,
            points: vec![(0.1, 0.1), (0.2, 0.2)],
            color: InkColor::Red,
            width_pts: 2.0,
        },
    ];

    let radius = ERASER_RADII[1];
    let modified =
        erase_ink_strokes_along_segment(&mut strokes, page, (0.4, 0.5), (0.6, 0.5), radius, radius);

    assert!(modified);
    assert_eq!(strokes.len(), 3);
}

#[test]
fn wave2_smooth_ink_points_subdivides_strokes() {
    use barepdf_core::smooth_ink_points;

    let points = vec![(0.0, 0.0), (0.5, 0.5), (1.0, 0.0)];
    let smoothed = smooth_ink_points(&points);
    assert!(smoothed.len() > points.len());
    assert_eq!(smoothed.first().copied(), Some((0.0, 0.0)));
    assert_eq!(smoothed.last().copied(), Some((1.0, 0.0)));
}
