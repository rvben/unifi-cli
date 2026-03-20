use owo_colors::OwoColorize;

use crate::api::{Device, UnifiClient, format_bytes, format_mac, format_uptime};
use crate::output::{OutputConfig, use_color};

fn render_devices(devices: &[Device], out: &OutputConfig) {
    if out.json {
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
        let header = format!(
            "{:<24} {:<14} {:<19} {:<15} {:<10} {}",
            "Name", "Model", "MAC", "IP", "State", "Firmware"
        );
        if color {
            println!("{}", header.bold());
            println!("{}", "-".repeat(95).dimmed());
        } else {
            println!("{header}");
            println!("{}", "-".repeat(95));
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

            if color {
                println!(
                    " {:<23} {:<14} {:<19} {:<15} {:<10} {}",
                    name.bold(),
                    model,
                    mac.dimmed(),
                    ip,
                    state,
                    fw,
                );
            } else {
                println!(
                    " {:<23} {:<14} {:<19} {:<15} {:<10} {}",
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
        render_devices(&devices, &out);
        Ok(())
    }
}

pub async fn show(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let d = client.get_device_detail(mac).await?;

    if out.json {
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "name": d.name,
            "model": d.model,
            "mac": d.mac,
            "ip": d.ip,
            "state": d.state_str(),
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

pub async fn ports(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let device = client.get_device_ports(mac).await?;

    if device.port_table.is_empty() {
        out.print_message("No port table available for this device (not a switch or router)");
        if out.json {
            out.print_data("[]");
        }
        return Ok(());
    }

    if out.json {
        out.print_data(
            &serde_json::to_string_pretty(
                &device
                    .port_table
                    .iter()
                    .map(|p| {
                        serde_json::json!({
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
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("failed to serialize JSON"),
        );
    } else {
        let device_label = device
            .name
            .as_deref()
            .unwrap_or(device.model.as_deref().unwrap_or("Device"));
        out.print_message(&format!("Ports for {device_label}:\n"));

        let color = use_color();
        let header = format!(
            "{:<6} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
            "Port", "Name", "Link", "Speed", "PoE", "TX", "RX"
        );
        if color {
            println!("{}", header.bold());
            println!("{}", "-".repeat(70).dimmed());
        } else {
            println!("{header}");
            println!("{}", "-".repeat(70));
        }

        for p in &device.port_table {
            let port = p
                .port_idx
                .map(|i| i.to_string())
                .unwrap_or_else(|| "-".into());
            let name = p.name.as_deref().unwrap_or("-");
            let link = if p.up { "up" } else { "down" };
            let speed = if p.up {
                match p.speed {
                    Some(s) => {
                        let duplex = if p.full_duplex { "FD" } else { "HD" };
                        format!("{s}{duplex}")
                    }
                    None => "up".into(),
                }
            } else {
                "down".into()
            };
            let poe = if p.poe_enable {
                match p.poe_power {
                    Some(w) if w > 0.0 => format!("{w:.1}W"),
                    _ => "on".into(),
                }
            } else if p.port_poe {
                "off".into()
            } else {
                "-".into()
            };
            let tx = p.tx_bytes.map(format_bytes).unwrap_or_else(|| "-".into());
            let rx = p.rx_bytes.map(format_bytes).unwrap_or_else(|| "-".into());

            if color {
                let link_display = if p.up {
                    format!("{}", "up".green())
                } else {
                    format!("{}", "down".dimmed())
                };
                println!(
                    " {:<5} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
                    port, name, link_display, speed, poe, tx, rx
                );
            } else {
                println!(
                    " {:<5} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
                    port, name, link, speed, poe, tx, rx
                );
            }
        }
    }
    out.print_message(&format!("\n{} ports", device.port_table.len()));
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
