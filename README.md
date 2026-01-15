# BambuTop

A terminal-based status monitor for Bambu Labs printers. Like htop, but for your 3D printer.

![BambuTop Screenshot](https://github.com/user-attachments/assets/placeholder.png)

## Features

- Real-time printer status monitoring
- Print progress with job name, speed, layer count, and time remaining
- Temperature monitoring (chamber, nozzle, bed) with visual gauges
- Fan speeds (part cooling, auxiliary, chamber)
- AMS status with humidity levels and filament info
- HMS error notifications
- Lightweight terminal UI - works over SSH

## Supported Printers

- Bambu Lab P1P
- Bambu Lab P1S
- Bambu Lab X1
- Bambu Lab X1C
- Bambu Lab X1E
- Bambu Lab A1
- Bambu Lab A1 Mini

## Installation

### Download Prebuilt Binary (Recommended)

Download the latest release for your platform:

**macOS (Apple Silicon):**
```bash
curl -LO https://github.com/rhoopr/bambutop/releases/latest/download/bambutop-macos-aarch64.tar.gz
tar xzf bambutop-macos-aarch64.tar.gz
sudo mv bambutop /usr/local/bin/
```

**macOS (Intel):**
```bash
curl -LO https://github.com/rhoopr/bambutop/releases/latest/download/bambutop-macos-x86_64.tar.gz
tar xzf bambutop-macos-x86_64.tar.gz
sudo mv bambutop /usr/local/bin/
```

**Linux (x86_64):**
```bash
curl -LO https://github.com/rhoopr/bambutop/releases/latest/download/bambutop-linux-x86_64.tar.gz
tar xzf bambutop-linux-x86_64.tar.gz
sudo mv bambutop /usr/local/bin/
```

### Build from Source

Requires [Rust](https://rustup.rs/):

```bash
cargo install --git https://github.com/rhoopr/bambutop.git
```

## Configuration

Create a config file at:
- **macOS:** `~/Library/Application Support/bambutop/config.toml`
- **Linux:** `~/.config/bambutop/config.toml`

```toml
[printer]
ip = "192.168.1.100"
serial = "YOUR_PRINTER_SERIAL"
access_code = "YOUR_ACCESS_CODE"
```

### Finding Your Printer Details

1. **IP Address:** Check your router's connected devices, or find it in Bambu Studio under Device > Network
2. **Serial Number:** Found on the printer's label or in Bambu Studio under Device info
3. **Access Code:** Found on the printer's screen under Settings > Network > Access Code (or LAN Mode)

## Usage

```bash
bambutop
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `q` | Quit |
| `r` | Toggle auto-refresh |

## Requirements

- Your printer must be on the same network as your computer
- LAN Mode must be enabled on the printer (for local MQTT access)

## Troubleshooting

**"Config file not found"**
Create the config file as described in the Configuration section above.

**"MQTT error: connection refused"**
- Verify your printer's IP address is correct
- Ensure LAN Mode is enabled on your printer
- Check that your computer can reach the printer (`ping 192.168.1.100`)

**"MQTT error: authentication failed"**
- Double-check your access code
- The access code may have changed - check the printer's screen for the current code

## License

MIT
