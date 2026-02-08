# BambuTop

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0) [![GitHub release](https://img.shields.io/github/v/release/rhoopr/bambutop)](https://github.com/rhoopr/bambutop/releases/latest) ![GitHub Downloads](https://img.shields.io/github/downloads/rhoopr/bambutop/total) [![Homebrew](https://img.shields.io/badge/homebrew-tap-FBB040?logo=homebrew)](https://github.com/rhoopr/homebrew-bambutop)

A terminal-based status monitor for Bambu Lab printers. `htop`, but for your 3D printer.

![BambuTop Detail View](screenshot-detail.png?v=2)
![BambuTop Aggregate View](screenshot-aggregate.png?v=2)

## Features

**Print Monitoring**
- Job name, layer count, elapsed time, time remaining, and ETA clock time
- Current print phase: Heating Bed, Heating Nozzle, Auto-Leveling, Printing, etc.
- Visual progress bar with percentage

**Temperatures & Fans**
- Nozzle, bed, and chamber temperatures with visual gauges (°C or °F)
- Safe chamber temperature range based on active filament type (PLA, PETG, ABS, etc.)
- Part cooling, auxiliary, chamber, and heatbreak fan speeds

**Printer Controls**
- Adjust speed: Silent / Standard / Sport / Ludicrous
- Toggle chamber light and work light
- Pause, resume, and cancel prints (with confirmation)
- Controls lock to prevent accidental changes

**AMS & Filament**
- Humidity grade (A-E) per AMS unit
- Filament colors, materials, brand, and remaining percentages
- Recommended nozzle temperature range per filament

**System Info**
- HMS (Health Management System) alerts with severity and timestamps
- WiFi signal strength with visual indicator
- Firmware version and nozzle diameter
- AI spaghetti detection, recording, and timelapse indicators

**Multi-Printer Support**
- Monitor multiple printers from a single terminal
- Aggregate overview grid with per-printer status cards
- Navigate with Tab, Shift+Tab, or number keys 1-9

## Supported Printers

| Series | Models |
|--------|--------|
| P Series | P1P, P1S, P2S |
| X Series | X1C, X1E |
| A Series | A1, A1 Mini |
| H Series | H2C, H2S, H2D, H2D Pro |

## Installation

### Homebrew (macOS / Linux)
```bash
brew install rhoopr/bambutop/bambutop
```

### macOS (Apple Silicon)
```bash
curl -LO https://github.com/rhoopr/bambutop/releases/latest/download/bambutop-macos-aarch64.tar.gz
tar xzf bambutop-macos-aarch64.tar.gz
sudo mv bambutop /usr/local/bin/
```

### macOS (Intel)
```bash
curl -LO https://github.com/rhoopr/bambutop/releases/latest/download/bambutop-macos-x86_64.tar.gz
tar xzf bambutop-macos-x86_64.tar.gz
sudo mv bambutop /usr/local/bin/
```

### Linux (x86_64)
```bash
curl -LO https://github.com/rhoopr/bambutop/releases/latest/download/bambutop-linux-x86_64.tar.gz
tar xzf bambutop-linux-x86_64.tar.gz
sudo mv bambutop /usr/local/bin/
```

### Build from Source
```bash
cargo install --git https://github.com/rhoopr/bambutop.git
```

## Getting Started

Run `bambutop` and follow the setup wizard to connect to your printer:

```bash
bambutop
```

You'll need three pieces of information:

| Setting | Where to Find It |
|---------|------------------|
| **IP Address** | Router's connected devices list, or Bambu Studio → Device → Network |
| **Serial Number** | Printer label, or Bambu Studio → Device info |
| **Access Code** | Printer screen → Settings → Network → Access Code |

## Multi-Printer Setup

The setup wizard will ask if you want to add additional printers. You can also edit the config file directly at `~/.config/bambutop/config.toml`:

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

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `?` / `h` | Show help overlay |
| `q` / `Esc` | Quit |
| `Tab` | Next printer |
| `Shift+Tab` | Previous printer |
| `1-9` | Jump to printer by number |
| `a` | Aggregate overview |
| `u` | Toggle °C / °F |
| `x` | Lock/unlock controls |
| `l` | Toggle chamber light |
| `w` | Toggle work light |
| `+` / `-` | Adjust print speed |
| `Space` | Pause/resume print |
| `c` | Cancel print |

Controls that affect the printer (`l`, `w`, `+/-`, `Space`, `c`) require unlocking first with `x`. Pause/resume and cancel require pressing the key twice to confirm.

## Command-Line Options

```bash
# Connect directly (saves to config file)
bambutop --ip 192.168.1.100 --serial YOUR_SERIAL --access-code YOUR_CODE

# Reset config and re-run setup wizard
bambutop --reset
```

## Troubleshooting

**"Connection refused" or timeout**
- Verify the printer's IP address is correct
- Ensure your computer is on the same network as the printer
- Try pinging the printer: `ping 192.168.1.100`

**"Authentication failed"**
- Double-check your access code on the printer's screen
- The access code may have changed — regenerate it if needed

**Display looks garbled**
- Ensure your terminal supports Unicode
- Try a different terminal (iTerm2, Ghostty, Alacritty, kitty, etc.)

## License

GPLv3 — See [LICENSE](LICENSE) for details.
