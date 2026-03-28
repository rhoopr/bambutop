//! First-run setup wizard for printer configuration.
//!
//! Provides an interactive terminal wizard that prompts users for their
//! Bambu printer's IP address, serial number, and access code. Validates
//! input and saves the configuration for subsequent runs.

use crate::config::{Config, PrinterConfig};
use anyhow::{Context, Result};
use std::io::{self, Write};
use std::net::IpAddr;

/// Minimum length for Bambu access codes
const MIN_ACCESS_CODE_LENGTH: usize = 4;

/// Expected length for Bambu serial numbers
const EXPECTED_SERIAL_LENGTH: usize = 15;

/// Runs the interactive first-run setup wizard.
///
/// Prompts the user for printer IP, serial number, and access code,
/// validates the input, saves the configuration, and returns it.
pub fn run_setup_wizard() -> Result<Config> {
    println!();
    println!("Welcome to bambutop!");
    println!("====================");
    println!();
    println!("No configuration file found. Let's set up your printer connection.");
    println!();
    println!("You'll need the following information from your Bambu printer:");
    println!("  - IP address (found in printer settings or router)");
    println!("  - Serial number (found on printer or in Bambu Studio)");
    println!("  - Access code (found in printer settings under LAN mode)");
    println!();

    let primary_ip = prompt_ip("Printer IP address")?;
    let primary_serial = prompt_serial("Printer serial number")?;
    let primary_access_code = prompt_access_code("Access code")?;

    // Ask if user wants to add more printers
    let mut extra_printers = Vec::new();
    println!();
    while prompt_yes_no("Add another printer?")? {
        println!();
        println!("Setting up additional printer...");
        println!();

        let name = prompt_optional("Printer name (optional, press Enter to skip)")?;
        let ip = prompt_ip("Printer IP address")?;
        let serial = prompt_serial("Printer serial number")?;
        let access_code = prompt_access_code("Access code")?;

        extra_printers.push(PrinterConfig {
            name,
            ip,
            serial,
            access_code,
            port: crate::config::DEFAULT_MQTT_PORT,
        });

        println!();
        println!("Printer added successfully!");
    }

    let mut printers = vec![PrinterConfig {
        name: None,
        ip: primary_ip,
        serial: primary_serial,
        access_code: primary_access_code,
        port: crate::config::DEFAULT_MQTT_PORT,
    }];
    printers.extend(extra_printers);

    let config = Config {
        printers,
        ..Config::default()
    };

    config.save()?;

    let config_path = Config::config_path()?;
    println!();
    println!("Configuration saved to: {}", config_path.display());
    let printer_count = config.printers.len();
    if printer_count > 1 {
        println!("  {printer_count} printers configured.");
    }
    println!();

    Ok(config)
}

/// Prompts for and validates an IP address.
fn prompt_ip(label: &str) -> Result<String> {
    loop {
        let input = prompt(label)?;

        if let Err(msg) = validate_ip(&input) {
            println!("  {msg}");
            println!("  Example: 192.168.1.100");
            continue;
        }

        return Ok(input);
    }
}

/// Prompts for and validates a serial number.
fn prompt_serial(label: &str) -> Result<String> {
    loop {
        let input = prompt(label)?;

        if let Err(msg) = validate_serial(&input) {
            println!("  {msg}");
            if input.len() < 3 {
                println!("  Example: 01P00A000000000");
            }
            continue;
        }

        // Warn but allow if length doesn't match expected
        if input.len() != EXPECTED_SERIAL_LENGTH {
            println!(
                "  Note: Serial number is {} characters (expected {}). Continuing anyway.",
                input.len(),
                EXPECTED_SERIAL_LENGTH
            );
        }

        return Ok(input);
    }
}

/// Prompts for and validates an access code.
fn prompt_access_code(label: &str) -> Result<String> {
    loop {
        let input = prompt(label)?;

        if let Err(msg) = validate_access_code(&input) {
            println!("  {msg}");
            if input.len() < MIN_ACCESS_CODE_LENGTH {
                println!("  Access codes are found in printer settings under LAN mode.");
            }
            continue;
        }

        return Ok(input);
    }
}

/// Prompts for user input with the given label.
fn prompt(label: &str) -> Result<String> {
    loop {
        print!("{label}: ");
        io::stdout()
            .flush()
            .context("Failed to flush stdout during prompt")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("Failed to read user input")?;

        let trimmed = input.trim();
        if trimmed.is_empty() {
            println!("  This field is required. Please enter a value.");
            continue;
        }

        return Ok(trimmed.to_string());
    }
}

/// Prompts for optional user input. Returns None if the user presses Enter without input.
fn prompt_optional(label: &str) -> Result<Option<String>> {
    print!("{label}: ");
    io::stdout()
        .flush()
        .context("Failed to flush stdout during prompt")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read user input")?;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

/// Validates an IP address string. Returns `Ok(())` if valid, `Err` with message if not.
pub(crate) fn validate_ip(input: &str) -> Result<(), &'static str> {
    input
        .parse::<IpAddr>()
        .map(|_| ())
        .map_err(|_| "Invalid IP address format. Please enter a valid IPv4 or IPv6 address.")
}

/// Validates a serial number. Returns `Ok(())` if valid, `Err` with message if not.
/// Non-standard lengths are allowed (the caller may warn separately).
pub(crate) fn validate_serial(input: &str) -> Result<(), &'static str> {
    if input.len() < 3 {
        return Err(
            "Serial number seems too short. Bambu serial numbers are typically 15 characters.",
        );
    }
    if !input.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Serial number should only contain letters and numbers.");
    }
    Ok(())
}

/// Validates an access code. Returns `Ok(())` if valid, `Err` with message if not.
pub(crate) fn validate_access_code(input: &str) -> Result<(), &'static str> {
    if input.len() < MIN_ACCESS_CODE_LENGTH {
        return Err("Access code seems too short (minimum 4 characters).");
    }
    if !input.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Access code should only contain letters and numbers.");
    }
    Ok(())
}

/// Prompts for a yes/no response. Returns true for 'y'/'yes', false for 'n'/'no'.
fn prompt_yes_no(label: &str) -> Result<bool> {
    loop {
        print!("{label} (y/n): ");
        io::stdout()
            .flush()
            .context("Failed to flush stdout during prompt")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("Failed to read user input")?;

        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            "" => {
                println!("  Please enter 'y' for yes or 'n' for no.");
                continue;
            }
            _ => {
                println!("  Invalid response. Please enter 'y' for yes or 'n' for no.");
                continue;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_access_code, validate_ip, validate_serial};

    // --- validate_ip ---

    #[test]
    fn ip_valid_ipv4() {
        assert!(validate_ip("192.168.1.100").is_ok());
    }

    #[test]
    fn ip_valid_ipv4_loopback() {
        assert!(validate_ip("127.0.0.1").is_ok());
    }

    #[test]
    fn ip_valid_ipv4_zeros() {
        assert!(validate_ip("0.0.0.0").is_ok());
    }

    #[test]
    fn ip_valid_ipv6_loopback() {
        assert!(validate_ip("::1").is_ok());
    }

    #[test]
    fn ip_valid_ipv6_full() {
        assert!(validate_ip("fe80::1").is_ok());
    }

    #[test]
    fn ip_valid_ipv6_long() {
        assert!(validate_ip("2001:0db8:85a3:0000:0000:8a2e:0370:7334").is_ok());
    }

    #[test]
    fn ip_empty_string() {
        assert!(validate_ip("").is_err());
    }

    #[test]
    fn ip_hostname_rejected() {
        assert!(validate_ip("printer.local").is_err());
    }

    #[test]
    fn ip_malformed_extra_octets() {
        assert!(validate_ip("192.168.1.1.1").is_err());
    }

    #[test]
    fn ip_malformed_letters() {
        assert!(validate_ip("abc.def.ghi.jkl").is_err());
    }

    #[test]
    fn ip_malformed_octet_out_of_range() {
        assert!(validate_ip("256.1.1.1").is_err());
    }

    #[test]
    fn ip_whitespace_only() {
        assert!(validate_ip("  ").is_err());
    }

    #[test]
    fn ip_with_port() {
        assert!(validate_ip("192.168.1.1:8080").is_err());
    }

    // --- validate_serial ---

    #[test]
    fn serial_valid_standard_length() {
        assert!(validate_serial("01P00A000000000").is_ok());
    }

    #[test]
    fn serial_valid_nonstandard_length() {
        // Shorter than 15 but >= 3 and alphanumeric: valid (caller warns)
        assert!(validate_serial("ABC").is_ok());
        assert!(validate_serial("ABCDEF1234").is_ok());
        assert!(validate_serial("01P00A0000000001234").is_ok());
    }

    #[test]
    fn serial_too_short_empty() {
        assert!(validate_serial("").is_err());
    }

    #[test]
    fn serial_too_short_one_char() {
        assert!(validate_serial("A").is_err());
    }

    #[test]
    fn serial_too_short_two_chars() {
        assert!(validate_serial("AB").is_err());
    }

    #[test]
    fn serial_boundary_three_chars() {
        assert!(validate_serial("ABC").is_ok());
    }

    #[test]
    fn serial_non_alphanumeric_dash() {
        assert!(validate_serial("01P-00A000000000").is_err());
    }

    #[test]
    fn serial_non_alphanumeric_space() {
        assert!(validate_serial("01P 0A000000000").is_err());
    }

    #[test]
    fn serial_non_alphanumeric_underscore() {
        assert!(validate_serial("01P_0A000000000").is_err());
    }

    #[test]
    fn serial_non_alphanumeric_special() {
        assert!(validate_serial("ABC!@#").is_err());
    }

    #[test]
    fn serial_unicode_rejected() {
        assert!(validate_serial("01P00A00000\u{00e9}000").is_err());
    }

    // --- validate_access_code ---

    #[test]
    fn access_code_valid_typical() {
        assert!(validate_access_code("12345678").is_ok());
    }

    #[test]
    fn access_code_valid_minimum_length() {
        assert!(validate_access_code("abcd").is_ok());
    }

    #[test]
    fn access_code_valid_mixed_case() {
        assert!(validate_access_code("AbCd1234").is_ok());
    }

    #[test]
    fn access_code_too_short_empty() {
        assert!(validate_access_code("").is_err());
    }

    #[test]
    fn access_code_too_short_one() {
        assert!(validate_access_code("a").is_err());
    }

    #[test]
    fn access_code_too_short_three() {
        assert!(validate_access_code("abc").is_err());
    }

    #[test]
    fn access_code_non_alphanumeric_dash() {
        assert!(validate_access_code("abcd-efgh").is_err());
    }

    #[test]
    fn access_code_non_alphanumeric_space() {
        assert!(validate_access_code("abcd efgh").is_err());
    }

    #[test]
    fn access_code_non_alphanumeric_special() {
        assert!(validate_access_code("abc!@#$%").is_err());
    }

    #[test]
    fn access_code_unicode_rejected() {
        assert!(validate_access_code("abcd\u{00e9}fgh").is_err());
    }
}
