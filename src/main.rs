use unifi_cli::api;
use unifi_cli::commands;
use unifi_cli::fields;
use unifi_cli::fields::InvalidFields;
use unifi_cli::output::{
    OutputConfig, OutputFormat, error_kind_and_code, exit_codes, print_error_envelope, use_color,
};

mod schema;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[command(name = "unifi", version, about = "CLI for UniFi Network controller")]
struct Cli {
    /// UniFi controller host (or set UNIFI_HOST env var)
    #[arg(long, env = "UNIFI_HOST")]
    host: Option<String>,

    /// API key (or set UNIFI_API_KEY env var)
    #[arg(long, env = "UNIFI_API_KEY")]
    api_key: Option<String>,

    /// Username for Protect direct API (or set UNIFI_USERNAME env var)
    #[arg(long, env = "UNIFI_USERNAME")]
    username: Option<String>,

    /// Password for Protect direct API (or set UNIFI_PASSWORD env var)
    #[arg(long, env = "UNIFI_PASSWORD")]
    password: Option<String>,

    /// Accept invalid TLS certificates from the controller (or set UNIFI_ACCEPT_INVALID_CERTS=true)
    #[arg(long, env = "UNIFI_ACCEPT_INVALID_CERTS")]
    accept_invalid_certs: bool,

    /// Config profile to use (or set UNIFI_PROFILE env var)
    #[arg(long, env = "UNIFI_PROFILE")]
    profile: Option<String>,

    /// Output format: auto (TTY detection), text, json
    #[arg(short = 'o', long = "output", global = true, default_value = "auto")]
    output: String,

    /// Output as JSON (alias for --output=json, auto-enabled when stdout is not a terminal)
    #[arg(long, global = true, hide = true)]
    json: bool,

    /// Suppress non-data output (summary lines, confirmations)
    #[arg(long, global = true)]
    quiet: bool,

    /// Skip confirmation prompt for destructive commands (required without a TTY)
    #[arg(long, global = true)]
    yes: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage clients
    #[command(subcommand)]
    Clients(ClientsCommand),

    /// Manage network devices
    #[command(subcommand)]
    Devices(DevicesCommand),

    /// List networks
    Networks {
        /// Defaults to `list`, so bare `unifi networks` keeps working.
        #[command(subcommand)]
        command: Option<NetworksCommand>,
    },

    /// Inspect and manage switch ports
    #[command(subcommand)]
    Ports(PortsCommand),

    /// View controller events
    #[command(subcommand)]
    Events(EventsCommand),

    /// System information
    #[command(subcommand)]
    System(SystemCommand),

    /// Inspect WAN interfaces and failover state
    #[command(subcommand)]
    Wan(WanCommand),

    /// Manage Protect cameras and RTSPS streams
    #[command(subcommand)]
    Protect(ProtectCommand),

    /// Dump all commands and arguments as JSON for agent introspection
    Schema,

    /// Describe supported UniFi applications without loading credentials
    Capabilities,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
        /// Install completions to the standard location for your shell
        #[arg(long)]
        install: bool,
    },

    /// Manage configuration
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Interactive TUI dashboard with real-time bandwidth and device status
    #[command(alias = "top")]
    Tui {
        /// Refresh interval in seconds
        #[arg(short = 'i', long, default_value = "2")]
        interval: u64,
    },
}

#[derive(Subcommand)]
enum ClientsCommand {
    /// List all connected clients
    List {
        /// Show only wired clients
        #[arg(long, conflicts_with = "wireless")]
        wired: bool,
        /// Show only wireless clients
        #[arg(long, conflicts_with = "wired")]
        wireless: bool,
        /// Filter by name (case-insensitive substring match)
        #[arg(long)]
        name: Option<String>,
        /// Refresh every N seconds
        #[arg(short, long, value_name = "SECONDS")]
        watch: Option<u64>,
        /// Maximum number of results to return
        #[arg(long, default_value = "100")]
        limit: usize,
        /// Number of results to skip
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Comma-separated list of fields to include in output (see `unifi schema`)
        #[arg(long)]
        fields: Option<String>,
    },
    /// Show details for a client by MAC address
    Show {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
    },
    /// Set a fixed IP (DHCP reservation) for a client
    SetFixedIp {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
        /// Fixed IP address to assign (e.g., 192.0.2.5)
        ip: String,
        /// Friendly name for the client
        #[arg(long)]
        name: Option<String>,
    },
    /// Block a client
    Block {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
    },
    /// Unblock a client
    Unblock {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
    },
    /// Kick (disconnect) a client
    Kick {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
    },
    /// Show top clients by bandwidth usage
    Top {
        /// Number of clients to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum DevicesCommand {
    /// List all network devices
    List {
        /// Refresh every N seconds
        #[arg(short, long, value_name = "SECONDS")]
        watch: Option<u64>,
        /// Maximum number of results to return
        #[arg(long, default_value = "100")]
        limit: usize,
        /// Number of results to skip
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Comma-separated list of fields to include in output (see `unifi schema`)
        #[arg(long)]
        fields: Option<String>,
    },
    /// Show details for a device by MAC address
    Show {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
    },
    /// Restart a device
    Restart {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
    },
    /// Toggle locate LED on a device
    Locate {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
        /// Turn off locate LED
        #[arg(long)]
        off: bool,
    },
    /// Show switch/router port table
    Ports {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
        /// Live-updating TUI view of port status
        #[arg(long)]
        live: bool,
        /// Refresh interval in seconds (only with --live)
        #[arg(short = 'i', long, default_value = "2")]
        interval: u64,
    },
    /// Trigger firmware upgrade on a device
    Upgrade {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
    },
}

#[derive(Subcommand)]
enum PortsCommand {
    /// List ports for one device, or across all devices
    List {
        /// MAC address of a switch or router. Omit to list every device's ports.
        mac: Option<String>,
        /// Maximum number of results to return
        #[arg(long, default_value = "100")]
        limit: usize,
        /// Number of results to skip
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Comma-separated list of fields to include in output (see `unifi schema`)
        #[arg(long)]
        fields: Option<String>,
        /// Live-updating TUI view of port status (requires MAC)
        #[arg(long, requires = "mac")]
        live: bool,
        /// Refresh interval in seconds (only with --live)
        #[arg(short = 'i', long, default_value = "2")]
        interval: u64,
    },
    /// Show details for a single port
    Show {
        /// MAC address of the switch or router
        mac: String,
        /// Port index (see `unifi ports list <MAC>`)
        port: u32,
    },
    /// Find which switch port a device is attached to
    Find {
        /// MAC address, IP address, or client name
        identifier: String,
        /// Comma-separated list of fields to include in output (see `unifi schema`)
        #[arg(long)]
        fields: Option<String>,
    },
    /// Power-cycle a single PoE port
    Cycle {
        /// MAC address of the switch (not the attached device)
        mac: String,
        /// Port index (see `unifi ports list <MAC>`)
        port: u32,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Create or update the configuration file interactively
    Init,
    /// Verify configuration and test connectivity
    Check,
}

#[derive(Subcommand)]
enum NetworksCommand {
    /// List networks
    List,
}

#[derive(Subcommand)]
enum EventsCommand {
    /// List recent events
    List {
        /// Number of events to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
        /// Number of results to skip
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Comma-separated list of fields to include in output (see `unifi schema`)
        #[arg(long)]
        fields: Option<String>,
    },
}

#[derive(Subcommand)]
enum SystemCommand {
    /// Show system health
    Health,
    /// Show system info
    Info,
}

#[derive(Subcommand)]
enum WanCommand {
    /// List WAN interfaces
    List,
}

#[derive(Subcommand)]
enum ProtectCommand {
    /// Manage cameras
    #[command(subcommand)]
    Cameras(ProtectCamerasCommand),

    /// Manage RTSPS streams for cameras
    #[command(subcommand)]
    Rtsps(ProtectRtspsCommand),
}

#[derive(Subcommand)]
enum ProtectCamerasCommand {
    /// List all Protect cameras
    List {
        /// Use direct Protect API for full details (requires --username/--password)
        #[arg(long)]
        full: bool,
    },
    /// Show details for a camera by ID or name
    Show {
        /// Camera ID (24-char hex) or name (case-insensitive)
        camera: String,
        /// Use direct Protect API for full details (requires --username/--password)
        #[arg(long)]
        full: bool,
    },
}

#[derive(Subcommand)]
enum ProtectRtspsCommand {
    /// List existing RTSPS stream URLs for a camera
    List {
        /// Camera ID (24-char hex) or name (case-insensitive)
        camera: String,
    },
    /// Create new RTSPS streams for a camera
    Create {
        /// Camera ID (24-char hex) or name (case-insensitive)
        camera: String,
        /// Quality levels to create (comma-separated: high,medium,low,package)
        #[arg(short, long, value_delimiter = ',', default_value = "high,medium")]
        quality: Vec<String>,
    },
    /// Delete RTSPS streams for a camera
    Delete {
        /// Camera ID (24-char hex) or name (case-insensitive)
        camera: String,
        /// Quality levels to delete (comma-separated: high,medium,low,package)
        #[arg(short, long, value_delimiter = ',', default_value = "high,medium")]
        quality: Vec<String>,
    },
}

/// Check TTY for destructive commands. When stdin is not a terminal and --yes was not
/// passed, emit a structured error and exit with code 2.
///
/// For commands that ask their question later, once they have loaded enough
/// context to describe what is about to change. Commands that can ask straight
/// away call `require_confirmation` instead.
fn refuse_without_tty(yes: bool, action: &str) {
    use std::io::IsTerminal;
    if !yes && !std::io::stdin().is_terminal() {
        print_error_envelope(
            "confirmation_required",
            &format!("Destructive action '{action}' requires confirmation."),
            Some("Re-run with --yes to confirm."),
        );
        std::process::exit(exit_codes::CONFIRMATION_REQUIRED);
    }
}

/// Gate a destructive command behind a confirmation. `--yes` proceeds. Without a
/// TTY the structured error is emitted and the process exits 2; on a TTY the
/// question is asked and a decline exits 2 the same way.
fn require_confirmation(yes: bool, action: &str, question: &str) {
    refuse_without_tty(yes, action);
    if yes {
        return;
    }
    let mut stdin = std::io::stdin().lock();
    let mut stderr = std::io::stderr();
    let confirmed = confirm_destructive(&mut stdin, &mut stderr, "", question).unwrap_or(false);
    if !confirmed {
        print_error_envelope(
            "confirmation_required",
            "Aborted: confirmation declined.",
            None,
        );
        std::process::exit(exit_codes::CONFIRMATION_REQUIRED);
    }
}

/// Prompt for confirmation of a destructive action. Returns true only on an
/// explicit yes; an empty line, EOF, or anything else declines.
///
/// `summary` is printed above the question when it is non-empty, for callers
/// that already know which port or device they are about to touch.
///
/// Separate from `prompt_line`, which returns `InitError` and belongs to the
/// config-init flow. Reader/writer are injected so this is unit-testable
/// without a TTY.
fn confirm_destructive(
    reader: &mut dyn std::io::BufRead,
    writer: &mut dyn std::io::Write,
    summary: &str,
    question: &str,
) -> std::io::Result<bool> {
    if !summary.is_empty() {
        writeln!(writer, "{summary}")?;
    }
    write!(writer, "{question} (y/N): ")?;
    writer.flush()?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    // The user's Enter is echoed by the terminal, not written to this stream,
    // so without this the prompt line above stays unterminated on our writer.
    // With stdin a TTY and stderr redirected, a decline's error envelope
    // (printed with `eprintln!` right after) would then land on the same
    // physical line as the prompt instead of starting fresh, breaking the
    // "envelope is the last line of stderr" contract (tests/cli_contract.rs,
    // `error_envelope_last_line_is_json` in tests/spec_compliance.rs).
    writeln!(writer)?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn print_schema() {
    schema::print_schema(Cli::command());
}

/// Validate a `--fields` request against the field table the schema publishes
/// for that command. Commands without `--fields` yield `Ok(None)`.
fn validate_requested_fields(command: &Command) -> Result<Option<Vec<String>>, InvalidFields> {
    let (spec, table) = match command {
        Command::Clients(ClientsCommand::List { fields, .. }) => (fields, fields::CLIENTS_LIST),
        Command::Devices(DevicesCommand::List { fields, .. }) => (fields, fields::DEVICES_LIST),
        Command::Events(EventsCommand::List { fields, .. }) => (fields, fields::EVENTS_LIST),
        Command::Ports(PortsCommand::List { fields, .. }) => (fields, fields::PORTS_LIST),
        Command::Ports(PortsCommand::Find { fields, .. }) => (fields, fields::PORTS_FIND),
        _ => return Ok(None),
    };

    match spec {
        Some(spec) => fields::validate(spec, table).map(Some),
        None => Ok(None),
    }
}

fn load_config_from(path: &std::path::Path, profile: Option<&str>) -> ConfigValues {
    if let Ok(contents) = std::fs::read_to_string(path)
        && let Ok(config) = contents.parse::<toml::Table>()
    {
        if let Some(table) = resolve_profile_table(&config, profile) {
            return extract_credentials(table);
        }
        if let Some(name) = profile {
            eprintln!("Warning: Profile '{name}' not found in config");
        }
    }
    ConfigValues::default()
}

fn load_config(profile: Option<&str>) -> ConfigValues {
    let config_path = dirs::config_dir().map(|d| d.join("unifi").join("config.toml"));

    if let Some(path) = config_path {
        return load_config_from(&path, profile);
    }

    ConfigValues::default()
}

fn default_config_path() -> Result<std::path::PathBuf, InitError> {
    dirs::config_dir()
        .map(|d| d.join("unifi").join("config.toml"))
        .ok_or(InitError("Could not determine config directory".into()))
}

#[derive(Debug)]
struct InitError(String);

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for InitError {}

impl From<std::io::Error> for InitError {
    fn from(e: std::io::Error) -> Self {
        InitError(e.to_string())
    }
}

#[derive(Debug, PartialEq)]
enum InitOutcome {
    Saved {
        profile: Option<String>,
        host: String,
        api_key: String,
    },
    Cancelled,
}

fn prompt_line(
    reader: &mut dyn std::io::BufRead,
    writer: &mut dyn std::io::Write,
    prompt: &str,
) -> Result<String, InitError> {
    write!(writer, "{prompt}")?;
    writer.flush()?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn prompt_secret(
    reader: &mut dyn std::io::BufRead,
    writer: &mut dyn std::io::Write,
    prompt: &str,
    use_tty: bool,
) -> Result<String, InitError> {
    if use_tty {
        write!(writer, "{prompt}")?;
        writer.flush()?;
        rpassword::read_password().map_err(|e| InitError(format!("Failed to read secret: {e}")))
    } else {
        // Fallback for tests (reader is not a real terminal)
        prompt_line(reader, writer, prompt)
    }
}

#[derive(Default)]
struct ConfigValues {
    host: Option<String>,
    api_key: Option<String>,
    username: Option<String>,
    password: Option<String>,
    accept_invalid_certs: bool,
}

fn extract_credentials(table: &toml::Table) -> ConfigValues {
    ConfigValues {
        host: table.get("host").and_then(|v| v.as_str()).map(String::from),
        api_key: table
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(String::from),
        username: table
            .get("username")
            .and_then(|v| v.as_str())
            .map(String::from),
        password: table
            .get("password")
            .and_then(|v| v.as_str())
            .map(String::from),
        accept_invalid_certs: table
            .get("accept_invalid_certs")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

fn resolve_profile_table<'a>(
    config: &'a toml::Table,
    profile: Option<&str>,
) -> Option<&'a toml::Table> {
    if let Some(name) = profile {
        config
            .get("profiles")
            .and_then(|v| v.as_table())
            .and_then(|p| p.get(name))
            .and_then(|v| v.as_table())
    } else {
        Some(config)
    }
}

// ── Color / symbol helpers for init flow ──────────────────────────────────

fn sym_ok() -> String {
    if use_color() {
        use owo_colors::OwoColorize;
        "\u{2714}".green().to_string()
    } else {
        "\u{2714}".to_owned()
    }
}

fn sym_fail() -> String {
    if use_color() {
        use owo_colors::OwoColorize;
        "\u{2718}".red().to_string()
    } else {
        "\u{2718}".to_owned()
    }
}

fn sym_dim(s: &str) -> String {
    if use_color() {
        use owo_colors::OwoColorize;
        s.dimmed().to_string()
    } else {
        s.to_owned()
    }
}

fn sym_bold(s: &str) -> String {
    if use_color() {
        use owo_colors::OwoColorize;
        s.bold().to_string()
    } else {
        s.to_owned()
    }
}

fn run_init_with_io(
    reader: &mut dyn std::io::BufRead,
    writer: &mut dyn std::io::Write,
    config_path: &std::path::Path,
    use_tty: bool,
    accept_invalid_certs: bool,
) -> Result<InitOutcome, InitError> {
    // Load existing config, warn if file exists but is corrupt
    let existing = match std::fs::read_to_string(config_path) {
        Ok(contents) => match contents.parse::<toml::Table>() {
            Ok(table) => Some(table),
            Err(e) => {
                writeln!(
                    writer,
                    "Warning: Existing config at {} is invalid TOML: {e}",
                    config_path.display()
                )?;
                writeln!(writer, "A new config will be created.\n")?;
                None
            }
        },
        Err(_) => None,
    };

    // Profile name
    let profile_input = prompt_line(reader, writer, "Profile name (leave empty for default): ")?;
    let profile_name = if profile_input.is_empty() {
        None
    } else {
        Some(profile_input)
    };

    // Current values for this profile/default
    let current = existing
        .as_ref()
        .and_then(|c| resolve_profile_table(c, profile_name.as_deref()))
        .map(extract_credentials)
        .unwrap_or_default();

    // Host prompt
    let host_prompt = match current.host {
        Some(ref h) => format!("Controller host [{h}]: "),
        None => "Controller host (e.g., https://unifi.local): ".to_string(),
    };
    let host_input = prompt_line(reader, writer, &host_prompt)?;
    let host = if host_input.is_empty() {
        current.host.ok_or(InitError("Host is required".into()))?
    } else {
        host_input
    };

    // API key prompt (masked input)
    let key_prompt = match current.api_key {
        Some(ref k) => format!("API key [{}]: ", mask_api_key(k)),
        None => "API key: ".to_string(),
    };
    let key_input = prompt_secret(reader, writer, &key_prompt, use_tty)?;
    let show_key_hint = current.api_key.is_none();
    let api_key = if key_input.is_empty() {
        current
            .api_key
            .ok_or(InitError("API key is required".into()))?
    } else {
        key_input
    };
    if show_key_hint {
        writeln!(
            writer,
            "  Create API keys at: UniFi Network \u{2192} Settings \u{2192} API"
        )?;
    }

    // Optional Protect credentials (username/password for --full commands)
    writeln!(writer)?;
    writeln!(
        writer,
        "Protect direct API credentials (optional, for --full camera details):"
    )?;
    let user_prompt = match current.username {
        Some(ref u) => format!("Username [{u}]: "),
        None => "Username (leave empty to skip): ".to_string(),
    };
    let user_input = prompt_line(reader, writer, &user_prompt)?;
    let username = if user_input.is_empty() {
        current.username.clone()
    } else {
        Some(user_input)
    };

    let password = if username.is_some() {
        let pass_prompt = match current.password {
            Some(_) => "Password [****]: ".to_string(),
            None => "Password: ".to_string(),
        };
        let pass_input = prompt_secret(reader, writer, &pass_prompt, use_tty)?;
        if pass_input.is_empty() {
            current.password.clone()
        } else {
            Some(pass_input)
        }
    } else {
        None
    };
    let accept_invalid_certs = accept_invalid_certs || current.accept_invalid_certs;

    // Show summary and confirm
    let label = profile_name
        .as_deref()
        .map(|n| format!(" (profile: {n})"))
        .unwrap_or_default();
    writeln!(writer)?;
    writeln!(writer, "Configuration{label}:")?;
    writeln!(writer, "  host     = {host}")?;
    writeln!(writer, "  api_key  = {}", mask_api_key(&api_key))?;
    if let Some(ref u) = username {
        writeln!(writer, "  username = {u}")?;
        writeln!(writer, "  password = ****")?;
    }
    if accept_invalid_certs {
        writeln!(writer, "  accept_invalid_certs = true")?;
    }
    writeln!(writer, "  path     = {}", config_path.display())?;

    let confirm = prompt_line(reader, writer, "\nSave? (y/n): ")?;
    if !matches!(confirm.to_lowercase().as_str(), "y" | "yes") {
        writeln!(writer, "Cancelled.")?;
        return Ok(InitOutcome::Cancelled);
    }

    // Build config TOML, preserving existing entries
    let mut config = existing.unwrap_or_default();
    if let Some(ref name) = profile_name {
        let profiles = config
            .entry("profiles")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or(InitError("'profiles' key exists but is not a table".into()))?;

        let mut section = toml::Table::new();
        section.insert("host".into(), toml::Value::String(host.clone()));
        section.insert("api_key".into(), toml::Value::String(api_key.clone()));
        if let Some(ref u) = username {
            section.insert("username".into(), toml::Value::String(u.clone()));
        }
        if let Some(ref p) = password {
            section.insert("password".into(), toml::Value::String(p.clone()));
        }
        if accept_invalid_certs {
            section.insert("accept_invalid_certs".into(), toml::Value::Boolean(true));
        }
        profiles.insert(name.clone(), toml::Value::Table(section));
    } else {
        config.insert("host".into(), toml::Value::String(host.clone()));
        config.insert("api_key".into(), toml::Value::String(api_key.clone()));
        if let Some(ref u) = username {
            config.insert("username".into(), toml::Value::String(u.clone()));
        }
        if let Some(ref p) = password {
            config.insert("password".into(), toml::Value::String(p.clone()));
        }
        if accept_invalid_certs {
            config.insert("accept_invalid_certs".into(), toml::Value::Boolean(true));
        } else {
            config.remove("accept_invalid_certs");
        }
    }

    // Write config
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| InitError(format!("Failed to create config directory: {e}")))?;
    }
    let toml_str = toml::to_string_pretty(&config)
        .map_err(|e| InitError(format!("Failed to serialize config: {e}")))?;
    write_config_file(config_path, &toml_str)?;

    Ok(InitOutcome::Saved {
        profile: profile_name,
        host,
        api_key,
    })
}

/// Set `accept_invalid_certs = true` in an already-written config, targeting the
/// default table or the given profile section.
fn enable_accept_invalid_certs_in_config(
    config_path: &std::path::Path,
    profile: Option<&str>,
) -> Result<(), InitError> {
    let contents = std::fs::read_to_string(config_path)
        .map_err(|e| InitError(format!("Failed to read config: {e}")))?;
    let mut config: toml::Table = contents
        .parse()
        .map_err(|e| InitError(format!("Failed to parse config: {e}")))?;

    if let Some(name) = profile {
        let profiles = config
            .entry("profiles")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or(InitError("'profiles' key exists but is not a table".into()))?;
        let section = profiles
            .entry(name.to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or(InitError("profile section is not a table".into()))?;
        section.insert("accept_invalid_certs".into(), toml::Value::Boolean(true));
    } else {
        config.insert("accept_invalid_certs".into(), toml::Value::Boolean(true));
    }

    let toml_str = toml::to_string_pretty(&config)
        .map_err(|e| InitError(format!("Failed to serialize config: {e}")))?;
    write_config_file(config_path, &toml_str)
}

/// Write the config file so that only its owner can read it.
///
/// The file holds an API key and optionally a password, so it is never written
/// in place. `OpenOptions::mode` applies only to a file the call creates, so
/// opening an existing config would fill a possibly world-readable file with
/// fresh credentials and only tighten it afterwards, leaving a window in which
/// any local account can read the key (and leaving it readable for good if the
/// chmod fails). Instead the content goes into a new 0600 file beside the
/// destination and is renamed over it. The rename is atomic: the credentials
/// exist only inside a 0600 file, and a concurrent reader sees either the old
/// config or the new one, never a half-written one.
///
/// Off unix the write goes through the same temp file and rename, so the config
/// is never left half-written, but the mode is not set: the file takes the
/// inherited ACL of its directory.
fn write_config_file(config_path: &std::path::Path, toml_str: &str) -> Result<(), InitError> {
    use std::io::Write;

    // The temp file must share a directory with the destination, since rename
    // does not cross filesystems.
    let dir = match config_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => std::path::Path::new("."),
    };
    let name = config_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.toml".to_string());
    // Unique per process and per call, so two writers never pick the same name.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp_path = dir.join(format!(".{name}.{}.{seq}.tmp", std::process::id()));

    let open_tmp = || {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(&tmp_path)
    };
    let opened = match open_tmp() {
        // A temp file left behind by a killed run. Reclaim the name;
        // `create_new` still guarantees we write a file we created ourselves.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::remove_file(&tmp_path)
                .map_err(|e| InitError(format!("Failed to write config: {e}")))?;
            open_tmp()
        }
        other => other,
    };

    let write_tmp = |mut file: std::fs::File| -> std::io::Result<()> {
        #[cfg(unix)]
        {
            // The umask can clear bits from the requested mode, so pin it here.
            // The file is still empty, so no secret has reached the disk yet.
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(toml_str.as_bytes())?;
        file.sync_all()
    };

    match opened.and_then(write_tmp) {
        Ok(()) => std::fs::rename(&tmp_path, config_path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            InitError(format!("Failed to write config: {e}"))
        }),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(InitError(format!("Failed to write config: {e}")))
        }
    }
}

/// Prompt for a yes/no answer on stderr (where init status is shown) and read
/// the reply from the given reader. Defaults to no on empty or read error.
fn prompt_yes_no_stderr(reader: &mut dyn std::io::BufRead, prompt: &str) -> bool {
    use std::io::Write;
    eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

async fn run_init(accept_invalid_certs: bool) {
    let path = match default_config_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(exit_codes::GENERAL_ERROR);
        }
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    // Mask secrets only on an interactive terminal; piped input is read from
    // stdin so non-interactive `config init` keeps working.
    let use_tty = std::io::IsTerminal::is_terminal(&stdin);
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    let outcome = match run_init_with_io(
        &mut reader,
        &mut writer,
        &path,
        use_tty,
        accept_invalid_certs,
    ) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(exit_codes::CONFIG_ERROR);
        }
    };

    let (profile, host, api_key) = match outcome {
        InitOutcome::Saved {
            profile,
            host,
            api_key,
        } => (profile, host, api_key),
        InitOutcome::Cancelled => return,
    };

    // Validate credentials against the API. On a TLS certificate failure (common
    // for self-signed UniFi controllers), offer to trust the controller and
    // retry with verification disabled, persisting the choice to config.
    use std::io::Write;
    let mut effective_accept = accept_invalid_certs;
    loop {
        eprint!("  Verifying credentials...");
        std::io::stderr().flush().ok();

        let client = match api::UnifiClient::new_with_options(
            &host,
            &api_key,
            api::ClientOptions {
                accept_invalid_certs: effective_accept,
            },
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(" {} Failed to create client: {e}", sym_fail());
                eprintln!(
                    "  {}",
                    sym_dim("Config saved. Check your settings and re-run 'unifi config init'.")
                );
                break;
            }
        };

        match client.get_health().await {
            Ok(_) => {
                eprintln!(" {} Connected", sym_ok());
                break;
            }
            Err(api::ApiError::Auth(msg)) => {
                eprintln!(" {} Authentication failed: {msg}", sym_fail());
                eprintln!(
                    "  {}",
                    sym_dim("Config saved. Fix your API key and re-run 'unifi config init'.")
                );
                break;
            }
            Err(e) if !effective_accept && use_tty && e.is_tls_cert_error() => {
                eprintln!(" {} certificate verification failed", sym_fail());
                eprintln!();
                eprintln!("  The controller's TLS certificate could not be verified.");
                eprintln!("  This is normal for UniFi controllers with self-signed certs.");
                if !prompt_yes_no_stderr(&mut reader, "  Trust this controller anyway? [y/N]: ") {
                    eprintln!(
                        "  {}",
                        sym_dim(
                            "Config saved. Set accept_invalid_certs to connect to this controller."
                        )
                    );
                    break;
                }
                if let Err(e) = enable_accept_invalid_certs_in_config(&path, profile.as_deref()) {
                    eprintln!(" {} Failed to update config: {e}", sym_fail());
                    break;
                }
                eprintln!("  {}", sym_dim("Saved accept_invalid_certs = true."));
                effective_accept = true;
            }
            Err(e) => {
                eprintln!(" {} Connection failed: {e}", sym_fail());
                eprintln!(
                    "  {}",
                    sym_dim("Config saved. Check your host and re-run 'unifi config init'.")
                );
                break;
            }
        }
    }

    // Next steps
    let label = profile
        .as_deref()
        .map(|n| format!(" (profile: {n})"))
        .unwrap_or_default();
    eprintln!();
    eprintln!(
        "  {} Configuration saved to {}{}",
        sym_ok(),
        path.display(),
        label
    );
    eprintln!();
    eprintln!("  {}:", sym_bold("Next steps"));

    let prefix = profile
        .as_deref()
        .map(|n| format!("unifi --profile {n}"))
        .unwrap_or_else(|| "unifi".to_string());
    eprintln!(
        "    {}   {}",
        sym_dim(&format!("{prefix} system health")),
        sym_dim("# verify connectivity")
    );
    eprintln!(
        "    {}   {}",
        sym_dim(&format!("{prefix} clients list")),
        sym_dim("# list connected clients")
    );
    eprintln!(
        "    {}   {}",
        sym_dim(&format!("{prefix} devices list")),
        sym_dim("# list network devices")
    );
    eprintln!(
        "    {}  {}",
        sym_dim("unifi completions zsh"),
        sym_dim("# shell completions")
    );
    eprintln!();
}

fn mask_api_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        "****".to_string()
    } else {
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[chars.len() - 4..].iter().collect();
        format!("{prefix}…{suffix}")
    }
}

fn install_completions(shell: Shell) {
    let (dir, filename) = match shell {
        Shell::Zsh => {
            let dir = dirs::home_dir()
                .map(|h| h.join(".zsh").join("completions"))
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            (dir, "_unifi".to_string())
        }
        Shell::Bash => {
            let dir = dirs::data_dir()
                .map(|d| d.join("bash-completion").join("completions"))
                .unwrap_or_else(|| {
                    dirs::home_dir()
                        .map(|h| {
                            h.join(".local")
                                .join("share")
                                .join("bash-completion")
                                .join("completions")
                        })
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                });
            (dir, "unifi".to_string())
        }
        Shell::Fish => {
            let dir = dirs::config_dir()
                .map(|d| d.join("fish").join("completions"))
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            (dir, "unifi.fish".to_string())
        }
        _ => {
            eprintln!(
                "--install is not supported for {shell}. Use 'completions {shell}' to print to stdout."
            );
            std::process::exit(exit_codes::GENERAL_ERROR);
        }
    };

    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("Failed to create directory {}: {e}", dir.display());
        std::process::exit(exit_codes::GENERAL_ERROR);
    }

    let path = dir.join(&filename);
    let mut file = match std::fs::File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create {}: {e}", path.display());
            std::process::exit(exit_codes::GENERAL_ERROR);
        }
    };

    clap_complete::generate(shell, &mut Cli::command(), "unifi", &mut file);
    eprintln!("Installed {shell} completions to {}", path.display());

    match shell {
        Shell::Zsh => {
            eprintln!(
                "\nMake sure {} is in your fpath. Add to ~/.zshrc:",
                dir.display()
            );
            eprintln!("  fpath=({}  $fpath)", dir.display());
            eprintln!("  autoload -Uz compinit && compinit");
        }
        Shell::Bash => {
            eprintln!("\nCompletions will be loaded automatically on next login.");
        }
        Shell::Fish => {
            eprintln!("\nCompletions will be loaded automatically on next login.");
        }
        _ => {}
    }
}

async fn require_protect_session(
    host: &str,
    username: &Option<String>,
    password: &Option<String>,
    client_options: api::ClientOptions,
) -> api::ProtectSession {
    let user = username.as_deref().unwrap_or_else(|| {
        eprintln!(
            "Error: --full requires --username (or UNIFI_USERNAME env var, or username in config)"
        );
        std::process::exit(exit_codes::CONFIG_ERROR);
    });
    let pass = password.as_deref().unwrap_or_else(|| {
        eprintln!(
            "Error: --full requires --password (or UNIFI_PASSWORD env var, or password in config)"
        );
        std::process::exit(exit_codes::CONFIG_ERROR);
    });
    match api::ProtectSession::login_with_options(host, user, pass, client_options).await {
        Ok(session) => session,
        Err(e) => {
            let (kind, code) = error_kind_and_code(&e as &dyn std::error::Error);
            print_error_envelope(kind, &format!("Protect login failed: {e}"), None);
            std::process::exit(code);
        }
    }
}

async fn run_config_check(client: &api::UnifiClient) {
    eprintln!("Checking connectivity...\n");

    // Test 1: Can we reach the controller?
    match client.get_health().await {
        Ok(subsystems) => {
            eprintln!("  \u{2714} Connected to controller");
            for s in &subsystems {
                let status = s.status.as_deref().unwrap_or("unknown");
                let icon = if status == "ok" {
                    "\u{2714}"
                } else {
                    "\u{26a0}"
                };
                eprintln!("  {icon} {} subsystem: {status}", s.subsystem);
            }
        }
        Err(api::ApiError::Auth(msg)) => {
            eprintln!("  \u{2718} Authentication failed: {msg}");
            eprintln!("\n  Hint: Check your API key. Generate one in UniFi Settings > API");
            std::process::exit(exit_codes::AUTH_ERROR);
        }
        Err(e) => {
            eprintln!("  \u{2718} Connection failed: {e}");
            std::process::exit(exit_codes::GENERAL_ERROR);
        }
    }

    // Test 2: Can we fetch sysinfo?
    match client.get_sysinfo().await {
        Ok(info) => {
            let host = info.hostname.as_deref().unwrap_or("unknown");
            let ver = info.version.as_deref().unwrap_or("unknown");
            eprintln!("\n  Controller: {host} v{ver}");
        }
        Err(_) => {
            eprintln!("\n  \u{26a0} Could not fetch system info (non-critical)");
        }
    }

    eprintln!("\nConfiguration is valid.");
}

/// Restore the default disposition for SIGPIPE, which Rust ignores at startup.
///
/// While it is ignored, the first write after a reader closes the pipe fails
/// with EPIPE and the standard library turns that into a panic. `unifi clients
/// list | head -5`, or any consumer that exits early, would then print a stack
/// trace and exit 101, which reads as this tool failing rather than as the
/// ordinary end of a pipeline. With the default restored the process is simply
/// terminated by the signal, which is what every other command-line tool does.
#[cfg(unix)]
fn restore_default_sigpipe() {
    // SAFETY: called before the tokio runtime starts any thread, and setting a
    // disposition to SIG_DFL touches nothing else in the process.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

fn main() {
    #[cfg(unix)]
    restore_default_sigpipe();
    run();
}

#[tokio::main]
async fn run() {
    let cli = Cli::try_parse().unwrap_or_else(|e| {
        // Help and version are not errors; let clap handle them normally.
        if matches!(
            e.kind(),
            clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                | clap::error::ErrorKind::DisplayVersion
        ) {
            e.exit();
        }
        // For genuine parse errors: emit clap's prose first, then add the
        // structured envelope as the last line of stderr (clispec principle 1).
        let kind = match e.kind() {
            clap::error::ErrorKind::UnknownArgument | clap::error::ErrorKind::InvalidSubcommand => {
                "general_error"
            }
            clap::error::ErrorKind::MissingRequiredArgument
            | clap::error::ErrorKind::MissingSubcommand => "config_error",
            _ => "general_error",
        };
        eprint!("{e}");
        print_error_envelope(kind, &e.to_string(), None);
        std::process::exit(2);
    });
    let format = if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::parse(&cli.output).unwrap_or(OutputFormat::Auto)
    };
    let out = OutputConfig::new(format, cli.quiet);

    match &cli.command {
        Command::Schema => {
            print_schema();
            return;
        }
        Command::Capabilities => {
            let value = serde_json::json!({
                "applications": ["network", "protect"],
                "resources": ["clients", "devices", "networks", "ports", "events", "cameras", "rtsps"],
                "structured_output": true
            });
            if out.is_json() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).expect("serialize capabilities")
                );
            } else {
                println!(
                    "Applications: Network, Protect\nResources: clients, devices, networks, ports, events, cameras, RTSPS"
                );
            }
            return;
        }
        Command::Completions { shell, install } => {
            if *install {
                install_completions(*shell);
            } else {
                clap_complete::generate(
                    *shell,
                    &mut Cli::command(),
                    "unifi",
                    &mut std::io::stdout(),
                );
            }
            return;
        }
        Command::Config(ConfigCommand::Init) => {
            run_init(cli.accept_invalid_certs).await;
            return;
        }
        _ => {}
    }

    // Validate --fields before loading config or opening a connection. An
    // unknown field is a usage error, and the caller should learn that without
    // a round trip to the controller.
    let requested_fields = validate_requested_fields(&cli.command).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        print_error_envelope(
            "config_error",
            &e.to_string(),
            Some("run `unifi schema` to see output_fields for each command"),
        );
        std::process::exit(exit_codes::CONFIG_ERROR);
    });

    let config = load_config(cli.profile.as_deref());

    let host = cli.host.or(config.host).unwrap_or_else(|| {
        eprintln!("Error: No host specified.");
        eprintln!();
        eprintln!("  Run 'unifi config init' for interactive setup, or:");
        eprintln!("  - Set UNIFI_HOST environment variable");
        eprintln!("  - Use --host flag");
        std::process::exit(exit_codes::CONFIG_ERROR);
    });

    let api_key = cli.api_key.or(config.api_key).unwrap_or_else(|| {
        eprintln!("Error: No API key specified.");
        eprintln!();
        eprintln!("  Run 'unifi config init' for interactive setup, or:");
        eprintln!("  - Set UNIFI_API_KEY environment variable");
        eprintln!("  - Use --api-key flag");
        eprintln!();
        eprintln!("  Generate an API key in UniFi Settings > API");
        std::process::exit(exit_codes::CONFIG_ERROR);
    });

    let username = cli.username.or(config.username);
    let password = cli.password.or(config.password);
    let client_options = api::ClientOptions {
        accept_invalid_certs: cli.accept_invalid_certs || config.accept_invalid_certs,
    };

    let mut client = match api::UnifiClient::new_with_options(&host, &api_key, client_options) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error creating client: {e}");
            std::process::exit(exit_codes::CONFIG_ERROR);
        }
    };

    let result: Result<(), Box<dyn std::error::Error>> = match cli.command {
        Command::Clients(cmd) => match cmd {
            ClientsCommand::List {
                wired,
                wireless,
                name,
                watch,
                limit,
                offset,
                fields: _,
            } => {
                let filter = commands::clients::ListFilter {
                    wired,
                    wireless,
                    name,
                };
                let pagination = commands::clients::Pagination {
                    limit,
                    offset,
                    fields: requested_fields,
                };
                commands::clients::list(&mut client, out, filter, watch, pagination).await
            }
            ClientsCommand::Show { mac } => commands::clients::show(&client, &mac, out).await,
            ClientsCommand::SetFixedIp { mac, ip, name } => {
                if ip.parse::<std::net::IpAddr>().is_err() {
                    eprintln!("Error: Invalid IP address: {ip}");
                    std::process::exit(exit_codes::CONFIG_ERROR);
                }
                commands::clients::set_fixed_ip(&client, &mac, &ip, name.as_deref(), out).await
            }
            ClientsCommand::Block { mac } => {
                require_confirmation(cli.yes, "block", &format!("Block client {mac}?"));
                commands::clients::block(&client, &mac, out).await
            }
            ClientsCommand::Unblock { mac } => {
                require_confirmation(cli.yes, "unblock", &format!("Unblock client {mac}?"));
                commands::clients::unblock(&client, &mac, out).await
            }
            ClientsCommand::Kick { mac } => {
                require_confirmation(cli.yes, "kick", &format!("Disconnect client {mac}?"));
                commands::clients::kick(&client, &mac, out).await
            }
            ClientsCommand::Top { limit } => commands::clients::top(&client, out, limit).await,
        },
        Command::Devices(cmd) => match cmd {
            DevicesCommand::List {
                watch,
                limit,
                offset,
                fields: _,
            } => {
                let pagination = commands::devices::Pagination {
                    limit,
                    offset,
                    fields: requested_fields,
                };
                commands::devices::list(&mut client, out, watch, pagination).await
            }
            DevicesCommand::Show { mac } => commands::devices::show(&client, &mac, out).await,
            DevicesCommand::Restart { mac } => {
                require_confirmation(cli.yes, "restart", &format!("Restart device {mac}?"));
                commands::devices::restart(&client, &mac, out).await
            }
            DevicesCommand::Locate { mac, off } => {
                commands::devices::locate(&client, &mac, off, out).await
            }
            DevicesCommand::Ports {
                mac,
                live,
                interval,
            } => {
                if live {
                    unifi_cli::tui::run_ports(&client, &mac, interval).await
                } else {
                    commands::devices::ports(&client, &mac, out).await
                }
            }
            DevicesCommand::Upgrade { mac } => {
                require_confirmation(cli.yes, "upgrade", &format!("Upgrade firmware on {mac}?"));
                commands::devices::upgrade(&client, &mac, out).await
            }
        },
        Command::Networks { .. } => commands::networks::list(&mut client, out).await,
        Command::Ports(cmd) => match cmd {
            PortsCommand::List {
                mac,
                limit,
                offset,
                fields: _,
                live,
                interval,
            } => {
                if live {
                    let mac = mac.expect("clap requires --live to be paired with a MAC");
                    unifi_cli::tui::run_ports(&client, &mac, interval).await
                } else {
                    let pagination = commands::ports::Pagination {
                        limit,
                        offset,
                        fields: requested_fields,
                    };
                    commands::ports::list(&client, mac.as_deref(), out, pagination).await
                }
            }
            PortsCommand::Show { mac, port } => {
                commands::ports::show(&client, &mac, port, out).await
            }
            PortsCommand::Find { identifier, .. } => {
                commands::ports::find(&client, &identifier, out, requested_fields).await
            }
            PortsCommand::Cycle { mac, port } => {
                // Asks after the port table is read, so the prompt can name what
                // is attached; the TTY gate still has to run before any HTTP.
                refuse_without_tty(cli.yes, "power-cycle");
                let skip_prompt = cli.yes;
                let outcome = commands::ports::cycle(&client, &mac, port, out, |summary| {
                    if skip_prompt {
                        return Ok(true);
                    }
                    // Reached only on a TTY: refuse_without_tty already
                    // exited for the piped-without---yes case.
                    let mut stdin = std::io::stdin().lock();
                    let mut stderr = std::io::stderr();
                    confirm_destructive(&mut stdin, &mut stderr, summary, "Power-cycle this port?")
                })
                .await;

                // main() returns (), so `?` cannot be used here; match keeps
                // this arm's value the same `Result<(), Box<dyn Error>>` every
                // other arm produces, so errors still flow through the single
                // `if let Err(e) = result` handler below.
                match outcome {
                    Ok(commands::ports::CycleOutcome::Cycled) => Ok(()),
                    Ok(commands::ports::CycleOutcome::Declined) => {
                        print_error_envelope(
                            "confirmation_required",
                            "Aborted: confirmation declined.",
                            None,
                        );
                        std::process::exit(exit_codes::CONFIRMATION_REQUIRED);
                    }
                    Err(e) => Err(e),
                }
            }
        },
        Command::Events(cmd) => match cmd {
            EventsCommand::List {
                limit,
                offset,
                fields: _,
            } => {
                let pagination = commands::events::Pagination {
                    limit,
                    offset,
                    fields: requested_fields,
                };
                commands::events::list(&client, out, pagination).await
            }
        },
        Command::System(cmd) => match cmd {
            SystemCommand::Health => commands::system::health(&client, out).await,
            SystemCommand::Info => commands::system::info(&client, out).await,
        },
        Command::Wan(WanCommand::List) => commands::wan::list(&client, out).await,
        Command::Protect(cmd) => match cmd {
            ProtectCommand::Cameras(cam_cmd) => match cam_cmd {
                ProtectCamerasCommand::List { full } => {
                    if full {
                        let session =
                            require_protect_session(&host, &username, &password, client_options)
                                .await;
                        commands::protect::cameras_list_full(&session, out).await
                    } else {
                        commands::protect::cameras_list(&client, out).await
                    }
                }
                ProtectCamerasCommand::Show { camera, full } => {
                    if full {
                        let session =
                            require_protect_session(&host, &username, &password, client_options)
                                .await;
                        commands::protect::cameras_show_full(&session, &client, &camera, out).await
                    } else {
                        commands::protect::cameras_show(&client, &camera, out).await
                    }
                }
            },
            ProtectCommand::Rtsps(rtsps_cmd) => match rtsps_cmd {
                ProtectRtspsCommand::List { camera } => {
                    commands::protect::rtsps_list(&client, &camera, out).await
                }
                ProtectRtspsCommand::Create { camera, quality } => {
                    commands::protect::rtsps_create(&client, &camera, &quality, out).await
                }
                ProtectRtspsCommand::Delete { camera, quality } => {
                    require_confirmation(
                        cli.yes,
                        "rtsps delete",
                        &format!("Delete {} RTSPS stream(s) on {camera}?", quality.join(", ")),
                    );
                    commands::protect::rtsps_delete(&client, &camera, &quality, out).await
                }
            },
        },
        Command::Tui { interval } => unifi_cli::tui::run(&client, interval).await,
        Command::Config(ConfigCommand::Check) => {
            run_config_check(&client).await;
            return;
        }
        Command::Schema
        | Command::Capabilities
        | Command::Completions { .. }
        | Command::Config(ConfigCommand::Init) => {
            unreachable!()
        }
    };

    if let Err(e) = result {
        let (kind, exit_code) = error_kind_and_code(e.as_ref());
        print_error_envelope(kind, &e.to_string(), None);
        std::process::exit(exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::io::Write;

    // --- Config loading ---

    #[test]
    fn load_config_both_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "host = \"unifi.example.com\"").unwrap();
        writeln!(f, "api_key = \"secret123\"").unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, None);
        assert_eq!(host.as_deref(), Some("unifi.example.com"));
        assert_eq!(api_key.as_deref(), Some("secret123"));
    }

    #[test]
    fn load_config_host_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"unifi.local\"").unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, None);
        assert_eq!(host.as_deref(), Some("unifi.local"));
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_missing_file() {
        let path = std::path::Path::new("/tmp/nonexistent-unifi-test.toml");
        let ConfigValues { host, api_key, .. } = load_config_from(path, None);
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "not valid toml {{{{").unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, None);
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "").unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, None);
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_extra_keys_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"h\"\napi_key = \"k\"\nfoo = \"bar\"").unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, None);
        assert_eq!(host.as_deref(), Some("h"));
        assert_eq!(api_key.as_deref(), Some("k"));
    }

    // --- Profile loading ---

    #[test]
    fn load_config_named_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
host = "default.local"
api_key = "default_key"

[profiles.office]
host = "office.local"
api_key = "office_key"
"#,
        )
        .unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, Some("office"));
        assert_eq!(host.as_deref(), Some("office.local"));
        assert_eq!(api_key.as_deref(), Some("office_key"));
    }

    #[test]
    fn load_config_default_ignores_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
host = "default.local"
api_key = "default_key"

[profiles.office]
host = "office.local"
api_key = "office_key"
"#,
        )
        .unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, None);
        assert_eq!(host.as_deref(), Some("default.local"));
        assert_eq!(api_key.as_deref(), Some("default_key"));
    }

    #[test]
    fn load_config_missing_profile_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"h\"\napi_key = \"k\"").unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, Some("nonexistent"));
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_multiple_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
host = "default.local"
api_key = "default_key"

[profiles.home]
host = "home.local"
api_key = "home_key"

[profiles.work]
host = "work.example.com"
api_key = "work_key"
"#,
        )
        .unwrap();

        let ConfigValues { host, .. } = load_config_from(&path, Some("home"));
        assert_eq!(host.as_deref(), Some("home.local"));

        let ConfigValues { host, .. } = load_config_from(&path, Some("work"));
        assert_eq!(host.as_deref(), Some("work.example.com"));
    }

    // --- confirm_destructive ---

    #[test]
    fn confirm_destructive_accepts_y_and_yes() {
        for input in ["y\n", "Y\n", "yes\n", "YES\n"] {
            let mut reader = std::io::BufReader::new(input.as_bytes());
            let mut writer: Vec<u8> = Vec::new();
            assert!(
                confirm_destructive(&mut reader, &mut writer, "Port 4", "Power-cycle this port?")
                    .unwrap(),
                "{input:?} must confirm"
            );
        }
    }

    #[test]
    fn confirm_destructive_declines_everything_else() {
        for input in ["n\n", "no\n", "\n", "maybe\n", ""] {
            let mut reader = std::io::BufReader::new(input.as_bytes());
            let mut writer: Vec<u8> = Vec::new();
            assert!(
                !confirm_destructive(&mut reader, &mut writer, "Port 4", "Power-cycle this port?")
                    .unwrap(),
                "{input:?} must decline"
            );
        }
    }

    #[test]
    fn confirm_destructive_shows_the_summary_and_default_no() {
        let mut reader = std::io::BufReader::new(&b"n\n"[..]);
        let mut writer: Vec<u8> = Vec::new();
        confirm_destructive(
            &mut reader,
            &mut writer,
            "Port 4 on SwitchA",
            "Power-cycle this port?",
        )
        .unwrap();
        let shown = String::from_utf8(writer).unwrap();
        assert!(shown.contains("Port 4 on SwitchA"), "got: {shown}");
        assert!(shown.contains("Power-cycle this port?"), "got: {shown}");
        assert!(shown.contains("(y/N)"), "default must read as No: {shown}");
    }

    #[test]
    fn confirm_destructive_asks_the_question_it_was_given() {
        // The question is per-command, so a caller that gates `clients block`
        // must not be able to show the power-cycle wording.
        let mut reader = std::io::BufReader::new(&b"n\n"[..]);
        let mut writer: Vec<u8> = Vec::new();
        confirm_destructive(
            &mut reader,
            &mut writer,
            "",
            "Block client aa:bb:cc:dd:ee:ff?",
        )
        .unwrap();
        let shown = String::from_utf8(writer).unwrap();
        assert!(
            shown.contains("Block client aa:bb:cc:dd:ee:ff?"),
            "got: {shown}"
        );
        assert!(
            !shown.to_lowercase().contains("power-cycle"),
            "must not leak another command's wording: {shown}"
        );
    }

    #[test]
    fn confirm_destructive_omits_an_empty_summary_line() {
        // Commands that gate before loading any context pass no summary. A
        // blank line above the question would read as a rendering glitch.
        let mut reader = std::io::BufReader::new(&b"n\n"[..]);
        let mut writer: Vec<u8> = Vec::new();
        confirm_destructive(&mut reader, &mut writer, "", "Restart device X?").unwrap();
        let shown = String::from_utf8(writer).unwrap();
        assert!(
            shown.starts_with("Restart device X?"),
            "question must be the first thing written: {shown:?}"
        );
    }

    #[test]
    fn confirm_destructive_terminates_the_prompt_line_with_a_newline() {
        // The user's Enter is echoed by the terminal, not by this writer, so
        // the prompt's own `write!` leaves the stream mid-line unless
        // `confirm_destructive` terminates it itself. A subsequent
        // `eprintln!` (e.g. the confirmation_required envelope printed on
        // decline) must start on a fresh line, not get appended to the
        // prompt.
        let mut reader = std::io::BufReader::new(&b"n\n"[..]);
        let mut writer: Vec<u8> = Vec::new();
        confirm_destructive(
            &mut reader,
            &mut writer,
            "Port 4 on SwitchA",
            "Power-cycle this port?",
        )
        .unwrap();
        let shown = String::from_utf8(writer).unwrap();
        assert!(
            shown.ends_with('\n'),
            "prompt output must end with a newline so a following line starts clean: {shown:?}"
        );
    }

    // --- mask_api_key ---

    #[test]
    fn mask_api_key_long() {
        assert_eq!(mask_api_key("abcdefghijklmnop"), "abcd…mnop");
    }

    #[test]
    fn mask_api_key_short() {
        assert_eq!(mask_api_key("abcd"), "****");
    }

    #[test]
    fn mask_api_key_exactly_eight() {
        assert_eq!(mask_api_key("12345678"), "****");
    }

    #[test]
    fn mask_api_key_nine_chars() {
        assert_eq!(mask_api_key("123456789"), "1234…6789");
    }

    #[test]
    fn mask_api_key_empty() {
        assert_eq!(mask_api_key(""), "****");
    }

    #[test]
    fn mask_api_key_unicode() {
        assert_eq!(mask_api_key("αβγδεζηθικλ"), "αβγδ…θικλ");
    }

    // --- init (interactive flow) ---

    fn run_init_test(existing_config: Option<&str>, input: &str) -> (InitOutcome, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        if let Some(content) = existing_config {
            std::fs::write(&path, content).unwrap();
        }

        let mut reader = std::io::Cursor::new(input.as_bytes().to_vec());
        let mut output = Vec::new();

        let result = run_init_with_io(&mut reader, &mut output, &path, false, false).unwrap();

        let written = std::fs::read_to_string(&path).unwrap_or_default();
        let display = String::from_utf8(output).unwrap();
        (result, written, display)
    }

    #[test]
    fn init_persists_protect_credentials() {
        // profile, host, key, username, password, confirm
        let (result, written, display) = run_init_test(
            None,
            "\nhttps://unifi.local\nmy-api-key\nadmin\nsecret\ny\n",
        );

        assert!(matches!(result, InitOutcome::Saved { profile: None, .. }));
        assert!(written.contains("username = \"admin\""));
        assert!(written.contains("password = \"secret\""));
        // The summary masks the password rather than echoing it.
        assert!(display.contains("password = ****"));
        assert!(!display.contains("secret"));
    }

    #[cfg(unix)]
    fn mode_of(path: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn init_writes_credentials_readable_only_by_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut reader = std::io::Cursor::new(b"\nhttps://unifi.local\nmy-api-key\n\ny\n".to_vec());
        let mut output = Vec::new();

        run_init_with_io(&mut reader, &mut output, &path, false, false).unwrap();

        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("my-api-key")
        );
        assert_eq!(mode_of(&path), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn init_tightens_permissions_of_a_preexisting_world_readable_config() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"https://unifi.local\"\napi_key = \"old\"\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let mut reader = std::io::Cursor::new(b"\nhttps://unifi.local\nnew-key\n\ny\n".to_vec());
        let mut output = Vec::new();
        run_init_with_io(&mut reader, &mut output, &path, false, false).unwrap();

        assert_eq!(mode_of(&path), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn init_keeps_credentials_out_of_a_preexisting_world_readable_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"https://unifi.local\"\napi_key = \"old\"\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        // A second name for the same inode. Writing the new key into the file
        // that already exists, even for the instant before a chmod, shows up
        // here; replacing that file by rename cannot.
        let witness = dir.path().join("witness.toml");
        std::fs::hard_link(&path, &witness).unwrap();

        let mut reader = std::io::Cursor::new(b"\nhttps://unifi.local\nnew-key\n\ny\n".to_vec());
        let mut output = Vec::new();
        run_init_with_io(&mut reader, &mut output, &path, false, false).unwrap();

        let exposed = std::fs::read_to_string(&witness).unwrap();
        assert!(
            !exposed.contains("new-key"),
            "the API key was written into a world-readable file: {exposed}"
        );
        assert_eq!(
            mode_of(&witness),
            0o644,
            "the world-readable file was the one that got written"
        );
        assert!(std::fs::read_to_string(&path).unwrap().contains("new-key"));
        assert_eq!(mode_of(&path), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn enable_accept_invalid_certs_keeps_the_config_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"https://unifi.local\"\napi_key = \"k\"\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        enable_accept_invalid_certs_in_config(&path, None).unwrap();

        assert_eq!(mode_of(&path), 0o600);
    }

    #[test]
    fn init_persists_explicit_invalid_cert_opt_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let input = "\nhttps://unifi.local\nmy-api-key\n\ny\n";
        let mut reader = std::io::Cursor::new(input.as_bytes().to_vec());
        let mut output = Vec::new();

        run_init_with_io(&mut reader, &mut output, &path, false, true).unwrap();

        let written = std::fs::read_to_string(&path).unwrap_or_default();
        let display = String::from_utf8(output).unwrap();
        assert!(written.contains("accept_invalid_certs = true"));
        assert!(display.contains("accept_invalid_certs = true"));
    }

    #[test]
    fn enable_accept_invalid_certs_updates_default_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"https://unifi.local\"\napi_key = \"k\"\n").unwrap();

        enable_accept_invalid_certs_in_config(&path, None).unwrap();

        let written: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        assert_eq!(
            written
                .get("accept_invalid_certs")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        // Existing keys are preserved.
        assert!(written.contains_key("host"));
        assert!(written.contains_key("api_key"));
    }

    #[test]
    fn enable_accept_invalid_certs_updates_profile_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[profiles.home]\nhost = \"https://unifi.local\"\napi_key = \"k\"\n",
        )
        .unwrap();

        enable_accept_invalid_certs_in_config(&path, Some("home")).unwrap();

        let written: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        let home = written["profiles"].as_table().unwrap()["home"]
            .as_table()
            .unwrap();
        assert_eq!(
            home.get("accept_invalid_certs").and_then(|v| v.as_bool()),
            Some(true)
        );
        // The default table is untouched.
        assert!(!written.contains_key("accept_invalid_certs"));
    }

    #[test]
    fn is_tls_cert_error_detects_certificate_failures() {
        assert!(
            api::ApiError::Other("invalid peer certificate: UnknownIssuer".into())
                .is_tls_cert_error()
        );
        assert!(
            api::ApiError::Other("the handshake failed: self-signed certificate".into())
                .is_tls_cert_error()
        );
    }

    #[test]
    fn is_tls_cert_error_ignores_unrelated_failures() {
        assert!(!api::ApiError::Other("connection refused".into()).is_tls_cert_error());
        assert!(!api::ApiError::NotFound("nope".into()).is_tls_cert_error());
    }

    #[test]
    fn init_skips_password_when_no_username() {
        // Empty username means no Protect credentials are stored at all.
        let (result, written, _) = run_init_test(None, "\nhttps://unifi.local\nmy-api-key\n\ny\n");

        assert!(matches!(result, InitOutcome::Saved { profile: None, .. }));
        assert!(!written.contains("username"));
        assert!(!written.contains("password"));
    }

    #[test]
    fn init_fresh_default_profile() {
        let (result, written, display) =
            run_init_test(None, "\nhttps://unifi.local\nmy-api-key\n\ny\n");

        assert!(matches!(result, InitOutcome::Saved { profile: None, .. }));
        assert!(written.contains("host = \"https://unifi.local\""));
        assert!(written.contains("api_key = \"my-api-key\""));
        assert!(display.contains("Create API keys at"));
    }

    #[test]
    fn init_fresh_named_profile() {
        let (result, written, _) =
            run_init_test(None, "office\nhttps://office.local\noffice-key\n\ny\n");

        assert!(matches!(
            result,
            InitOutcome::Saved {
                profile: Some(ref p),
                ..
            } if p == "office"
        ));
        assert!(written.contains("[profiles.office]"));
        assert!(written.contains("host = \"https://office.local\""));
    }

    #[test]
    fn init_cancelled() {
        let (result, written, display) =
            run_init_test(None, "\nhttps://unifi.local\nmy-api-key\n\nn\n");

        assert_eq!(result, InitOutcome::Cancelled);
        assert!(written.is_empty());
        assert!(display.contains("Cancelled"));
    }

    #[test]
    fn init_preserves_existing_default_when_adding_profile() {
        let existing = "host = \"default.local\"\napi_key = \"default-key\"\n";
        let (result, written, _) =
            run_init_test(Some(existing), "work\nhttps://work.local\nwork-key\n\ny\n");

        assert!(matches!(
            result,
            InitOutcome::Saved {
                profile: Some(ref p),
                ..
            } if p == "work"
        ));
        assert!(written.contains("host = \"default.local\""));
        assert!(written.contains("api_key = \"default-key\""));
        assert!(written.contains("[profiles.work]"));
        assert!(written.contains("host = \"https://work.local\""));
    }

    #[test]
    fn init_keeps_existing_value_on_empty_input() {
        let existing = "host = \"existing.local\"\napi_key = \"existing-key\"\n";
        // Empty host and key inputs → keep existing values
        let (result, written, _) = run_init_test(Some(existing), "\n\n\n\ny\n");

        assert!(matches!(result, InitOutcome::Saved { profile: None, .. }));
        assert!(written.contains("host = \"existing.local\""));
        assert!(written.contains("api_key = \"existing-key\""));
    }

    #[test]
    fn init_overwrites_existing_value() {
        let existing = "host = \"old.local\"\napi_key = \"old-key\"\n";
        let (_, written, _) = run_init_test(Some(existing), "\nnew.local\nnew-key\n\ny\n");

        assert!(written.contains("host = \"new.local\""));
        assert!(written.contains("api_key = \"new-key\""));
        assert!(!written.contains("old.local"));
    }

    #[test]
    fn init_shows_masked_key_in_prompt() {
        let existing = "host = \"h\"\napi_key = \"abcdefghij\"\n";
        let (_, _, display) = run_init_test(Some(existing), "\n\n\n\ny\n");

        assert!(display.contains("abcd…ghij"));
    }

    #[test]
    fn init_shows_summary_before_confirm() {
        let (_, _, display) =
            run_init_test(None, "\nhttps://test.local\ntest-key-1234567890\n\ny\n");

        assert!(display.contains("Configuration:"));
        assert!(display.contains("host     = https://test.local"));
        assert!(display.contains("api_key  = test…7890"));
        assert!(display.contains("Save? (y/n)"));
    }

    #[test]
    fn init_warns_on_corrupt_existing_config() {
        let (result, written, display) = run_init_test(
            Some("not valid {{{ toml"),
            "\nhttps://new.local\nnew-key\n\ny\n",
        );

        assert!(matches!(result, InitOutcome::Saved { profile: None, .. }));
        assert!(display.contains("Warning: Existing config"));
        assert!(display.contains("invalid TOML"));
        assert!(written.contains("host = \"https://new.local\""));
    }

    #[test]
    fn init_accepts_mixed_case_yes() {
        let (result, _, _) = run_init_test(None, "\nhttps://h\nk\n\nYes\n");
        assert!(matches!(result, InitOutcome::Saved { profile: None, .. }));
    }

    #[test]
    fn init_host_required_when_no_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let input = "\n\nkey\ny\n";
        let mut reader = std::io::Cursor::new(input.as_bytes().to_vec());
        let mut output = Vec::new();

        let result = run_init_with_io(&mut reader, &mut output, &path, false, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Host is required"));
    }

    #[test]
    fn init_api_key_required_when_no_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let input = "\nhttps://test.local\n\ny\n";
        let mut reader = std::io::Cursor::new(input.as_bytes().to_vec());
        let mut output = Vec::new();

        let result = run_init_with_io(&mut reader, &mut output, &path, false, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("API key is required")
        );
    }

    // --- CLI parsing ---

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn cli_clients_list() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "clients", "list"]);
        assert_eq!(cli.host.as_deref(), Some("h"));
        assert_eq!(cli.api_key.as_deref(), Some("k"));
        assert!(!cli.json);
        assert!(!cli.quiet);
        assert!(matches!(
            cli.command,
            Command::Clients(ClientsCommand::List { .. })
        ));
    }

    #[test]
    fn cli_clients_show() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "show",
            "aa:bb:cc:dd:ee:ff",
        ]);
        match cli.command {
            Command::Clients(ClientsCommand::Show { mac }) => {
                assert_eq!(mac, "aa:bb:cc:dd:ee:ff")
            }
            _ => panic!("expected Clients Show"),
        }
    }

    #[test]
    fn cli_clients_set_fixed_ip_with_name() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "set-fixed-ip",
            "aa:bb:cc:dd:ee:ff",
            "192.0.2.5",
            "--name",
            "MyDevice",
        ]);
        match cli.command {
            Command::Clients(ClientsCommand::SetFixedIp { mac, ip, name }) => {
                assert_eq!(mac, "aa:bb:cc:dd:ee:ff");
                assert_eq!(ip, "192.0.2.5");
                assert_eq!(name.as_deref(), Some("MyDevice"));
            }
            _ => panic!("expected Clients SetFixedIp"),
        }
    }

    #[test]
    fn cli_clients_set_fixed_ip_without_name() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "set-fixed-ip",
            "aa:bb:cc:dd:ee:ff",
            "192.0.2.5",
        ]);
        match cli.command {
            Command::Clients(ClientsCommand::SetFixedIp { name, .. }) => assert!(name.is_none()),
            _ => panic!("expected Clients SetFixedIp"),
        }
    }

    #[test]
    fn cli_clients_block() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "block",
            "aa:bb:cc:dd:ee:ff",
        ]);
        assert!(matches!(
            cli.command,
            Command::Clients(ClientsCommand::Block { .. })
        ));
    }

    #[test]
    fn cli_clients_unblock() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "unblock",
            "aa:bb:cc:dd:ee:ff",
        ]);
        assert!(matches!(
            cli.command,
            Command::Clients(ClientsCommand::Unblock { .. })
        ));
    }

    #[test]
    fn cli_clients_kick() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "kick",
            "aa:bb:cc:dd:ee:ff",
        ]);
        assert!(matches!(
            cli.command,
            Command::Clients(ClientsCommand::Kick { .. })
        ));
    }

    #[test]
    fn cli_devices_list() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "devices", "list"]);
        assert!(matches!(
            cli.command,
            Command::Devices(DevicesCommand::List { .. })
        ));
    }

    #[test]
    fn cli_devices_restart() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "devices",
            "restart",
            "aa:bb:cc:dd:ee:ff",
        ]);
        match cli.command {
            Command::Devices(DevicesCommand::Restart { mac, .. }) => {
                assert_eq!(mac, "aa:bb:cc:dd:ee:ff")
            }
            _ => panic!("expected Devices Restart"),
        }
    }

    #[test]
    fn cli_devices_locate_on() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "devices",
            "locate",
            "aa:bb:cc:dd:ee:ff",
        ]);
        match cli.command {
            Command::Devices(DevicesCommand::Locate { mac, off }) => {
                assert_eq!(mac, "aa:bb:cc:dd:ee:ff");
                assert!(!off);
            }
            _ => panic!("expected Devices Locate"),
        }
    }

    #[test]
    fn cli_devices_locate_off() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "devices",
            "locate",
            "aa:bb:cc:dd:ee:ff",
            "--off",
        ]);
        match cli.command {
            Command::Devices(DevicesCommand::Locate { off, .. }) => assert!(off),
            _ => panic!("expected Devices Locate"),
        }
    }

    #[test]
    fn cli_networks_bare_defaults_to_list() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "networks"]);
        assert!(matches!(cli.command, Command::Networks { command: None }));
    }

    #[test]
    fn cli_networks_list_subcommand() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "networks", "list"]);
        assert!(matches!(
            cli.command,
            Command::Networks {
                command: Some(NetworksCommand::List)
            }
        ));
    }

    // --- --fields validation ---

    #[test]
    fn validate_fields_accepts_known_client_fields() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "list",
            "--fields",
            "mac,ssid",
        ]);
        let got = validate_requested_fields(&cli.command).unwrap();
        assert_eq!(got, Some(vec!["mac".to_string(), "ssid".to_string()]));
    }

    #[test]
    fn validate_fields_rejects_unknown_client_field() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "list",
            "--fields",
            "bogus",
        ]);
        let err = validate_requested_fields(&cli.command).unwrap_err();
        assert_eq!(err.unknown, vec!["bogus"]);
    }

    #[test]
    fn validate_fields_is_none_without_the_flag() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "clients", "list"]);
        assert_eq!(validate_requested_fields(&cli.command).unwrap(), None);
    }

    #[test]
    fn validate_fields_ignores_commands_without_the_flag() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "system", "health"]);
        assert_eq!(validate_requested_fields(&cli.command).unwrap(), None);
    }

    #[test]
    fn validate_fields_uses_the_right_table_per_command() {
        // `ssid` is a client field, not a device field.
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "devices",
            "list",
            "--fields",
            "ssid",
        ]);
        assert!(validate_requested_fields(&cli.command).is_err());
    }

    #[test]
    fn cli_system_health() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "system", "health"]);
        assert!(matches!(
            cli.command,
            Command::System(SystemCommand::Health)
        ));
    }

    #[test]
    fn cli_system_info() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "system", "info"]);
        assert!(matches!(cli.command, Command::System(SystemCommand::Info)));
    }

    #[test]
    fn cli_json_flag() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "--json",
            "networks",
        ]);
        assert!(cli.json);
    }

    #[test]
    fn cli_json_flag_after_subcommand() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "networks",
            "--json",
        ]);
        assert!(cli.json);
    }

    #[test]
    fn cli_quiet_flag() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "--quiet",
            "networks",
        ]);
        assert!(cli.quiet);
    }

    #[test]
    fn cli_schema_command() {
        let cli = parse(&["unifi", "schema"]);
        assert!(matches!(cli.command, Command::Schema));
    }

    #[test]
    fn cli_schema_no_host_required() {
        let cli = parse(&["unifi", "schema"]);
        assert!(cli.host.is_none());
        assert!(cli.api_key.is_none());
    }

    #[test]
    fn cli_missing_subcommand_fails() {
        assert!(Cli::try_parse_from(["unifi", "--host", "h", "--api-key", "k"]).is_err());
    }

    #[test]
    fn cli_host_and_key_optional() {
        let cli = parse(&["unifi", "networks"]);
        assert!(cli.host.is_none());
        assert!(cli.api_key.is_none());
    }

    #[test]
    fn cli_wired_and_wireless_conflict() {
        let result = Cli::try_parse_from([
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "list",
            "--wired",
            "--wireless",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_clients_list_wired_flag() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "list",
            "--wired",
        ]);
        match cli.command {
            Command::Clients(ClientsCommand::List {
                wired,
                wireless,
                name,
                watch,
                ..
            }) => {
                assert!(wired);
                assert!(!wireless);
                assert!(name.is_none());
                assert!(watch.is_none());
            }
            _ => panic!("expected Clients List"),
        }
    }

    #[test]
    fn cli_clients_list_name_filter() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "list",
            "--name",
            "phone",
        ]);
        match cli.command {
            Command::Clients(ClientsCommand::List { name, .. }) => {
                assert_eq!(name.as_deref(), Some("phone"));
            }
            _ => panic!("expected Clients List"),
        }
    }

    #[test]
    fn cli_clients_list_watch() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "list",
            "--watch",
            "5",
        ]);
        match cli.command {
            Command::Clients(ClientsCommand::List { watch, .. }) => {
                assert_eq!(watch, Some(5));
            }
            _ => panic!("expected Clients List"),
        }
    }

    #[test]
    fn cli_devices_show() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "devices",
            "show",
            "aa:bb:cc:dd:ee:ff",
        ]);
        match cli.command {
            Command::Devices(DevicesCommand::Show { mac }) => {
                assert_eq!(mac, "aa:bb:cc:dd:ee:ff");
            }
            _ => panic!("expected Devices Show"),
        }
    }

    #[test]
    fn cli_completions() {
        let cli = parse(&["unifi", "completions", "bash"]);
        assert!(matches!(cli.command, Command::Completions { .. }));
    }

    #[test]
    fn cli_completions_install() {
        let cli = parse(&["unifi", "completions", "zsh", "--install"]);
        match cli.command {
            Command::Completions { shell, install } => {
                assert_eq!(shell, Shell::Zsh);
                assert!(install);
            }
            _ => panic!("expected Completions"),
        }
    }

    #[test]
    fn cli_config_init() {
        let cli = parse(&["unifi", "config", "init"]);
        assert!(matches!(cli.command, Command::Config(ConfigCommand::Init)));
        assert!(cli.host.is_none());
        assert!(cli.api_key.is_none());
    }

    #[test]
    fn cli_config_check() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "config", "check"]);
        assert!(matches!(cli.command, Command::Config(ConfigCommand::Check)));
    }

    #[test]
    fn cli_profile_flag() {
        let cli = parse(&[
            "unifi",
            "--profile",
            "office",
            "--host",
            "h",
            "--api-key",
            "k",
            "networks",
        ]);
        assert_eq!(cli.profile.as_deref(), Some("office"));
    }

    #[test]
    fn cli_accept_invalid_certs_default_false() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "networks"]);
        assert!(!cli.accept_invalid_certs);
    }

    #[test]
    fn cli_accept_invalid_certs_flag() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "--accept-invalid-certs",
            "networks",
        ]);
        assert!(cli.accept_invalid_certs);
    }

    #[test]
    fn cli_profile_default_none() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "networks"]);
        assert!(cli.profile.is_none());
    }

    #[test]
    fn cli_events_list() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "events", "list"]);
        match cli.command {
            Command::Events(EventsCommand::List { limit, .. }) => {
                assert_eq!(limit, 10); // default
            }
            _ => panic!("expected Events List"),
        }
    }

    #[test]
    fn cli_events_list_custom_limit() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "events",
            "list",
            "-n",
            "50",
        ]);
        match cli.command {
            Command::Events(EventsCommand::List { limit, .. }) => {
                assert_eq!(limit, 50);
            }
            _ => panic!("expected Events List"),
        }
    }

    #[test]
    fn cli_clients_top() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "clients", "top"]);
        match cli.command {
            Command::Clients(ClientsCommand::Top { limit }) => {
                assert_eq!(limit, 10); // default
            }
            _ => panic!("expected Clients Top"),
        }
    }

    #[test]
    fn cli_clients_top_custom_limit() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "top",
            "-n",
            "5",
        ]);
        match cli.command {
            Command::Clients(ClientsCommand::Top { limit }) => {
                assert_eq!(limit, 5);
            }
            _ => panic!("expected Clients Top"),
        }
    }

    #[test]
    fn cli_tui_default_interval() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "tui"]);
        match cli.command {
            Command::Tui { interval } => assert_eq!(interval, 2),
            _ => panic!("expected Tui"),
        }
    }

    #[test]
    fn cli_tui_custom_interval() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "tui", "-i", "5"]);
        match cli.command {
            Command::Tui { interval } => assert_eq!(interval, 5),
            _ => panic!("expected Tui"),
        }
    }

    #[test]
    fn cli_top_alias_for_tui() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "top"]);
        match cli.command {
            Command::Tui { interval } => assert_eq!(interval, 2),
            _ => panic!("expected Tui via top alias"),
        }
    }

    #[test]
    fn cli_devices_ports() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "devices",
            "ports",
            "aa:bb:cc:dd:ee:ff",
        ]);
        match cli.command {
            Command::Devices(DevicesCommand::Ports { mac, live, .. }) => {
                assert_eq!(mac, "aa:bb:cc:dd:ee:ff");
                assert!(!live);
            }
            _ => panic!("expected Devices Ports"),
        }
    }

    #[test]
    fn cli_devices_ports_live() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "devices",
            "ports",
            "aa:bb:cc:dd:ee:ff",
            "--live",
            "-i",
            "3",
        ]);
        match cli.command {
            Command::Devices(DevicesCommand::Ports {
                mac,
                live,
                interval,
            }) => {
                assert_eq!(mac, "aa:bb:cc:dd:ee:ff");
                assert!(live);
                assert_eq!(interval, 3);
            }
            _ => panic!("expected Devices Ports"),
        }
    }

    #[test]
    fn cli_parses_ports_list_without_mac() {
        let cli = Cli::parse_from(["unifi", "ports", "list"]);
        match cli.command {
            Command::Ports(PortsCommand::List { mac, limit, .. }) => {
                assert!(mac.is_none());
                assert_eq!(limit, 100);
            }
            _ => panic!("expected Ports List"),
        }
    }

    #[test]
    fn cli_parses_ports_list_with_mac() {
        let cli = Cli::parse_from(["unifi", "ports", "list", "aa:bb:cc:dd:ee:ff"]);
        match cli.command {
            Command::Ports(PortsCommand::List { mac, .. }) => {
                assert_eq!(mac.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
            }
            _ => panic!("expected Ports List"),
        }
    }

    #[test]
    fn cli_rejects_ports_live_without_mac() {
        assert!(Cli::try_parse_from(["unifi", "ports", "list", "--live"]).is_err());
    }

    #[test]
    fn cli_devices_upgrade() {
        let cli = parse(&[
            "unifi",
            "--host",
            "h",
            "--api-key",
            "k",
            "devices",
            "upgrade",
            "aa:bb:cc:dd:ee:ff",
        ]);
        match cli.command {
            Command::Devices(DevicesCommand::Upgrade { mac, .. }) => {
                assert_eq!(mac, "aa:bb:cc:dd:ee:ff");
            }
            _ => panic!("expected Devices Upgrade"),
        }
    }

    // --- Config edge cases ---

    #[test]
    fn load_config_profile_only_no_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[profiles.office]
host = "office.local"
api_key = "office_key"
"#,
        )
        .unwrap();

        // No profile → returns default (no top-level host/key)
        let ConfigValues { host, api_key, .. } = load_config_from(&path, None);
        assert!(host.is_none());
        assert!(api_key.is_none());

        // Named profile → returns profile
        let ConfigValues { host, api_key, .. } = load_config_from(&path, Some("office"));
        assert_eq!(host.as_deref(), Some("office.local"));
        assert_eq!(api_key.as_deref(), Some("office_key"));
    }

    #[test]
    fn load_config_profile_partial_creds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[profiles.partial]
host = "partial.local"
"#,
        )
        .unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, Some("partial"));
        assert_eq!(host.as_deref(), Some("partial.local"));
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_profile_with_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[profiles.my-home]
host = "home.local"
api_key = "key-with-special=chars"
"#,
        )
        .unwrap();

        let ConfigValues { host, api_key, .. } = load_config_from(&path, Some("my-home"));
        assert_eq!(host.as_deref(), Some("home.local"));
        assert_eq!(api_key.as_deref(), Some("key-with-special=chars"));
    }

    // --- resolve_profile_table ---

    #[test]
    fn resolve_profile_table_none_returns_root() {
        let config: toml::Table = r#"
host = "root.local"
api_key = "root_key"
"#
        .parse()
        .unwrap();

        let table = resolve_profile_table(&config, None);
        assert!(table.is_some());
        assert_eq!(
            table.unwrap().get("host").and_then(|v| v.as_str()),
            Some("root.local")
        );
    }

    #[test]
    fn resolve_profile_table_named_returns_profile() {
        let config: toml::Table = r#"
[profiles.work]
host = "work.local"
"#
        .parse()
        .unwrap();

        let table = resolve_profile_table(&config, Some("work"));
        assert!(table.is_some());
        assert_eq!(
            table.unwrap().get("host").and_then(|v| v.as_str()),
            Some("work.local")
        );
    }

    #[test]
    fn resolve_profile_table_missing_returns_none() {
        let config: toml::Table = "host = \"h\"".parse().unwrap();
        let table = resolve_profile_table(&config, Some("nope"));
        assert!(table.is_none());
    }

    // --- extract_credentials ---

    #[test]
    fn extract_credentials_both() {
        let table: toml::Table = r#"
host = "h"
api_key = "k"
"#
        .parse()
        .unwrap();
        let ConfigValues {
            host, api_key: key, ..
        } = extract_credentials(&table);
        assert_eq!(host.as_deref(), Some("h"));
        assert_eq!(key.as_deref(), Some("k"));
    }

    #[test]
    fn extract_credentials_empty_table() {
        let table = toml::Table::new();
        let ConfigValues {
            host, api_key: key, ..
        } = extract_credentials(&table);
        assert!(host.is_none());
        assert!(key.is_none());
    }

    #[test]
    fn extract_credentials_non_string_values() {
        let table: toml::Table = "host = 12345".parse().unwrap();
        let ConfigValues { host, .. } = extract_credentials(&table);
        assert!(host.is_none()); // integer, not string
    }

    #[test]
    fn extract_credentials_accept_invalid_certs() {
        let table: toml::Table = r#"
host = "h"
api_key = "k"
accept_invalid_certs = true
"#
        .parse()
        .unwrap();
        let ConfigValues {
            accept_invalid_certs,
            ..
        } = extract_credentials(&table);
        assert!(accept_invalid_certs);
    }

    // --- UnifiClient::new base URL ---

    #[test]
    fn client_new_strips_trailing_slash() {
        let client = api::UnifiClient::new("https://unifi.local/", "key").unwrap();
        assert_eq!(client.base_url(), "https://unifi.local");
    }

    #[test]
    fn client_new_preserves_https() {
        let client = api::UnifiClient::new("https://unifi.local", "key").unwrap();
        assert_eq!(client.base_url(), "https://unifi.local");
    }

    #[test]
    fn client_new_preserves_http() {
        let client = api::UnifiClient::new("http://unifi.local", "key").unwrap();
        assert_eq!(client.base_url(), "http://unifi.local");
    }

    #[test]
    fn client_new_adds_https_for_bare_host() {
        let client = api::UnifiClient::new("unifi.local", "key").unwrap();
        assert_eq!(client.base_url(), "https://unifi.local");
    }

    #[test]
    fn client_new_adds_https_for_ip() {
        let client = api::UnifiClient::new("198.51.100.1", "key").unwrap();
        assert_eq!(client.base_url(), "https://198.51.100.1");
    }

    #[test]
    fn client_new_rejects_unsupported_scheme() {
        let err = match api::UnifiClient::new("file:///tmp/controller", "key") {
            Ok(_) => panic!("expected unsupported scheme error"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("http:// or https://"));
    }

    #[test]
    fn client_new_rejects_missing_host() {
        let err = match api::UnifiClient::new("https://", "key") {
            Ok(_) => panic!("expected invalid host error"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("Invalid controller host"));
    }

    #[test]
    fn client_new_accepts_uppercase_scheme() {
        // URL schemes are case-insensitive; the scheme is normalized to lowercase.
        let client = api::UnifiClient::new("HTTPS://unifi.local", "key").unwrap();
        assert_eq!(client.base_url(), "HTTPS://unifi.local");
    }

    // --- error_for_status ---

    #[test]
    fn error_for_status_401_returns_auth() {
        let err = api::error_for_status(401, "Unauthorized".into());
        assert!(matches!(err, api::ApiError::Auth(_)));
    }

    #[test]
    fn error_for_status_403_returns_auth() {
        let err = api::error_for_status(403, "Forbidden".into());
        assert!(matches!(err, api::ApiError::Auth(_)));
    }

    #[test]
    fn error_for_status_404_returns_not_found() {
        let err = api::error_for_status(404, "Not Found".into());
        assert!(matches!(err, api::ApiError::NotFound(_)));
    }

    #[test]
    fn error_for_status_500_returns_api_error() {
        let err = api::error_for_status(500, "Server Error".into());
        match err {
            api::ApiError::Api { status, message } => {
                assert_eq!(status, 500);
                assert_eq!(message, "Server Error");
            }
            _ => panic!("expected Api error"),
        }
    }

    #[test]
    fn error_for_status_200_returns_api_error() {
        let err = api::error_for_status(200, "unexpected".into());
        assert!(matches!(err, api::ApiError::Api { status: 200, .. }));
    }
}
