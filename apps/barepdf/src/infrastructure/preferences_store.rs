use barepdf_core::UserPreferences;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub(crate) enum PreferencesLoadError {
    #[error("failed to read preferences: {0}")]
    Read(#[from] std::io::Error),
    #[error("failed to parse preferences: {0}")]
    Parse(#[from] serde_json::Error),
}

#[must_use]
pub(crate) fn default_config_path() -> PathBuf {
    std::env::var_os("APPDATA").map_or_else(
        || PathBuf::from("config.json"),
        |app_data| PathBuf::from(app_data).join("BarePDF").join("config.json"),
    )
}

/// Reads preferences while preserving I/O and JSON errors for callers that can report them.
///
/// # Errors
///
/// Returns an error when the preference file cannot be read or parsed.
pub(crate) fn try_load_from_file(path: &Path) -> Result<UserPreferences, PreferencesLoadError> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Atomically replaces the preference file after flushing its contents.
///
/// # Errors
///
/// Returns an error when the preference file cannot be serialized, created, written, or replaced.
pub(crate) fn save_to_file(
    preferences: &UserPreferences,
    path: &Path,
) -> Result<(), std::io::Error> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let json = serde_json::to_string_pretty(preferences)?;
    let mut file = tempfile::Builder::new()
        .prefix(".barepdf-config-")
        .tempfile_in(parent)?;
    file.write_all(json.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use barepdf_core::{ThemeMode, MAX_RECENT_FILES};
    use barepdf_i18n::Language;

    #[test]
    fn preferences_round_trip_and_replace_atomically() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("nested").join("config.json");
        let mut preferences = UserPreferences {
            theme: ThemeMode::Dark,
            language: Language::Turkish,
            ..UserPreferences::default()
        };
        preferences.add_recent_file("C:\\Documents\\book.pdf".into());
        save_to_file(&preferences, &path).expect("save preferences");

        let loaded = try_load_from_file(&path).expect("load preferences");
        assert_eq!(loaded.theme, ThemeMode::Dark);
        assert_eq!(loaded.language, Language::Turkish);
        assert_eq!(loaded.recent_files, vec!["C:\\Documents\\book.pdf"]);

        preferences.theme = ThemeMode::Light;
        save_to_file(&preferences, &path).expect("replace preferences");
        assert_eq!(
            try_load_from_file(&path)
                .expect("load replaced preferences")
                .theme,
            ThemeMode::Light
        );
    }

    #[test]
    fn missing_preferences_use_defaults() {
        let directory = tempfile::tempdir().expect("create temp directory");

        let loaded = try_load_from_file(&directory.path().join("missing.json")).unwrap_or_default();

        assert_eq!(loaded.theme, ThemeMode::System);
        assert_eq!(loaded.max_recent_files, MAX_RECENT_FILES);
    }

    #[test]
    fn malformed_preferences_are_reported() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("config.json");
        fs::write(&path, "not json").expect("write invalid preferences");

        assert!(matches!(
            try_load_from_file(&path),
            Err(PreferencesLoadError::Parse(_))
        ));
    }

    #[test]
    fn oversized_recent_file_preferences_are_clamped_when_loaded() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("config.json");
        let recent_files: Vec<_> = (0..MAX_RECENT_FILES + 2)
            .map(|index| format!("book-{index}.pdf"))
            .collect();
        let json = serde_json::json!({
            "max_recent_files": usize::MAX,
            "recent_files": recent_files,
        });
        fs::write(&path, json.to_string()).expect("write oversized preferences");

        let preferences = try_load_from_file(&path).expect("load oversized preferences");

        assert_eq!(preferences.max_recent_files, MAX_RECENT_FILES);
        assert_eq!(preferences.recent_files.len(), MAX_RECENT_FILES);
        assert_eq!(preferences.recent_files[0], "book-0.pdf");
        assert_eq!(preferences.recent_files[9], "book-9.pdf");
    }

    #[test]
    fn older_preferences_default_update_consent() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("config.json");
        fs::write(
            &path,
            r#"{
                "language":"System",
                "theme":"System",
                "viewing_mode":"ContinuousVertical",
                "reading_direction":"LeftToRight",
                "zoom_mode":"FitWidth",
                "memory_budget_bytes":67108864,
                "max_recent_files":10,
                "recent_files":[],
                "last_window_width":1100,
                "last_window_height":800,
                "sidebar_visible":true
            }"#,
        )
        .expect("write old preferences");

        let preferences = try_load_from_file(&path).expect("load old preferences");

        assert_eq!(preferences.update_checks_enabled, None);
        assert_eq!(preferences.last_update_check_unix, None);
    }
}
