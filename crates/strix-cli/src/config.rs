//! Configuration management for the CLI.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration file structure.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Server aliases.
    #[serde(default)]
    pub aliases: HashMap<String, Alias>,
}

/// A server alias configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    /// Server URL (e.g., http://localhost:9000)
    pub url: String,
    /// Access key ID
    pub access_key: String,
    /// Secret access key
    pub secret_key: String,
    /// Admin API URL (optional, defaults to port 9001)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin_url: Option<String>,
}

impl Config {
    /// Get the config file path.
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("strix")
            .join("config.toml")
    }

    /// Load configuration from file.
    pub fn load() -> Result<Self> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Config::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }

    /// Save configuration to file.
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))
    }

    /// Get an alias by name.
    pub fn get_alias(&self, name: &str) -> Option<&Alias> {
        self.aliases.get(name)
    }

    /// Set an alias.
    pub fn set_alias(&mut self, name: String, alias: Alias) {
        self.aliases.insert(name, alias);
    }

    /// Remove an alias.
    pub fn remove_alias(&mut self, name: &str) -> Option<Alias> {
        self.aliases.remove(name)
    }
}

/// Parse a path like "alias/bucket/key" into (alias, bucket, key).
pub fn parse_path(path: &str) -> Result<(String, Option<String>, Option<String>)> {
    let parts: Vec<&str> = path.splitn(3, '/').collect();

    match parts.len() {
        1 => Ok((parts[0].to_string(), None, None)),
        2 => Ok((parts[0].to_string(), Some(parts[1].to_string()), None)),
        3 => Ok((
            parts[0].to_string(),
            Some(parts[1].to_string()),
            Some(parts[2].to_string()),
        )),
        _ => anyhow::bail!("Invalid path format: {}", path),
    }
}

/// Check if a path is a local file path.
pub fn is_local_path(path: &str) -> bool {
    path.starts_with('/') || path.starts_with("./") || path.starts_with("../") || path == "."
}
