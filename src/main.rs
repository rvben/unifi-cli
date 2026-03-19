mod api;
mod commands;

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

fn load_config() -> (Option<String>, Option<String>) {
    let config_path = dirs::config_dir()
        .map(|d| d.join("unifi-cli").join("config.toml"));

    if let Some(path) = config_path {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(config) = contents.parse::<toml::Table>() {
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
        }
    }

    (None, None)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let (config_host, config_api_key) = load_config();

    let host = cli
        .host
        .or(config_host)
        .unwrap_or_else(|| {
            eprintln!("Error: No host specified. Set UNIFI_HOST or use --host");
            std::process::exit(1);
        });

    let api_key = cli
        .api_key
        .or(config_api_key)
        .unwrap_or_else(|| {
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
            ClientsCommand::Show { mac } => {
                commands::clients::show(&client, &mac, cli.json).await
            }
            ClientsCommand::SetFixedIp { mac, ip, name } => {
                commands::clients::set_fixed_ip(&client, &mac, &ip, name.as_deref()).await
            }
            ClientsCommand::Block { mac } => commands::clients::block(&client, &mac).await,
            ClientsCommand::Unblock { mac } => commands::clients::unblock(&client, &mac).await,
            ClientsCommand::Kick { mac } => commands::clients::kick(&client, &mac).await,
        },
        Command::Devices(cmd) => match cmd {
            DevicesCommand::List => commands::devices::list(&mut client, cli.json).await,
            DevicesCommand::Restart { mac } => {
                commands::devices::restart(&client, &mac).await
            }
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
