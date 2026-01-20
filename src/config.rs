//! Configuration file management for bambutop.
//!
//! Handles loading and saving printer configuration from `~/.config/bambutop/config.toml`.
//! The configuration includes printer IP address, serial number, access code, and MQTT port.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Default MQTT port for Bambu printers (TLS)
pub const DEFAULT_MQTT_PORT: u16 = 8883;

/// Application configuration stored in `~/.config/bambutop/config.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Printer connection settings.
    pub printer: PrinterConfig,
}

/// Printer connection settings for MQTT communication.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PrinterConfig {
    /// IP address of the Bambu printer on the local network.
    pub ip: String,
    /// Printer serial number (found on printer or in Bambu Studio).
    pub serial: String,
    /// Access code for LAN mode authentication.
    pub access_code: String,
    /// MQTT port (defaults to 8883 for TLS).
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    DEFAULT_MQTT_PORT
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
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;

        Ok(Some(config))
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self).with_context(|| "Failed to serialize config")?;

        fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {:?}", config_path))?;

        Ok(())
    }

    /// Returns the path to the configuration file.
    ///
    /// The config file is stored at `~/.config/bambutop/config.toml`.
    ///
    /// # Errors
    ///
    /// Returns an error if the home directory cannot be determined.
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".config").join("bambutop").join("config.toml"))
    }
}
