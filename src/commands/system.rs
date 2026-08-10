use owo_colors::OwoColorize;

use crate::api::{UnifiClient, format_uptime};
use crate::output::{OutputConfig, use_color};

pub async fn health(
    client: &UnifiClient,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let subsystems = client.get_health().await?;

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(
            &subsystems
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "subsystem": s.subsystem,
                        "status": s.status,
                        "num_sta": s.num_sta,
                        "num_ap": s.num_ap,
                        "num_switches": s.num_switches,
                        "wan_ip": s.wan_ip,
                        "isp_name": s.isp_name,
                    })
                })
                .collect::<Vec<_>>(),
        )?);
        return Ok(());
    }

    let color = use_color();
    let header = format!("{:<14} {:<10} {}", "Subsystem", "Status", "Details");
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(60).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(60));
    }

    for s in &subsystems {
        let status = s.status.as_deref().unwrap_or("-");
        let details = match s.subsystem.as_str() {
            "wan" => {
                let mut parts = Vec::new();
                if let Some(ref ip) = s.wan_ip {
                    parts.push(format!("IP: {ip}"));
                }
                if let Some(ref isp) = s.isp_name {
                    parts.push(format!("ISP: {isp}"));
                }
                parts.join(", ")
            }
            "wlan" => {
                let mut parts = Vec::new();
                if let Some(ap) = s.num_ap {
                    parts.push(format!("{ap} APs"));
                }
                if let Some(sta) = s.num_sta {
                    parts.push(format!("{sta} clients"));
                }
                parts.join(", ")
            }
            "lan" => {
                let mut parts = Vec::new();
                if let Some(sw) = s.num_switches {
                    parts.push(format!("{sw} switches"));
                }
                if let Some(sta) = s.num_sta {
                    parts.push(format!("{sta} clients"));
                }
                parts.join(", ")
            }
            _ => String::new(),
        };

        let status_display = if color {
            if status == "ok" {
                format!("{}", status.green())
            } else {
                format!("{}", status.red())
            }
        } else {
            status.to_string()
        };

        println!(" {:<13} {:<10} {}", s.subsystem, status_display, details);
    }

    Ok(())
}

pub async fn info(
    client: &UnifiClient,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let sys = client.get_sysinfo().await?;
    let host = client.get_host_system().await.ok();
    // Unknown when the host system request failed as well as when it answered
    // without a device state: a check that could not be made is not a check that
    // came back clean, and reporting one as the other tells a caller its firmware
    // is current when nothing established that.
    let update_available = host.as_ref().and_then(|h| h.update_available());

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "hostname": sys.hostname,
            "version": sys.version,
            "timezone": sys.timezone,
            "uptime": sys.uptime,
            "update_available": update_available,
        }))?);
        return Ok(());
    }

    let color = use_color();
    let label = |l: &str| -> String {
        if color {
            format!("{}", l.dimmed())
        } else {
            l.to_string()
        }
    };

    if let Some(ref h) = sys.hostname {
        if color {
            println!("{}", h.bold());
        } else {
            println!("{h}");
        }
    }

    if let Some(ref v) = sys.version {
        println!("  {}  {v}", label("Version: "));
    }
    if let Some(ref tz) = sys.timezone {
        println!("  {}  {tz}", label("Timezone:"));
    }
    if let Some(uptime) = sys.uptime {
        println!("  {}  {}", label("Uptime:  "), format_uptime(uptime));
    }
    // Nothing is printed for a host that reported being up to date, since the
    // absence of the line has always meant that. An unknown state gets a line of
    // its own so it cannot be read the same way.
    match update_available {
        Some(true) => {
            if color {
                println!("  {}  {}", label("Update:  "), "Available".yellow());
            } else {
                println!("  {}  Available", label("Update:  "));
            }
        }
        None => println!(
            "  {}  Unknown (host system did not report)",
            label("Update:  ")
        ),
        Some(false) => {}
    }

    Ok(())
}
