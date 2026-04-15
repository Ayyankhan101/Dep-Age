//! Configuration file support for dep-age
//!
//! Supports `.dep-age.toml` and custom config paths via --config

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration structure for dep-age
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default, alias = "dep-age")]
    pub tool: Option<ToolConfig>,
}

/// [tool.dep-age] section
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolConfig {
    /// Days threshold for "fresh" status
    pub fresh: Option<i64>,
    /// Days threshold for "aging" status
    pub aging: Option<i64>,
    /// Days threshold for "stale" status
    pub stale: Option<i64>,
    /// Skip dev-dependencies
    pub no_dev: Option<bool>,
    /// Exit code 1 when packages match this status or worse
    pub fail_on: Option<String>,
    /// Packages to ignore
    pub ignore: Option<Vec<String>>,
    /// Registry base URLs
    pub registry: Option<RegistryConfig>,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            fresh: Some(90),
            aging: Some(365),
            stale: Some(730),
            no_dev: Some(false),
            fail_on: None,
            ignore: Some(vec![]),
            registry: None,
        }
    }
}

impl ToolConfig {
    /// Get fresh threshold with default
    pub fn get_fresh(&self) -> i64 {
        self.fresh.unwrap_or(90)
    }

    /// Get aging threshold with default  
    pub fn get_aging(&self) -> i64 {
        self.aging.unwrap_or(365)
    }

    /// Get stale threshold with default
    pub fn get_stale(&self) -> i64 {
        self.stale.unwrap_or(730)
    }

    /// Get no_dev with default
    pub fn get_no_dev(&self) -> bool {
        self.no_dev.unwrap_or(false)
    }

    /// Get ignore list with default
    pub fn get_ignore(&self) -> Vec<String> {
        self.ignore.clone().unwrap_or_default()
    }

    pub fn from_file(path: &PathBuf) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;

        config.tool.ok_or(ConfigError::MissingToolSection)
    }

    pub fn detect() -> Option<Self> {
        let config_paths = [
            PathBuf::from(".dep-age.toml"),
            PathBuf::from("dep-age.toml"),
        ];

        for path in &config_paths {
            if path.exists() {
                if let Ok(config) = Self::from_file(path) {
                    return Some(config);
                }
            }
        }

        None
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RegistryConfig {
    #[serde(default)]
    pub crates_base_url: Option<String>,
    #[serde(default)]
    pub npm_base_url: Option<String>,
    #[serde(default)]
    pub pypi_base_url: Option<String>,
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
    MissingToolSection,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO error: {}", e),
            ConfigError::Parse(e) => write!(f, "TOML parse error: {}", e),
            ConfigError::MissingToolSection => write!(f, "Missing [tool.dep-age] section"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Parse(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let toml_content = r#"[tool.dep-age]
fresh = 60
aging = 180
stale = 540
no-dev = true
ignore = ["time", "old-crate"]
"#;

        let config: Config = toml::from_str(toml_content).unwrap();
        let tool = config.tool;

        assert!(tool.is_some(), "Should have tool.dep-age section");
    }

    #[test]
    fn test_config_missing_tool_section() {
        // When there's no [tool.dep-age] section at all, tool should be None
        let toml_content = "[package]\nname = \"test\"";
        let config: Config = toml::from_str(toml_content).unwrap();
        assert!(config.tool.is_none(), "Should have no tool section");
    }

    #[test]
    fn test_defaults() {
        let config = ToolConfig::default();
        assert_eq!(config.get_fresh(), 90);
        assert_eq!(config.get_aging(), 365);
        assert_eq!(config.get_stale(), 730);
        assert!(!config.get_no_dev());
        assert!(config.get_ignore().is_empty());
    }
}
