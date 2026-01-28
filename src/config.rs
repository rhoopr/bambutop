//! Configuration file management for bambutop.
//!
//! Handles loading and saving printer configuration from `~/.config/bambutop/config.toml`.
//! The configuration includes printer IP address, serial number, access code, and MQTT port.
//!
//! Supports two formats:
//! - **New format (multi-printer)**: Uses `[[printers]]` array for multiple printers
//! - **Legacy format**: Uses single `[printer]` section (automatically migrated on save)
//!
//! # Printer Ordering Guarantee
//!
//! Printers maintain a **deterministic order** across application restarts:
//!
//! - The order in which printers appear in the config file is preserved when loading
//! - The first printer in the `[[printers]]` array becomes the primary printer
//! - Additional printers follow in the same order they appear in the file
//! - When saving, printers are written in the same order: primary first, then extras
//!
//! This guarantee is provided by:
//! 1. TOML specification requires arrays to preserve element order
//! 2. The `toml` crate correctly implements ordered array parsing/serialization
//! 3. Internal data structures (`Vec`) maintain insertion order
//!
//! Users can rely on this ordering for consistent UI presentation across restarts.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Default MQTT port for Bambu printers (TLS)
pub const DEFAULT_MQTT_PORT: u16 = 8883;

/// Application configuration stored in `~/.config/bambutop/config.toml`.
///
/// Supports both the new multi-printer format (`[[printers]]` array) and the
/// legacy single-printer format (`[printer]` section) for backwards compatibility.
///
/// The `printer` field is maintained for backwards compatibility with existing code
/// that constructs and accesses configs using `config.printer`. New code should use
/// `config.all_printers()` to access all configured printers.
///
/// # Ordering Guarantee
///
/// Printers are returned in a deterministic order that is preserved across restarts:
/// 1. The primary `printer` field (first printer from config file)
/// 2. The `extra_printers` in the order they appear in the config file
///
/// This order matches the order in which printers appear in the `[[printers]]` array
/// in the TOML config file.
///
/// When constructing with struct literal syntax, the `extra_printers` field can be
/// omitted by using `..Default::default()`:
/// ```ignore
/// Config {
///     printer: PrinterConfig { ... },
///     ..Default::default()
/// }
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    /// Primary printer configuration (for backwards compatibility).
    /// When using multi-printer setups, this will be the first printer.
    #[serde(default)]
    pub printer: PrinterConfig,

    /// Additional printer configurations (new multi-printer format).
    /// This does NOT include the primary printer - use `all_printers()` method to get all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_printers: Vec<PrinterConfig>,
}

/// Raw configuration format for deserializing config files.
/// Handles both legacy `[printer]` and new `[[printers]]` formats.
#[derive(Debug, Clone, Deserialize)]
struct RawConfig {
    /// Legacy single printer (optional when using new format).
    printer: Option<PrinterConfig>,
    /// New multi-printer array (optional when using legacy format).
    #[serde(default)]
    printers: Vec<PrinterConfig>,
}

/// Serialization format for saving configs in the new multi-printer format.
#[derive(Debug, Clone, Serialize)]
struct SaveConfig {
    printers: Vec<PrinterConfig>,
}

/// Printer connection settings for MQTT communication.
///
/// When constructing with struct literal syntax, the `name` field can be omitted
/// by using `..Default::default()`:
/// ```ignore
/// PrinterConfig {
///     ip: "192.168.1.100".to_string(),
///     serial: "SERIAL".to_string(),
///     access_code: "CODE".to_string(),
///     port: DEFAULT_MQTT_PORT,
///     ..Default::default()
/// }
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PrinterConfig {
    /// Optional friendly name for the printer (e.g., "Office P1S").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// IP address of the Bambu printer on the local network.
    #[serde(default)]
    pub ip: String,
    /// Printer serial number (found on printer or in Bambu Studio).
    #[serde(default)]
    pub serial: String,
    /// Access code for LAN mode authentication.
    #[serde(default)]
    pub access_code: String,
    /// MQTT port (defaults to 8883 for TLS).
    #[serde(default = "default_port")]
    pub port: u16,
}

/// Returns the default MQTT port for serde deserialization.
fn default_port() -> u16 {
    DEFAULT_MQTT_PORT
}

impl Config {
    /// Loads the configuration from the config file.
    ///
    /// Supports both the new multi-printer format (`[[printers]]` array) and the
    /// legacy single-printer format (`[printer]` section). When loading a legacy
    /// config, it is automatically converted to the new format in memory.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(config))` if the config file exists and was parsed successfully
    /// - `Ok(None)` if the config file does not exist
    /// - `Err(...)` if the file exists but cannot be read or parsed
    pub fn load() -> Result<Option<Self>> {
        let config_path = Self::config_path().context("failed to determine config file path")?;

        if !config_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;

        Self::parse(&content)
            .map(Some)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))
    }

    /// Parses configuration from a TOML string.
    ///
    /// Supports both the new multi-printer format (`[[printers]]` array) and the
    /// legacy single-printer format (`[printer]` section).
    pub fn parse(content: &str) -> Result<Self> {
        let raw: RawConfig =
            toml::from_str(content).with_context(|| "Failed to parse config TOML")?;

        // Determine which format was used and build the config
        let all_printers = if !raw.printers.is_empty() {
            // New format: [[printers]] array
            raw.printers
        } else if let Some(printer) = raw.printer {
            // Legacy format: [printer] section
            vec![printer]
        } else {
            anyhow::bail!("Config must have either [printer] or [[printers]] section");
        };

        // Split into primary printer and extras for backwards compatibility
        let mut printers_iter = all_printers.into_iter();
        let printer = printers_iter
            .next()
            .context("Config must have at least one printer")?;
        let extra_printers: Vec<PrinterConfig> = printers_iter.collect();

        Ok(Config {
            printer,
            extra_printers,
        })
    }

    /// Saves the configuration to the config file.
    ///
    /// Always saves in the new multi-printer format (`[[printers]]` array),
    /// even if the config was originally loaded from a legacy format.
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path().context("failed to determine config file path")?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        // Serialize using the new multi-printer format
        let save_config = SaveConfig {
            printers: self.all_printers(),
        };
        let content =
            toml::to_string_pretty(&save_config).with_context(|| "Failed to serialize config")?;

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

    /// Returns all configured printers as a Vec in deterministic order.
    ///
    /// This combines the primary `printer` field with any `extra_printers`.
    /// This is the preferred way to access all printer configurations.
    ///
    /// # Ordering
    ///
    /// Printers are returned in a stable, deterministic order:
    /// 1. The primary printer (index 0)
    /// 2. Extra printers in the order they were added/loaded (indices 1, 2, ...)
    ///
    /// This order matches the `[[printers]]` array order in the config file and
    /// is preserved across application restarts.
    pub fn all_printers(&self) -> Vec<PrinterConfig> {
        let mut all = Vec::with_capacity(1 + self.extra_printers.len());
        all.push(self.printer.clone());
        all.extend(self.extra_printers.iter().cloned());
        all
    }

    /// Returns a reference to the slice of extra printers (excluding the primary).
    ///
    /// Use `all_printers()` to get all printers including the primary.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn extra_printers(&self) -> &[PrinterConfig] {
        &self.extra_printers
    }

    /// Adds an extra printer to the configuration.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn add_printer(&mut self, printer: PrinterConfig) {
        self.extra_printers.push(printer);
    }
}

impl PrinterConfig {
    /// Returns the display name for this printer.
    ///
    /// If a friendly name is set, returns that. Otherwise, returns the serial number.
    #[allow(dead_code)] // Will be used by UI for printer selection
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.serial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_legacy_format() {
        let content = r#"
[printer]
ip = "192.168.1.100"
serial = "01P00A000000000"
access_code = "12345678"
"#;

        let config = Config::parse(content).expect("Failed to parse legacy config");

        assert_eq!(config.all_printers().len(), 1);
        assert_eq!(config.printer.ip, "192.168.1.100");
        assert_eq!(config.printer.serial, "01P00A000000000");
        assert_eq!(config.printer.access_code, "12345678");
        assert_eq!(config.printer.port, DEFAULT_MQTT_PORT);
        assert!(config.printer.name.is_none());
    }

    #[test]
    fn test_parse_legacy_format_with_port() {
        let content = r#"
[printer]
ip = "192.168.1.100"
serial = "01P00A000000000"
access_code = "12345678"
port = 9000
"#;

        let config = Config::parse(content).expect("Failed to parse legacy config with port");

        assert_eq!(config.printer.port, 9000);
    }

    #[test]
    fn test_parse_new_single_printer_format() {
        let content = r#"
[[printers]]
name = "Office P1S"
ip = "192.168.1.100"
serial = "01P00A000000000"
access_code = "12345678"
"#;

        let config = Config::parse(content).expect("Failed to parse new single printer config");

        assert_eq!(config.all_printers().len(), 1);
        assert_eq!(config.printer.name.as_deref(), Some("Office P1S"));
        assert_eq!(config.printer.ip, "192.168.1.100");
        assert_eq!(config.printer.serial, "01P00A000000000");
        assert_eq!(config.printer.access_code, "12345678");
        assert_eq!(config.printer.port, DEFAULT_MQTT_PORT);
    }

    #[test]
    fn test_parse_new_multi_printer_format() {
        let content = r#"
[[printers]]
name = "Office P1S"
ip = "192.168.1.100"
serial = "01P00A000000000"
access_code = "12345678"

[[printers]]
name = "Workshop X1C"
ip = "192.168.1.101"
serial = "00M00B111111111"
access_code = "87654321"
"#;

        let config = Config::parse(content).expect("Failed to parse multi-printer config");

        let all = config.all_printers();
        assert_eq!(all.len(), 2);

        assert_eq!(all[0].name.as_deref(), Some("Office P1S"));
        assert_eq!(all[0].ip, "192.168.1.100");
        assert_eq!(all[0].serial, "01P00A000000000");

        assert_eq!(all[1].name.as_deref(), Some("Workshop X1C"));
        assert_eq!(all[1].ip, "192.168.1.101");
        assert_eq!(all[1].serial, "00M00B111111111");

        // Verify primary printer is first one
        assert_eq!(config.printer.ip, "192.168.1.100");
        // Verify extra_printers has the second one
        assert_eq!(config.extra_printers().len(), 1);
        assert_eq!(config.extra_printers()[0].ip, "192.168.1.101");
    }

    #[test]
    fn test_parse_new_format_without_name() {
        let content = r#"
[[printers]]
ip = "192.168.1.100"
serial = "01P00A000000000"
access_code = "12345678"
"#;

        let config = Config::parse(content).expect("Failed to parse config without name");

        assert!(config.printer.name.is_none());
        assert_eq!(config.printer.ip, "192.168.1.100");
    }

    #[test]
    fn test_serialize_to_new_format() {
        let config = Config {
            printer: PrinterConfig {
                name: Some("My Printer".to_string()),
                ip: "192.168.1.100".to_string(),
                serial: "01P00A000000000".to_string(),
                access_code: "12345678".to_string(),
                port: DEFAULT_MQTT_PORT,
            },
            extra_printers: vec![],
        };

        let save_config = SaveConfig {
            printers: config.all_printers(),
        };
        let serialized = toml::to_string_pretty(&save_config).expect("Failed to serialize");

        // Verify it uses the new [[printers]] format
        assert!(serialized.contains("[[printers]]"));
        assert!(serialized.contains("name = \"My Printer\""));
        assert!(serialized.contains("ip = \"192.168.1.100\""));

        // Verify it doesn't use the old [printer] format
        assert!(!serialized.contains("[printer]"));
    }

    #[test]
    fn test_serialize_multiple_printers() {
        let config = Config {
            printer: PrinterConfig {
                name: Some("Printer 1".to_string()),
                ip: "192.168.1.100".to_string(),
                serial: "SERIAL1".to_string(),
                access_code: "CODE1".to_string(),
                port: DEFAULT_MQTT_PORT,
            },
            extra_printers: vec![PrinterConfig {
                name: Some("Printer 2".to_string()),
                ip: "192.168.1.101".to_string(),
                serial: "SERIAL2".to_string(),
                access_code: "CODE2".to_string(),
                port: DEFAULT_MQTT_PORT,
            }],
        };

        let save_config = SaveConfig {
            printers: config.all_printers(),
        };
        let serialized = toml::to_string_pretty(&save_config).expect("Failed to serialize");

        // Count occurrences of [[printers]]
        let count = serialized.matches("[[printers]]").count();
        assert_eq!(count, 2);

        assert!(serialized.contains("name = \"Printer 1\""));
        assert!(serialized.contains("name = \"Printer 2\""));
    }

    #[test]
    fn test_serialize_without_optional_name() {
        let config = Config {
            printer: PrinterConfig {
                name: None,
                ip: "192.168.1.100".to_string(),
                serial: "01P00A000000000".to_string(),
                access_code: "12345678".to_string(),
                port: DEFAULT_MQTT_PORT,
            },
            extra_printers: vec![],
        };

        let save_config = SaveConfig {
            printers: config.all_printers(),
        };
        let serialized = toml::to_string_pretty(&save_config).expect("Failed to serialize");

        // Verify name is not included when None
        assert!(!serialized.contains("name"));
        assert!(serialized.contains("[[printers]]"));
    }

    #[test]
    fn test_display_name_with_friendly_name() {
        let printer = PrinterConfig {
            name: Some("My Cool Printer".to_string()),
            ip: "192.168.1.100".to_string(),
            serial: "01P00A000000000".to_string(),
            access_code: "12345678".to_string(),
            port: DEFAULT_MQTT_PORT,
        };

        assert_eq!(printer.display_name(), "My Cool Printer");
    }

    #[test]
    fn test_display_name_without_friendly_name() {
        let printer = PrinterConfig {
            name: None,
            ip: "192.168.1.100".to_string(),
            serial: "01P00A000000000".to_string(),
            access_code: "12345678".to_string(),
            port: DEFAULT_MQTT_PORT,
        };

        assert_eq!(printer.display_name(), "01P00A000000000");
    }

    #[test]
    fn test_backwards_compatible_construction() {
        // Test that existing code pattern still works
        let config = Config {
            printer: PrinterConfig {
                name: None,
                ip: "192.168.1.1".to_string(),
                serial: "SERIAL".to_string(),
                access_code: "CODE".to_string(),
                port: DEFAULT_MQTT_PORT,
            },
            extra_printers: vec![],
        };

        assert_eq!(config.all_printers().len(), 1);
        assert_eq!(config.printer.ip, "192.168.1.1");
    }

    #[test]
    fn test_add_printer() {
        let mut config = Config {
            printer: PrinterConfig {
                name: Some("P1".to_string()),
                ip: "192.168.1.1".to_string(),
                serial: "S1".to_string(),
                access_code: "C1".to_string(),
                port: DEFAULT_MQTT_PORT,
            },
            extra_printers: vec![],
        };

        config.add_printer(PrinterConfig {
            name: Some("P2".to_string()),
            ip: "192.168.1.2".to_string(),
            serial: "S2".to_string(),
            access_code: "C2".to_string(),
            port: DEFAULT_MQTT_PORT,
        });

        assert_eq!(config.all_printers().len(), 2);
        assert_eq!(config.extra_printers().len(), 1);
    }

    #[test]
    fn test_roundtrip_legacy_to_new_format() {
        // Parse legacy format
        let legacy_content = r#"
[printer]
ip = "192.168.1.100"
serial = "01P00A000000000"
access_code = "12345678"
"#;

        let config = Config::parse(legacy_content).expect("Failed to parse legacy");

        // Serialize to new format
        let save_config = SaveConfig {
            printers: config.all_printers(),
        };
        let new_content = toml::to_string_pretty(&save_config).expect("Failed to serialize");

        // Parse the new format
        let reparsed = Config::parse(&new_content).expect("Failed to reparse");

        // Verify data is preserved
        assert_eq!(reparsed.all_printers().len(), 1);
        assert_eq!(reparsed.printer.ip, "192.168.1.100");
        assert_eq!(reparsed.printer.serial, "01P00A000000000");
        assert_eq!(reparsed.printer.access_code, "12345678");
    }

    #[test]
    fn test_default_port_applied() {
        let content = r#"
[[printers]]
ip = "192.168.1.100"
serial = "SERIAL"
access_code = "CODE"
"#;

        let config = Config::parse(content).expect("Failed to parse");
        assert_eq!(config.printer.port, DEFAULT_MQTT_PORT);
    }

    #[test]
    fn test_custom_port_preserved() {
        let content = r#"
[[printers]]
ip = "192.168.1.100"
serial = "SERIAL"
access_code = "CODE"
port = 9999
"#;

        let config = Config::parse(content).expect("Failed to parse");
        assert_eq!(config.printer.port, 9999);
    }

    #[test]
    fn test_direct_printer_field_access() {
        // Verify backwards compatibility with direct field access
        let content = r#"
[printer]
ip = "192.168.1.100"
serial = "01P00A000000000"
access_code = "12345678"
"#;

        let config = Config::parse(content).expect("Failed to parse");

        // Direct field access should work (backwards compatibility)
        assert_eq!(config.printer.ip, "192.168.1.100");
        assert_eq!(config.printer.serial, "01P00A000000000");
        assert_eq!(config.printer.access_code, "12345678");
    }

    #[test]
    fn test_mutable_printer_field_access() {
        // Verify backwards compatibility with mutable field access
        let mut config = Config {
            printer: PrinterConfig {
                name: None,
                ip: "192.168.1.100".to_string(),
                serial: "SERIAL".to_string(),
                access_code: "CODE".to_string(),
                port: DEFAULT_MQTT_PORT,
            },
            extra_printers: vec![],
        };

        // Mutable field access should work (backwards compatibility)
        config.printer.ip = "192.168.1.200".to_string();
        config.printer.serial = "NEW_SERIAL".to_string();
        config.printer.access_code = "NEW_CODE".to_string();

        assert_eq!(config.printer.ip, "192.168.1.200");
        assert_eq!(config.printer.serial, "NEW_SERIAL");
        assert_eq!(config.printer.access_code, "NEW_CODE");
    }

    #[test]
    fn test_three_printers() {
        let content = r#"
[[printers]]
name = "Printer A"
ip = "192.168.1.1"
serial = "A"
access_code = "1"

[[printers]]
name = "Printer B"
ip = "192.168.1.2"
serial = "B"
access_code = "2"

[[printers]]
name = "Printer C"
ip = "192.168.1.3"
serial = "C"
access_code = "3"
"#;

        let config = Config::parse(content).expect("Failed to parse");

        let all = config.all_printers();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].serial, "A");
        assert_eq!(all[1].serial, "B");
        assert_eq!(all[2].serial, "C");

        assert_eq!(config.extra_printers().len(), 2);
    }

    /// Verifies that printer ordering is preserved through a full round-trip:
    /// parse -> serialize -> reparse. This ensures deterministic ordering across
    /// application restarts.
    #[test]
    fn test_printer_ordering_preserved_through_roundtrip() {
        // Start with a specific order
        let original_content = r#"
[[printers]]
name = "First Printer"
ip = "192.168.1.1"
serial = "FIRST"
access_code = "111"

[[printers]]
name = "Second Printer"
ip = "192.168.1.2"
serial = "SECOND"
access_code = "222"

[[printers]]
name = "Third Printer"
ip = "192.168.1.3"
serial = "THIRD"
access_code = "333"

[[printers]]
name = "Fourth Printer"
ip = "192.168.1.4"
serial = "FOURTH"
access_code = "444"
"#;

        // Parse the original config
        let config = Config::parse(original_content).expect("Failed to parse original");

        // Verify initial ordering
        let printers = config.all_printers();
        assert_eq!(printers.len(), 4);
        assert_eq!(printers[0].serial, "FIRST");
        assert_eq!(printers[1].serial, "SECOND");
        assert_eq!(printers[2].serial, "THIRD");
        assert_eq!(printers[3].serial, "FOURTH");

        // Serialize to new format (simulating a save)
        let save_config = SaveConfig {
            printers: config.all_printers(),
        };
        let serialized = toml::to_string_pretty(&save_config).expect("Failed to serialize");

        // Parse the serialized content (simulating a restart/reload)
        let reloaded = Config::parse(&serialized).expect("Failed to reparse");

        // Verify ordering is preserved after round-trip
        let reloaded_printers = reloaded.all_printers();
        assert_eq!(reloaded_printers.len(), 4);
        assert_eq!(reloaded_printers[0].serial, "FIRST");
        assert_eq!(reloaded_printers[0].name.as_deref(), Some("First Printer"));
        assert_eq!(reloaded_printers[1].serial, "SECOND");
        assert_eq!(reloaded_printers[1].name.as_deref(), Some("Second Printer"));
        assert_eq!(reloaded_printers[2].serial, "THIRD");
        assert_eq!(reloaded_printers[2].name.as_deref(), Some("Third Printer"));
        assert_eq!(reloaded_printers[3].serial, "FOURTH");
        assert_eq!(reloaded_printers[3].name.as_deref(), Some("Fourth Printer"));

        // Verify primary printer is correct
        assert_eq!(reloaded.printer.serial, "FIRST");

        // Verify extra_printers order
        assert_eq!(reloaded.extra_printers().len(), 3);
        assert_eq!(reloaded.extra_printers()[0].serial, "SECOND");
        assert_eq!(reloaded.extra_printers()[1].serial, "THIRD");
        assert_eq!(reloaded.extra_printers()[2].serial, "FOURTH");
    }
}
