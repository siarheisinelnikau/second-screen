use crate::capture::StreamConfig;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub startup_updates: bool,
    pub bind_ip: String,
    pub port: u16,
    pub stream: StreamConfig,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            startup_updates: true,
            bind_ip: "127.0.0.1".into(),
            port: 7420,
            stream: StreamConfig::default(),
        }
    }
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
    fn startup_opt_out_survives_atomic_replacement() {
        let dir = tempfile::tempdir().unwrap();
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
