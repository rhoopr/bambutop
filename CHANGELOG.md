# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-03-28

### Added

- Desktop notifications for print completions, failures, and HMS alerts
- Notification toggles: `n` for completions, `e` for errors
- CI pipeline with fmt, clippy, and test checks on every push and PR
- 116 new unit tests (372 total), covering wizard validation, toast queue, MQTT payloads, and UI helpers

### Changed

- Unified header into a single box with a status-colored border
- Mutex locks now recover from poisoning instead of crashing the app
- `App::new_multi` returns `Result` instead of panicking on empty input
- MQTT subscribe and publish failures are reported as errors instead of silently dropped
- Config file written with owner-only permissions (0o600) since it contains access codes
- Documented CLI `--access-code` visibility in process listings; recommend config file instead

### Fixed

- "All systems normal" shown when a print had actually failed
- AMS temperature unit toggle not applying

### Security

- Documented the TLS certificate verification bypass as an accepted risk ([#20])

## [0.4.2] - 2026-02-16

### Added

- Manual refresh with `r` key (re-subscribes and requests full status from all printers)
- Acknowledgments section in README

### Fixed

- Re-subscribe to MQTT topics after reconnection, fixing stale printer data after network interruptions
- Speed percentage now shows the actual value from MQTT (`spd_mag`) instead of the level's default
- Print failure reason and error codes displayed when a job fails

## [0.4.1] - 2026-02-08

### Added

- Homebrew tap distribution (`brew install rhoopr/bambutop/bambutop`)

### Fixed

- Terminal not restored to normal mode on panic or unexpected exit
- AMS array bounds check for printers reporting unexpected tray counts
- Cleaned up dead code and replaced magic numbers with named constants

## [0.4.0] - 2026-01-29

### Added

- Demo mode (`--demo`) with pre-populated printer data, no connection needed
- Aggregate view card layout improvements

### Changed

- General UI polish and codebase cleanup

## [0.3.0] - 2026-01-22

This was a big release. Multi-printer support, printer controls, and a lot of new UI.

### Added

- Multi-printer monitoring with `[[printers]]` config array (legacy `[printer]` format auto-migrates)
- Aggregate overview grid showing all printers at once
- Keyboard navigation: Tab/Shift+Tab to cycle, 1-9 to jump, `a` for overview
- Setup wizard supports adding multiple printers
- Printer controls: speed adjustment, chamber/work lights, pause/resume/cancel
- Controls lock (`x`) to prevent accidental changes; pause and cancel require double-press to confirm
- Toast notifications for control actions and state changes
- Help overlay (`?` or `h`)
- Print phase display during active jobs (heating bed, heating nozzle, auto-leveling, etc.)
- ETA shown as absolute clock time in local timezone
- HMS error timestamps
- Connection staleness detection with color-coded warning
- Compact printer title showing model and serial suffix
- Smart job name truncation that preserves file extensions
- Graceful MQTT disconnect on exit
- MQTT sequence ID tracking for request/response correlation

### Changed

- Complete UI layout overhaul
- Timezone offset computed once at startup via libc instead of spawning a subprocess
- Connected printer count tracked incrementally (O(1) instead of O(n))

### Fixed

- HMS section showed no data instead of a placeholder before the first report arrived

## [0.2.2] - 2026-01-19

### Changed

- Optimized binary size (release profile: opt-level=z, LTO, strip)
- Reduced heap allocations in hot paths
- Code review remediation across the codebase

### Added

- First unit test suite

## [0.2.1] - 2026-01-19

### Changed

- Applied Rust best practices across the codebase (error handling, naming, structure)

## [0.2.0] - 2026-01-17

### Added

- WiFi signal strength indicator in header
- Multi-AMS unit support (up to 4 units)

### Fixed

- Header layout and AMS display alignment issues

## [0.1.2] - 2026-01-16

### Added

- Interactive setup wizard for first-run configuration
- CLI flags: `--ip`, `--serial`, `--access-code`
- CLI args saved to config file for subsequent runs

### Fixed

- Chamber temperature hidden on printers that don't have an enclosure

## [0.1.1] - 2026-01-16

### Fixed

- Printer model detection from serial number prefixes

## [0.1.0] - 2026-01-15

Initial release.

- Real-time printer monitoring over MQTT with TLS
- Nozzle, bed, and chamber temperatures with visual gauges
- Print progress with job name, layers, elapsed time, and ETA
- AMS filament status (material, color, remaining percentage)
- HMS error display
- Fan speeds (part cooling, auxiliary, heatbreak)
- Celsius/Fahrenheit toggle
- Config file at `~/.config/bambutop/config.toml`
- Automated release builds for macOS (Apple Silicon, Intel) and Linux x86_64

[#20]: https://github.com/rhoopr/bambutop/issues/20
[1.0.0]: https://github.com/rhoopr/bambutop/compare/v0.4.2...v1.0.0
[0.4.2]: https://github.com/rhoopr/bambutop/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/rhoopr/bambutop/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/rhoopr/bambutop/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/rhoopr/bambutop/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/rhoopr/bambutop/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/rhoopr/bambutop/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/rhoopr/bambutop/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/rhoopr/bambutop/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/rhoopr/bambutop/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rhoopr/bambutop/releases/tag/v0.1.0
