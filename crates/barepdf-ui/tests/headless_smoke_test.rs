use barepdf_ui::{AppWindow, BookmarkItem, PageItem, TabItem};
use slint::{Image, Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{LazyLock, Mutex};

/// Slint's default winit platform initializes a process-wide EventLoop on the first
/// thread that creates a window and cannot recreate it on other threads.
/// A dedicated worker thread allows multiple independent `#[test]` functions to
/// instantiate `AppWindow` headlessly (suspended, without showing an OS window or creating an OpenGL context).
struct UiTestThread {
    tx: std::sync::mpsc::SyncSender<Box<dyn FnOnce() + Send>>,
}

static UI_TEST_THREAD: LazyLock<Mutex<UiTestThread>> = LazyLock::new(|| {
    let (tx, rx) = std::sync::mpsc::sync_channel::<Box<dyn FnOnce() + Send>>(0);
    std::thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            job();
        }
    });
    Mutex::new(UiTestThread { tx })
});

fn run_on_ui_thread<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    let guard = UI_TEST_THREAD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    guard
        .tx
        .send(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            let _ = done_tx.send(result);
        }))
        .expect("UI test worker thread should be running");
    match done_rx
        .recv()
        .expect("UI test worker should report completion")
    {
        Ok(()) => {}
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
fn app_window_initializes_with_expected_defaults() {
    run_on_ui_thread(|| {
        let app = AppWindow::new().expect("headless AppWindow should initialize");

        assert!(!app.get_has_document());
        assert_eq!(app.get_zoom_str().as_str(), "100%");
        assert!(app.get_sidebar_visible());
        assert_eq!(app.get_current_page_str().as_str(), "1");
        assert_eq!(app.get_total_pages_str().as_str(), "0");
        assert_eq!(app.get_status_text().as_str(), "Ready");
        assert_eq!(app.get_tab_items().row_count(), 0);
        assert_eq!(app.get_visible_pages().row_count(), 0);
        assert_eq!(app.get_bookmark_items().row_count(), 0);
    });
}

#[test]
fn app_window_binds_and_reads_tab_page_and_bookmark_models() {
    run_on_ui_thread(|| {
        let app = AppWindow::new().expect("headless AppWindow should initialize");

        let tabs = Rc::new(VecModel::from(vec![
            TabItem {
                id: 1,
                title: SharedString::from("first.pdf"),
                is_active: true,
                is_loading: false,
            },
            TabItem {
                id: 2,
                title: SharedString::from("second.pdf"),
                is_active: false,
                is_loading: true,
            },
        ]));
        app.set_tab_items(ModelRc::from(tabs));

        let pages = Rc::new(VecModel::from(vec![PageItem {
            page_index: 0,
            page_number: SharedString::from("1"),
            width: 612.0,
            height: 792.0,
            y_offset: 0.0,
            bitmap: Image::default(),
            has_bitmap: false,
            selection_boxes: ModelRc::default(),
            search_highlights: ModelRc::default(),
        }]));
        app.set_visible_pages(ModelRc::from(pages));

        let bookmarks = Rc::new(VecModel::from(vec![BookmarkItem {
            title: SharedString::from("Overview"),
            page_index: 0,
            page_number: SharedString::from("1"),
        }]));
        app.set_bookmark_items(ModelRc::from(bookmarks));

        let bound_tabs = app.get_tab_items();
        assert_eq!(bound_tabs.row_count(), 2);
        let first_tab = bound_tabs.row_data(0).expect("first tab row should exist");
        assert_eq!(first_tab.id, 1);
        assert_eq!(first_tab.title.as_str(), "first.pdf");
        assert!(first_tab.is_active);

        let bound_pages = app.get_visible_pages();
        assert_eq!(bound_pages.row_count(), 1);
        let first_page = bound_pages
            .row_data(0)
            .expect("first page row should exist");
        assert_eq!(first_page.page_index, 0);
        assert_eq!(first_page.page_number.as_str(), "1");

        let bound_bookmarks = app.get_bookmark_items();
        assert_eq!(bound_bookmarks.row_count(), 1);
        let first_bookmark = bound_bookmarks
            .row_data(0)
            .expect("first bookmark row should exist");
        assert_eq!(first_bookmark.title.as_str(), "Overview");
        assert_eq!(first_bookmark.page_index, 0);
    });
}

#[test]
fn app_window_registers_and_invokes_core_callbacks() {
    run_on_ui_thread(|| {
        let app = AppWindow::new().expect("headless AppWindow should initialize");

        let opened = Rc::new(AtomicBool::new(false));
        let zoomed_in = Rc::new(AtomicBool::new(false));
        let activated_tab = Rc::new(AtomicI32::new(-1));

        {
            let opened = Rc::clone(&opened);
            app.on_request_open_file(move || {
                opened.store(true, Ordering::SeqCst);
            });
        }
        {
            let zoomed_in = Rc::clone(&zoomed_in);
            app.on_request_zoom_in(move || {
                zoomed_in.store(true, Ordering::SeqCst);
            });
        }
        {
            let activated_tab = Rc::clone(&activated_tab);
            app.on_request_activate_tab(move |tab_id| {
                activated_tab.store(tab_id, Ordering::SeqCst);
            });
        }

        app.invoke_request_open_file();
        app.invoke_request_zoom_in();
        app.invoke_request_activate_tab(42);

        assert!(opened.load(Ordering::SeqCst));
        assert!(zoomed_in.load(Ordering::SeqCst));
        assert_eq!(activated_tab.load(Ordering::SeqCst), 42);
    });
}
