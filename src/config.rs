use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Override backend: "git" or "jj"
    pub backend: Option<String>,
    /// Editor to use for commit messages (falls back to $EDITOR)
    pub editor: Option<String>,
    /// Number of log entries to show
    pub log_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backend: None,
            editor: None,
            log_limit: 50,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = Self::config_path();
        if let Some(path) = config_path {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(config) = toml::from_str::<Config>(&content) {
                        return config;
                    }
                }
            }
        }
        Config::default()
    }

    fn config_path() -> Option<PathBuf> {
        let base = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                dirs_path()
            })?;
        Some(base.join("rugit").join("config.toml"))
    }

    pub fn editor(&self) -> String {
        self.editor
            .clone()
            .or_else(|| std::env::var("EDITOR").ok())
            .or_else(|| std::env::var("VISUAL").ok())
            .unwrap_or_else(|| "vi".to_string())
    }
}

fn dirs_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config"))
}
