use owo_colors::OwoColorize;

use crate::api::{Device, UnifiClient, format_mac, format_uptime};
use crate::commands::ports::{self, PortRow};
use crate::output::{OutputConfig, use_color};

pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
    /// Field names already validated against `fields::DEVICES_LIST`.
    pub fields: Option<Vec<String>>,
}

fn render_devices(devices: &[Device], out: &OutputConfig) {
    if out.is_json() {
        out.print_data(
            &serde_json::to_string_pretty(
                &devices
                    .iter()
                    .map(|d| {
                        serde_json::json!({
                            "name": d.name,
                            "model": d.model,
                            "mac": d.mac_address,
                            "ip": d.ip_address,
                            "state": d.state,
                            "firmware": d.firmware_version,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("failed to serialize JSON"),
        );
    } else {
        let color = use_color();

        // Compute dynamic column widths from data
        let col = |min: usize, label_len: usize, vals: Vec<usize>| -> usize {
            vals.into_iter().max().unwrap_or(0).max(label_len).max(min) + 2
        };
        let names: Vec<&str> = devices
            .iter()
            .map(|d| d.name.as_deref().unwrap_or("-"))
            .collect();
        let models: Vec<&str> = devices
            .iter()
            .map(|d| d.model.as_deref().unwrap_or("-"))
            .collect();

        let name_w = col(4, 4, names.iter().map(|n| n.len()).collect());
        let model_w = col(5, 5, models.iter().map(|m| m.len()).collect());
        let total_w = name_w + model_w + 19 + 15 + 10 + 10;

        let header = format!(
            "{:<name_w$} {:<model_w$} {:<19} {:<15} {:<10} {}",
            "Name", "Model", "MAC", "IP", "State", "Firmware"
        );
        if color {
            println!("{}", header.bold());
            println!("{}", "-".repeat(total_w).dimmed());
        } else {
            println!("{header}");
            println!("{}", "-".repeat(total_w));
        }

        for d in devices {
            let name = d.name.as_deref().unwrap_or("-");
            let model = d.model.as_deref().unwrap_or("-");
            let mac = d
                .mac_address
                .as_deref()
                .map(format_mac)
                .unwrap_or_else(|| "-".into());
            let ip = d.ip_address.as_deref().unwrap_or("-");
            let state = d.state.as_deref().unwrap_or("-");
            let fw = d.firmware_version.as_deref().unwrap_or("-");
            let name_pad = name_w - 1;
            let model_pad = model_w;

            if color {
                println!(
                    " {:<name_pad$} {:<model_pad$} {:<19} {:<15} {:<10} {}",
                    name.bold(),
                    model,
                    mac.dimmed(),
                    ip,
                    state,
                    fw,
                );
            } else {
                println!(
                    " {:<name_pad$} {:<model_pad$} {:<19} {:<15} {:<10} {}",
                    name, model, mac, ip, state, fw
                );
            }
        }
    }
    out.print_message(&format!("\n{} devices", devices.len()));
}

pub async fn list(
    client: &mut UnifiClient,
    out: OutputConfig,
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
            eprintln!("Every {interval}s | devices list (press Ctrl+C to exit)\n");
            match client.list_devices().await {
                Ok(devices) => {
                    render_devices(&devices, &out);
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        }
    } else {
        let devices = client.list_devices().await?;
        let total = devices.len();
        let paginated: Vec<Device> = devices
            .into_iter()
            .skip(pagination.offset)
            .take(pagination.limit)
            .collect();
        if out.is_json() {
            let items: Vec<serde_json::Value> = paginated
                .iter()
                .map(|d| {
                    let mut obj = serde_json::json!({
                        "name": d.name,
                        "model": d.model,
                        "mac": d.mac_address,
                        "ip": d.ip_address,
                        "state": d.state,
                        "firmware": d.firmware_version,
                    });
                    if let Some(ref keep) = pagination.fields {
                        let map = obj.as_object_mut().expect("device is a JSON object");
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
            render_devices(&paginated, &out);
        }
        Ok(())
    }
}

pub async fn show(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let d = client.get_device_detail(mac).await?;

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "name": d.name,
            "model": d.model,
            "mac": d.mac,
            "ip": d.ip,
            "state": d.state_str(),
            "firmware": d.version,
            "version": d.version,
            "uptime": d.uptime,
            "num_sta": d.num_sta,
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

    let name = d.name.as_deref().unwrap_or("Device");
    if color {
        println!("{}", name.bold());
    } else {
        println!("{name}");
    }

    println!(
        "  {}  {}",
        label("Model:   "),
        d.model.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("MAC:     "),
        d.mac
            .as_deref()
            .map(format_mac)
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  {}  {}",
        label("IP:      "),
        d.ip.as_deref().unwrap_or("-")
    );
    println!("  {}  {}", label("State:   "), d.state_str());

    if let Some(ref v) = d.version {
        println!("  {}  {v}", label("Firmware:"));
    }
    if let Some(uptime) = d.uptime {
        println!("  {}  {}", label("Uptime:  "), format_uptime(uptime));
    }
    if let Some(num_sta) = d.num_sta {
        println!("  {}  {num_sta}", label("Clients: "));
    }

    Ok(())
}

pub async fn restart(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    client.restart_device(mac).await?;
    out.print_result(
        &serde_json::json!({"status": "ok", "action": "restart", "mac": format_mac(mac)}),
        &format!("Restarting {}", format_mac(mac)),
    );
    Ok(())
}

/// Alias for `ports list <MAC>`. Deliberately keeps the historical bare JSON
/// array shape: `ports list` emits the paginated `{items,total,...}` envelope,
/// but changing this one from array to object would break every consumer
/// indexing the top level.
pub async fn ports(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let device = client.get_device_ports(mac).await?;

    if device.port_table.is_empty() {
        out.print_message("No port table available for this device (not a switch or router)");
        if out.is_json() {
            out.print_data("[]");
        }
        return Ok(());
    }

    let devices = vec![device];
    // Historical label for a device with neither `name` nor `model`; `ports
    // list` / `ports find` keep "-" via `collect_rows`.
    let rows = ports::collect_rows_with_fallback(&devices, "Device");

    if out.is_json() {
        let items: Vec<serde_json::Value> = rows.iter().map(ports::row_json).collect();
        out.print_data(&serde_json::to_string_pretty(&items)?);
    } else {
        let label = &rows[0].device_name;
        out.print_message(&format!("Ports for {label}:\n"));
        let refs: Vec<&PortRow> = rows.iter().collect();
        let dev_w = ports::device_col_width(&refs);
        ports::render_text(&refs, false, dev_w, &out);
    }
    Ok(())
}

pub async fn upgrade(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    client.upgrade_device(mac).await?;
    out.print_result(
        &serde_json::json!({"status": "ok", "action": "upgrade", "mac": format_mac(mac)}),
        &format!("Upgrading firmware on {}", format_mac(mac)),
    );
    Ok(())
}

pub async fn locate(
    client: &UnifiClient,
    mac: &str,
    off: bool,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    client.locate_device(mac, !off).await?;
    let action = if off { "locate_off" } else { "locate_on" };
    let msg = if off {
        format!("Stopped locating {}", format_mac(mac))
    } else {
        format!("Locating {} (LED blinking)", format_mac(mac))
    };
    out.print_result(
        &serde_json::json!({"status": "ok", "action": action, "mac": format_mac(mac)}),
        &msg,
    );
    Ok(())
}
