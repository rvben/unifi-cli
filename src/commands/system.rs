use tabled::{Table, Tabled};

use crate::api::{format_uptime, UnifiClient};

#[derive(Tabled)]
struct HealthRow {
    #[tabled(rename = "Subsystem")]
    subsystem: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Details")]
    details: String,
}

#[derive(Tabled)]
struct InfoRow {
    #[tabled(rename = "Field")]
    field: String,
    #[tabled(rename = "Value")]
    value: String,
}

pub async fn health(
    client: &UnifiClient,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let subsystems = client.get_health().await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &subsystems
                    .iter()
                    .map(|s| serde_json::json!({
                        "subsystem": s.subsystem,
                        "status": s.status,
                        "num_sta": s.num_sta,
                        "wan_ip": s.wan_ip,
                        "isp_name": s.isp_name,
                    }))
                    .collect::<Vec<_>>()
            )?
        );
        return Ok(());
    }

    let rows: Vec<HealthRow> = subsystems
        .iter()
        .map(|s| {
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

            HealthRow {
                subsystem: s.subsystem.clone(),
                status: s.status.as_deref().unwrap_or("-").to_string(),
                details,
            }
        })
        .collect();

    println!("{}", Table::new(rows));
    Ok(())
}

pub async fn info(
    client: &UnifiClient,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let sys = client.get_sysinfo().await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "hostname": sys.hostname,
                "version": sys.version,
                "timezone": sys.timezone,
                "uptime": sys.uptime,
            }))?
        );
        return Ok(());
    }

    let mut rows = Vec::new();

    if let Some(ref h) = sys.hostname {
        rows.push(InfoRow {
            field: "Hostname".into(),
            value: h.clone(),
        });
    }
    if let Some(ref v) = sys.version {
        rows.push(InfoRow {
            field: "Version".into(),
            value: v.clone(),
        });
    }
    if let Some(ref tz) = sys.timezone {
        rows.push(InfoRow {
            field: "Timezone".into(),
            value: tz.clone(),
        });
    }
    if let Some(uptime) = sys.uptime {
        rows.push(InfoRow {
            field: "Uptime".into(),
            value: format_uptime(uptime),
        });
    }

    println!("{}", Table::new(rows));
    Ok(())
}
