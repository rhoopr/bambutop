use crate::config::{Config, PrinterConfig};
use anyhow::Result;
use std::io::{self, Write};

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

    let ip = prompt("Printer IP address")?;
    let serial = prompt("Printer serial number")?;
    let access_code = prompt("Access code")?;

    let config = Config {
        printer: PrinterConfig {
            ip,
            serial,
            access_code,
            port: 8883,
        },
    };

    config.save()?;

    let config_path = Config::config_path()?;
    println!();
    println!("Configuration saved to: {}", config_path.display());
    println!();

    Ok(config)
}

fn prompt(label: &str) -> Result<String> {
    loop {
        print!("{}: ", label);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() {
            println!("  This field is required. Please enter a value.");
            continue;
        }

        return Ok(input);
    }
}
