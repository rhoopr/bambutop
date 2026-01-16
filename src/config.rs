use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub printer: PrinterConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PrinterConfig {
    pub ip: String,
    pub serial: String,
    pub access_code: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    8883
}

impl Config {
    pub fn load() -> Result<Option<Self>> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| "Failed to parse config file")?;

        Ok(Some(config))
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self)
            .with_context(|| "Failed to serialize config")?;

        fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {:?}", config_path))?;

        Ok(())
    }

    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".config").join("bambutop").join("config.toml"))
    }
}
