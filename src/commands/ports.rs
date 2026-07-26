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
        let speed = speed_cell(p);
        let poe = poe_cell(p);
        let tx = p.tx_bytes.map(format_bytes).unwrap_or_else(|| "-".into());
        let rx = p.rx_bytes.map(format_bytes).unwrap_or_else(|| "-".into());

        if show_device_col {
            println!(
                " {:<dev_w$} {:<5} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
                r.device_name, port, name, link, speed, poe, tx, rx
            );
        } else {
            println!(
                " {:<5} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
                port, name, link, speed, poe, tx, rx
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
