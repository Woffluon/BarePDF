use barepdf_core::preferences::BookmarkEntry;
use barepdf_core::types::PageIndex;

pub struct BookmarkController;

impl BookmarkController {
    pub fn toggle_bookmark(
        bookmarks: &mut Vec<BookmarkEntry>,
        current_page: PageIndex,
        default_title: Option<String>,
    ) -> bool {
        let raw = current_page.get();
        if let Some(pos) = bookmarks.iter().position(|b| b.page_index == raw) {
            bookmarks.remove(pos);
            false
        } else {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            bookmarks.push(BookmarkEntry {
                page_index: raw,
                title: default_title.unwrap_or_else(|| format!("Page {}", raw + 1)),
                created_unix: timestamp,
            });
            bookmarks.sort_by_key(|b| b.page_index);
            true
        }
    }

    pub fn remove_bookmark(bookmarks: &mut Vec<BookmarkEntry>, page_raw: u32) -> bool {
        if let Some(pos) = bookmarks.iter().position(|b| b.page_index == page_raw) {
            bookmarks.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn rename_bookmark(
        bookmarks: &mut [BookmarkEntry],
        page_raw: u32,
        new_title: String,
    ) -> bool {
        if let Some(b) = bookmarks.iter_mut().find(|b| b.page_index == page_raw) {
            b.title = new_title;
            true
        } else {
            false
        }
    }
}
