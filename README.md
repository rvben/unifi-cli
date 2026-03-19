# unifi-cli

CLI for UniFi Network controller. Designed for both human operators and AI agents.

## Installation

### From source

```bash
cargo install unifi-cli
```

### From GitHub releases

Pre-built binaries are available for Linux (x64, arm64), macOS (x64, arm64), and Windows (x64) on the [releases page](https://github.com/rvben/unifi-cli/releases).

## Configuration

Configuration can be provided via CLI flags, environment variables, or a config file.

### CLI flags

```bash
unifi-cli --host https://unifi.example.com --api-key YOUR_KEY clients list
```

### Environment variables

```bash
export UNIFI_HOST=https://unifi.example.com
export UNIFI_API_KEY=YOUR_KEY
unifi-cli clients list
```

### Config file

Create `~/.config/unifi-cli/config.toml`:

```toml
host = "https://unifi.example.com"
api_key = "YOUR_KEY"
```

## Usage

### Clients

```bash
unifi-cli clients list                          # List connected clients
unifi-cli clients show aa:bb:cc:dd:ee:ff        # Show client details
unifi-cli clients block aa:bb:cc:dd:ee:ff       # Block a client
unifi-cli clients unblock aa:bb:cc:dd:ee:ff     # Unblock a client
unifi-cli clients kick aa:bb:cc:dd:ee:ff        # Disconnect a client
unifi-cli clients set-fixed-ip MAC IP [--name]  # Set DHCP reservation
```

### Devices

```bash
unifi-cli devices list                          # List network devices
unifi-cli devices restart aa:bb:cc:dd:ee:ff     # Restart a device
unifi-cli devices locate aa:bb:cc:dd:ee:ff      # Blink locate LED
unifi-cli devices locate aa:bb:cc:dd:ee:ff --off  # Stop blinking
```

### Networks

```bash
unifi-cli networks                              # List all networks
```

### System

```bash
unifi-cli system health                         # Show subsystem health
unifi-cli system info                           # Show controller info
```

## Agent-friendly design

unifi-cli is designed to work well with AI agents and automation scripts. Instead of requiring an MCP server, agents can call the CLI directly with lower overhead and better composability.

### Automatic JSON output

When stdout is not a terminal (piped or redirected), output switches to JSON automatically. No flags needed.

```bash
# Human at terminal: gets a formatted table
unifi-cli clients list

# Agent piping output: gets JSON automatically
data=$(unifi-cli clients list)
```

You can also force JSON mode explicitly:

```bash
unifi-cli --json clients list
```

### Clean stdout/stderr separation

Data goes to stdout. Human messages (summaries, confirmations) go to stderr. This means piping and redirection always capture clean, parseable data.

```bash
# stdout has only the JSON data, stderr has "12 clients"
unifi-cli clients list > clients.json
```

### Quiet mode

Suppress all non-data output with `--quiet`:

```bash
unifi-cli --quiet clients list    # No summary line on stderr
```

### Structured mutation responses

Commands that change state (block, kick, restart, etc.) return structured JSON:

```bash
unifi-cli --json clients block aa:bb:cc:dd:ee:ff
# {"action": "block", "mac": "AA:BB:CC:DD:EE:FF", "status": "ok"}
```

### Runtime schema introspection

The `schema` command dumps all commands, arguments, output fields, and exit codes as JSON. Agents can discover capabilities at runtime without parsing `--help` text.

```bash
unifi-cli schema
```

This outputs the full command tree including which commands are mutating, what arguments they accept, and what fields appear in the JSON output.

### Distinct exit codes

Agents can branch on specific failure modes without parsing error messages:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Configuration error (missing host or API key) |
| 3 | Authentication error (401/403) |
| 4 | Not found (404) |
| 5 | API error (server error) |

### Why CLI over MCP?

For AI agent integrations, a well-designed CLI has several advantages over an MCP server:

- **Token efficiency** -- a CLI call uses ~35x fewer tokens than the MCP tool-call protocol
- **Composability** -- pipe output to `jq`, `grep`, or other tools
- **No server process** -- no sidecar to run, no port to manage
- **Universal** -- works from any language, shell, or automation framework

## Development

```bash
make check      # Lint and test
make test       # Run tests
make install    # Build and install to ~/.local/bin
```

## License

MIT
