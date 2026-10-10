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

#[test]
fn add_free_text_annotation_adds_to_document_annotations() {
    use crate::application::{DocumentState, ReadyDocument};
    use crate::presentation::callbacks::drawing::add_free_text_annotation;
    use crate::presentation::state::AppState;
    use barepdf_core::{DocumentId, PageCount, UserPreferences};
    use std::time::Instant;

    let mut app = AppState::new(UserPreferences::default());
    let doc_id = DocumentId::new(1);
    app.application
        .tabs
        .open(PathBuf::from("test.pdf"), "test.pdf".to_string());
    if let Some(tab) = app.application.tabs.active_mut() {
        tab.document = Some(DocumentState::Ready(ReadyDocument {
            id: doc_id,
            path: PathBuf::from("test.pdf"),
            page_count: PageCount::new(1).unwrap(),
            started_at: Instant::now(),
        }));
    }
    app.first_page_dimensions = (600.0, 800.0);
    app.page_dimensions = std::sync::Arc::new(vec![(600.0, 800.0)]);

    add_free_text_annotation(
        &mut app,
        0,
        0.1,
        0.2,
        "Hello Typewriter".to_string(),
        14.0,
        [0, 0, 0, 255],
    );

    let ann = app.annotations.get(&doc_id).expect("annotations exist");
    assert_eq!(ann.free_texts.len(), 1);
    let ft = &ann.free_texts[0];
    assert_eq!(ft.text, "Hello Typewriter");
    assert_eq!(ft.page_index, 0);
    assert!((ft.x - 60.0).abs() < 1.0);
    assert!((ft.y - 640.0).abs() < 1.0);
}

#[test]
fn app_state_file_watcher_lifecycle_and_channel() {
    use crate::presentation::state::AppState;
    use barepdf_core::UserPreferences;

    let mut app = AppState::new(UserPreferences::default());
    assert!(app.file_watcher.is_none());

    let temp_file = tempfile::NamedTempFile::new().expect("temp file");
    app.start_file_watcher(temp_file.path());
    assert!(app.file_watcher.is_some());

    app.notify_file_changed(temp_file.path().to_path_buf());
    assert_eq!(
        app.try_recv_file_change(),
        Some(temp_file.path().to_path_buf())
    );

    app.stop_file_watcher();
    assert!(app.file_watcher.is_none());
}

#[test]
fn drawing_tool_selection_is_mutually_exclusive() {
    crate::presentation::test_support::run_on_ui_thread(|| {
        use crate::presentation::callbacks::drawing::sync_drawing_tool_ui;
        use crate::presentation::state::{AppState, DrawingTool};
        use barepdf_core::UserPreferences;

        let mut app = AppState::new(UserPreferences::default());
        let window = barepdf_ui::AppWindow::new().expect("slint app window");

        // Initially pan_mode is true
        assert!(app.pan_mode);
        assert!(!app.drawing_eraser);
        assert!(!app.drawing_typewriter);
        sync_drawing_tool_ui(&app, &window);
        assert!(window.get_pan_mode_active());
        assert!(!window.get_drawing_eraser_active());
        assert!(!window.get_drawing_typewriter_active());

        // Activate Pen -> Pan/Eraser/Typewriter false
        app.activate_drawing_tool(DrawingTool::Pen);
        assert!(!app.pan_mode);
        assert!(!app.drawing_eraser);
        assert!(!app.drawing_typewriter);
        sync_drawing_tool_ui(&app, &window);
        assert!(!window.get_pan_mode_active());
        assert!(!window.get_drawing_eraser_active());
        assert!(!window.get_drawing_typewriter_active());

        // Activate Eraser -> Eraser true, Pan/Typewriter false
        app.activate_drawing_tool(DrawingTool::Eraser);
        assert!(!app.pan_mode);
        assert!(app.drawing_eraser);
        assert!(!app.drawing_typewriter);
        sync_drawing_tool_ui(&app, &window);
        assert!(!window.get_pan_mode_active());
        assert!(window.get_drawing_eraser_active());
        assert!(!window.get_drawing_typewriter_active());

        // Activate Typewriter -> Typewriter true, Pan/Eraser false
        app.activate_drawing_tool(DrawingTool::Typewriter);
        assert!(!app.pan_mode);
        assert!(!app.drawing_eraser);
        assert!(app.drawing_typewriter);
        sync_drawing_tool_ui(&app, &window);
        assert!(!window.get_pan_mode_active());
        assert!(!window.get_drawing_eraser_active());
        assert!(window.get_drawing_typewriter_active());

        // Activate Pan -> Pan true, Eraser/Typewriter false
        app.activate_drawing_tool(DrawingTool::Pan);
        assert!(app.pan_mode);
        assert!(!app.drawing_eraser);
        assert!(!app.drawing_typewriter);
        sync_drawing_tool_ui(&app, &window);
        assert!(window.get_pan_mode_active());
        assert!(!window.get_drawing_eraser_active());
        assert!(!window.get_drawing_typewriter_active());
    });
}

#[test]
fn select_drawing_tool_callback_switches_all_tools() {
    crate::presentation::test_support::run_on_ui_thread(|| {
        use crate::presentation::callbacks::drawing::sync_drawing_tool_ui;
        use crate::presentation::state::{AppState, DrawingTool};
        use barepdf_core::UserPreferences;
        use slint::ComponentHandle;

        let state = std::rc::Rc::new(std::cell::RefCell::new(AppState::new(
            UserPreferences::default(),
        )));
        let window = barepdf_ui::AppWindow::new().expect("slint app window");

        let weak = window.as_weak();
        let state_tool = state.clone();
        window.on_select_drawing_tool(move |tool_id| {
            let mut app = state_tool.borrow_mut();
            match tool_id {
                0 => app.activate_drawing_tool(DrawingTool::Pan),
                1 => app.activate_drawing_tool(DrawingTool::Pen),
                2 => app.activate_drawing_tool(DrawingTool::Eraser),
                3 => app.activate_drawing_tool(DrawingTool::Typewriter),
                _ => app.activate_drawing_tool(DrawingTool::Pen),
            }
            if let Some(window) = weak.upgrade() {
                sync_drawing_tool_ui(&app, &window);
            }
        });

        // Select Pan (0)
        window.invoke_select_drawing_tool(0);
        assert!(window.get_pan_mode_active());
        assert!(!window.get_drawing_eraser_active());
        assert!(!window.get_drawing_typewriter_active());

        // Select Pen (1)
        window.invoke_select_drawing_tool(1);
        assert!(!window.get_pan_mode_active());
        assert!(!window.get_drawing_eraser_active());
        assert!(!window.get_drawing_typewriter_active());

        // Select Eraser (2)
        window.invoke_select_drawing_tool(2);
        assert!(!window.get_pan_mode_active());
        assert!(window.get_drawing_eraser_active());
        assert!(!window.get_drawing_typewriter_active());

        // Select Typewriter (3)
        window.invoke_select_drawing_tool(3);
        assert!(!window.get_pan_mode_active());
        assert!(!window.get_drawing_eraser_active());
        assert!(window.get_drawing_typewriter_active());
    });
}

#[test]
fn typewriter_pointer_down_opens_dialog_and_custom_text_note_inserts() {
    crate::presentation::test_support::run_on_ui_thread(|| {
        use crate::application::{DocumentState, ReadyDocument};
        use crate::presentation::callbacks::drawing::{
            add_free_text_annotation, sync_drawing_tool_ui,
        };
        use crate::presentation::state::{AppState, DrawingTool};
        use barepdf_core::{DocumentId, InkColor, PageCount, PageIndex, UserPreferences};
        use slint::ComponentHandle;
        use std::path::PathBuf;
        use std::time::Instant;

        let state = std::rc::Rc::new(std::cell::RefCell::new(AppState::new(
            UserPreferences::default(),
        )));
        let doc_id = DocumentId::new(42);
        {
            let mut app = state.borrow_mut();
            app.application
                .tabs
                .open(PathBuf::from("doc.pdf"), "doc.pdf".to_string());
            if let Some(tab) = app.application.tabs.active_mut() {
                tab.document = Some(DocumentState::Ready(ReadyDocument {
                    id: doc_id,
                    path: PathBuf::from("doc.pdf"),
                    page_count: PageCount::new(2).unwrap(),
                    started_at: Instant::now(),
                }));
            }
            app.first_page_dimensions = (600.0, 800.0);
            app.page_dimensions = std::sync::Arc::new(vec![(600.0, 800.0), (600.0, 800.0)]);
            app.activate_drawing_tool(DrawingTool::Typewriter);
            app.drawing_color = InkColor::Blue;
        }

        let window = barepdf_ui::AppWindow::new().expect("slint app window");
        sync_drawing_tool_ui(&state.borrow(), &window);
        assert!(window.get_drawing_typewriter_active());

        let weak = window.as_weak();
        let state_draw_down = state.clone();
        window.on_drawing_pointer_down(move |page, nx, ny| {
            if page < 0 {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let app = state_draw_down.borrow();
            let Some(_doc_id) = app.active_document() else {
                return;
            };
            let page_idx = PageIndex::from_raw(page as u32);
            let pt = (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0));
            if app.pan_mode {
                return;
            }
            if app.drawing_typewriter {
                let color_index = match app.drawing_color {
                    barepdf_core::InkColor::Black => 0,
                    barepdf_core::InkColor::Red => 1,
                    barepdf_core::InkColor::Blue => 2,
                    barepdf_core::InkColor::Yellow => 3,
                };
                window.set_text_note_page_index(page_idx.get() as i32);
                window.set_text_note_norm_x(pt.0);
                window.set_text_note_norm_y(pt.1);
                window.set_text_note_content("".into());
                window.set_text_note_color_index(color_index);
                window.set_text_note_dialog_open(true);
            }
        });

        let weak = window.as_weak();
        let state_free_text = state.clone();
        window.on_request_add_free_text(move |page_idx, nx, ny, text, font_size, color_index| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return;
            }
            let mut app = state_free_text.borrow_mut();
            app.activate_drawing_tool(DrawingTool::Typewriter);
            sync_drawing_tool_ui(&app, &window);
            let color = match color_index {
                1 => [230, 50, 50, 255],
                2 => [30, 100, 220, 255],
                3 => [220, 180, 20, 255],
                _ => [0, 0, 0, 255],
            };
            add_free_text_annotation(
                &mut app,
                page_idx as usize,
                nx,
                ny,
                text.to_string(),
                font_size,
                color,
            );
        });

        // Pointer down on page 1 at (0.25, 0.40)
        assert!(!window.get_text_note_dialog_open());
        window.invoke_drawing_pointer_down(1, 0.25, 0.40);

        // Dialog should now be open with page 1, coords, and Blue color (index 2)
        assert!(window.get_text_note_dialog_open());
        assert_eq!(window.get_text_note_page_index(), 1);
        assert!((window.get_text_note_norm_x() - 0.25).abs() < 1e-4);
        assert!((window.get_text_note_norm_y() - 0.40).abs() < 1e-4);
        assert_eq!(window.get_text_note_color_index(), 2);
        assert!(!state.borrow().annotations.contains_key(&doc_id));

        // Submit custom note
        window.invoke_request_add_free_text(1, 0.25, 0.40, "Custom Reviewed Text".into(), 18.0, 2);

        let app = state.borrow();
        let ann = app.annotations.get(&doc_id).expect("annotations exist");
        assert_eq!(ann.free_texts.len(), 1);
        let ft = &ann.free_texts[0];
        assert_eq!(ft.page_index, 1);
        assert_eq!(ft.text, "Custom Reviewed Text");
        assert!((ft.font_size - 18.0).abs() < 1e-4);
        assert_eq!(ft.color_rgba, [30, 100, 220, 255]);
    });
}
