use tabled::{Table, Tabled};

use crate::api::{UnifiClient, format_bytes, format_mac, format_uptime};

#[derive(Tabled)]
struct ClientRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "MAC")]
    mac: String,
    #[tabled(rename = "IP")]
    ip: String,
    #[tabled(rename = "Type")]
    client_type: String,
}

#[derive(Tabled)]
struct ClientDetailRow {
    #[tabled(rename = "Field")]
    field: String,
    #[tabled(rename = "Value")]
    value: String,
}

pub async fn list(client: &mut UnifiClient, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let clients = client.list_clients().await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &clients
                    .iter()
                    .map(|c| serde_json::json!({
                        "name": c.display_name(),
                        "mac": c.mac_address,
                        "ip": c.ip_address,
                        "type": c.client_type,
                    }))
                    .collect::<Vec<_>>()
            )?
        );
        return Ok(());
    }

    let rows: Vec<ClientRow> = clients
        .iter()
        .map(|c| ClientRow {
            name: c.display_name().to_string(),
            mac: c
                .mac_address
                .as_deref()
                .map(format_mac)
                .unwrap_or_else(|| "-".into()),
            ip: c.ip_address.as_deref().unwrap_or("-").to_string(),
            client_type: c.client_type.as_deref().unwrap_or("-").to_string(),
        })
        .collect();

    println!("{}", Table::new(rows));
    println!("\n{} clients", clients.len());
    Ok(())
}

pub async fn show(
    client: &UnifiClient,
    mac: &str,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let c = client.get_client_detail(mac).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": c.display_name(),
                "mac": c.mac,
                "ip": c.ip,
                "wired": c.is_wired,
                "uptime": c.uptime,
                "tx_bytes": c.tx_bytes,
                "rx_bytes": c.rx_bytes,
                "signal": c.signal,
                "ssid": c.ssid,
                "ap_mac": c.ap_mac,
            }))?
        );
        return Ok(());
    }

    let mut rows = vec![
        ClientDetailRow {
            field: "Name".into(),
            value: c.display_name().to_string(),
        },
        ClientDetailRow {
            field: "MAC".into(),
            value: c
                .mac
                .as_deref()
                .map(format_mac)
                .unwrap_or_else(|| "-".into()),
        },
        ClientDetailRow {
            field: "IP".into(),
            value: c.ip.as_deref().unwrap_or("-").to_string(),
        },
        ClientDetailRow {
            field: "Type".into(),
            value: if c.is_wired { "Wired" } else { "Wireless" }.into(),
        },
    ];

    if let Some(uptime) = c.uptime {
        rows.push(ClientDetailRow {
            field: "Uptime".into(),
            value: format_uptime(uptime),
        });
    }
    if let Some(tx) = c.tx_bytes {
        rows.push(ClientDetailRow {
            field: "TX".into(),
            value: format_bytes(tx),
        });
    }
    if let Some(rx) = c.rx_bytes {
        rows.push(ClientDetailRow {
            field: "RX".into(),
            value: format_bytes(rx),
        });
    }
    if !c.is_wired {
        if let Some(signal) = c.signal {
            rows.push(ClientDetailRow {
                field: "Signal".into(),
                value: format!("{signal} dBm"),
            });
        }
        if let Some(ref ssid) = c.ssid {
            rows.push(ClientDetailRow {
                field: "SSID".into(),
                value: ssid.clone(),
            });
        }
        if let Some(ref ap) = c.ap_mac {
            rows.push(ClientDetailRow {
                field: "AP".into(),
                value: format_mac(ap),
            });
        }
    }

    println!("{}", Table::new(rows));
    Ok(())
}

pub async fn set_fixed_ip(
    client: &UnifiClient,
    mac: &str,
    ip: &str,
    name: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    client.set_fixed_ip(mac, ip, name).await?;
    println!("Set fixed IP {ip} for {}", format_mac(mac));
    if let Some(n) = name {
        println!("Set name: {n}");
    }
    Ok(())
}

pub async fn block(client: &UnifiClient, mac: &str) -> Result<(), Box<dyn std::error::Error>> {
    client.block_client(mac).await?;
    println!("Blocked {}", format_mac(mac));
    Ok(())
}

pub async fn unblock(client: &UnifiClient, mac: &str) -> Result<(), Box<dyn std::error::Error>> {
    client.unblock_client(mac).await?;
    println!("Unblocked {}", format_mac(mac));
    Ok(())
}

pub async fn kick(client: &UnifiClient, mac: &str) -> Result<(), Box<dyn std::error::Error>> {
    client.kick_client(mac).await?;
    println!("Kicked {}", format_mac(mac));
    Ok(())
}
