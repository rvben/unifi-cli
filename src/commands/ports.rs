use owo_colors::OwoColorize;

use crate::api::{DeviceWithPorts, PortEntry, UnifiClient, format_bytes, format_mac};
use crate::output::{OutputConfig, use_color};

/// One port, flattened with the device that owns it. Every `ports` subcommand
/// renders these, so the filtered and unfiltered listings cannot drift apart.
pub struct PortRow<'a> {
    pub device_mac: String,
    pub device_name: String,
    pub port: &'a PortEntry,
}

pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
    /// Field names already validated against `fields::PORTS_LIST`.
    pub fields: Option<Vec<String>>,
}

/// Flatten devices into port rows, skipping devices with no port table.
pub fn collect_rows(devices: &[DeviceWithPorts]) -> Vec<PortRow<'_>> {
    let mut rows = Vec::new();
    for d in devices {
        if d.port_table.is_empty() {
            continue;
        }
        let device_mac = d
            .mac
            .as_deref()
            .map(format_mac)
            .unwrap_or_else(|| "-".into());
        let device_name = d
            .name
            .as_deref()
            .or(d.model.as_deref())
            .unwrap_or("-")
            .to_string();
        for port in &d.port_table {
            rows.push(PortRow {
                device_mac: device_mac.clone(),
                device_name: device_name.clone(),
                port,
            });
        }
    }
    rows
}

/// The `PORTS_LIST` field set for one row.
pub fn row_json(row: &PortRow) -> serde_json::Value {
    let p = row.port;
    serde_json::json!({
        "device_mac": row.device_mac,
        "device_name": row.device_name,
        "port_idx": p.port_idx,
        "name": p.name,
        "media": p.media,
        "up": p.up,
        "speed": p.speed,
        "full_duplex": p.full_duplex,
        "poe_enable": p.poe_enable,
        "poe_power": p.poe_power,
        "port_poe": p.port_poe,
        "tx_bytes": p.tx_bytes,
        "rx_bytes": p.rx_bytes,
    })
}

/// Apply a validated `--fields` projection in place.
pub fn project(value: &mut serde_json::Value, fields: &Option<Vec<String>>) {
    if let Some(keep) = fields
        && let Some(map) = value.as_object_mut()
    {
        map.retain(|k, _| keep.iter().any(|f| f == k));
    }
}

/// Human-readable PoE cell: draw in watts, or on/off/- .
fn poe_cell(p: &PortEntry) -> String {
    if p.poe_enable {
        match p.poe_power {
            Some(w) if w > 0.0 => format!("{w:.1}W"),
            _ => "on".into(),
        }
    } else if p.port_poe {
        "off".into()
    } else {
        "-".into()
    }
}

fn speed_cell(p: &PortEntry) -> String {
    if !p.up {
        return "down".into();
    }
    match p.speed {
        Some(s) => format!("{s}{}", if p.full_duplex { "FD" } else { "HD" }),
        None => "up".into(),
    }
}

/// Render rows as a table. `show_device_col` is true only for the unfiltered
/// listing; the filtered table stays byte-identical to what `devices ports`
/// has always printed.
pub fn render_text(rows: &[&PortRow], show_device_col: bool, out: &OutputConfig) {
    let color = use_color();
    let dev_w = rows
        .iter()
        .map(|r| r.device_name.len())
        .max()
        .unwrap_or(6)
        .max(6)
        + 2;

    let header = if show_device_col {
        format!(
            "{:<dev_w$} {:<6} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
            "Device", "Port", "Name", "Link", "Speed", "PoE", "TX", "RX"
        )
    } else {
        format!(
            "{:<6} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
            "Port", "Name", "Link", "Speed", "PoE", "TX", "RX"
        )
    };
    let rule_w = if show_device_col { 70 + dev_w } else { 70 };
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(rule_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(rule_w));
    }

    for r in rows {
        let p = r.port;
        let port = p
            .port_idx
            .map(|i| i.to_string())
            .unwrap_or_else(|| "-".into());
        let name = p.name.as_deref().unwrap_or("-");
        let link = if p.up { "up" } else { "down" };
        let link_display = if color {
            if p.up {
                format!("{}", "up".green())
            } else {
                format!("{}", "down".dimmed())
            }
        } else {
            link.to_string()
        };
        let speed = speed_cell(p);
        let poe = poe_cell(p);
        let tx = p.tx_bytes.map(format_bytes).unwrap_or_else(|| "-".into());
        let rx = p.rx_bytes.map(format_bytes).unwrap_or_else(|| "-".into());

        if show_device_col {
            println!(
                " {:<dev_w$} {:<5} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
                r.device_name, port, name, link_display, speed, poe, tx, rx
            );
        } else {
            println!(
                " {:<5} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
                port, name, link_display, speed, poe, tx, rx
            );
        }
    }
    out.print_message(&format!("\n{} ports", rows.len()));
}

pub async fn list(
    client: &UnifiClient,
    mac: Option<&str>,
    out: OutputConfig,
    pagination: Pagination,
) -> Result<(), Box<dyn std::error::Error>> {
    let devices = match mac {
        Some(m) => vec![client.get_device_ports(m).await?],
        None => client.list_all_device_ports().await?,
    };
    let rows = collect_rows(&devices);
    let total = rows.len();
    let page: Vec<&PortRow> = rows
        .iter()
        .skip(pagination.offset)
        .take(pagination.limit)
        .collect();

    if out.is_json() {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|r| {
                let mut v = row_json(r);
                project(&mut v, &pagination.fields);
                v
            })
            .collect();
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "items": items,
            "total": total,
            "limit": pagination.limit,
            "offset": pagination.offset,
        }))?);
    } else {
        render_text(&page, mac.is_none(), &out);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `DeviceWithPorts` fixture from a JSON literal, exercising the
    /// same `Deserialize` impl the API layer uses.
    fn device(json: serde_json::Value) -> DeviceWithPorts {
        serde_json::from_value(json).expect("test fixture must deserialize as DeviceWithPorts")
    }

    #[test]
    fn collect_rows_flattens_multiple_devices_and_skips_empty_port_tables() {
        let devices = vec![
            device(serde_json::json!({
                "mac": "aa:bb:cc:dd:ee:01", "name": "SwitchA",
                "port_table": [{"port_idx": 1}, {"port_idx": 2}]
            })),
            device(serde_json::json!({
                "mac": "aa:bb:cc:dd:ee:02", "name": "APWithNoPorts",
                "port_table": []
            })),
            device(serde_json::json!({
                "mac": "aa:bb:cc:dd:ee:03", "name": "SwitchC",
                "port_table": [{"port_idx": 1}]
            })),
        ];

        let rows = collect_rows(&devices);

        assert_eq!(
            rows.len(),
            3,
            "device with an empty port_table must contribute no rows"
        );
        assert_eq!(rows[0].device_name, "SwitchA");
        assert_eq!(rows[0].port.port_idx, Some(1));
        assert_eq!(rows[1].device_name, "SwitchA");
        assert_eq!(rows[1].port.port_idx, Some(2));
        assert_eq!(rows[2].device_name, "SwitchC");
        assert_eq!(rows[2].port.port_idx, Some(1));
        assert!(
            rows.iter().all(|r| r.device_name != "APWithNoPorts"),
            "a device with no ports must never appear in the flattened rows"
        );
    }

    #[test]
    fn collect_rows_formats_device_mac_and_falls_back_to_model_when_name_is_absent() {
        let devices = vec![
            device(serde_json::json!({
                "mac": "9c05d6bc0643", "name": "USW-24-PoE",
                "port_table": [{"port_idx": 1}]
            })),
            device(serde_json::json!({
                "mac": "aabbccddeeff", "model": "USW-Lite-8",
                "port_table": [{"port_idx": 1}]
            })),
            device(serde_json::json!({
                "mac": "112233445566",
                "port_table": [{"port_idx": 1}]
            })),
        ];

        let rows = collect_rows(&devices);

        assert_eq!(
            rows[0].device_mac, "9c:05:d6:bc:06:43",
            "device_mac must be formatted via format_mac, not passed through raw"
        );
        assert_eq!(rows[0].device_name, "USW-24-PoE");

        assert_eq!(rows[1].device_mac, "aa:bb:cc:dd:ee:ff");
        assert_eq!(
            rows[1].device_name, "USW-Lite-8",
            "device_name must fall back to model when name is absent"
        );

        assert_eq!(rows[2].device_mac, "11:22:33:44:55:66");
        assert_eq!(
            rows[2].device_name, "-",
            "device_name must fall back to '-' when both name and model are absent"
        );
    }

    #[test]
    fn row_json_emits_exactly_the_fields_declared_in_ports_list() {
        let devices = vec![device(serde_json::json!({
            "mac": "aa:bb:cc:dd:ee:ff", "name": "SwitchA",
            "port_table": [{
                "port_idx": 1, "name": "Port 1", "media": "GE", "up": true,
                "speed": 1000, "full_duplex": true, "poe_enable": true,
                "poe_power": 4.5, "port_poe": true, "tx_bytes": 100, "rx_bytes": 200
            }]
        }))];
        let rows = collect_rows(&devices);
        let value = row_json(&rows[0]);
        let obj = value.as_object().expect("row_json must emit a JSON object");

        let mut emitted: Vec<&str> = obj.keys().map(String::as_str).collect();
        emitted.sort_unstable();
        let mut declared: Vec<&str> = crate::fields::names(crate::fields::PORTS_LIST);
        declared.sort_unstable();

        assert_eq!(
            emitted, declared,
            "row_json keys must exactly match fields::PORTS_LIST, so the two cannot drift"
        );
    }

    #[test]
    fn project_retains_only_the_requested_fields() {
        let mut value = serde_json::json!({"a": 1, "b": 2, "c": 3});
        project(&mut value, &Some(vec!["a".to_string(), "c".to_string()]));

        let obj = value.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert!(obj.contains_key("a"));
        assert!(obj.contains_key("c"));
        assert!(!obj.contains_key("b"), "unrequested fields must be dropped");
    }

    #[test]
    fn project_is_a_noop_when_fields_is_none() {
        let mut value = serde_json::json!({"a": 1, "b": 2, "c": 3});
        let before = value.clone();

        project(&mut value, &None);

        assert_eq!(
            value, before,
            "a None projection must leave the value untouched"
        );
    }
}
