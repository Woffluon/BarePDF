#[path = "../src/controllers/bookmark_controller.rs"]
mod bookmark_controller;

use barepdf_core::preferences::BookmarkEntry;
use barepdf_core::types::PageIndex;
use bookmark_controller::BookmarkController;

#[test]
fn toggle_adds_bookmark_when_missing() {
    let mut bookmarks = Vec::new();
    let page = PageIndex::from_raw(5);
    let added =
        BookmarkController::toggle_bookmark(&mut bookmarks, page, Some("My Bookmark".to_string()));

    assert!(added);
    assert_eq!(bookmarks.len(), 1);
    assert_eq!(bookmarks[0].page_index, 5);
    assert_eq!(bookmarks[0].title, "My Bookmark");
}

#[test]
fn toggle_removes_bookmark_when_present() {
    let mut bookmarks = vec![BookmarkEntry {
        page_index: 5,
        title: "Existing".to_string(),
        created_unix: 0,
    }];
    let page = PageIndex::from_raw(5);
    let added = BookmarkController::toggle_bookmark(&mut bookmarks, page, None);

    assert!(!added);
    assert!(bookmarks.is_empty());
}

#[test]
fn toggle_sorts_bookmarks_by_page() {
    let mut bookmarks = vec![BookmarkEntry {
        page_index: 8,
        title: "Page 8".to_string(),
        created_unix: 0,
    }];
    let page = PageIndex::from_raw(2);
    BookmarkController::toggle_bookmark(&mut bookmarks, page, Some("Page 2".to_string()));

    assert_eq!(bookmarks.len(), 2);
    assert_eq!(bookmarks[0].page_index, 2);
    assert_eq!(bookmarks[1].page_index, 8);
}

#[test]
fn remove_bookmark_deletes_by_raw_page() {
    let mut bookmarks = vec![
        BookmarkEntry {
            page_index: 1,
            title: "A".to_string(),
            created_unix: 0,
        },
        BookmarkEntry {
            page_index: 3,
            title: "B".to_string(),
            created_unix: 0,
        },
    ];
    let removed = BookmarkController::remove_bookmark(&mut bookmarks, 3);

    assert!(removed);
    assert_eq!(bookmarks.len(), 1);
    assert_eq!(bookmarks[0].page_index, 1);
}

#[test]
fn rename_bookmark_changes_title_by_raw_page() {
    let mut bookmarks = vec![BookmarkEntry {
        page_index: 4,
        title: "Old Title".to_string(),
        created_unix: 0,
    }];
    let renamed = BookmarkController::rename_bookmark(&mut bookmarks, 4, "New Title".to_string());

    assert!(renamed);
    assert_eq!(bookmarks[0].title, "New Title");
}
