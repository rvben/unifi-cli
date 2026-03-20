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

pub(crate) fn apply_filter(clients: Vec<Client>, filter: &ListFilter) -> Vec<Client> {
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

pub(crate) fn total_bytes(c: &LegacyClient) -> u64 {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_client(name: &str, mac: &str, ip: &str, ctype: &str) -> Client {
        serde_json::from_str(&format!(
            r#"{{"name": "{name}", "macAddress": "{mac}", "ipAddress": "{ip}", "type": "{ctype}"}}"#
        ))
        .unwrap()
    }

    // --- total_bytes ---

    #[test]
    fn total_bytes_both_present() {
        let c: LegacyClient =
            serde_json::from_str(r#"{"_id": "x", "tx_bytes": 100, "rx_bytes": 200}"#).unwrap();
        assert_eq!(total_bytes(&c), 300);
    }

    #[test]
    fn total_bytes_none_values() {
        let c: LegacyClient = serde_json::from_str(r#"{"_id": "x"}"#).unwrap();
        assert_eq!(total_bytes(&c), 0);
    }

    #[test]
    fn total_bytes_partial() {
        let c: LegacyClient = serde_json::from_str(r#"{"_id": "x", "tx_bytes": 500}"#).unwrap();
        assert_eq!(total_bytes(&c), 500);
    }

    // --- apply_filter ---

    #[test]
    fn filter_no_constraints() {
        let clients = vec![
            make_client("A", "aa:bb:cc:dd:ee:ff", "10.0.0.1", "WIRED"),
            make_client("B", "11:22:33:44:55:66", "10.0.0.2", "WIRELESS"),
        ];
        let filter = ListFilter {
            wired: false,
            wireless: false,
            name: None,
        };
        assert_eq!(apply_filter(clients, &filter).len(), 2);
    }

    #[test]
    fn filter_wired_only() {
        let clients = vec![
            make_client("A", "aa:bb:cc:dd:ee:ff", "10.0.0.1", "WIRED"),
            make_client("B", "11:22:33:44:55:66", "10.0.0.2", "WIRELESS"),
        ];
        let filter = ListFilter {
            wired: true,
            wireless: false,
            name: None,
        };
        let result = apply_filter(clients, &filter);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].display_name(), "A");
    }

    #[test]
    fn filter_wireless_only() {
        let clients = vec![
            make_client("A", "aa:bb:cc:dd:ee:ff", "10.0.0.1", "WIRED"),
            make_client("B", "11:22:33:44:55:66", "10.0.0.2", "WIRELESS"),
        ];
        let filter = ListFilter {
            wired: false,
            wireless: true,
            name: None,
        };
        let result = apply_filter(clients, &filter);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].display_name(), "B");
    }

    #[test]
    fn filter_by_name_case_insensitive() {
        let clients = vec![
            make_client("Mac Mini", "aa:bb:cc:dd:ee:ff", "10.0.0.1", "WIRED"),
            make_client("iPhone", "11:22:33:44:55:66", "10.0.0.2", "WIRELESS"),
        ];
        let filter = ListFilter {
            wired: false,
            wireless: false,
            name: Some("mac".to_string()),
        };
        let result = apply_filter(clients, &filter);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].display_name(), "Mac Mini");
    }

    #[test]
    fn filter_by_name_no_match() {
        let clients = vec![make_client("A", "aa:bb:cc:dd:ee:ff", "10.0.0.1", "WIRED")];
        let filter = ListFilter {
            wired: false,
            wireless: false,
            name: Some("zzz".to_string()),
        };
        assert!(apply_filter(clients, &filter).is_empty());
    }

    #[test]
    fn filter_combined_type_and_name() {
        let clients = vec![
            make_client("Mac Mini", "aa:bb:cc:dd:ee:ff", "10.0.0.1", "WIRED"),
            make_client("MacBook", "11:22:33:44:55:66", "10.0.0.2", "WIRELESS"),
            make_client("iPhone", "22:33:44:55:66:77", "10.0.0.3", "WIRELESS"),
        ];
        let filter = ListFilter {
            wired: false,
            wireless: true,
            name: Some("mac".to_string()),
        };
        let result = apply_filter(clients, &filter);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].display_name(), "MacBook");
    }

    #[test]
    fn filter_empty_list() {
        let filter = ListFilter {
            wired: true,
            wireless: false,
            name: None,
        };
        assert!(apply_filter(vec![], &filter).is_empty());
    }

    #[test]
    fn filter_name_matches_hostname_fallback() {
        let client: Client =
            serde_json::from_str(r#"{"hostname": "raspberrypi", "type": "WIRED"}"#).unwrap();
        let filter = ListFilter {
            wired: false,
            wireless: false,
            name: Some("raspberry".to_string()),
        };
        let result = apply_filter(vec![client], &filter);
        assert_eq!(result.len(), 1);
    }
}
