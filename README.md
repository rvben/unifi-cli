# unifi-cli

[![codecov](https://codecov.io/gh/rvben/unifi-cli/graph/badge.svg)](https://codecov.io/gh/rvben/unifi-cli)

CLI for UniFi Network controller with an interactive TUI dashboard. Designed for both human operators and AI agents.

## Quick start

```bash
# Install (pick one)
cargo install unifi-cli        # From source
uvx unifi-cli --help           # Run without installing (via uv)
pip install unifi-cli           # Via pip

# Configure
unifi config init               # Interactive setup (prompts for host + API key)

# Use
unifi clients list              # List connected clients
unifi devices list              # List network devices
unifi tui                       # Interactive dashboard
```

Generate an API key in your UniFi controller under **Settings > API**.

## Installation

### From crates.io

```bash
cargo install unifi-cli
```

### From PyPI

```bash
pip install unifi-cli
# or run without installing:
uvx unifi-cli clients list
```

### From GitHub releases

Pre-built binaries for Linux (x64, arm64), macOS (x64, arm64), and Windows (x64) on the [releases page](https://github.com/rvben/unifi-cli/releases).

## Configuration

Run `unifi config init` for interactive setup, or configure manually:

### Environment variables

```bash
export UNIFI_HOST=https://unifi.example.com
export UNIFI_API_KEY=YOUR_KEY
# Optional for lab controllers with self-signed or otherwise invalid TLS certs:
export UNIFI_ACCEPT_INVALID_CERTS=true
```

### Config file

`~/.config/unifi/config.toml`:

```toml
host = "https://unifi.example.com"
api_key = "YOUR_KEY"
# Optional; defaults to false.
accept_invalid_certs = false
```

### Multi-controller profiles

```toml
[profiles.home]
host = "https://home.example.com"
api_key = "KEY_1"

[profiles.office]
host = "https://office.example.com"
api_key = "KEY_2"
```

```bash
unifi --profile office clients list
# or: UNIFI_PROFILE=office unifi clients list
```

### CLI flags

```bash
unifi --host https://unifi.example.com --api-key YOUR_KEY clients list
```

Priority: CLI flags > environment variables > config file.

TLS certificates are verified by default. For a local controller with a
self-signed certificate, pass `--accept-invalid-certs`, set
`UNIFI_ACCEPT_INVALID_CERTS=true`, or set `accept_invalid_certs = true` in the
config file. Only use this on trusted networks because it weakens protection for
API keys, passwords, session cookies, and stream URLs.

When `unifi config init` cannot verify the controller's certificate, it offers
to trust the controller and saves `accept_invalid_certs = true` for you.

## TUI dashboard

```bash
unifi tui                       # Launch interactive dashboard
```

Real-time dashboard with:
- Client list with bandwidth, connection info, and signal strength
- Device overview with status and firmware versions
- Event feed from the controller
- Client actions: kick, block/unblock, lock/unlock AP
- Device actions: restart, upgrade firmware, locate LED
- Filter clients by name with `/`

### Live port monitor

```bash
unifi devices ports aa:bb:cc:dd:ee:ff --live   # Real-time port stats
```

## Commands

### Clients

```bash
unifi clients list                          # List connected clients
unifi clients list --wired                  # Wired clients only
unifi clients list --wireless --name tasmota  # Filter by type and name
unifi clients list --watch                  # Auto-refresh
unifi clients list --fields name,ssid,ip    # Project specific fields (see `unifi schema`)
unifi clients show aa:bb:cc:dd:ee:ff        # Show client details
unifi clients top                           # Top clients by bandwidth
unifi clients block aa:bb:cc:dd:ee:ff       # Block a client
unifi clients unblock aa:bb:cc:dd:ee:ff     # Unblock a client
unifi clients kick aa:bb:cc:dd:ee:ff        # Disconnect a client
unifi clients set-fixed-ip MAC IP [--name]  # Set DHCP reservation
```

### Devices

```bash
unifi devices list                            # List network devices
unifi devices list --watch                    # Auto-refresh
unifi devices show aa:bb:cc:dd:ee:ff          # Show device details
unifi devices ports aa:bb:cc:dd:ee:ff         # Show switch/router ports
unifi devices restart aa:bb:cc:dd:ee:ff       # Restart a device
unifi devices upgrade aa:bb:cc:dd:ee:ff       # Upgrade firmware
unifi devices locate aa:bb:cc:dd:ee:ff        # Blink locate LED
unifi devices locate aa:bb:cc:dd:ee:ff --off  # Stop blinking
```

### Ports

Find which switch port a device is plugged into, then power-cycle just that
port instead of rebooting the whole switch:

```bash
# Which port is my Pi on? Matches by name (case-insensitive substring),
# MAC, or IP.
unifi ports find allsky
unifi ports find d8:3a:dd:2b:fa:8a

# Inspect it — PoE mode, class, voltage, current, and what's attached
unifi ports show 8c:ed:e1:b0:74:e2 5

# Bounce PoE on that port only — the rest of the switch is untouched
unifi ports cycle 8c:ed:e1:b0:74:e2 5
```

`ports find`'s output feeds directly into `show` and `cycle`: `device_mac`
and `port_idx` are the *switch's* MAC and port index, not the attached
device's. A name matching more than one client returns `kind: conflict`
(exit 6) listing the candidates rather than guessing. A device that has
moved between switch ports appears once per port it has ever used, with a
`connected` field distinguishing its current port from stale history.

`ports show` exposes PoE telemetry the CLI previously discarded:
`poe_mode`, `poe_class`, `poe_voltage`, `poe_current`, `poe_good`, and the
MAC of the attached device (`attached_mac`).

`ports cycle` is destructive. On a terminal it shows what is about to lose
power and asks for confirmation; when piped it requires `--yes` and
otherwise exits 2 with `kind: confirmation_required`. It reads the port
table first, then refuses **without ever sending the power-cycle command**
when:

- the port is not PoE-capable (an SFP+ port, say) → `kind: conflict`, exit 6
- the port's PoE is administratively off → `kind: conflict`, exit 6
- the port isn't currently delivering PoE (`poe_enable: false`) → `kind: conflict`, exit 6
- the device has no such port index → `kind: not_found`, exit 4

The off interval — how long the port stays unpowered — is chosen by the
switch firmware, not by this CLI. The power-cycle command takes only the
target port, with no duration parameter, on either the legacy endpoint or
the Integration API, so the interval isn't configurable and varies by
device model and firmware version. IEEE 802.3 PoE detection timing imposes
a floor regardless: expect the port to sit dark for roughly 1-2 seconds at
minimum before power returns.

List ports for one device, or across every device:

```bash
unifi ports list 8c:ed:e1:b0:74:e2
unifi ports list --limit 20 --fields port_idx,poe_power
```

`ports list` returns the paginated `{items, total, limit, offset}` envelope
used by the other list commands. `unifi devices ports <MAC>` remains an
alias for `unifi ports list <MAC>`; it keeps its original bare-JSON-array
shape for backward compatibility, and both emit the same per-row fields,
including `device_mac` and `device_name`.

### Events

```bash
unifi events list                           # Recent controller events
unifi events list --limit 50                # Last 50 events
```

### Networks

```bash
unifi networks list                         # List all networks
unifi networks                              # Same thing
```

### System

```bash
unifi system health                         # Show subsystem health
unifi system info                           # Show controller info
```

### Configuration

```bash
unifi config init                           # Interactive setup
unifi config check                          # Verify connectivity and API key
```

### Shell completions

```bash
unifi completions zsh --install             # Install zsh completions
unifi completions bash --install            # Install bash completions
unifi completions fish --install            # Install fish completions
```

## Agent-friendly design

unifi-cli is designed to work well with AI agents and automation scripts.

### Automatic JSON output

When stdout is not a terminal (piped or redirected), output switches to JSON automatically:

```bash
# Human at terminal: formatted table
unifi clients list

# Agent piping output: JSON automatically
data=$(unifi clients list)

# Force JSON mode
unifi --json clients list
```

### Clean stdout/stderr separation

Data goes to stdout. Messages go to stderr. Piping always captures clean data:

```bash
unifi clients list > clients.json     # stdout: JSON, stderr: "66 clients"
unifi --quiet clients list            # Suppress stderr messages
```

### Structured mutation responses

```bash
unifi --json clients block aa:bb:cc:dd:ee:ff
# {"action": "block", "mac": "AA:BB:CC:DD:EE:FF", "status": "ok"}
```

### Runtime schema introspection

```bash
unifi schema    # Dumps all commands, arguments, output fields as JSON
```

### Distinct exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Configuration error |
| 3 | Authentication error (401/403) |
| 4 | Not found (404) |
| 5 | API error (server error) |
| 6 | Conflict (ambiguous match or failed precondition) |

## Development

```bash
make check      # Lint and test
make test       # Run tests
make install    # Build and install
```

## License

MIT
