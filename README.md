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
```

### Config file

`~/.config/unifi/config.toml`:

```toml
host = "https://unifi.example.com"
api_key = "YOUR_KEY"
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

### Events

```bash
unifi events list                           # Recent controller events
unifi events list --limit 50                # Last 50 events
```

### Networks

```bash
unifi networks                              # List all networks
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

## Development

```bash
make check      # Lint and test
make test       # Run tests
make install    # Build and install
```

## License

MIT
