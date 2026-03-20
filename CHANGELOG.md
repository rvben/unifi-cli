# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.3] - 2026-03-20

### Added
- TUI: AP lock/unlock from client overlay (`a` key) with AP picker to select target AP
- TUI: confirmation dialogs before destructive actions (kick, block, restart, upgrade)
- TUI: firmware upgrade availability shown in device overlay (`current → new` version)
- TUI: `u` upgrade action only shown when device has update available
- TUI: AP name displayed in client connection column (resolves MAC to device name)
- TUI: proper viewport scrolling for client list with cursor tracking

### Fixed
- TUI: device panel shows all devices without scrolling, cursor moves through items
- TUI: client list viewport follows cursor instead of cursor staying at top
- CLI: dynamic column widths adapt to data instead of fixed widths causing misalignment
- CLI: MAC suffixes stripped from display names (e.g., "garage-bluetooth-proxy" instead of "garage-bluetooth-proxy 43:3c")

### Changed
- TUI: overlay shortcut hints moved from footer to overlay bottom border
- TUI: signal strength bars split into separate column for vertical alignment
- CLI: replaced `tabled` box tables with clean borderless formatting using `owo-colors`
- CLI: bold headers, dimmed MAC addresses and separators, colored status indicators
- CLI: detail views (`show`, `info`) use labeled key-value format

## [0.1.2] - 2026-03-20

### Added
- Binary renamed from `unifi-cli` to `unifi` (both names still work via `uvx`)
- TUI detail overlays: press Enter on a client or device to see full info popup
- TUI client actions from overlay: kick (`k`), block/unblock (`b`)
- TUI device actions from overlay: restart (`r`), upgrade firmware (`u`), locate/blink LEDs (`l`)
- Connection column in client list showing SSID and signal strength bars for wireless clients
- Non-blocking data fetch via background tasks (UI stays responsive during API calls)
- Loading indicator on TUI startup while connecting to controller
- Controller update availability shown in TUI header and `system info`
- Contextual footer hints based on current mode (overlay, filter, normal)
- `unifi-cli` binary alias for `uvx unifi-cli` compatibility

### Fixed
- TUI dashboard decluttered: removed noisy Rate column, simplified layout
- Client list sorted by cumulative total bytes for stable ordering (no more flickering)
- Device panel sized to content instead of fixed percentage
- Firmware version no longer clipped in device table
- Unnamed clients show full MAC address instead of `-`

### Changed
- Config directory changed from `unifi-cli` to `unifi`

## [0.1.1] - 2026-03-20

### Added
- Interactive TUI dashboard (`unifi-cli tui`) with real-time bandwidth monitoring, event feed, and device/client overview
- Live port monitoring TUI (`devices ports <mac> --live`) with color-coded link speeds, TX/RX rates, and PoE power display
- `events list` command showing recent controller events
- `clients top` command ranking clients by bandwidth usage
- `devices ports <mac>` command showing switch/router port details
- `devices upgrade <mac>` command to trigger firmware upgrades
- `config check` command to verify connectivity and API key validity
- Shell completion installation (`completions <shell> --install`) for zsh, bash, and fish
- Watch mode now uses alternate screen for flicker-free refresh (`clients list --watch`, `devices list --watch`)

### Fixed
- TUI dashboard: VecDeque ring buffer for O(1) event history instead of Vec::remove(0)
- TUI dashboard: rounded borders, empty state messages, interval display in footer
- Truncate helper hardened with proper Unicode boundary handling
- Better error messages with contextual hints for connection, DNS, timeout, TLS, and auth errors

### Changed
- `init` command moved under `config` subcommand group (`config init`)

## [0.1.0] - 2026-03-19

### Added
- Interactive config setup via `unifi-cli init` with confirmation before writing
- Multi-controller support via `--profile` flag and `UNIFI_PROFILE` env var
- `devices show` command with detailed device info from legacy API
- Shell completion generation for bash, zsh, fish, and PowerShell (`completions` command)
- Client filtering: `--wired`, `--wireless`, `--name` flags on `clients list`
- Watch mode: `--watch` / `-w` flag on `clients list` and `devices list`
- MAC format hints on all MAC-accepting commands
- Pre-commit hooks via prek (fmt, clippy, test on push)
- Pre-release verification script (`make verify-release`)

### Fixed
- Exit code mapping uses type downcast instead of fragile string matching
- `--wired` and `--wireless` flags are now mutually exclusive
- MAC addresses normalized in JSON output
- `set-fixed-ip` correctly falls back to POST on 404

### Changed
- Config file supports `[profiles.<name>]` sections (backward-compatible)
- `LegacyDevice` type for richer device data
- Explicit module exports instead of wildcard re-export

## [0.0.4] - 2026-03-19

### Added
- PyPI publishing via maturin binary wheels (`pip install unifi-cli`)
- Source distribution (sdist) build in release workflow
- `skip_pypi` option for manual workflow dispatch

## [0.0.3] - 2026-03-19

### Added
- Agent-friendly CLI design with automatic TTY detection
- `--json` flag auto-enabled when stdout is piped
- `--quiet` flag to suppress non-data output on stderr
- `schema` command for runtime introspection (commands, args, output fields as JSON)
- Structured JSON responses for all mutation commands (block, kick, restart, etc.)
- Distinct exit codes: config (2), auth (3), not-found (4), API error (5)
- Clean stdout/stderr separation (data to stdout, messages to stderr)
- README with usage, configuration, and agent-friendly design documentation

## [0.0.2] - 2026-03-19

### Added
- CI workflow (lint + test on push and PRs)
- Release workflow with cross-platform builds (Linux x64/arm64, macOS x64/arm64, Windows x64)
- Automated crates.io publishing on tag push
- GitHub Releases with archives and SHA256 checksums
- `--version` flag

## [0.0.1] - 2026-03-19

### Added
- Initial release
- Client management: list, show, block, unblock, kick, set-fixed-ip
- Device management: list, restart, locate LED
- Network listing
- System health and info
- JSON output mode (`--json`)
- Config file support (`~/.config/unifi-cli/config.toml`)
- Environment variable support (`UNIFI_HOST`, `UNIFI_API_KEY`)
- MAC address normalization (accepts any format)
- 120+ tests (unit, CLI parsing, mock server integration)

[Unreleased]: https://github.com/rvben/unifi-cli/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/rvben/unifi-cli/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/rvben/unifi-cli/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/rvben/unifi-cli/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rvben/unifi-cli/compare/v0.0.4...v0.1.0
[0.0.4]: https://github.com/rvben/unifi-cli/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/rvben/unifi-cli/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/rvben/unifi-cli/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/rvben/unifi-cli/releases/tag/v0.0.1
