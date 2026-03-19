use tabled::{Table, Tabled};

use crate::api::{
    Client, LegacyClient, UnifiClient, format_bytes, format_mac, format_uptime, normalize_mac,
};
use crate::output::OutputConfig;

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

pub struct ListFilter {
    pub wired: bool,
    pub wireless: bool,
    pub name: Option<String>,
}

fn apply_filter(clients: Vec<Client>, filter: &ListFilter) -> Vec<Client> {
    clients
        .into_iter()
        .filter(|c| {
            if filter.wired && c.client_type.as_deref() != Some("WIRED") {
                return false;
            }
            if filter.wireless && c.client_type.as_deref() != Some("WIRELESS") {
                return false;
            }
            if let Some(ref name_filter) = filter.name {
                let needle = name_filter.to_lowercase();
                let display = c.display_name().to_lowercase();
                if !display.contains(&needle) {
                    return false;
                }
            }
            true
        })
        .collect()
}

fn render_clients(clients: &[Client], out: &OutputConfig) {
    if out.json {
        out.print_data(
            &serde_json::to_string_pretty(
                &clients
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "name": c.display_name(),
                            "mac": c.mac_address.as_deref().map(|m| format_mac(&normalize_mac(m))),
                            "ip": c.ip_address,
                            "type": c.client_type,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("failed to serialize JSON"),
        );
    } else {
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

        out.print_data(&Table::new(rows).to_string());
    }
    out.print_message(&format!("\n{} clients", clients.len()));
}

pub async fn list(
    client: &mut UnifiClient,
    out: OutputConfig,
    filter: ListFilter,
    watch: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(interval) = watch {
        use crossterm::execute;
        use crossterm::terminal::EnterAlternateScreen;

        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;

        loop {
            execute!(stdout, crossterm::cursor::MoveTo(0, 0))?;
            execute!(
                stdout,
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
            )?;
            eprintln!("Every {interval}s | clients list (press Ctrl+C to exit)\n");
            match client.list_clients().await {
                Ok(clients) => {
                    let filtered = apply_filter(clients, &filter);
                    render_clients(&filtered, &out);
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        }
    } else {
        let clients = client.list_clients().await?;
        let filtered = apply_filter(clients, &filter);
        render_clients(&filtered, &out);
        Ok(())
    }
}

pub async fn show(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let c = client.get_client_detail(mac).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
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
        }))?);
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

    out.print_data(&Table::new(rows).to_string());
    Ok(())
}

pub async fn set_fixed_ip(
    client: &UnifiClient,
    mac: &str,
    ip: &str,
    name: Option<&str>,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    client.set_fixed_ip(mac, ip, name).await?;

    let mut result = serde_json::json!({
        "status": "ok",
        "action": "set_fixed_ip",
        "mac": format_mac(mac),
        "ip": ip,
    });
    if let Some(n) = name {
        result["name"] = serde_json::json!(n);
    }

    let mut msg = format!("Set fixed IP {ip} for {}", format_mac(mac));
    if let Some(n) = name {
        msg.push_str(&format!("\nSet name: {n}"));
    }

    out.print_result(&result, &msg);
    Ok(())
}

pub async fn block(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    client.block_client(mac).await?;
    out.print_result(
        &serde_json::json!({"status": "ok", "action": "block", "mac": format_mac(mac)}),
        &format!("Blocked {}", format_mac(mac)),
    );
    Ok(())
}

pub async fn unblock(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    client.unblock_client(mac).await?;
    out.print_result(
        &serde_json::json!({"status": "ok", "action": "unblock", "mac": format_mac(mac)}),
        &format!("Unblocked {}", format_mac(mac)),
    );
    Ok(())
}

pub async fn kick(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    client.kick_client(mac).await?;
    out.print_result(
        &serde_json::json!({"status": "ok", "action": "kick", "mac": format_mac(mac)}),
        &format!("Kicked {}", format_mac(mac)),
    );
    Ok(())
}

#[derive(Tabled)]
struct TopClientRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "MAC")]
    mac: String,
    #[tabled(rename = "IP")]
    ip: String,
    #[tabled(rename = "TX")]
    tx: String,
    #[tabled(rename = "RX")]
    rx: String,
    #[tabled(rename = "Total")]
    total: String,
}

fn total_bytes(c: &LegacyClient) -> u64 {
    c.tx_bytes.unwrap_or(0) + c.rx_bytes.unwrap_or(0)
}

pub async fn top(
    client: &UnifiClient,
    out: OutputConfig,
    limit: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut clients = client.list_clients_legacy().await?;
    clients.sort_by_key(|c| std::cmp::Reverse(total_bytes(c)));
    let top_clients: Vec<&LegacyClient> = clients.iter().take(limit).collect();

    if out.json {
        out.print_data(
            &serde_json::to_string_pretty(
                &top_clients
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "name": c.display_name(),
                            "mac": c.mac.as_deref().map(|m| format_mac(&normalize_mac(m))),
                            "ip": c.ip,
                            "tx_bytes": c.tx_bytes,
                            "rx_bytes": c.rx_bytes,
                            "total_bytes": total_bytes(c),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("failed to serialize JSON"),
        );
    } else {
        let rows: Vec<TopClientRow> = top_clients
            .iter()
            .map(|c| TopClientRow {
                name: c.display_name().to_string(),
                mac: c
                    .mac
                    .as_deref()
                    .map(|m| format_mac(&normalize_mac(m)))
                    .unwrap_or_else(|| "-".into()),
                ip: c.ip.as_deref().unwrap_or("-").to_string(),
                tx: format_bytes(c.tx_bytes.unwrap_or(0)),
                rx: format_bytes(c.rx_bytes.unwrap_or(0)),
                total: format_bytes(total_bytes(c)),
            })
            .collect();

        out.print_data(&Table::new(rows).to_string());
    }
    out.print_message(&format!(
        "\nTop {} of {} clients by bandwidth",
        top_clients.len(),
        clients.len()
    ));
    Ok(())
}
