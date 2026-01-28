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

    let config = Config {
        printer: PrinterConfig {
            name: None,
            ip: primary_ip,
            serial: primary_serial,
            access_code: primary_access_code,
            port: crate::config::DEFAULT_MQTT_PORT,
        },
        extra_printers,
    };

    config.save()?;

    let config_path = Config::config_path()?;
    println!();
    println!("Configuration saved to: {}", config_path.display());
    let printer_count = 1 + config.extra_printers.len();
    if printer_count > 1 {
        println!("  {} printers configured.", printer_count);
    }
    println!();

    Ok(config)
}

/// Prompts for and validates an IP address.
fn prompt_ip(label: &str) -> Result<String> {
    loop {
        let input = prompt(label)?;

        // Try to parse as IP address
        match input.parse::<IpAddr>() {
            Ok(_) => return Ok(input),
            Err(_) => {
                println!("  Invalid IP address format. Please enter a valid IPv4 or IPv6 address.");
                println!("  Example: 192.168.1.100");
                continue;
            }
        }
    }
}

/// Prompts for and validates a serial number.
fn prompt_serial(label: &str) -> Result<String> {
    loop {
        let input = prompt(label)?;

        // Bambu serial numbers are typically 15 alphanumeric characters
        if input.len() < 3 {
            println!("  Serial number seems too short. Bambu serial numbers are typically {} characters.", EXPECTED_SERIAL_LENGTH);
            println!("  Example: 01P00A000000000");
            continue;
        }

        // Check for valid characters (alphanumeric only)
        if !input.chars().all(|c| c.is_ascii_alphanumeric()) {
            println!("  Serial number should only contain letters and numbers.");
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

        // Bambu access codes are typically 8 alphanumeric characters
        if input.len() < MIN_ACCESS_CODE_LENGTH {
            println!(
                "  Access code seems too short (minimum {} characters).",
                MIN_ACCESS_CODE_LENGTH
            );
            println!("  Access codes are found in printer settings under LAN mode.");
            continue;
        }

        // Check for valid characters
        if !input.chars().all(|c| c.is_ascii_alphanumeric()) {
            println!("  Access code should only contain letters and numbers.");
            continue;
        }

        return Ok(input);
    }
}

/// Prompts for user input with the given label.
fn prompt(label: &str) -> Result<String> {
    loop {
        print!("{}: ", label);
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
    print!("{}: ", label);
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

/// Prompts for a yes/no response. Returns true for 'y'/'yes', false for 'n'/'no'.
fn prompt_yes_no(label: &str) -> Result<bool> {
    loop {
        print!("{} (y/n): ", label);
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
