# BambuTop


[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0) [![GitHub release](https://img.shields.io/github/v/release/rhoopr/bambutop)](https://github.com/rhoopr/bambutop/releases/latest)   [![Homebrew](https://img.shields.io/badge/homebrew-todo-orange?logo=homebrew)](#) 

 [![Vibe Coded](https://img.shields.io/badge/vibe-coded%20%E2%9C%A8-ff69b4)](#) [![Claude Code](https://img.shields.io/badge/Claude%20Code-D97757?logo=claude&logoColor=fff)](https://claude.com/product/claude-code) [![Bambu Lab](https://img.shields.io/badge/Bambu%20Lab-00AE42?style=flat&logo=data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI+PHBhdGggZmlsbD0id2hpdGUiIGQ9Ik0xMiAyTDIgN3Y2YzAgNS41NSAzLjg0IDEwLjc0IDEwIDEyIDYuMTYtMS4yNiAxMC02LjQ1IDEwLTEyVjdsLTEwLTV6Ii8+PC9zdmc+)](https://bambulab.com/) [![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/) [![TUI](https://img.shields.io/badge/TUI-Terminal%20App-orange)](#)

A terminal-based status monitor for Bambu Lab printers. top/htop/btop/*top, but for your 3D printer. 

![BambuTop Screenshot](screenshot.png)<p align=center>Screenshot in [![Ghostty](https://custom-icon-badges.demolab.com/badge/Ghostty-0000ff?logo=ghostty_term)](https://github.com/ghostty-org/ghostty)</p>

## Features

- Real-time printer status monitoring
- Print progress with job name, speed, layer count, and time remaining
- Temperature monitoring (chamber, nozzle, bed) with visual gauges
- Fan speeds (part cooling, auxiliary, chamber)
- AMS status with humidity levels and filament info
- HMS error notifications

## Supported Printers

- **P Series:** P1P, P1S, P2S
- **X Series:** X1C, X1E
- **A Series:** A1, A1 Mini
- **H Series:** H2C, H2S, H2D, H2D Pro

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

On first run, BambuTop will launch a setup wizard to configure your printer connection. The config is saved to `~/.config/bambutop/config.toml`.

You can also run with command-line flags to skip the config file entirely:

```bash
bambutop --ip 192.168.1.100 --serial YOUR_SERIAL --access-code YOUR_CODE
```

Or create the config file manually:

```toml
[printer]
ip = "192.168.1.100"
serial = "YOUR_PRINTER_SERIAL"
access_code = "YOUR_ACCESS_CODE"
```

### Finding Your Printer Details

1. **IP Address:** Check your router's connected devices, or find it in Bambu Studio under Device > Network
2. **Serial Number:** Found on the printer's label or in Bambu Studio under Device info
3. **Access Code:** Found on the printer's screen under Settings > Network > Access Code

## Usage

```bash
bambutop
```

### Command-Line Options

| Flag | Description |
|------|-------------|
| `--ip <IP>` | Printer IP address |
| `--serial <SERIAL>` | Printer serial number |
| `--access-code <CODE>` | Printer access code |
| `--reset` | Delete config and re-run setup wizard |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `q` | Quit |
| `r` | Toggle auto-refresh |

## Requirements

- Your printer must be on the same network as your computer

## Troubleshooting

**"MQTT error: connection refused"**
- Verify your printer's IP address is correct
- Check that your computer can reach the printer (`ping 192.168.1.100`)

**"MQTT error: authentication failed"**
- Double-check your access code
- The access code may have changed - check the printer's screen for the current code

## License

GPLv3 - See [LICENSE](LICENSE) for details.
