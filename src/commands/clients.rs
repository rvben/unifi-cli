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

pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
    /// Field names already validated against `fields::CLIENTS_LIST`.
    pub fields: Option<Vec<String>>,
}

/// A client as `clients list` reports it: the integration-API record joined with
/// the live legacy record for the same MAC.
///
/// The integration API knows every client the controller has seen and carries
/// `connectedAt`. Only the legacy `stat/sta` record carries the live address,
/// SSID, signal, network and VLAN. Neither alone answers "which SSID is each
/// client on", and reading `ip` from the integration record makes `clients list`
/// disagree with `clients show` whenever a client has lost its lease.
pub(crate) struct ClientRow {
    pub client: Client,
    pub detail: Option<LegacyClient>,
}

impl ClientRow {
    /// The address the client holds right now, or None while it has none.
    ///
    /// Deliberately not `Client::ip_address`, which is the last address the
    /// controller ever saw and lingers long after the lease is gone.
    pub fn live_ip(&self) -> Option<&str> {
        self.detail.as_ref().and_then(|d| d.ip.as_deref())
    }

    pub fn to_json(&self) -> serde_json::Value {
        let c = &self.client;
        let d = self.detail.as_ref();
        serde_json::json!({
            "name": c.clean_name(),
            "mac": c.mac_address.as_deref().map(|m| format_mac(&normalize_mac(m))),
            "ip": self.live_ip(),
            "type": c.client_type,
            "ssid": d.and_then(|d| d.ssid.as_deref()),
            "signal": d.and_then(|d| d.signal),
            "uptime": d.and_then(|d| d.uptime),
            "network": d.and_then(|d| d.network.as_deref()),
            "vlan": d.and_then(|d| d.vlan),
            "tx_bytes": d.and_then(|d| d.tx_bytes),
            "rx_bytes": d.and_then(|d| d.rx_bytes),
            "blocked": d.map(|d| d.blocked),
            "connected_at": c.connected_at,
        })
    }
}

/// Join integration-API clients to their live legacy records by MAC.
///
/// A client with no legacy record is kept, with its live fields null. That is
/// the honest answer: it is associated (the controller lists it) but the
/// controller has no current address or SSID for it.
pub(crate) fn merge_rows(clients: Vec<Client>, legacy: Vec<LegacyClient>) -> Vec<ClientRow> {
    let mut by_mac: std::collections::HashMap<String, LegacyClient> = legacy
        .into_iter()
        .filter_map(|l| {
            let key = l.mac.as_deref().map(normalize_mac)?;
            Some((key, l))
        })
        .collect();

    clients
        .into_iter()
        .map(|c| {
            let detail = c
                .mac_address
                .as_deref()
                .and_then(|m| by_mac.remove(&normalize_mac(m)));
            ClientRow { client: c, detail }
        })
        .collect()
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

fn render_clients(rows: &[ClientRow], out: &OutputConfig) {
    if out.is_json() {
        out.print_data(
            &serde_json::to_string_pretty(&rows.iter().map(ClientRow::to_json).collect::<Vec<_>>())
                .expect("failed to serialize JSON"),
        );
    } else {
        let color = use_color();
        let names: Vec<String> = rows.iter().map(|r| r.client.clean_name()).collect();
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

        for (r, name) in rows.iter().zip(&names) {
            let mac = r
                .client
                .mac_address
                .as_deref()
                .map(format_mac)
                .unwrap_or_else(|| "-".into());
            let ip = r.live_ip().unwrap_or("-");
            let ctype = r.client.client_type.as_deref().unwrap_or("-");
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
    out.print_message(&format!("\n{} clients", rows.len()));
}

pub async fn list(
    client: &mut UnifiClient,
    out: OutputConfig,
    filter: ListFilter,
    watch: Option<u64>,
    pagination: Pagination,
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
            match fetch_rows(client, &filter).await {
                Ok(rows) => render_clients(&rows, &out),
                Err(e) => eprintln!("Error: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        }
    } else {
        let rows = fetch_rows(client, &filter).await?;
        let total = rows.len();
        let paginated: Vec<ClientRow> = rows
            .into_iter()
            .skip(pagination.offset)
            .take(pagination.limit)
            .collect();
        if out.is_json() {
            let items: Vec<serde_json::Value> = paginated
                .iter()
                .map(|r| {
                    let mut obj = r.to_json();
                    if let Some(ref keep) = pagination.fields {
                        let map = obj.as_object_mut().expect("row is a JSON object");
                        map.retain(|k, _| keep.iter().any(|f| f == k));
                    }
                    obj
                })
                .collect();
            out.print_data(
                &serde_json::to_string_pretty(&serde_json::json!({
                    "items": items,
                    "total": total,
                    "limit": pagination.limit,
                    "offset": pagination.offset,
                }))
                .expect("failed to serialize JSON"),
            );
        } else {
            render_clients(&paginated, &out);
        }
        Ok(())
    }
}

/// Fetch both client views and join them. Two calls, once, rather than one call
/// per client for anything beyond name/mac/ip/type.
async fn fetch_rows(
    client: &mut UnifiClient,
    filter: &ListFilter,
) -> Result<Vec<ClientRow>, Box<dyn std::error::Error>> {
    let clients = client.list_clients().await?;
    let legacy = client.list_clients_legacy().await?;
    Ok(merge_rows(apply_filter(clients, filter), legacy))
}

pub async fn show(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let c = client.get_client_detail(mac).await?;

    if out.is_json() {
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
            "network": c.network,
            "vlan": c.vlan,
            "blocked": c.blocked,
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
    if let Some(ref network) = c.network {
        println!("  {}  {network}", label("Network:"));
    }
    if let Some(vlan) = c.vlan {
        println!("  {}  {vlan}", label("VLAN:  "));
    }
    if c.blocked {
        println!("  {}  {}", label("Blocked:"), "yes");
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

    if out.is_json() {
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

    fn legacy(json: &str) -> LegacyClient {
        serde_json::from_str(json).unwrap()
    }

    // --- merge_rows: `clients list` must describe a client the way `show` does ---

    #[test]
    fn merge_joins_legacy_detail_by_mac() {
        let clients = vec![make_client(
            "Plug",
            "70:03:9f:90:3c:29",
            "10.10.20.202",
            "WIRELESS",
        )];
        let legacy_records = vec![legacy(
            r#"{"_id":"1","mac":"70:03:9f:90:3c:29","ip":"10.10.20.202","essid":"Notwork",
                "signal":-57,"uptime":321125,"network":"IoT","vlan":20}"#,
        )];

        let rows = merge_rows(clients, legacy_records);
        assert_eq!(rows.len(), 1);
        let json = rows[0].to_json();
        assert_eq!(json["ssid"], "Notwork");
        assert_eq!(json["signal"], -57);
        assert_eq!(json["network"], "IoT");
        assert_eq!(json["vlan"], 20);
        assert_eq!(json["ip"], "10.10.20.202");
    }

    #[test]
    fn merge_matches_macs_in_different_formats() {
        let clients = vec![make_client("X", "AA-BB-CC-DD-EE-FF", "10.0.0.1", "WIRED")];
        let legacy_records = vec![legacy(
            r#"{"_id":"1","mac":"aa:bb:cc:dd:ee:ff","ip":"10.0.0.5"}"#,
        )];
        let rows = merge_rows(clients, legacy_records);
        assert_eq!(rows[0].live_ip(), Some("10.0.0.5"));
    }

    /// The defect this guards: `clients list` read `ip` from the integration API,
    /// which keeps the last address a client ever held. A plug that had dropped
    /// its lease four days earlier was still reported at that address, while
    /// `clients show` correctly reported none.
    #[test]
    fn live_ip_wins_over_the_integration_apis_last_known_address() {
        let clients = vec![make_client(
            "Refrigerator",
            "c4:dd:57:1d:07:e6",
            "10.10.20.203",
            "WIRELESS",
        )];
        let legacy_records = vec![legacy(
            r#"{"_id":"1","mac":"c4:dd:57:1d:07:e6","essid":"Network"}"#,
        )];

        let rows = merge_rows(clients, legacy_records);
        assert_eq!(
            rows[0].live_ip(),
            None,
            "a client with no current lease must report no ip"
        );
        assert_eq!(rows[0].to_json()["ip"], serde_json::Value::Null);
        assert_eq!(rows[0].to_json()["ssid"], "Network");
    }

    #[test]
    fn clients_without_a_legacy_record_survive_with_null_detail() {
        let clients = vec![make_client(
            "Ghost",
            "aa:bb:cc:dd:ee:ff",
            "10.0.0.1",
            "WIRED",
        )];
        let rows = merge_rows(clients, vec![]);
        assert_eq!(rows.len(), 1);
        let json = rows[0].to_json();
        assert_eq!(json["name"], "Ghost");
        assert_eq!(json["ip"], serde_json::Value::Null);
        assert_eq!(json["ssid"], serde_json::Value::Null);
        assert_eq!(json["blocked"], serde_json::Value::Null);
    }

    #[test]
    fn merge_preserves_client_order_and_count() {
        let clients = vec![
            make_client("A", "aa:bb:cc:dd:ee:01", "10.0.0.1", "WIRED"),
            make_client("B", "aa:bb:cc:dd:ee:02", "10.0.0.2", "WIRELESS"),
            make_client("C", "aa:bb:cc:dd:ee:03", "10.0.0.3", "WIRELESS"),
        ];
        let legacy_records = vec![legacy(
            r#"{"_id":"1","mac":"aa:bb:cc:dd:ee:02","ip":"1.2.3.4"}"#,
        )];
        let rows = merge_rows(clients, legacy_records);
        let names: Vec<String> = rows
            .iter()
            .map(|r| r.client.clean_name())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["A", "B", "C"]);
        assert_eq!(rows[1].live_ip(), Some("1.2.3.4"));
        assert_eq!(rows[0].live_ip(), None);
    }

    #[test]
    fn merge_ignores_legacy_records_with_no_matching_client() {
        let clients = vec![make_client("A", "aa:bb:cc:dd:ee:01", "10.0.0.1", "WIRED")];
        let legacy_records = vec![
            legacy(r#"{"_id":"1","mac":"aa:bb:cc:dd:ee:01","ip":"1.1.1.1"}"#),
            legacy(r#"{"_id":"2","mac":"ff:ff:ff:ff:ff:ff","ip":"2.2.2.2"}"#),
        ];
        let rows = merge_rows(clients, legacy_records);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].live_ip(), Some("1.1.1.1"));
    }

    #[test]
    fn to_json_emits_exactly_the_published_field_set() {
        let rows = merge_rows(
            vec![make_client("A", "aa:bb:cc:dd:ee:01", "10.0.0.1", "WIRED")],
            vec![],
        );
        let json = rows[0].to_json();
        let mut emitted: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        let mut published = crate::fields::names(crate::fields::CLIENTS_LIST);
        emitted.sort_unstable();
        published.sort_unstable();
        assert_eq!(
            emitted, published,
            "clients list output must match the fields the schema publishes"
        );
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
