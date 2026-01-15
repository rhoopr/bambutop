use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub printer: PrinterConfig,
}

#[derive(Debug, Clone, Deserialize)]
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
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            anyhow::bail!(
                "Config file not found at {:?}\n\n\
                Create it with the following content:\n\n\
                [printer]\n\
                ip = \"192.168.1.100\"\n\
                serial = \"YOUR_PRINTER_SERIAL\"\n\
                access_code = \"YOUR_ACCESS_CODE\"\n",
                config_path
            );
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| "Failed to parse config file")?;

        Ok(config)
    }

    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Could not determine config directory")?;
        Ok(config_dir.join("bambutop").join("config.toml"))
    }
}
