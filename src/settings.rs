use crate::capture::StreamConfig;
use crate::i18n::Language;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub theme: Theme,
    pub startup_updates: bool,
    pub bind_ip: String,
    pub port: u16,
    pub stream: StreamConfig,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: Theme::default(),
            startup_updates: true,
            bind_ip: "127.0.0.1".into(),
            port: 7420,
            stream: StreamConfig::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    #[default]
    Dark,
}
pub fn path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("SecondScreen")
        .join("settings.json")
}
pub fn load(path: &Path) -> Result<Settings, String> {
    match fs::read(path) {
        Ok(bytes) => {
            if bytes.len() > 16384 {
                return Err("Settings file is too large".into());
            }
            let settings: Settings =
                serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read settings: {e}"))?;
            settings.stream.validate()?;
            Ok(settings)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(format!("Cannot load settings: {e}")),
    }
}
pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    settings.stream.validate()?;
    let parent = path.parent().ok_or("Missing settings folder")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&temp, data).map_err(|e| format!("Cannot save settings: {e}"))?;
    fs::rename(temp, path).map_err(|e| format!("Cannot replace settings: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_settings_preserve_connection_and_gain_safe_preferences() {
        let old = r#"{"startup_updates":false,"bind_ip":"192.168.1.20","port":8123}"#;
        let settings: Settings = serde_json::from_str(old).unwrap();
        assert_eq!(settings.language, Language::English);
        assert_eq!(settings.theme, Theme::Dark);
        assert_eq!(settings.bind_ip, "192.168.1.20");
        assert_eq!(settings.port, 8123);
        assert!(!settings.startup_updates);
    }
    #[test]
    fn language_and_theme_survive_save_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let settings = Settings {
            language: Language::Italian,
            theme: Theme::Light,
            ..Default::default()
        };
        save(&file, &settings).unwrap();
        let restored = load(&file).unwrap();
        assert_eq!(restored.language, Language::Italian);
        assert_eq!(restored.theme, Theme::Light);
    }
    #[test]
    fn startup_opt_out_survives_atomic_replacement() {
        fs::create_dir_all(".test-output").unwrap();
        let dir = tempfile::tempdir_in(".test-output").unwrap();
        let file = dir.path().join("settings.json");
        save(&file, &Settings::default()).unwrap();
        let settings = Settings {
            startup_updates: false,
            ..Default::default()
        };
        save(&file, &settings).unwrap();
        assert!(!load(&file).unwrap().startup_updates);
        fs::write(&file, "not json").unwrap();
        assert!(load(&file).is_err());
    }
}
