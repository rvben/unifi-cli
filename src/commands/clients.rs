use owo_colors::OwoColorize;

use crate::api::{
    Client, LegacyClient, UnifiClient, format_bytes, format_mac, format_uptime, normalize_mac,
};
use crate::output::{OutputConfig, use_color};

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
                            "name": c.clean_name(),
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
        let color = use_color();
        let names: Vec<String> = clients.iter().map(|c| c.clean_name()).collect();
        let name_w = names.iter().map(|n| n.len()).max().unwrap_or(4).max(4) + 2;
        let total_w = name_w + 19 + 15 + 10;
        let header = format!("{:<name_w$} {:<19} {:<15} {}", "Name", "MAC", "IP", "Type");
        if color {
            println!("{}", header.bold());
            println!("{}", "-".repeat(total_w).dimmed());
        } else {
            println!("{header}");
            println!("{}", "-".repeat(total_w));
        }

        for (c, name) in clients.iter().zip(&names) {
            let mac = c
                .mac_address
                .as_deref()
                .map(format_mac)
                .unwrap_or_else(|| "-".into());
            let ip = c.ip_address.as_deref().unwrap_or("-");
            let ctype = c.client_type.as_deref().unwrap_or("-");
            let pad = name_w - 1;

            if color {
                println!(
                    " {:<pad$} {:<19} {:<15} {}",
                    name.bold(),
                    mac.dimmed(),
                    ip,
                    ctype,
                );
            } else {
                println!(" {:<pad$} {:<19} {:<15} {}", name, mac, ip, ctype);
            }
        }
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

    let color = use_color();
    let label = |l: &str| -> String {
        if color {
            format!("{}", l.dimmed())
        } else {
            l.to_string()
        }
    };

    let name = c.display_name();
    if color {
        println!("{}", name.bold());
    } else {
        println!("{name}");
    }

    println!(
        "  {}  {}",
        label("MAC:   "),
        c.mac
            .as_deref()
            .map(format_mac)
            .unwrap_or_else(|| "-".into())
    );
    println!("  {}  {}", label("IP:    "), c.ip.as_deref().unwrap_or("-"));
    println!(
        "  {}  {}",
        label("Type:  "),
        if c.is_wired { "Wired" } else { "Wireless" }
    );

    if let Some(uptime) = c.uptime {
        println!("  {}  {}", label("Uptime:"), format_uptime(uptime));
    }
    if let Some(tx) = c.tx_bytes {
        println!("  {}  {}", label("TX:    "), format_bytes(tx));
    }
    if let Some(rx) = c.rx_bytes {
        println!("  {}  {}", label("RX:    "), format_bytes(rx));
    }
    if !c.is_wired {
        if let Some(signal) = c.signal {
            println!("  {}  {} dBm", label("Signal:"), signal);
        }
        if let Some(ref ssid) = c.ssid {
            println!("  {}  {ssid}", label("SSID:  "));
        }
        if let Some(ref ap) = c.ap_mac {
            println!("  {}  {}", label("AP:    "), format_mac(ap));
        }
    }

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
                            "name": c.clean_name(),
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
        let color = use_color();
        let names: Vec<String> = top_clients.iter().map(|c| c.clean_name()).collect();
        let name_w = names.iter().map(|n| n.len()).max().unwrap_or(4).max(4) + 2;
        let total_w = name_w + 19 + 15 + 10 + 10 + 10;
        let header = format!(
            "{:<name_w$} {:<19} {:<15} {:>10} {:>10} {:>10}",
            "Name", "MAC", "IP", "TX", "RX", "Total"
        );
        if color {
            println!("{}", header.bold());
            println!("{}", "-".repeat(total_w).dimmed());
        } else {
            println!("{header}");
            println!("{}", "-".repeat(total_w));
        }

        for (c, name) in top_clients.iter().zip(&names) {
            let mac = c
                .mac
                .as_deref()
                .map(|m| format_mac(&normalize_mac(m)))
                .unwrap_or_else(|| "-".into());
            let ip = c.ip.as_deref().unwrap_or("-");
            let tx = format_bytes(c.tx_bytes.unwrap_or(0));
            let rx = format_bytes(c.rx_bytes.unwrap_or(0));
            let total = format_bytes(total_bytes(c));
            let pad = name_w - 1;

            if color {
                println!(
                    " {:<pad$} {:<19} {:<15} {:>10} {:>10} {:>10}",
                    name.bold(),
                    mac.dimmed(),
                    ip,
                    tx,
                    rx,
                    total,
                );
            } else {
                println!(
                    " {:<pad$} {:<19} {:<15} {:>10} {:>10} {:>10}",
                    name, mac, ip, tx, rx, total
                );
            }
        }
    }
    out.print_message(&format!(
        "\nTop {} of {} clients by bandwidth",
        top_clients.len(),
        clients.len()
    ));
    Ok(())
}
