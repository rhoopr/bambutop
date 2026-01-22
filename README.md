# BambuTop

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0) [![GitHub release](https://img.shields.io/github/v/release/rhoopr/bambutop)](https://github.com/rhoopr/bambutop/releases/latest)

A terminal-based status monitor for Bambu Lab printers. Think `htop`, but for your 3D printer.

![BambuTop Screenshot](screenshot.png)

## What You Can Monitor

- **Print Progress** — Job name, layer count, time remaining, and ETA clock time (e.g., "1h 30m (ETA 2:45 PM)")
- **Print Phase** — Current activity: Heating Bed, Heating Nozzle, Auto-Leveling, Printing, etc.
- **Temperatures** — Nozzle, bed, and chamber with visual gauges
- **Smart Chamber Display** — Shows safe temperature range based on active filament type (PLA, PETG, ABS, etc.)
- **Fan Speeds** — Part cooling, auxiliary, and chamber fan percentages
- **Printer Controls** — Current speed setting (Silent/Standard/Sport/Ludicrous), chamber light toggle
- **AMS Status** — Humidity grade (A-E), filament colors, materials, and remaining percentages with active slot highlighting
- **HMS Errors** — Health Management System notifications with severity coloring and timestamps
- **Multi-Printer Support** — Monitor multiple printers with Tab/number key navigation
- **Toast Notifications** — Brief feedback messages when commands succeed or fail
- **Help Overlay** — Press `?` or `h` to see all keyboard shortcuts

## Supported Printers

| Series | Models |
|--------|--------|
| P Series | P1P, P1S, P2S |
| X Series | X1C, X1E |
| A Series | A1, A1 Mini |
| H Series | H2C, H2S, H2D, H2D Pro |

## Quick Start

### 1. Download

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

### 2. Run

```bash
bambutop
```

On first run, you'll be guided through a setup wizard to connect to your printer. You can add multiple printers during setup or later by editing the config file.

## Multi-Printer Setup

BambuTop supports monitoring multiple printers simultaneously. The setup wizard will ask if you want to add additional printers after configuring the first one.

Configuration is stored in `~/.config/bambutop/config.toml`:

```toml
[[printers]]
name = "Office P1S"
ip = "192.168.1.100"
serial = "01P00A123456789"
access_code = "12345678"

[[printers]]
name = "Workshop X1C"
ip = "192.168.1.101"
serial = "01S00A987654321"
access_code = "87654321"
```

Use `Tab`/`Shift+Tab` to cycle between printers, or press `1-9` to jump directly.

## Finding Your Printer Details

You'll need three pieces of information from your printer:

| Setting | Where to Find It |
|---------|------------------|
| **IP Address** | Router's connected devices list, or Bambu Studio → Device → Network |
| **Serial Number** | Printer label, or Bambu Studio → Device info |
| **Access Code** | Printer screen → Settings → Network → Access Code |

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `?` / `h` | Show help overlay |
| `x` | Toggle controls lock/unlock |
| `+` / `-` | Adjust print speed (Silent/Standard/Sport/Ludicrous) |
| `l` | Toggle chamber light |
| `Space` | Pause/resume print (requires confirmation) |
| `c` | Cancel print (requires confirmation) |
| `u` | Toggle temperature unit (Celsius/Fahrenheit) |
| `Tab` | Switch to next printer |
| `Shift+Tab` | Switch to previous printer |
| `1-9` | Jump to printer by number |
| `q` / `Esc` | Quit |

## Command-Line Options

```bash
# Run with specific printer (skips config file)
bambutop --ip 192.168.1.100 --serial YOUR_SERIAL --access-code YOUR_CODE

# Reset config and re-run setup wizard
bambutop --reset
```

## Troubleshooting

**"MQTT error: connection refused"**
- Verify your printer's IP address is correct
- Make sure your computer is on the same network as the printer
- Try pinging the printer: `ping 192.168.1.100`

**"MQTT error: authentication failed"**
- Double-check your access code on the printer's screen
- The access code may have changed — regenerate it if needed

**Display looks garbled**
- Make sure your terminal supports Unicode
- Try a different terminal emulator (iTerm2, Ghostty, Alacritty, etc.)

## Building from Source

If you prefer to build from source, you'll need [Rust](https://rustup.rs/):

```bash
cargo install --git https://github.com/rhoopr/bambutop.git
```

## License

GPLv3 — See [LICENSE](LICENSE) for details.
