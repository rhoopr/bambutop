# BambuTop

[![CI](https://github.com/rhoopr/bambutop/actions/workflows/ci.yml/badge.svg)](https://github.com/rhoopr/bambutop/actions/workflows/ci.yml) [![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0) [![GitHub release](https://img.shields.io/github/v/release/rhoopr/bambutop)](https://github.com/rhoopr/bambutop/releases/latest) ![GitHub Downloads](https://img.shields.io/github/downloads/rhoopr/bambutop/total) [![Homebrew](https://img.shields.io/badge/homebrew-tap-FBB040?logo=homebrew)](https://github.com/rhoopr/homebrew-bambutop)

A terminal-based status monitor for Bambu Lab printers. `htop`, but for your 3D printer.

![BambuTop Detail View](screenshot-detail.png?v=3)
![BambuTop Aggregate View](screenshot-aggregate.png?v=3)

## Features

- Job name, layers, elapsed time, time remaining, and ETA clock time
- Print phase display: heating bed, heating nozzle, auto-leveling, printing, etc.
- Visual progress bar with percentage
- Print failure reason and error codes
- Nozzle, bed, and chamber temperatures with visual gauges
- Celsius/Fahrenheit toggle
- Safe chamber temperature range based on active filament type
- Part cooling, auxiliary, chamber, and heatbreak fan speeds
- Speed control: Silent / Standard / Sport / Ludicrous
- Chamber light and work light toggles
- Pause, resume, and cancel with confirmation prompts
- Controls lock to prevent accidental changes
- AMS humidity grade (A-E), filament colors, materials, brand, remaining percentage, and nozzle temp range
- HMS alerts with severity and timestamps
- WiFi signal strength indicator
- Firmware version and nozzle diameter
- AI spaghetti detection, recording, and timelapse indicators
- Desktop notifications for print completions, failures, and HMS alerts
- Multi-printer monitoring with aggregate overview grid
- Demo mode for trying it out without a printer connection

## Supported Printers

| Series | Models |
|--------|--------|
| X Series | X1C, X1E |
| P Series | P1P, P1S, P2S |
| A Series | A1, A1 Mini |
| H Series | H2C, H2S, H2D, H2D Pro |

Any Bambu printer that supports LAN mode should work. If yours isn't listed and it does, open an issue.

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

Run `bambutop` and follow the setup wizard:

```bash
bambutop
```

You'll need three pieces of information per printer:

| Setting | Where to Find It |
|---------|------------------|
| IP Address | Router's connected devices list, or Bambu Studio > Device > Network |
| Serial Number | Printer label, or Bambu Studio > Device info |
| Access Code | Printer screen > Settings > Network > Access Code |

## Multi-Printer Setup

The setup wizard asks if you want to add more printers. You can also edit the config file directly at `~/.config/bambutop/config.toml`:

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

[notifications]
errors = true       # print failures and HMS alerts
completions = true  # print finished
```

Both notification settings default to `true` if omitted. You can also toggle them at runtime with `e` and `n`.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `?` / `h` | Show help overlay |
| `q` / `Esc` | Quit |
| `Tab` | Next printer |
| `Shift+Tab` | Previous printer |
| `1-9` | Jump to printer by number |
| `a` | Aggregate overview |
| `r` | Refresh all printers |
| `u` | Toggle °C / °F |
| `n` | Toggle completion notifications |
| `e` | Toggle error notifications |
| `x` | Lock/unlock controls |
| `l` | Toggle chamber light |
| `w` | Toggle work light |
| `+` / `-` | Adjust print speed |
| `Space` | Pause/resume print |
| `c` | Cancel print |

Controls that affect the printer (`l`, `w`, `+/-`, `Space`, `c`) require unlocking first with `x`. Pause/resume and cancel require pressing the key twice to confirm.

## Command-Line Options

```bash
# First run - setup wizard
bambutop

# Connect directly (saves to config)
bambutop --ip 192.168.1.100 --serial YOUR_SERIAL --access-code YOUR_CODE

# Reset config and re-run setup wizard
bambutop --reset

# Try it out with fake data, no printer needed
bambutop --demo
```

> **Note:** Command-line arguments are visible to other users on the system via `ps`. For persistent use, prefer the config file at `~/.config/bambutop/config.toml` which is created with owner-only permissions.

## Troubleshooting

**"Connection refused" or timeout**
- Verify the printer's IP address is correct
- Make sure your computer is on the same network as the printer
- Try pinging the printer: `ping 192.168.1.100`

**"Authentication failed"**
- Double-check your access code on the printer's screen
- The access code may have changed - regenerate it if needed

**Display looks garbled**
- Make sure your terminal supports Unicode
- Try a different terminal (iTerm2, Ghostty, Alacritty, kitty, etc.)

## Contributing

PRs and bug reports welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## Acknowledgments

This project wouldn't exist without the Bambu Lab community's reverse-engineering work:

- [OpenBambuAPI](https://github.com/Doridian/OpenBambuAPI) - MQTT protocol documentation
- [ha-bambulab](https://github.com/greghesp/ha-bambulab) - Home Assistant integration with field mappings and protocol insights

## License

GPLv3 - see [LICENSE](LICENSE). Full version history in [CHANGELOG.md](CHANGELOG.md).
