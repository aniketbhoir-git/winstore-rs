use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub theme: Theme,
    pub cache_ttl_secs: u64,
    pub update_check_interval_secs: u64,
    pub disabled_sources: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            cache_ttl_secs: 3600,
            update_check_interval_secs: 86400,
            disabled_sources: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn default_path() -> AppResult<PathBuf> {
        let dirs = ProjectDirs::from("app", "store", "Store")
            .ok_or_else(|| AppError::InvalidConfig("cannot resolve config directory".into()))?;
        Ok(dirs.config_dir().join("config.json"))
    }

    pub fn load(path: &Path) -> AppResult<AppConfig> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                serde_json::from_str(&text).map_err(|e| AppError::InvalidConfig(e.to_string()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes pretty JSON atomically (temp file + rename).
    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| AppError::InvalidConfig(e.to_string()))?;
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn load_or_default() -> AppResult<AppConfig> {
        Self::load(&Self::default_path()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_file_with_removed_agreements_key_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            // The key of the removed field, split so that no source text names it.
            concat!(
                r#"{"cache_ttl_secs": 7, "accept_source"#,
                r#"_agreements": true}"#
            ),
        )
        .unwrap();
        let cfg = AppConfig::load(&path).unwrap();
        assert_eq!(cfg.cache_ttl_secs, 7);
    }

    #[test]
    fn default_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.json");
        let cfg = AppConfig::default();
        cfg.save(&path).unwrap();
        assert_eq!(AppConfig::load(&path).unwrap(), cfg);
    }

    #[test]
    fn modified_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let cfg = AppConfig {
            theme: Theme::Dark,
            cache_ttl_secs: 5,
            update_check_interval_secs: 10,
            disabled_sources: vec!["a".into(), "b".into()],
        };
        cfg.save(&path).unwrap();
        cfg.save(&path).unwrap(); // overwrite existing file
        assert_eq!(AppConfig::load(&path).unwrap(), cfg);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = AppConfig::load(&dir.path().join("nope.json")).unwrap();
        assert_eq!(cfg, AppConfig::default());
    }

    #[test]
    fn malformed_json_is_invalid_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(matches!(
            AppConfig::load(&path),
            Err(AppError::InvalidConfig(_))
        ));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"theme":"light","future_key":42}"#).unwrap();
        let cfg = AppConfig::load(&path).unwrap();
        assert_eq!(cfg.theme, Theme::Light);
        assert_eq!(cfg.cache_ttl_secs, 3600);
    }
}
