use crate::types::{MemoryBudget, ReadingDirection, ViewingMode, ZoomMode};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use barepdf_i18n::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub language: Language,
    pub theme: ThemeMode,
    pub viewing_mode: ViewingMode,
    pub reading_direction: ReadingDirection,
    pub zoom_mode: ZoomMode,
    pub memory_budget_bytes: usize,
    pub max_recent_files: usize,
    pub recent_files: Vec<String>,
    pub last_window_width: u32,
    pub last_window_height: u32,
    pub sidebar_visible: bool,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            language: Language::System,
            theme: ThemeMode::System,
            viewing_mode: ViewingMode::ContinuousVertical,
            reading_direction: ReadingDirection::LeftToRight,
            zoom_mode: ZoomMode::FitWidth,
            memory_budget_bytes: MemoryBudget::DEFAULT_BYTES,
            max_recent_files: 10,
            recent_files: Vec::new(),
            last_window_width: 1100,
            last_window_height: 800,
            sidebar_visible: true,
        }
    }
}

impl UserPreferences {
    pub fn load_from_file(path: &Path) -> Self {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(prefs) = serde_json::from_str::<UserPreferences>(&content) {
                return prefs;
            }
        }
        Self::default()
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("tmp");

        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
        }

        fs::rename(tmp_path, path)?;
        Ok(())
    }

    pub fn add_recent_file(&mut self, file_path: String) {
        self.recent_files.retain(|p| p != &file_path);
        self.recent_files.insert(0, file_path);
        if self.recent_files.len() > self.max_recent_files {
            self.recent_files.truncate(self.max_recent_files);
        }
    }
}

pub fn default_config_path() -> PathBuf {
    if let Some(app_data) = std::env::var_os("APPDATA") {
        PathBuf::from(app_data).join("BarePDF").join("config.json")
    } else {
        PathBuf::from("config.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn theme_preference_round_trips() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("barepdf-preferences-{unique}.json"));
        let mut preferences = UserPreferences {
            theme: ThemeMode::Dark,
            language: Language::Turkish,
            ..UserPreferences::default()
        };
        preferences.add_recent_file("C:\\Documents\\book.pdf".into());
        preferences.save_to_file(&path).expect("save preferences");

        let loaded = UserPreferences::load_from_file(&path);
        assert_eq!(loaded.theme, ThemeMode::Dark);
        assert_eq!(loaded.language, Language::Turkish);
        assert_eq!(loaded.recent_files, vec!["C:\\Documents\\book.pdf"]);
        let _ = fs::remove_file(path);
    }

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
}
