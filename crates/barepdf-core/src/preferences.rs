use std::path::PathBuf;

use crate::types::{ReadingDirection, ViewingMode, ZoomMode};
use crate::{MAX_OPEN_TABS, MAX_RECENT_FILES};
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
#[allow(clippy::struct_excessive_bools)] // Persisted user preference flags map 1:1 to JSON configuration keys.
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
    #[serde(deserialize_with = "deserialize_open_tabs")]
    pub open_tabs: Vec<DocumentSession>,
    pub paper_tint: u8,
    pub invert_colors: bool,
    pub welcome_manifesto_seen: bool,
}

pub type AppPreferences = UserPreferences;

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
            invert_colors: false,
            welcome_manifesto_seen: false,
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

    /// Clamps persisted collections (`recent_files`, `open_tabs`) and indices (`max_recent_files`,
    /// `active_tab_index`) to product resource limits.
    pub fn sanitize_bounds(&mut self) {
        self.max_recent_files = self.max_recent_files.min(MAX_RECENT_FILES);
        self.recent_files.truncate(self.max_recent_files);
        self.open_tabs.truncate(MAX_OPEN_TABS);
        if self.open_tabs.is_empty() {
            self.active_tab_index = 0;
        } else if self.active_tab_index >= self.open_tabs.len() {
            self.active_tab_index = self.open_tabs.len() - 1;
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

fn deserialize_open_tabs<'de, D>(deserializer: D) -> Result<Vec<DocumentSession>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut open_tabs = Vec::<DocumentSession>::deserialize(deserializer)?;
    open_tabs.truncate(MAX_OPEN_TABS);
    Ok(open_tabs)
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
    fn sanitize_bounds_truncates_collections_and_clamps_active_tab_index() {
        let session = DocumentSession {
            path: PathBuf::from("doc.pdf"),
            page_index: 0,
            scroll_y: 0.0,
            zoom_mode: ZoomMode::FitWidth,
            bookmarks: Vec::new(),
        };
        let mut preferences = UserPreferences {
            max_recent_files: MAX_RECENT_FILES + 50,
            recent_files: (0..(MAX_RECENT_FILES + 5))
                .map(|i| format!("file_{i}.pdf"))
                .collect(),
            open_tabs: vec![session; MAX_OPEN_TABS + 5],
            active_tab_index: MAX_OPEN_TABS + 10,
            ..UserPreferences::default()
        };

        preferences.sanitize_bounds();

        assert_eq!(preferences.max_recent_files, MAX_RECENT_FILES);
        assert_eq!(preferences.recent_files.len(), MAX_RECENT_FILES);
        assert_eq!(preferences.open_tabs.len(), MAX_OPEN_TABS);
        assert_eq!(preferences.active_tab_index, MAX_OPEN_TABS - 1);

        preferences.open_tabs.clear();
        preferences.active_tab_index = 5;
        preferences.sanitize_bounds();
        assert_eq!(preferences.active_tab_index, 0);
    }

    #[test]
    fn legacy_preferences_default_to_efficient_ui() {
        let preferences = UserPreferences::default();
        let legacy_preferences: UserPreferences = serde_json::from_str("{}").unwrap();

        assert!(!preferences.enhanced_ui);
        assert_eq!(preferences.paper_tint, 0);
        assert!(preferences.open_tabs.is_empty());
        assert!(!preferences.welcome_manifesto_seen);
        assert!(!legacy_preferences.welcome_manifesto_seen);
    }

    #[test]
    fn invert_colors_defaults_to_false_and_serializes() {
        let preferences = UserPreferences::default();
        assert!(!preferences.invert_colors);

        // Deserializing empty JSON object should default invert_colors to false
        let deserialized: UserPreferences = serde_json::from_str("{}").unwrap();
        assert!(!deserialized.invert_colors);

        // Deserializing with invert_colors: true
        let deserialized_true: UserPreferences =
            serde_json::from_str(r#"{"invert_colors":true}"#).unwrap();
        assert!(deserialized_true.invert_colors);

        // Serializing with invert_colors: true
        let prefs = UserPreferences {
            invert_colors: true,
            ..UserPreferences::default()
        };
        let json = serde_json::to_string(&prefs).unwrap();
        assert!(json.contains(r#""invert_colors":true"#));
    }
}
