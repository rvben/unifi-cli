use unifi_cli::api;
use unifi_cli::commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "unifi-cli", about = "Minimal CLI for UniFi Network controller")]
struct Cli {
    /// UniFi controller host (or set UNIFI_HOST env var)
    #[arg(long, env = "UNIFI_HOST")]
    host: Option<String>,

    /// API key (or set UNIFI_API_KEY env var)
    #[arg(long, env = "UNIFI_API_KEY")]
    api_key: Option<String>,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

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

    /// System information
    #[command(subcommand)]
    System(SystemCommand),
}

#[derive(Subcommand)]
enum ClientsCommand {
    /// List all connected clients
    List,
    /// Show details for a client by MAC address
    Show {
        /// MAC address (any format: aa:bb:cc:dd:ee:ff, aa-bb-cc-dd-ee-ff, aabbccddeeff)
        mac: String,
    },
    /// Set a fixed IP (DHCP reservation) for a client
    SetFixedIp {
        /// MAC address
        mac: String,
        /// Fixed IP address to assign
        ip: String,
        /// Friendly name for the client
        #[arg(long)]
        name: Option<String>,
    },
    /// Block a client
    Block {
        /// MAC address
        mac: String,
    },
    /// Unblock a client
    Unblock {
        /// MAC address
        mac: String,
    },
    /// Kick (disconnect) a client
    Kick {
        /// MAC address
        mac: String,
    },
}

#[derive(Subcommand)]
enum DevicesCommand {
    /// List all network devices
    List,
    /// Restart a device
    Restart {
        /// MAC address
        mac: String,
    },
    /// Toggle locate LED on a device
    Locate {
        /// MAC address
        mac: String,
        /// Turn off locate LED
        #[arg(long)]
        off: bool,
    },
}

#[derive(Subcommand)]
enum SystemCommand {
    /// Show system health
    Health,
    /// Show system info
    Info,
}

fn load_config_from(path: &std::path::Path) -> (Option<String>, Option<String>) {
    if let Ok(contents) = std::fs::read_to_string(path)
        && let Ok(config) = contents.parse::<toml::Table>()
    {
        let host = config
            .get("host")
            .and_then(|v| v.as_str())
            .map(String::from);
        let api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(String::from);
        return (host, api_key);
    }
    (None, None)
}

fn load_config() -> (Option<String>, Option<String>) {
    let config_path = dirs::config_dir().map(|d| d.join("unifi-cli").join("config.toml"));

    if let Some(path) = config_path {
        return load_config_from(&path);
    }

    (None, None)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let (config_host, config_api_key) = load_config();

    let host = cli.host.or(config_host).unwrap_or_else(|| {
        eprintln!("Error: No host specified. Set UNIFI_HOST or use --host");
        std::process::exit(1);
    });

    let api_key = cli.api_key.or(config_api_key).unwrap_or_else(|| {
        eprintln!("Error: No API key specified. Set UNIFI_API_KEY or use --api-key");
        std::process::exit(1);
    });

    let mut client = match api::UnifiClient::new(&host, &api_key) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error creating client: {e}");
            std::process::exit(1);
        }
    };

    let result: Result<(), Box<dyn std::error::Error>> = match cli.command {
        Command::Clients(cmd) => match cmd {
            ClientsCommand::List => commands::clients::list(&mut client, cli.json).await,
            ClientsCommand::Show { mac } => commands::clients::show(&client, &mac, cli.json).await,
            ClientsCommand::SetFixedIp { mac, ip, name } => {
                commands::clients::set_fixed_ip(&client, &mac, &ip, name.as_deref()).await
            }
            ClientsCommand::Block { mac } => commands::clients::block(&client, &mac).await,
            ClientsCommand::Unblock { mac } => commands::clients::unblock(&client, &mac).await,
            ClientsCommand::Kick { mac } => commands::clients::kick(&client, &mac).await,
        },
        Command::Devices(cmd) => match cmd {
            DevicesCommand::List => commands::devices::list(&mut client, cli.json).await,
            DevicesCommand::Restart { mac } => commands::devices::restart(&client, &mac).await,
            DevicesCommand::Locate { mac, off } => {
                commands::devices::locate(&client, &mac, off).await
            }
        },
        Command::Networks => commands::networks::list(&mut client, cli.json).await,
        Command::System(cmd) => match cmd {
            SystemCommand::Health => commands::system::health(&client, cli.json).await,
            SystemCommand::Info => commands::system::info(&client, cli.json).await,
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
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

        let (host, api_key) = load_config_from(&path);
        assert_eq!(host.as_deref(), Some("unifi.example.com"));
        assert_eq!(api_key.as_deref(), Some("secret123"));
    }

    #[test]
    fn load_config_host_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"unifi.local\"").unwrap();

        let (host, api_key) = load_config_from(&path);
        assert_eq!(host.as_deref(), Some("unifi.local"));
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_missing_file() {
        let path = std::path::Path::new("/tmp/nonexistent-unifi-cli-test.toml");
        let (host, api_key) = load_config_from(path);
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "not valid toml {{{{").unwrap();

        let (host, api_key) = load_config_from(&path);
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "").unwrap();

        let (host, api_key) = load_config_from(&path);
        assert!(host.is_none());
        assert!(api_key.is_none());
    }

    #[test]
    fn load_config_extra_keys_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "host = \"h\"\napi_key = \"k\"\nfoo = \"bar\"").unwrap();

        let (host, api_key) = load_config_from(&path);
        assert_eq!(host.as_deref(), Some("h"));
        assert_eq!(api_key.as_deref(), Some("k"));
    }

    // --- CLI parsing ---

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn cli_clients_list() {
        let cli = parse(&[
            "unifi-cli",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "list",
        ]);
        assert_eq!(cli.host.as_deref(), Some("h"));
        assert_eq!(cli.api_key.as_deref(), Some("k"));
        assert!(!cli.json);
        assert!(matches!(
            cli.command,
            Command::Clients(ClientsCommand::List)
        ));
    }

    #[test]
    fn cli_clients_show() {
        let cli = parse(&[
            "unifi-cli",
            "--host",
            "h",
            "--api-key",
            "k",
            "clients",
            "show",
            "aa:bb:cc:dd:ee:ff",
        ]);
        match cli.command {
            Command::Clients(ClientsCommand::Show { mac }) => assert_eq!(mac, "aa:bb:cc:dd:ee:ff"),
            _ => panic!("expected Clients Show"),
        }
    }

    #[test]
    fn cli_clients_set_fixed_ip_with_name() {
        let cli = parse(&[
            "unifi-cli",
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
            "unifi-cli",
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
            "unifi-cli",
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
            "unifi-cli",
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
            "unifi-cli",
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
        let cli = parse(&[
            "unifi-cli",
            "--host",
            "h",
            "--api-key",
            "k",
            "devices",
            "list",
        ]);
        assert!(matches!(
            cli.command,
            Command::Devices(DevicesCommand::List)
        ));
    }

    #[test]
    fn cli_devices_restart() {
        let cli = parse(&[
            "unifi-cli",
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
            "unifi-cli",
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
            "unifi-cli",
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
        let cli = parse(&["unifi-cli", "--host", "h", "--api-key", "k", "networks"]);
        assert!(matches!(cli.command, Command::Networks));
    }

    #[test]
    fn cli_system_health() {
        let cli = parse(&[
            "unifi-cli",
            "--host",
            "h",
            "--api-key",
            "k",
            "system",
            "health",
        ]);
        assert!(matches!(
            cli.command,
            Command::System(SystemCommand::Health)
        ));
    }

    #[test]
    fn cli_system_info() {
        let cli = parse(&[
            "unifi-cli",
            "--host",
            "h",
            "--api-key",
            "k",
            "system",
            "info",
        ]);
        assert!(matches!(cli.command, Command::System(SystemCommand::Info)));
    }

    #[test]
    fn cli_json_flag() {
        let cli = parse(&[
            "unifi-cli",
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
            "unifi-cli",
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
    fn cli_missing_subcommand_fails() {
        assert!(Cli::try_parse_from(["unifi-cli", "--host", "h", "--api-key", "k"]).is_err());
    }

    #[test]
    fn cli_host_and_key_optional() {
        let cli = parse(&["unifi-cli", "networks"]);
        assert!(cli.host.is_none());
        assert!(cli.api_key.is_none());
    }
}
