//! Pure persistence for user settings (currently: the selected chat model).
//! Stored as JSON in `app_config_dir/settings.json`.

use std::fs;
use std::path::Path;

use super::errors::AppError;

/// User settings persisted across app restarts.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct Settings {
    /// The id of the selected chat model (e.g. `openai/gpt-4o`), or `None`
    /// when the user has not chosen one yet.
    pub model_id: Option<String>,
}

/// Loads settings from `file`. A missing file yields the defaults; a corrupt
/// file is an error.
pub fn load_settings(file: &Path) -> Result<Settings, AppError> {
    match fs::read_to_string(file) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|e| AppError::Io(std::io::Error::other(e), file.to_path_buf())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(AppError::Io(e, file.to_path_buf())),
    }
}

/// Saves settings to `file`. Parent directories are created if needed.
pub fn save_settings(file: &Path, settings: &Settings) -> Result<(), AppError> {
    let contents = serde_json::to_string_pretty(settings)
        .map_err(|e| AppError::Io(std::io::Error::other(e), file.to_path_buf()))?;
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Io(e, parent.to_path_buf()))?;
    }
    fs::write(file, contents).map_err(|e| AppError::Io(e, file.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_settings_missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let settings = load_settings(&file).unwrap();
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.model_id, None);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let settings = Settings {
            model_id: Some("openai/gpt-4o".to_string()),
        };
        save_settings(&file, &settings).unwrap();
        assert_eq!(load_settings(&file).unwrap(), settings);
    }

    #[test]
    fn load_settings_corrupt_file_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        fs::write(&file, "not json").unwrap();
        assert!(load_settings(&file).is_err());
    }
}
