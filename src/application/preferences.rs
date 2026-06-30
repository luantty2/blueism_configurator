use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UserPreferences {
    pub language: String,
    pub appearance: String,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            language: "en_us".to_owned(),
            appearance: "system".to_owned(),
        }
    }
}

pub fn load_user_preferences() -> UserPreferences {
    let Some(path) = preferences_path() else {
        return UserPreferences::default();
    };
    let Ok(source) = fs::read_to_string(path) else {
        return UserPreferences::default();
    };
    serde_json::from_str(&source).unwrap_or_default()
}

pub fn save_user_preferences(preferences: &UserPreferences) -> Result<(), String> {
    let path = preferences_path().ok_or_else(|| "Cannot find user config directory".to_owned())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot create preferences directory: {error}"))?;
    }
    let source = serde_json::to_string_pretty(preferences)
        .map_err(|error| format!("Cannot serialize preferences: {error}"))?;
    fs::write(path, source).map_err(|error| format!("Cannot save preferences: {error}"))
}

fn preferences_path() -> Option<PathBuf> {
    Some(
        config_dir()?
            .join("Blueism Configurator")
            .join("preferences.json"),
    )
}

#[cfg(target_os = "macos")]
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library").join("Application Support"))
}

#[cfg(target_os = "windows")]
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        })
}
