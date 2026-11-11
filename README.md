# unifi-cli

[![codecov](https://codecov.io/gh/rvben/unifi-cli/graph/badge.svg)](https://codecov.io/gh/rvben/unifi-cli)

CLI for UniFi Network controller with an interactive TUI dashboard. Designed for both human operators and AI agents.

## Quick start

```bash
# Install (pick one)
brew install rvben/tap/unifi-cli # macOS or Linux
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

Generate an API key in your controller under **UniFi Network → Integrations**.
Ubiquiti documents the current flow in
[Getting Started with the Official UniFi API](https://help.ui.com/hc/en-us/articles/30076656117655-Getting-Started-with-the-Official-UniFi-API).

## Installation

### With Homebrew

```bash
brew install rvben/tap/unifi-cli
```

The tap provides pre-built binaries for Apple Silicon and Intel Macs, and ARM64
and x86_64 Linux. It installs both `unifi` and `unifi-cli`, with shell completions
for Bash, Zsh, and Fish.

To update:

```bash
brew update
brew upgrade unifi-cli
```


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

### With Nix

The flake supports Linux (x64, arm64) and macOS (Apple Silicon). For Intel Macs, use the pre-built release binaries above; current nixpkgs no longer supports that platform.

Run without installing:

```bash
nix run github:rvben/unifi-cli -- clients list
```

To install it from a flake-based NixOS or Home Manager setup, add the input and have it follow your `nixpkgs`:

```nix
# flake.nix
inputs = {
  nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  unifi-cli = {
    url = "github:rvben/unifi-cli";
    inputs.nixpkgs.follows = "nixpkgs";
  };
};
```

Then add the package. As a Home Manager module:

```nix
{ inputs, pkgs, ... }:

{
  home.packages = [
    inputs.unifi-cli.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

Or as a NixOS module:

```nix
{ inputs, pkgs, ... }:

{
  environment.systemPackages = [
    inputs.unifi-cli.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

Pass `inputs` to your modules with `extraSpecialArgs = { inherit inputs; };` in `home-manager.lib.homeManagerConfiguration`, or `specialArgs = { inherit inputs; };` in `nixpkgs.lib.nixosSystem`. When using Home Manager as a NixOS module, set `home-manager.extraSpecialArgs = { inherit inputs; };` as well.

The package installs both `unifi` and `unifi-cli`. An overlay is also exported as `inputs.unifi-cli.overlays.default`, which adds `pkgs.unifi-cli`.

The flake reads the version from `Cargo.toml` and Rust dependencies from `Cargo.lock`, so it needs no manual updates on release. The committed `flake.lock` pins nixpkgs and the Rust toolchain for standalone builds and development shells; update it deliberately with `nix flake update`.

For development, `nix develop` (or `direnv allow` with nix-direnv installed) provides the Rust toolchain, language server, nextest, and Nix formatter. `nix flake check` builds the package, runs the Rust tests, checks both installed commands and shell completions, and verifies that the overlay respects the consumer’s Rust toolchain. `nix flake check --all-systems --no-build --option allow-import-from-derivation false` evaluates every supported platform without building during evaluation. Run `nix fmt -- --check flake.nix nix/*.nix` to check Nix formatting.

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

Linux: `~/.config/unifi/config.toml`.
macOS: `~/Library/Application Support/unifi/config.toml`.
Windows: `%APPDATA%\unifi\config.toml`.

`unifi config init` writes a new file and renames it over any existing config
instead of writing in place, so a failed write leaves the previous config
intact. On Linux and macOS the new file is created with mode 0600, since it
holds an API key and optionally a Protect password. Because the credentials go
straight into a file that is already 0600, they are never readable by another
local account, not even for the moment between the write and a chmod. On
Windows the file inherits its directory's ACL.

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

### Session authentication for live events

```bash
unifi auth login                         # Prompt for username/password and MFA if required
unifi auth status                        # Verify the saved Network session
unifi --profile office auth login        # Separate session for this host/profile
unifi auth logout                        # Remove only the local saved session
```

`auth login` stores the session in macOS Keychain, Windows Credential Manager,
or Linux Secret Service. The password is used once and is not saved. Sessions
are isolated by controller origin and profile, expire according to controller
policy, and are never written to the config file. An unavailable or locked
credential store produces an error; there is no plaintext fallback. Logout
removes the local entry and does not revoke sessions on the controller. Login
uses `rememberMe: false`, including when saving a session; this does not request
an extended lifetime. Run `auth login` again when the controller expires it.

Existing `UNIFI_USERNAME` / `UNIFI_PASSWORD`, `--username` / `--password`, or
configured credentials can also supply a session login. Prefer the hidden
password prompt to passing a password in shell history or process arguments.
For automation without a credential store, provide both environment variables
to `events watch --login`; this creates an in-memory session for that invocation and
saves nothing. When required, `auth login` prompts for a hidden authenticator
code and completes the controller's MFA challenge. Codes and challenge cookies
are used only in memory and are never saved. Noninteractive callers can supply
a current `UNIFI_MFA_CODE` with their login credentials; no TOTP seed is needed
or stored. Codes are single-use and short-lived; for repeated runs, prefer an
interactive login followed by the saved session. Interactive `auth login` always
prompts for a fresh code, ignoring any exported `UNIFI_MFA_CODE`. Only one code
submission is attempted, with no automatic password retry or re-login. Push approval, passkeys, and sending email codes are not supported.

Session login requires an HTTPS origin without a URL path or embedded credentials
(HTTP is allowed only on loopback IP addresses for testing). Redirects are refused. The existing
TLS verification policy applies to both login and streaming. Session requests
connect directly to the controller and ignore HTTP proxy environment variables.

TLS certificates are verified by default. For a local controller with a
self-signed certificate, pass `--accept-invalid-certs`, set
`UNIFI_ACCEPT_INVALID_CERTS=true`, or set `accept_invalid_certs = true` in the
config file. Only use this on trusted networks because it weakens protection for
API keys, passwords, session cookies, and stream URLs.

When `unifi config init` cannot verify the controller's certificate, it offers
to trust the controller and saves `accept_invalid_certs = true` for you.

### Destructive commands

`clients block`, `clients unblock`, `clients kick`, `devices restart`,
`devices upgrade`, `ports cycle`, `networks set-dns`, `protect rtsps delete` and `dns delete` all
ask before they act. On a terminal you get a yes/no question naming the target;
declining exits 2 with `kind: confirmation_required` and sends nothing. When
stdin is not a terminal there is nobody to ask, so they refuse with the same
error unless you pass `--yes`, which skips the question everywhere.

`unifi schema` marks exactly these commands `confirmation_required: true`, so a
caller can tell them apart from mutating commands that act immediately
(`devices locate`, `clients set-fixed-ip`, `protect rtsps create`, `dns create`,
`dns update`) without hardcoding the list.

## TUI dashboard

```bash
unifi tui                       # Launch interactive dashboard
```

If the controller is not configured or rejects the saved API key, the dashboard
offers guided configuration in place. Press `Enter` or `a`, complete the secure
prompt outside the alternate screen, and the dashboard resumes automatically.

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
unifi ports find garage-pi
unifi ports find aa:bb:cc:dd:ee:10

# Inspect it: PoE mode, class, voltage, current, and what's attached
unifi ports show aa:bb:cc:dd:ee:ff 5

# Bounce PoE on that port only, leaving the rest of the switch untouched
unifi ports cycle aa:bb:cc:dd:ee:ff 5
```

`ports find`'s output feeds directly into `show` and `cycle`: `device_mac`
and `port_idx` are the *switch's* MAC and port index, not the attached
device's. A name is ambiguous only when it matches more than one device
that's actually on a switch port; that returns `kind: conflict` (exit 6)
listing the candidates rather than guessing. Other client records sharing
the name (a device's WiFi interface reporting under the same name as its
wired one, say) don't cause a conflict if they're not themselves on a port.
A device that has moved between switch ports appears once per port it has
ever used, with a `connected` field distinguishing its current port from
stale history.

`ports show` exposes the port's PoE telemetry: `poe_mode`, `poe_class`,
`poe_voltage`, `poe_current`, `poe_good`, and the MAC of the attached device
(`attached_mac`). The controller keeps a port's last connection record after
the device is unplugged, so `attached_mac` is set only when the controller
affirms the record is live. The MAC is still reported as
`attached_last_seen_mac` and the controller's own flag as `attached_connected`
(true, false, or null when the firmware does not report it), so a caller can
tell "gone" from "not reported". The text output renders the three cases as
`aa:bb:cc:dd:ee:ff`, `- (last seen aa:bb:cc:dd:ee:ff)` and
`unknown (last seen aa:bb:cc:dd:ee:ff)`. Nothing that has been unplugged is
ever presented as currently attached.

`ports cycle` is destructive. On a terminal it shows what is about to lose
power and asks for confirmation; when piped it requires `--yes` and
otherwise exits 2 with `kind: confirmation_required`. It reads the port
table first, then refuses **without ever sending the power-cycle command**
when:

- the port is not PoE-capable (an SFP+ port, say) → `kind: conflict`, exit 6
- the port's PoE is administratively off → `kind: conflict`, exit 6
- the port isn't currently delivering PoE (`poe_enable: false`) → `kind: conflict`, exit 6
- the device has no such port index → `kind: not_found`, exit 4

The off interval (how long the port stays unpowered) is chosen by the
switch firmware, not by this CLI. The power-cycle command takes only the
target port, with no duration parameter, on either the legacy endpoint or
the Integration API, so the interval isn't configurable and varies by
device model and firmware version. IEEE 802.3 PoE detection timing imposes
a floor regardless: expect the port to sit dark for roughly 1-2 seconds at
minimum before power returns.

List ports for one device, or across every device:

```bash
unifi ports list aa:bb:cc:dd:ee:ff
unifi ports list --limit 20 --fields port_idx,poe_power
```

`ports list` returns the paginated `{items, total, limit, offset}` envelope
used by the other list commands. `unifi devices ports <MAC>` remains an
alias for `unifi ports list <MAC>`; it keeps its original bare-JSON-array
shape for backward compatibility, and both emit the same per-row fields,
including `device_mac` and `device_name`.

### Port forwards

```bash
unifi port-forwards list
unifi port-forwards show plex
```

### DNS records

Static DNS records the controller answers locally. Network 10.1+ uses the
official DNS-policies Integration API; older controllers fall back to the v2
`static-dns` API. Domain-forward policies are not listed.

```bash
unifi dns list
unifi dns list --type A
unifi dns show nas.example.com
unifi dns create nas.example.com 192.0.2.10
unifi dns create printer.example.com 2001:db8::10 --type AAAA
unifi dns create media.example.com nas.example.com --type CNAME
unifi dns update nas.example.com --value 192.0.2.11
unifi dns delete nas.example.com
```

`dns delete` is destructive and follows the same confirmation rules as
`clients block`. Create and update act immediately.

The v2 API has no PUT, so `dns update` is refused on a controller that falls
back to it. Editing a record there means `dns delete` followed by
`dns create`, which keeps the delete explicit and confirmed instead of hiding
it inside an update that cannot undo it when the create fails.

### Events

```bash
unifi events list                           # Recent controller events
unifi events list --limit 50                # Last 50 events
unifi events list --limit 10 --offset 10    # Next 10 recent events
unifi events watch                         # Live events and client/device updates
unifi events watch --duration 30            # Stop after 30 seconds
unifi events watch --site default --limit 5 # Stop after 5 messages
```

The command fetches enough recent events to cover both the offset and the
requested page. JSON `total` counts the events retrieved for that window; it is
not a count of the controller's complete event history.

`events watch` uses a UniFi OS session from `auth login`, or an ephemeral login
when `--login` is supplied with username/password credentials. The saved session
takes priority over configured Protect credentials; if no session is saved, a
complete username/password pair can also create a temporary login. An API key alone is insufficient
on some controllers and is never substituted for session authentication. Other
Network commands continue using your API key.

JSON output is JSON Lines: each line is `{"message":"device:sync","data":[...]}`.
These are live update envelopes, distinct from the historical records returned by
`events list`. Credential fields (including `x_authkey`, `x_vwirekey`, `syslog_key`,
`guest_token`, and password/token/secret/cookie fields) are recursively replaced
with `"[REDACTED]"`. Other fields are preserved. This is field-name redaction;
free-form messages and other client/device details can still contain private data.
Terminal output shows client/device update counts and up to three short event
descriptions per message; use `--json` for detailed, redacted envelopes. `--limit` counts
messages, including client/device updates; `--site` takes the legacy site name,
not the Integration API UUID. `--duration` (1 to 86400 seconds) starts after connection.
Ctrl-C, reaching either bound, and a closed output pipe stop cleanly. A controller
disconnect, malformed message, or failed heartbeat returns a structured error;
there is no automatic reconnect that could silently hide missed updates.
Temporary sessions are left to expire under the controller's session policy.

### Networks

```bash
unifi networks list                         # List all networks
unifi networks                              # Same thing
unifi networks show IoT                     # DHCP, DNS, mDNS, and cellular-backup details
unifi networks set-dns IoT 192.0.2.53       # DNS servers handed out by DHCP (up to 4, in order)
unifi networks set-dns IoT --auto           # Hand out the gateway's own DNS again
```

`set-dns` replaces the whole list and leaves every other network setting as it
was. It shows the current and new servers and asks before writing; a network
that already hands out exactly those servers is left alone (`changed: false`).
Clients pick up the change when they renew their DHCP lease.

### System

```bash
unifi system health                         # Show subsystem health
unifi system info                           # Show controller info
```

### WAN

```bash
unifi wan list                              # Show uplinks and failover state
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

| Code | `kind` | Meaning |
|------|--------|---------|
| 0 | - | Success |
| 1 | `general_error` | General error (including transport failures) |
| 2 | `config_error` | Configuration or usage error |
| 2 | `confirmation_required` | A destructive command ran without `--yes` and without a TTY |
| 3 | `auth_error` | Authentication error (401/403) |
| 4 | `not_found` | Not found (404) |
| 4 | `unsupported` | The controller does not serve this endpoint at all: it answered a JSON endpoint with HTML (how UniFi OS reports an application it does not have) or rejected the endpoint itself (how UniFi Network reports one the firmware has dropped); unlike `not_found` there is no other identifier or parameter worth trying |
| 5 | `client_error` | The controller rejected the request itself (4xx other than 401/403/404/408/429); retrying it unchanged cannot help |
| 5 | `retry_later` | The controller invited a retry (429 rate limited, 408 request timeout); back off and send the same request again |
| 5 | `api_error` | The controller failed to serve the request (5xx); may be transient |
| 6 | `conflict` | The request cannot succeed against the resource's current state, refused locally before any API call |

`unifi schema` publishes the same table under `errors`, with a `retryable` flag
per kind, so an agent can branch on it without parsing prose.

## Development

```bash
make check      # Lint and test
make test       # Run tests
make install    # Build and install
```

## License

MIT

## Releasing

Vership owns versioning, changelog generation, release commits, and tags. See
[the release runbook](docs/releases.md) for the verified workflow and recovery policy.
