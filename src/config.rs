use serde::Deserialize;
use std::path::PathBuf;

/// Missing keys take their default, so a partial config.toml still applies.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Override backend: "git" or "jj"
    pub backend: Option<String>,
    /// Number of log entries to show in the log buffer
    pub log_limit: usize,
    /// Number of recent commits to show in the status buffer
    pub recent_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backend: None,
            log_limit: 50,
            recent_limit: 10,
        }
    }
}

impl Config {
    /// Falls back to defaults when the file is missing or invalid.
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| toml::from_str(&content).ok())
            .unwrap_or_default()
    }

    fn config_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?;
        Some(base.join("rugit").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn partial_config_keeps_other_defaults() {
        let config: Config = toml::from_str("log_limit = 200").unwrap();
        assert_eq!(config.log_limit, 200);
        assert_eq!(config.recent_limit, Config::default().recent_limit);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        // `editor` was a config key once; old files must still load.
        let config: Config = toml::from_str("editor = \"vim\"\nrecent_limit = 3").unwrap();
        assert_eq!(config.recent_limit, 3);
    }
}
