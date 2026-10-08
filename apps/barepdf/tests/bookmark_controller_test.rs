use barepdf::controllers::bookmark_controller;
use barepdf_core::preferences::BookmarkEntry;
use barepdf_core::types::PageIndex;

#[test]
fn toggle_adds_bookmark_when_missing() {
    let mut bookmarks = Vec::new();
    let page = PageIndex::from_raw(5);
    let added =
        bookmark_controller::toggle_bookmark(&mut bookmarks, page, Some("My Bookmark".to_string()));

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
    let added = bookmark_controller::toggle_bookmark(&mut bookmarks, page, None);

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
    let _ = bookmark_controller::toggle_bookmark(&mut bookmarks, page, Some("Page 2".to_string()));

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
    let removed = bookmark_controller::remove_bookmark(&mut bookmarks, 3);

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
    let renamed = bookmark_controller::rename_bookmark(&mut bookmarks, 4, "New Title".to_string());

    assert!(renamed);
    assert_eq!(bookmarks[0].title, "New Title");
}

#[cfg(target_os = "windows")]
#[test]
fn executable_embeds_windows_icon_and_version_resources() {
    let exe_bytes = std::fs::read(env!("CARGO_BIN_EXE_barepdf")).unwrap();
    let ico_bytes = include_bytes!("../../../assets/app.ico");
    let first_icon_offset =
        u32::from_le_bytes([ico_bytes[18], ico_bytes[19], ico_bytes[20], ico_bytes[21]]) as usize;
    let icon_probe = &ico_bytes[first_icon_offset..first_icon_offset + 64];
    assert!(exe_bytes.windows(icon_probe.len()).any(|w| w == icon_probe));

    let version_utf16: Vec<u8> = env!("CARGO_PKG_VERSION")
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert!(exe_bytes
        .windows(version_utf16.len())
        .any(|w| w == version_utf16.as_slice()));
}
