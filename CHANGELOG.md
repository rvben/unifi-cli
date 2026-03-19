# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- Shell completion generation for bash, zsh, fish, and PowerShell (`completions` command)
- Client filtering: `--wired`, `--wireless`, `--name` flags on `clients list`
- Watch mode: `--watch` / `-w` flag on `clients list` and `devices list`

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

[Unreleased]: https://github.com/rvben/unifi-cli/compare/v0.0.4...HEAD
[0.0.4]: https://github.com/rvben/unifi-cli/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/rvben/unifi-cli/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/rvben/unifi-cli/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/rvben/unifi-cli/releases/tag/v0.0.1
