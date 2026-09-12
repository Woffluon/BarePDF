use std::path::PathBuf;

use crate::types::{ReadingDirection, ViewingMode, ZoomMode};
use crate::MAX_RECENT_FILES;
use serde::{Deserialize, Serialize};

use barepdf_i18n::Language;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentSession {
    pub path: PathBuf,
    pub page_index: u32,
    pub scroll_y: f32,
    pub zoom_mode: ZoomMode,
    #[serde(default)]
    pub bookmarks: Vec<BookmarkEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookmarkEntry {
    pub page_index: u32,
    pub title: String,
    pub created_unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserPreferences {
    pub language: Language,
    pub theme: ThemeMode,
    pub viewing_mode: ViewingMode,
    pub reading_direction: ReadingDirection,
    pub zoom_mode: ZoomMode,
    #[serde(deserialize_with = "deserialize_max_recent_files")]
    pub max_recent_files: usize,
    #[serde(deserialize_with = "deserialize_recent_files")]
    pub recent_files: Vec<String>,
    pub last_window_width: u32,
    pub last_window_height: u32,
    pub sidebar_visible: bool,
    pub enhanced_ui: bool,
    pub update_checks_enabled: Option<bool>,
    pub last_update_check_unix: Option<u64>,
    pub active_tab_index: usize,
    pub open_tabs: Vec<DocumentSession>,
    pub paper_tint: u8,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            language: Language::System,
            theme: ThemeMode::System,
            viewing_mode: ViewingMode::ContinuousVertical,
            reading_direction: ReadingDirection::LeftToRight,
            zoom_mode: ZoomMode::FitWidth,
            max_recent_files: MAX_RECENT_FILES,
            recent_files: Vec::new(),
            last_window_width: 1100,
            last_window_height: 800,
            sidebar_visible: true,
            enhanced_ui: false,
            update_checks_enabled: None,
            last_update_check_unix: None,
            active_tab_index: 0,
            open_tabs: Vec::new(),
            paper_tint: 0,
        }
    }
}

impl UserPreferences {
    pub fn add_recent_file(&mut self, file_path: String) {
        self.max_recent_files = self.max_recent_files.min(MAX_RECENT_FILES);
        self.recent_files.retain(|p| p != &file_path);
        self.recent_files.insert(0, file_path);
        if self.recent_files.len() > self.max_recent_files {
            self.recent_files.truncate(self.max_recent_files);
        }
    }
}

fn deserialize_max_recent_files<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(usize::deserialize(deserializer)?.min(MAX_RECENT_FILES))
}

fn deserialize_recent_files<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut recent_files = Vec::<String>::deserialize(deserializer)?;
    recent_files.truncate(MAX_RECENT_FILES);
    Ok(recent_files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_are_unique_and_bounded() {
        let mut preferences = UserPreferences {
            max_recent_files: 2,
            ..UserPreferences::default()
        };
        preferences.add_recent_file("one.pdf".into());
        preferences.add_recent_file("two.pdf".into());
        preferences.add_recent_file("one.pdf".into());
        preferences.add_recent_file("three.pdf".into());
        assert_eq!(preferences.recent_files, vec!["three.pdf", "one.pdf"]);
    }

    #[test]
    fn legacy_preferences_default_to_efficient_ui() {
        let preferences = UserPreferences::default();

        assert!(!preferences.enhanced_ui);
        assert_eq!(preferences.paper_tint, 0);
        assert!(preferences.open_tabs.is_empty());
    }
}
