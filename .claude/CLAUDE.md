# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
# Build (debug)
cargo build

# Build (release)
cargo build --release

# Run
cargo run

# Run with CLI flags (saves to config)
cargo run -- --ip 192.168.1.100 --serial YOUR_SERIAL --access-code YOUR_CODE

# Reset config and re-run setup wizard
cargo run -- --reset

# Check for errors without building
cargo check

# Run clippy lints
cargo clippy

# Run all tests
cargo test

# Format code
cargo fmt

# Full CI check (format, lint, test)
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Configuration

On first run, a setup wizard prompts for printer details. Config is saved to `~/.config/bambutop/config.toml`:
```toml
[[printers]]
name = "Office P1S"  # optional friendly name
ip = "192.168.1.100"
serial = "YOUR_PRINTER_SERIAL"
access_code = "YOUR_ACCESS_CODE"

[[printers]]  # additional printers
name = "Workshop X1C"
ip = "192.168.1.101"
serial = "ANOTHER_SERIAL"
access_code = "ANOTHER_CODE"
```

Legacy single-printer `[printer]` format is still supported and auto-migrated.

## Architecture

```
src/
├── main.rs      # Entry point, CLI args, terminal setup, main event loop
├── app.rs       # Application state management
├── config.rs    # TOML config file loading/saving from ~/.config/bambutop/
├── wizard.rs    # First-run setup wizard for printer configuration
├── mqtt.rs      # MQTT client connecting to Bambu printer on port 8883 (TLS)
├── printer.rs   # Printer state model and JSON deserialization from MQTT
└── ui/
    ├── mod.rs       # Main layout (header, progress, temps, status panels)
    ├── aggregate.rs # Multi-printer grid view (when multiple printers configured)
    ├── common.rs    # Shared UI utilities (WiFi thresholds, title formatting)
    ├── controls.rs  # Printer controls panel (speed, light, pause/cancel)
    ├── header.rs    # Printer status, WiFi signal, HMS status, light status
    ├── help.rs      # Help overlay showing keyboard shortcuts
    ├── progress.rs  # Print job progress bar, layer info, time remaining, ETA
    ├── status.rs    # AMS filament status display
    ├── temps.rs     # Temperature gauges (nozzle, bed, chamber) and fan speeds
    └── toast.rs     # Toast notifications for command feedback
```

## Key Patterns

- **MQTT Protocol**: Bambu printers use MQTT over TLS (port 8883) with username "bblp" and the printer's access code as password
- **Topics**: Subscribe to `device/{serial}/report`, publish to `device/{serial}/request`
- **State Updates**: Printer sends partial JSON updates; `PrinterState::update_from_message()` merges them incrementally
- **TUI Loop**: Uses crossterm for terminal input, ratatui for rendering. Main loop polls both MQTT events and keyboard input

## Keyboard Shortcuts

- `?` / `h` - Show help overlay
- `x` - Toggle controls lock/unlock
- `+` / `-` - Adjust print speed (Silent/Standard/Sport/Ludicrous)
- `l` - Toggle chamber light
- `w` - Toggle work light
- `Space` - Pause/resume print (requires confirmation)
- `c` - Cancel print (requires confirmation)
- `u` - Toggle temperature unit (Celsius/Fahrenheit)
- `Tab` / `Shift+Tab` - Cycle between printers
- `1-9` - Jump to printer by number
- `a` - Aggregate overview
- `q` / `Esc` - Quit

## Code Style

- **Formatting**: Use `cargo fmt` before committing - configured in `rustfmt.toml`
- **Linting**: Run `cargo clippy` - pedantic lints enabled in `Cargo.toml`
- **Imports**: Avoid wildcard imports (`use module::*`); prefer explicit imports
- **Line width**: 100 characters max
- **Indentation**: 4 spaces

## Testing

- **Unit tests**: Place in same file using `#[cfg(test)]` module
- **Run tests**: `cargo test` runs all unit tests
- **Test naming**: Use descriptive names like `test_display_name_strips_extensions`

## Error Handling

- **Use `anyhow`** for application-level error propagation (already configured)
- **Context**: Add context with `.context("description")` for better error messages
- **Avoid bare `unwrap()`** in production code; use `expect()` with context or proper error handling

## Development Rules

- **Always update README.md** when adding or changing user-facing functionality (new features, CLI flags, keyboard shortcuts, configuration options, etc.)
- **Run `cargo fmt` and `cargo clippy`** before committing
- **Write tests** for new functionality when practical
- **Never add `#[allow(...)]` without asking** - Always prompt the user before adding any `#[allow(..)]` attribute to suppress warnings or lints. Explain what warning is being suppressed and why, then ask if the user wants to suppress it or fix the underlying issue. Prefer fixing the root cause over suppression.

## Rust Coding Standards

Key points for this codebase:

### Constants & Magic Numbers
- **Extract all magic numbers** to named constants with doc comments
- **Group related constants** at module top or in impl blocks
- Examples in this codebase:
  - `KEEPALIVE_SECS`, `RECONNECT_DELAY` in mqtt.rs
  - `MAX_NOZZLE_TEMP`, `MAX_BED_TEMP` in ui/temps.rs
  - `WIFI_STRONG_THRESHOLD`, `WIFI_MEDIUM_THRESHOLD` in ui/header.rs

### String Handling
- **Lookup functions**: Use `&'static str` or `Cow<'static, str>` for functions that return constant strings
  - Use `&'static str` when all return values are static literals
  - Use `Cow<'static, str>` when most are static but some need formatting (e.g., fallback cases)
  - Use `Cow::Borrowed()` for static strings, `Cow::Owned()` only for formatted strings
- **Avoid unnecessary allocations**: Use `as_deref()` instead of `clone().unwrap_or_default()`
- **Use `clone_from()`** when updating strings in place to potentially reuse allocations
- **Render functions**: Prefer `&str` references over owned `String` when displaying text

### Memory Allocation
- **Pre-allocate collections**: Use `Vec::with_capacity()` when the size is known or predictable
- **Cache computed values**: Parse/compute expensive values once (e.g., colors from hex strings) and cache in data structures
- **Hot paths**: Minimize allocations in frequently-called code (render functions, MQTT message handlers)
  - The MQTT message handler in printer.rs uses `clone_from()` for string updates
  - UI render functions use pre-allocated Vecs where size is predictable

### Async Patterns
- **Always use timeouts** for network operations - see `OPERATION_TIMEOUT` in mqtt.rs
- **Wrap async calls** with `tokio::time::timeout()` for subscribe/publish operations
- **Add context** to timeout errors with `.context("description")?`

### Ratatui Patterns
- **Styles**: Use `Style::new()` instead of `Style::default()` for const-friendly construction
- **Deduplication**: Extract repeated widget rendering logic into helper functions (e.g., `TempGaugeConfig`)
- **Lifetimes**: Prefer `&str` and borrowed data in UI code over owned `String`

### Documentation
- **Document public structs** with `///` doc comments explaining purpose
- **Document public functions** with description, parameters, and error conditions
- **Use module-level docs** with `//!` for file headers

### Iterator Usage
- **Prefer iterators**: Use iterator chains over explicit `for` loops where it improves clarity
- **Type hints**: Use `collect::<Vec<_>>()` or turbofish syntax for clear type inference
- **Avoid intermediate allocations**: Chain iterator operations instead of collecting intermediates

## Versioning & Releases

This project follows [Semantic Versioning](https://semver.org/). A version bump and GitHub release are required for every push that includes code changes (changes to `src/`, `Cargo.toml`, or `Cargo.lock`). Docs-only changes do not require a release.

### Semver Rules for This Project

- **PATCH** (0.x.Y): Bug fixes, code cleanup, refactoring, dependency updates, performance improvements, test additions
- **MINOR** (0.X.0): New features — keyboard shortcuts, UI panels, CLI flags, new printer support, config options
- **MAJOR** (X.0.0): Breaking changes — config format changes that break existing files, removed features, changed CLI behavior

When in doubt, ask the user which version bump is appropriate.

### Release Process

1. **Determine version**: Check the current version in `Cargo.toml` and decide MAJOR/MINOR/PATCH bump
2. **Update `Cargo.toml`**: Set the new version string
3. **Include in commit**: The version bump should be part of the work commit (not a separate commit)
4. **Create GitHub release** after pushing:
   ```bash
   gh release create vX.Y.Z --title "vX.Y.Z" --notes "release notes here"
   ```
5. **Release notes**: Use `### Fixes`, `### Features`, or `### Breaking Changes` headers as appropriate. Keep it concise — one bullet per change.

The GitHub Actions workflow (`.github/workflows/release.yml`) automatically builds binaries and updates the Homebrew tap on release creation.

## Session Completion

**When ending a work session**, complete ALL steps below. Work is NOT complete until `git push` succeeds.

1. **Run quality gates** (if code changed) - `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
2. **Version bump** (if code changed) - Bump version in `Cargo.toml` per semver rules above, include in commit
3. **Push to remote** (MANDATORY):
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
4. **Create release** (if code changed) - `gh release create vX.Y.Z --title "vX.Y.Z" --notes "..."`
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed, release created if applicable
7. **Hand off** - Provide context for next session

**Critical**: Work is NOT complete until `git push` succeeds. Never stop before pushing.
