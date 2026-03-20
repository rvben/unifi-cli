use unifi_cli::api;
use unifi_cli::commands;
use unifi_cli::output::{OutputConfig, exit_code_for_error, exit_codes};

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

    /// Config profile to use (or set UNIFI_PROFILE env var)
    #[arg(long, env = "UNIFI_PROFILE")]
    profile: Option<String>,

    /// Output as JSON (auto-enabled when stdout is not a terminal)
    #[arg(long, global = true)]
    json: bool,

    /// Suppress non-data output (summary lines, confirmations)
    #[arg(long, global = true)]
    quiet: bool,

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
    Networks,

    /// View controller events
    #[command(subcommand)]
    Events(EventsCommand),

    /// System information
    #[command(subcommand)]
    System(SystemCommand),

    /// Dump all commands and arguments as JSON for agent introspection
    Schema,

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
        /// Fixed IP address to assign (e.g., 10.0.0.5)
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
enum ConfigCommand {
    /// Create or update the configuration file interactively
    Init,
    /// Verify configuration and test connectivity
    Check,
}

#[derive(Subcommand)]
enum EventsCommand {
    /// List recent events
    List {
        /// Number of events to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum SystemCommand {
    /// Show system health
    Health,
    /// Show system info
    Info,
}

fn print_schema() {
    let schema = serde_json::json!({
        "name": "unifi",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "CLI for UniFi Network controller",
        "global_flags": {
            "--json": "Output as JSON (auto-enabled when piped)",
            "--quiet": "Suppress non-data output",
            "--host <HOST>": "UniFi controller host (env: UNIFI_HOST)",
            "--api-key <KEY>": "API key (env: UNIFI_API_KEY)",
            "--profile <NAME>": "Config profile to use (env: UNIFI_PROFILE)",
        },
        "exit_codes": {
            "0": "success",
            "1": "general error",
            "2": "configuration error (missing host/api-key)",
            "3": "authentication error (401/403)",
            "4": "not found (404)",
            "5": "API error (server error)",
        },
        "commands": {
            "clients list": {
                "description": "List all connected clients",
                "args": [
                    {"name": "--wired", "required": false, "description": "Show only wired clients"},
                    {"name": "--wireless", "required": false, "description": "Show only wireless clients"},
                    {"name": "--name", "required": false, "description": "Filter by name (substring)"},
                    {"name": "--watch", "required": false, "description": "Refresh every N seconds"},
                ],
                "output_fields": ["name", "mac", "ip", "type"],
            },
            "clients show": {
                "description": "Show details for a client by MAC address",
                "args": [{"name": "mac", "required": true, "description": "MAC address"}],
                "output_fields": ["name", "mac", "ip", "wired", "uptime", "tx_bytes", "rx_bytes", "signal", "ssid", "ap_mac"],
            },
            "clients set-fixed-ip": {
                "description": "Set a fixed IP (DHCP reservation) for a client",
                "args": [
                    {"name": "mac", "required": true, "description": "MAC address"},
                    {"name": "ip", "required": true, "description": "Fixed IP address"},
                    {"name": "--name", "required": false, "description": "Friendly name"},
                ],
                "output_fields": ["status", "action", "mac", "ip", "name"],
                "mutating": true,
            },
            "clients block": {
                "description": "Block a client",
                "args": [{"name": "mac", "required": true, "description": "MAC address"}],
                "output_fields": ["status", "action", "mac"],
                "mutating": true,
            },
            "clients unblock": {
                "description": "Unblock a client",
                "args": [{"name": "mac", "required": true, "description": "MAC address"}],
                "output_fields": ["status", "action", "mac"],
                "mutating": true,
            },
            "clients kick": {
                "description": "Kick (disconnect) a client",
                "args": [{"name": "mac", "required": true, "description": "MAC address"}],
                "output_fields": ["status", "action", "mac"],
                "mutating": true,
            },
            "clients top": {
                "description": "Show top clients by bandwidth usage",
                "args": [
                    {"name": "--limit", "required": false, "description": "Number of clients (default: 10)"},
                ],
                "output_fields": ["name", "mac", "ip", "tx_bytes", "rx_bytes", "total_bytes"],
            },
            "devices list": {
                "description": "List all network devices",
                "args": [
                    {"name": "--watch", "required": false, "description": "Refresh every N seconds"},
                ],
                "output_fields": ["name", "model", "mac", "ip", "state", "firmware"],
            },
            "devices show": {
                "description": "Show details for a device by MAC address",
                "args": [{"name": "mac", "required": true, "description": "MAC address"}],
                "output_fields": ["name", "model", "mac", "ip", "state", "firmware", "uptime", "num_sta", "version"],
            },
            "devices restart": {
                "description": "Restart a device",
                "args": [{"name": "mac", "required": true, "description": "MAC address"}],
                "output_fields": ["status", "action", "mac"],
                "mutating": true,
            },
            "devices locate": {
                "description": "Toggle locate LED on a device",
                "args": [
                    {"name": "mac", "required": true, "description": "MAC address"},
                    {"name": "--off", "required": false, "description": "Turn off locate LED"},
                ],
                "output_fields": ["status", "action", "mac"],
                "mutating": true,
            },
            "devices ports": {
                "description": "Show switch/router port table",
                "args": [{"name": "mac", "required": true, "description": "MAC address"}],
                "output_fields": ["port_idx", "name", "media", "up", "speed", "full_duplex", "poe_enable", "poe_power", "tx_bytes", "rx_bytes"],
            },
            "networks": {
                "description": "List networks",
                "args": [],
                "output_fields": ["name", "vlan_id", "enabled", "default"],
            },
            "events list": {
                "description": "List recent controller events",
                "args": [
                    {"name": "--limit", "required": false, "description": "Number of events (default: 10)"},
                ],
                "output_fields": ["key", "msg", "subsystem", "time", "datetime"],
            },
            "devices upgrade": {
                "description": "Trigger firmware upgrade on a device",
                "args": [{"name": "mac", "required": true, "description": "MAC address"}],
                "output_fields": ["status", "action", "mac"],
                "mutating": true,
            },
            "system health": {
                "description": "Show system health",
                "args": [],
                "output_fields": ["subsystem", "status", "num_sta", "num_ap", "num_switches", "wan_ip", "isp_name"],
            },
            "system info": {
                "description": "Show system info",
                "args": [],
                "output_fields": ["hostname", "version", "timezone", "uptime"],
            },
            "completions": {
                "description": "Generate shell completions",
                "args": [{"name": "shell", "required": true, "description": "Shell (bash, zsh, fish, powershell)"}],
                "note": "Does not require --host or --api-key",
            },
            "config init": {
                "description": "Create or update config file interactively",
                "args": [],
                "note": "Does not require --host or --api-key. Supports named profiles.",
            },
            "config check": {
                "description": "Verify configuration and test connectivity",
                "args": [],
                "note": "Requires --host and --api-key (or config file).",
            },
            "top": {
                "description": "Live dashboard with real-time bandwidth and device status",
                "args": [
                    {"name": "--interval", "required": false, "description": "Refresh interval in seconds (default: 2)"},
                ],
                "note": "Interactive TUI. Keys: q quit, s sort, tab focus, / filter, ↑↓ scroll",
            },
        },
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&schema).expect("failed to serialize schema")
    );
}

fn load_config_from(
    path: &std::path::Path,
    profile: Option<&str>,
) -> (Option<String>, Option<String>) {
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
    (None, None)
}

fn load_config(profile: Option<&str>) -> (Option<String>, Option<String>) {
    let config_path = dirs::config_dir().map(|d| d.join("unifi").join("config.toml"));

    if let Some(path) = config_path {
        return load_config_from(&path, profile);
    }

    (None, None)
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
    Saved { profile: Option<String> },
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

fn extract_credentials(table: &toml::Table) -> (Option<String>, Option<String>) {
    (
        table.get("host").and_then(|v| v.as_str()).map(String::from),
        table
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(String::from),
    )
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

fn run_init_with_io(
    reader: &mut dyn std::io::BufRead,
    writer: &mut dyn std::io::Write,
    config_path: &std::path::Path,
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
    let (current_host, current_key) = existing
        .as_ref()
        .and_then(|c| resolve_profile_table(c, profile_name.as_deref()))
        .map(extract_credentials)
        .unwrap_or((None, None));

    // Host prompt
    let host_prompt = match current_host {
        Some(ref h) => format!("Controller host [{h}]: "),
        None => "Controller host (e.g., https://unifi.local): ".to_string(),
    };
    let host_input = prompt_line(reader, writer, &host_prompt)?;
    let host = if host_input.is_empty() {
        current_host.ok_or(InitError("Host is required".into()))?
    } else {
        host_input
    };

    // API key prompt
    let key_prompt = match current_key {
        Some(ref k) => format!("API key [{}]: ", mask_api_key(k)),
        None => "API key: ".to_string(),
    };
    let key_input = prompt_line(reader, writer, &key_prompt)?;
    let api_key = if key_input.is_empty() {
        current_key.ok_or(InitError("API key is required".into()))?
    } else {
        key_input
    };

    // Show summary and confirm
    let label = profile_name
        .as_deref()
        .map(|n| format!(" (profile: {n})"))
        .unwrap_or_default();
    writeln!(writer)?;
    writeln!(writer, "Configuration{label}:")?;
    writeln!(writer, "  host    = {host}")?;
    writeln!(writer, "  api_key = {}", mask_api_key(&api_key))?;
    writeln!(writer, "  path    = {}", config_path.display())?;

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
        section.insert("host".into(), toml::Value::String(host));
        section.insert("api_key".into(), toml::Value::String(api_key));
        profiles.insert(name.clone(), toml::Value::Table(section));
    } else {
        config.insert("host".into(), toml::Value::String(host));
        config.insert("api_key".into(), toml::Value::String(api_key));
    }

    // Write config
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| InitError(format!("Failed to create config directory: {e}")))?;
    }
    let toml_str = toml::to_string_pretty(&config)
        .map_err(|e| InitError(format!("Failed to serialize config: {e}")))?;
    std::fs::write(config_path, &toml_str)
        .map_err(|e| InitError(format!("Failed to write config: {e}")))?;

    writeln!(writer, "Config saved to {}{label}", config_path.display())?;
    writeln!(
        writer,
        "\nRun 'unifi system health' to verify your connection."
    )?;

    Ok(InitOutcome::Saved {
        profile: profile_name,
    })
}

fn run_init() {
    let path = match default_config_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(exit_codes::GENERAL_ERROR);
        }
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    match run_init_with_io(&mut reader, &mut writer, &path) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(exit_codes::CONFIG_ERROR);
        }
    }
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let out = OutputConfig::new(cli.json, cli.quiet);

    match &cli.command {
        Command::Schema => {
            print_schema();
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
            run_init();
            return;
        }
        _ => {}
    }

    let (config_host, config_api_key) = load_config(cli.profile.as_deref());

    let host = cli.host.or(config_host).unwrap_or_else(|| {
        eprintln!("Error: No host specified.");
        eprintln!();
        eprintln!("  Run 'unifi config init' for interactive setup, or:");
        eprintln!("  - Set UNIFI_HOST environment variable");
        eprintln!("  - Use --host flag");
        std::process::exit(exit_codes::CONFIG_ERROR);
    });

    let api_key = cli.api_key.or(config_api_key).unwrap_or_else(|| {
        eprintln!("Error: No API key specified.");
        eprintln!();
        eprintln!("  Run 'unifi config init' for interactive setup, or:");
        eprintln!("  - Set UNIFI_API_KEY environment variable");
        eprintln!("  - Use --api-key flag");
        eprintln!();
        eprintln!("  Generate an API key in UniFi Settings > API");
        std::process::exit(exit_codes::CONFIG_ERROR);
    });

    let mut client = match api::UnifiClient::new(&host, &api_key) {
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
            } => {
                let filter = commands::clients::ListFilter {
                    wired,
                    wireless,
                    name,
                };
                commands::clients::list(&mut client, out, filter, watch).await
            }
            ClientsCommand::Show { mac } => commands::clients::show(&client, &mac, out).await,
            ClientsCommand::SetFixedIp { mac, ip, name } => {
                if ip.parse::<std::net::IpAddr>().is_err() {
                    eprintln!("Error: Invalid IP address: {ip}");
                    std::process::exit(exit_codes::CONFIG_ERROR);
                }
                commands::clients::set_fixed_ip(&client, &mac, &ip, name.as_deref(), out).await
            }
            ClientsCommand::Block { mac } => commands::clients::block(&client, &mac, out).await,
            ClientsCommand::Unblock { mac } => commands::clients::unblock(&client, &mac, out).await,
            ClientsCommand::Kick { mac } => commands::clients::kick(&client, &mac, out).await,
            ClientsCommand::Top { limit } => commands::clients::top(&client, out, limit).await,
        },
        Command::Devices(cmd) => match cmd {
            DevicesCommand::List { watch } => {
                commands::devices::list(&mut client, out, watch).await
            }
            DevicesCommand::Show { mac } => commands::devices::show(&client, &mac, out).await,
            DevicesCommand::Restart { mac } => commands::devices::restart(&client, &mac, out).await,
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
            DevicesCommand::Upgrade { mac } => commands::devices::upgrade(&client, &mac, out).await,
        },
        Command::Networks => commands::networks::list(&mut client, out).await,
        Command::Events(cmd) => match cmd {
            EventsCommand::List { limit } => commands::events::list(&client, out, limit).await,
        },
        Command::System(cmd) => match cmd {
            SystemCommand::Health => commands::system::health(&client, out).await,
            SystemCommand::Info => commands::system::info(&client, out).await,
        },
        Command::Tui { interval } => unifi_cli::tui::run(&client, interval).await,
        Command::Config(ConfigCommand::Check) => {
            run_config_check(&client).await;
            return;
        }
        Command::Schema | Command::Completions { .. } | Command::Config(ConfigCommand::Init) => {
            unreachable!()
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(exit_code_for_error(e.as_ref()));
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

        let (host, api_key) = load_config_from(&path, None);
        assert_eq!(host.as_deref(), Some("unifi.example.com"));
        assert_eq!(api_key.as_deref(), Some("secret123"));
    }

    #[test]
    fn load_config_host_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"unifi.local\"").unwrap();

        let (host, api_key) = load_config_from(&path, None);
        assert_eq!(host.as_deref(), Some("unifi.local"));
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_missing_file() {
        let path = std::path::Path::new("/tmp/nonexistent-unifi-test.toml");
        let (host, api_key) = load_config_from(path, None);
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "not valid toml {{{{").unwrap();

        let (host, api_key) = load_config_from(&path, None);
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "").unwrap();

        let (host, api_key) = load_config_from(&path, None);
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_extra_keys_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"h\"\napi_key = \"k\"\nfoo = \"bar\"").unwrap();

        let (host, api_key) = load_config_from(&path, None);
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

        let (host, api_key) = load_config_from(&path, Some("office"));
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

        let (host, api_key) = load_config_from(&path, None);
        assert_eq!(host.as_deref(), Some("default.local"));
        assert_eq!(api_key.as_deref(), Some("default_key"));
    }

    #[test]
    fn load_config_missing_profile_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"h\"\napi_key = \"k\"").unwrap();

        let (host, api_key) = load_config_from(&path, Some("nonexistent"));
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

        let (host, _) = load_config_from(&path, Some("home"));
        assert_eq!(host.as_deref(), Some("home.local"));

        let (host, _) = load_config_from(&path, Some("work"));
        assert_eq!(host.as_deref(), Some("work.example.com"));
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

        let result = run_init_with_io(&mut reader, &mut output, &path).unwrap();

        let written = std::fs::read_to_string(&path).unwrap_or_default();
        let display = String::from_utf8(output).unwrap();
        (result, written, display)
    }

    #[test]
    fn init_fresh_default_profile() {
        let (result, written, display) =
            run_init_test(None, "\nhttps://unifi.local\nmy-api-key\ny\n");

        assert_eq!(result, InitOutcome::Saved { profile: None });
        assert!(written.contains("host = \"https://unifi.local\""));
        assert!(written.contains("api_key = \"my-api-key\""));
        assert!(display.contains("Config saved to"));
        assert!(display.contains("unifi system health"));
    }

    #[test]
    fn init_fresh_named_profile() {
        let (result, written, _) =
            run_init_test(None, "office\nhttps://office.local\noffice-key\ny\n");

        assert_eq!(
            result,
            InitOutcome::Saved {
                profile: Some("office".into())
            }
        );
        assert!(written.contains("[profiles.office]"));
        assert!(written.contains("host = \"https://office.local\""));
    }

    #[test]
    fn init_cancelled() {
        let (result, written, display) =
            run_init_test(None, "\nhttps://unifi.local\nmy-api-key\nn\n");

        assert_eq!(result, InitOutcome::Cancelled);
        assert!(written.is_empty());
        assert!(display.contains("Cancelled"));
    }

    #[test]
    fn init_preserves_existing_default_when_adding_profile() {
        let existing = "host = \"default.local\"\napi_key = \"default-key\"\n";
        let (result, written, _) =
            run_init_test(Some(existing), "work\nhttps://work.local\nwork-key\ny\n");

        assert_eq!(
            result,
            InitOutcome::Saved {
                profile: Some("work".into())
            }
        );
        assert!(written.contains("host = \"default.local\""));
        assert!(written.contains("api_key = \"default-key\""));
        assert!(written.contains("[profiles.work]"));
        assert!(written.contains("host = \"https://work.local\""));
    }

    #[test]
    fn init_keeps_existing_value_on_empty_input() {
        let existing = "host = \"existing.local\"\napi_key = \"existing-key\"\n";
        // Empty host and key inputs → keep existing values
        let (result, written, _) = run_init_test(Some(existing), "\n\n\ny\n");

        assert_eq!(result, InitOutcome::Saved { profile: None });
        assert!(written.contains("host = \"existing.local\""));
        assert!(written.contains("api_key = \"existing-key\""));
    }

    #[test]
    fn init_overwrites_existing_value() {
        let existing = "host = \"old.local\"\napi_key = \"old-key\"\n";
        let (_, written, _) = run_init_test(Some(existing), "\nnew.local\nnew-key\ny\n");

        assert!(written.contains("host = \"new.local\""));
        assert!(written.contains("api_key = \"new-key\""));
        assert!(!written.contains("old.local"));
    }

    #[test]
    fn init_shows_masked_key_in_prompt() {
        let existing = "host = \"h\"\napi_key = \"abcdefghij\"\n";
        let (_, _, display) = run_init_test(Some(existing), "\n\n\ny\n");

        assert!(display.contains("abcd…ghij"));
    }

    #[test]
    fn init_shows_summary_before_confirm() {
        let (_, _, display) = run_init_test(None, "\nhttps://test.local\ntest-key-1234567890\ny\n");

        assert!(display.contains("Configuration:"));
        assert!(display.contains("host    = https://test.local"));
        assert!(display.contains("api_key = test…7890"));
        assert!(display.contains("Save? (y/n)"));
    }

    #[test]
    fn init_warns_on_corrupt_existing_config() {
        let (result, written, display) = run_init_test(
            Some("not valid {{{ toml"),
            "\nhttps://new.local\nnew-key\ny\n",
        );

        assert_eq!(result, InitOutcome::Saved { profile: None });
        assert!(display.contains("Warning: Existing config"));
        assert!(display.contains("invalid TOML"));
        assert!(written.contains("host = \"https://new.local\""));
    }

    #[test]
    fn init_accepts_mixed_case_yes() {
        let (result, _, _) = run_init_test(None, "\nhttps://h\nk\nYes\n");
        assert_eq!(result, InitOutcome::Saved { profile: None });
    }

    #[test]
    fn init_host_required_when_no_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let input = "\n\nkey\ny\n";
        let mut reader = std::io::Cursor::new(input.as_bytes().to_vec());
        let mut output = Vec::new();

        let result = run_init_with_io(&mut reader, &mut output, &path);
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

        let result = run_init_with_io(&mut reader, &mut output, &path);
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
            "10.0.0.5",
            "--name",
            "MyDevice",
        ]);
        match cli.command {
            Command::Clients(ClientsCommand::SetFixedIp { mac, ip, name }) => {
                assert_eq!(mac, "aa:bb:cc:dd:ee:ff");
                assert_eq!(ip, "10.0.0.5");
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
            "10.0.0.5",
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
            Command::Devices(DevicesCommand::Restart { mac }) => {
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
    fn cli_networks() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "networks"]);
        assert!(matches!(cli.command, Command::Networks));
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
    fn cli_profile_default_none() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "networks"]);
        assert!(cli.profile.is_none());
    }

    #[test]
    fn cli_events_list() {
        let cli = parse(&["unifi", "--host", "h", "--api-key", "k", "events", "list"]);
        match cli.command {
            Command::Events(EventsCommand::List { limit }) => {
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
            Command::Events(EventsCommand::List { limit }) => {
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
            Command::Devices(DevicesCommand::Upgrade { mac }) => {
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
        let (host, api_key) = load_config_from(&path, None);
        assert!(host.is_none());
        assert!(api_key.is_none());

        // Named profile → returns profile
        let (host, api_key) = load_config_from(&path, Some("office"));
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

        let (host, api_key) = load_config_from(&path, Some("partial"));
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

        let (host, api_key) = load_config_from(&path, Some("my-home"));
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
        let (host, key) = extract_credentials(&table);
        assert_eq!(host.as_deref(), Some("h"));
        assert_eq!(key.as_deref(), Some("k"));
    }

    #[test]
    fn extract_credentials_empty_table() {
        let table = toml::Table::new();
        let (host, key) = extract_credentials(&table);
        assert!(host.is_none());
        assert!(key.is_none());
    }

    #[test]
    fn extract_credentials_non_string_values() {
        let table: toml::Table = "host = 12345".parse().unwrap();
        let (host, _) = extract_credentials(&table);
        assert!(host.is_none()); // integer, not string
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
        let client = api::UnifiClient::new("192.168.1.1", "key").unwrap();
        assert_eq!(client.base_url(), "https://192.168.1.1");
    }

    // --- error_for_status ---

    #[test]
    fn error_for_status_401_returns_auth() {
        let err = api::UnifiClient::error_for_status_pub(401, "Unauthorized".into());
        assert!(matches!(err, api::ApiError::Auth(_)));
    }

    #[test]
    fn error_for_status_403_returns_auth() {
        let err = api::UnifiClient::error_for_status_pub(403, "Forbidden".into());
        assert!(matches!(err, api::ApiError::Auth(_)));
    }

    #[test]
    fn error_for_status_404_returns_not_found() {
        let err = api::UnifiClient::error_for_status_pub(404, "Not Found".into());
        assert!(matches!(err, api::ApiError::NotFound(_)));
    }

    #[test]
    fn error_for_status_500_returns_api_error() {
        let err = api::UnifiClient::error_for_status_pub(500, "Server Error".into());
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
        let err = api::UnifiClient::error_for_status_pub(200, "unexpected".into());
        assert!(matches!(err, api::ApiError::Api { status: 200, .. }));
    }
}
