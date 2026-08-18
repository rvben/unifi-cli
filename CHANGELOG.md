# Changelog

All notable changes to this project will be documented in this file.

## [0.4.0](https://github.com/rvben/unifi-cli/compare/v0.3.5...v0.4.0) - 2026-08-18

### Added

- add WAN status inventory ([c4f908e](https://github.com/rvben/unifi-cli/commit/c4f908ea5b474b0ef26ca16dd6edc5f4b644edf1))
- add port forward inventory ([9d9b835](https://github.com/rvben/unifi-cli/commit/9d9b835edf33ce70c3c94babc62e25c59b6cc5ce))
- add network detail inspection ([8047cbb](https://github.com/rvben/unifi-cli/commit/8047cbb03fb2104f5c2f0bcbc3d247a11902909a))

### Fixed

- **release**: scope assets to the current tag ([732b4fa](https://github.com/rvben/unifi-cli/commit/732b4faa345750515c8d52396ffe3c4e7d7d046f))

## [0.3.4](https://github.com/rvben/unifi-cli/compare/v0.3.3...v0.3.4) - 2026-08-11

### Breaking Changes

- **protect**: wrap the camera list in the standard items envelope ([e7b77bf](https://github.com/rvben/unifi-cli/commit/e7b77bf06dc64def4de1312bd3a5d6a207e60262))

### Added

- **protect**: wrap the camera list in the standard items envelope ([e7b77bf](https://github.com/rvben/unifi-cli/commit/e7b77bf06dc64def4de1312bd3a5d6a207e60262))

### Fixed

- **devices**: emit the firmware field devices show declares ([0f007c1](https://github.com/rvben/unifi-cli/commit/0f007c186a285b7250d5a539cebc1b561ba2be80))
- **system**: report an unchecked update state as unknown, not up to date ([38492bb](https://github.com/rvben/unifi-cli/commit/38492bb6ff45f62e234bcc7409180b683f9a64c0))
- **protect**: make the remaining reported-flag defaults tri-state ([4decf08](https://github.com/rvben/unifi-cli/commit/4decf08b38a7ed1ef31cfa792d492f3bfc5df76e))
- **clients**: rank and print unknown traffic as unknown, not zero ([cbe0b66](https://github.com/rvben/unifi-cli/commit/cbe0b663fd9529c918c40a91f27e1b504e674984))
- **schema**: publish the fields that say an rtsps create was partial ([9eb083e](https://github.com/rvben/unifi-cli/commit/9eb083e2031e192fceec6bbb1aa09c2fac5a3055))
- **protect**: stop turning absent camera data into confident values ([29cf091](https://github.com/rvben/unifi-cli/commit/29cf091955b5452814957961b066ff4ddc41a8e0))
- **tui**: let a throughput measurement expire instead of standing forever ([d7ca1f8](https://github.com/rvben/unifi-cli/commit/d7ca1f8499461990d6390427cdffa071716ecec8))
- **tui**: stop reporting throughput for a port whose link has gone down ([00a18d8](https://github.com/rvben/unifi-cli/commit/00a18d811e969697b78e7260e5515715c336df3e))
- **tui**: measure port throughput over the window the counters actually changed ([b2ae602](https://github.com/rvben/unifi-cli/commit/b2ae60228f1d8be44a2ae158a4b8a840d371d8fd))
- **cli**: exit quietly when the output pipe closes early ([518fd96](https://github.com/rvben/unifi-cli/commit/518fd963f9c6eaf21b994ec6439ffa16cb6097a9))
- **tui**: aim overlay actions at the entity the panel was opened on ([43fdd32](https://github.com/rvben/unifi-cli/commit/43fdd32abfa71fe1a1e7810e87af9c1033deb224))
- **events**: report an absent event surface as unsupported, not a rejected request ([b3be651](https://github.com/rvben/unifi-cli/commit/b3be65186c65dd0d5849c47f388da63d5eb3a347))

## [0.3.3](https://github.com/rvben/unifi-cli/compare/v0.3.2...v0.3.3) - 2026-08-10

### Fixed

- **api**: decode a response body before judging its content type ([0e49404](https://github.com/rvben/unifi-cli/commit/0e4940467e428f4cea20f54c1eafbc337b6cca00))
- **api**: report an absent controller application as unsupported ([0f5384c](https://github.com/rvben/unifi-cli/commit/0f5384c84caccc3dfcbf5b5b63831cd97eca55b5))

## [0.3.2](https://github.com/rvben/unifi-cli/compare/v0.3.1...v0.3.2) - 2026-08-10

### Fixed

- **config**: write the config atomically on Windows too ([0c2977e](https://github.com/rvben/unifi-cli/commit/0c2977e238ccb09ca2949560568c4f8b8fd528db))
- **config**: replace the config file atomically instead of writing in place ([9ba19fb](https://github.com/rvben/unifi-cli/commit/9ba19fbde8b498488f43b1af1ab1d2771189d672))
- **output**: keep 408 and 429 retryable instead of permanent client errors ([1068bae](https://github.com/rvben/unifi-cli/commit/1068baed75ce232028fec0277c1ca38324723a35))
- **ports**: keep the attachment flag tri-state instead of reading absence as gone ([de2e770](https://github.com/rvben/unifi-cli/commit/de2e770d30cbc3cba64cf5b06640549a9655c776))
- **config**: create the credentials file with owner-only permissions ([8b7789d](https://github.com/rvben/unifi-cli/commit/8b7789d2d87edc5819e950573130000370c6939f))
- **schema**: publish an error and confirmation contract that matches runtime ([5176ccf](https://github.com/rvben/unifi-cli/commit/5176ccfefeb08df4fecd0d7a6d83882f5e06c57f))
- **ports**: report a stale last connection as unattached ([e80cda5](https://github.com/rvben/unifi-cli/commit/e80cda542fce27db385fc43bd69093c265ead464))

## [0.3.1](https://github.com/rvben/unifi-cli/compare/v0.3.0...v0.3.1) - 2026-08-10

### Added

- **ports**: add ports command tree with single-port PoE power-cycling ([b14794e](https://github.com/rvben/unifi-cli/commit/b14794e976638f7c4a5a21639ebe8a8b011ab25c))

### Fixed

- **cli**: prompt before every destructive command, not just ports cycle ([ae49798](https://github.com/rvben/unifi-cli/commit/ae4979886d63b4dd982a3a38de34a0ff343d5f18))
- **schema**: publish flags as boolean and port/watch as integer ([7c21144](https://github.com/rvben/unifi-cli/commit/7c21144d93afa68cb021a9191c769dea4af496ba))
- **ports**: give the PoE class label its colon and align the detail column ([1cd56d0](https://github.com/rvben/unifi-cli/commit/1cd56d03c3e0ec12a025030a051c135a1fa0865c))

## [0.3.0](https://github.com/rvben/unifi-cli/compare/v0.2.3...v0.3.0) - 2026-07-09

### Added

- **clients list**: project `ssid`, `signal`, `uptime`, `network`, `vlan`, `tx_bytes`, `rx_bytes`, `blocked` and `connected_at`. Answering "which SSID is each client on" cost one API call per client; it is now one call for the whole list.
- **networks list**: a `list` subcommand, matching `clients`, `devices` and `events`. Bare `unifi networks` still works.

### Changed

- **clients list**: `ip` is now the address a client currently holds, read from the live `stat/sta` record. It previously came from the integration API's `ipAddress`, which retains the last address a client ever had and outlives the lease. A client with no current lease now reports `ip: null` instead of a stale address, and `clients list` agrees with `clients show`.

### Fixed

- **clients**: stop three silent-failure modes in the agent contract ([5e50bb7](https://github.com/rvben/unifi-cli/commit/5e50bb704777407acf244856bc435f7ceccd7b00))
- **--fields**: an unknown field is now a usage error (exit 2) naming the offender and the valid set, raised before the config loads or a connection opens. It previously returned `{}` per row with exit 0, which is indistinguishable from a query that matched nothing. `schema` and `--fields` read the same field tables, so the published contract and the enforced one cannot drift.

## [0.2.3](https://github.com/rvben/unifi-cli/compare/v0.2.2...v0.2.3) - 2026-07-07

### Fixed

- **events**: fall back to rest/alarm when stat/event is 404 on UniFi OS 9+ ([ac8a69c](https://github.com/rvben/unifi-cli/commit/ac8a69c743c5eab05def7bcd67e08e5b060275ea))

## [0.2.2](https://github.com/rvben/unifi-cli/compare/v0.2.1...v0.2.2) - 2026-06-11

### Added

- **schema**: upgrade to clispec v0.2 schema shape and add compliance features ([25ba5fd](https://github.com/rvben/unifi-cli/commit/25ba5fd365e4124117dcd648d08f4888c118d6a4))

### Fixed

- **schema**: promote --yes to global flag and add conflict error kind ([2da61c5](https://github.com/rvben/unifi-cli/commit/2da61c57b3d4e4863293e795950f6a28e5999dd9))

## [0.2.1](https://github.com/rvben/unifi-cli/compare/v0.2.0...v0.2.1) - 2026-06-03

Thanks to [@captbaritone](https://github.com/captbaritone) for reporting and fixing the `poe_power` decode error ([#3](https://github.com/rvben/unifi-cli/pull/3)).

### Fixed

- **devices**: accept poe_power as JSON string or number from the legacy /stat/device endpoint (#3) ([2cd0b5f](https://github.com/rvben/unifi-cli/commit/2cd0b5fc6547bbb2b77eb8259304114d2a779778))

## [0.2.0](https://github.com/rvben/unifi-cli/compare/v0.1.7...v0.2.0) - 2026-05-23

Thanks to [@l3wi](https://github.com/l3wi) for reporting the TLS certificate validation issue ([#2](https://github.com/rvben/unifi-cli/pull/2)).

### Breaking Changes

- **security**: verify controller TLS certificates by default ([9555a86](https://github.com/rvben/unifi-cli/commit/9555a8671d80a706b7c9ee00215a3cbfcf842d16))

### Added

- **init**: offer to trust self-signed controllers during config init ([c9ee049](https://github.com/rvben/unifi-cli/commit/c9ee04909f5f9d953be1ee36ab7e591df54b2ee1))

### Fixed

- **security**: scope TLS cert detection to the error source chain ([5b440a8](https://github.com/rvben/unifi-cli/commit/5b440a865d21b9b1a9ca3cddd4247c1f64d5c6f2))
- **security**: detect TLS cert failures via the error source chain ([636b051](https://github.com/rvben/unifi-cli/commit/636b05100b4cb26e01b1be2e9f2fae60a625fbaf))
- **deps**: bump ratatui and crossterm to clear RUSTSEC advisories ([639fc73](https://github.com/rvben/unifi-cli/commit/639fc739585fd8f60610184196a4c0dd61b15474))

## [0.1.7](https://github.com/rvben/unifi-cli/compare/v0.1.6...v0.1.7) - 2026-05-21

### Added

- auto-generate schema from clap command tree ([d9fd5b8](https://github.com/rvben/unifi-cli/commit/d9fd5b8a4ea46dcc7cadeed7ab852283b5766bd6))
- mask secrets in config init and add Protect credential prompts ([0a3474c](https://github.com/rvben/unifi-cli/commit/0a3474c9652cc733cf8e1eaf3c354bf262ff473e))
- add Protect camera and RTSPS stream management ([2ea39e7](https://github.com/rvben/unifi-cli/commit/2ea39e795c8cf43a06e453f5b714cb5912a9333e))

### Fixed

- **init**: detect terminal for masked secret input ([1c02f3d](https://github.com/rvben/unifi-cli/commit/1c02f3d5d939e4002ce2b24e8a04ad0030963bfa))

## [0.1.6](https://github.com/rvben/unifi-cli/compare/v0.1.5...v0.1.6) - 2026-04-03

### Added

- **init**: add API key URL, credential validation, and next steps to config init ([e09bd0e](https://github.com/rvben/unifi-cli/commit/e09bd0e7d02a68345f1b67b60ef147f2a847ead2))

## [0.1.5](https://github.com/rvben/unifi-cli/compare/v0.1.4...v0.1.5) - 2026-04-03

## [0.1.4] - 2026-03-20

### Added
- `unifi tui` command (renamed from `top`, which remains as an alias)
- Helpful onboarding errors guiding new users to `unifi config init`
- API key generation hint in error messages

### Changed
- README rewritten with quick-start section, TUI documentation, and all commands
- Fixed config path in README (`~/.config/unifi/` not `unifi-cli/`)
- All README examples use `unifi` binary name (not `unifi-cli`)

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
- CLI: MAC suffixes stripped from display names (e.g., "lobby-bluetooth-proxy" instead of "lobby-bluetooth-proxy 43:3c")

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
